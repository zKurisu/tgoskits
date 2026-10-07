use alloc::{boxed::Box, vec::Vec};

use axfs_ng_vfs::{FileNode, VfsError, VfsResult};

use super::{CacheMappingEvent, CacheMappingResult, CachedFileShared, PAGE_SIZE};

/// Upper bound for one detached writeback snapshot batch.
///
/// Linux writeback submits bounded folio/bio batches and never concatenates an
/// arbitrarily long dirty extent into a second full-size heap buffer.  The VFS
/// backing interface is not scatter/gather-aware yet, so this implementation
/// writes each stable page snapshot separately while bounding the number of
/// snapshots retained across I/O.
pub(super) const MAX_WRITEBACK_SNAPSHOT_PAGES: usize = 64;

struct DirtyPageSnapshot {
    pn: u32,
    generation: u64,
    data: Box<[u8]>,
    len: usize,
}

/// 把一段字节写到文件的 `offset`，直到写完或出错（`write_at` 允许短写）。
fn write_all_at(backing: &FileNode, mut data: &[u8], mut offset: u64) -> VfsResult<()> {
    while !data.is_empty() {
        let count = backing.write_at(data, offset)?;
        if count == 0 || count > data.len() {
            return Err(VfsError::Io);
        }
        data = &data[count..];
        offset += count as u64;
    }
    Ok(())
}

impl CachedFileShared {
    pub(super) fn writeback(&self) -> VfsResult<Vec<u32>> {
        let dirty_keys = self.begin_writeback_all_dirty()?;
        self.protect_dirty_pages_before_writeback(&dirty_keys)
            .inspect_err(|_| self.cancel_writeback_tracking(&dirty_keys))?;
        let _io = self.io_lock.lock();
        let result = self.writeback_page_runs(self.len(), &dirty_keys);
        self.finish_writeback_tracking(&dirty_keys);
        result?;
        self.backing()?.sync(false)?;
        Ok(dirty_keys)
    }

    pub(super) fn writeback_pages(&self, pns: &[u32]) -> VfsResult<()> {
        let dirty_keys = self.begin_writeback_pages(pns)?;
        self.protect_dirty_pages_before_writeback(&dirty_keys)
            .inspect_err(|_| self.cancel_writeback_tracking(&dirty_keys))?;
        let _io = self.io_lock.lock();
        let result = self.writeback_page_runs(self.len(), &dirty_keys);
        self.finish_writeback_tracking(&dirty_keys);
        result?;
        self.backing()?.sync(false)?;
        Ok(())
    }

    pub(super) fn sync(&self, data_only: bool) -> VfsResult<()> {
        let dirty_keys = self.begin_writeback_all_dirty()?;
        self.protect_dirty_pages_before_writeback(&dirty_keys)
            .inspect_err(|_| self.cancel_writeback_tracking(&dirty_keys))?;
        let _io = self.io_lock.lock();
        let result = self.writeback_page_runs(self.len(), &dirty_keys);
        self.finish_writeback_tracking(&dirty_keys);
        result?;
        self.backing()?.sync(data_only)?;
        Ok(())
    }

    #[cfg(any(feature = "vfs", feature = "ext4"))]
    pub(super) fn writeback_dirty_for_global_sync(&self) -> VfsResult<()> {
        let dirty_keys = self.begin_writeback_all_dirty()?;
        if dirty_keys.is_empty() {
            return Ok(());
        }
        self.protect_dirty_pages_before_writeback(&dirty_keys)
            .inspect_err(|_| self.cancel_writeback_tracking(&dirty_keys))?;
        let _io = self.io_lock.lock();
        let result = self.writeback_page_runs(self.len(), &dirty_keys);
        self.finish_writeback_tracking(&dirty_keys);
        result
    }

    #[cfg(feature = "vfs")]
    pub(super) fn has_dirty_pages(&self) -> bool {
        self.page_cache.lock().iter().any(|(_, page)| page.dirty)
    }

    /// Writes back every dirty page while the caller already holds `io_lock`.
    ///
    /// Page-cache insertion runs under `io_lock`; when the bounded disk cache
    /// is full of dirty pages it must make capacity by writing them back.  The
    /// regular [`Self::writeback`] path reacquires `io_lock`, so it would
    /// self-deadlock in that context.  This variant reuses the exact same dirty
    /// tracking, snapshotting, and generation-checked clearing, only assuming
    /// the lock is already held and skipping the whole-filesystem sync.
    ///
    /// Precondition: the caller holds `io_lock` and no live mapping endpoint is
    /// installed, so mapping protection cannot run while the lock is held.
    pub(super) fn drain_dirty_pages_locked(&self) -> VfsResult<usize> {
        let dirty_keys = self.begin_writeback_locked(None)?;
        if dirty_keys.is_empty() {
            return Ok(0);
        }
        self.protect_dirty_pages_before_writeback(&dirty_keys)
            .inspect_err(|_| self.finish_writeback_tracking(&dirty_keys))?;
        let result = self.writeback_page_runs(self.len(), &dirty_keys);
        self.finish_writeback_tracking(&dirty_keys);
        result?;
        Ok(dirty_keys.len())
    }

    pub(super) fn protect_dirty_pages_before_writeback(&self, pns: &[u32]) -> VfsResult<()> {
        for pn in pns {
            let Some(paddr) = ({
                let mut cache = self.page_cache.lock();
                cache.get_mut(pn).map(|page| page.paddr()).transpose()?
            }) else {
                continue;
            };
            let event = CacheMappingEvent::WritebackProtect(self.cache_page_identity(*pn, paddr));
            match self.publish_mapping_event(event) {
                CacheMappingResult::Protected => {}
                CacheMappingResult::Busy | CacheMappingResult::Quarantined => {
                    return Err(VfsError::ResourceBusy);
                }
                CacheMappingResult::Retired | CacheMappingResult::Failed => {
                    return Err(VfsError::BadState);
                }
            }
        }
        Ok(())
    }

    fn begin_writeback_all_dirty(&self) -> VfsResult<Vec<u32>> {
        self.begin_writeback(None)
    }

    fn begin_writeback_pages(&self, pns: &[u32]) -> VfsResult<Vec<u32>> {
        self.begin_writeback(Some(pns))
    }

    fn begin_writeback(&self, requested: Option<&[u32]>) -> VfsResult<Vec<u32>> {
        let _io = self.io_lock.lock();
        self.begin_writeback_locked(requested)
    }

    /// Selects the dirty pages to write back.  The caller must hold `io_lock`.
    fn begin_writeback_locked(&self, requested: Option<&[u32]>) -> VfsResult<Vec<u32>> {
        let file_len = self.len();
        let mut requested_pns = if let Some(requested) = requested {
            let mut copy = Vec::new();
            copy.try_reserve_exact(requested.len())
                .map_err(|_| VfsError::NoMemory)?;
            copy.extend_from_slice(requested);
            Some(copy)
        } else {
            None
        };
        if let Some(pns) = requested_pns.as_mut() {
            pns.sort_unstable();
            pns.dedup();
        }
        let mut dirty_keys = Vec::new();
        loop {
            dirty_keys.clear();
            let required = self.page_cache.lock().len();
            if dirty_keys.capacity() < required {
                dirty_keys
                    .try_reserve_exact(required)
                    .map_err(|_| VfsError::NoMemory)?;
            }

            let mut guard = self.page_cache.lock();
            if guard.len() > dirty_keys.capacity() {
                continue;
            }
            for (&pn, page) in guard.iter_mut() {
                if !page.dirty {
                    continue;
                }
                if let Some(requested) = requested_pns.as_ref()
                    && requested.binary_search(&pn).is_err()
                {
                    continue;
                }
                let page_start = pn as u64 * PAGE_SIZE as u64;
                let len = file_len.saturating_sub(page_start).min(PAGE_SIZE as u64);
                if len == 0 {
                    continue;
                }
                page.writeback_protecting = true;
                page.dirty_during_writeback = false;
                dirty_keys.push(pn);
            }
            break;
        }
        dirty_keys.sort_unstable();
        Ok(dirty_keys)
    }

    // The caller samples EOF only after reacquiring io_lock: mapping
    // protection runs lock-external and may race a committed truncate/write.
    fn writeback_page_runs(&self, file_len: u64, pns: &[u32]) -> VfsResult<()> {
        for batch in pns.chunks(MAX_WRITEBACK_SNAPSHOT_PAGES) {
            let snapshots = self.snapshot_dirty_pages(file_len, batch)?;
            self.writeback_snapshot_batch(&snapshots)?;
        }
        Ok(())
    }

    fn writeback_snapshot_batch(&self, snapshots: &[DirtyPageSnapshot]) -> VfsResult<()> {
        let backing = self.backing()?;
        // D1：把**连续**的脏页合并成一次 `write_at`。
        //
        // 原来这里即使拿到的是一批连续页快照，也仍然每页单独下发一次
        // `backing.write_at` —— 8 MiB 顺序写就是 2048 次 4 KiB 写，每次都走一遍
        // ext4 → 日志 → 块层的请求路径。同一张卡上 Linux 的「8 MiB 写 + fsync」
        // 是 0.79–0.86 s，我们是 7.6–8.0 s（见
        // results/2026-10-07-h5f-atime-policy.txt [f]）；而 2048 次 4 KiB 请求正是
        // 我们这边与 Linux（bio 合并成大请求）最大的结构性差异。
        //
        // 合并只在**同一批快照内**做（批大小已由 MAX_WRITEBACK_SNAPSHOT_PAGES
        // 限定），所以写回期间额外持有的内存仍是常数级；缓冲区上限
        // 64 页 = 256 KiB。
        let mut index = 0;
        while index < snapshots.len() {
            // 一段 run：页号连续，且除最后一页外都是整页（非整页只可能出现在
            // 文件末尾，后面不可能再有脏页）。
            let start = index;
            let mut end = index + 1;
            while end < snapshots.len()
                && snapshots[end].pn == snapshots[end - 1].pn + 1
                && snapshots[end - 1].len == PAGE_SIZE
            {
                end += 1;
            }
            let run = &snapshots[start..end];
            if run.len() == 1 {
                write_all_at(backing, &run[0].data[..run[0].len], run[0].pn as u64 * PAGE_SIZE as u64)?;
            } else {
                let total: usize = run.iter().map(|page| page.len).sum();
                let mut merged = Vec::new();
                merged.try_reserve_exact(total).map_err(|_| VfsError::NoMemory)?;
                for page in run {
                    merged.extend_from_slice(&page.data[..page.len]);
                }
                write_all_at(backing, &merged, run[0].pn as u64 * PAGE_SIZE as u64)?;
            }
            index = end;
        }

        let mut guard = self.page_cache.lock();
        for page in snapshots {
            if let Some(current) = guard.get_mut(&page.pn)
                && current.dirty
                && current.dirty_generation == page.generation
                && !current.dirty_during_writeback
            {
                current.dirty = false;
            }
        }
        Ok(())
    }

    fn snapshot_dirty_pages(
        &self,
        file_len: u64,
        pns: &[u32],
    ) -> VfsResult<Vec<DirtyPageSnapshot>> {
        let mut snapshots = Vec::new();
        snapshots
            .try_reserve_exact(pns.len())
            .map_err(|_| VfsError::NoMemory)?;
        for pn in pns {
            let page_start = *pn as u64 * PAGE_SIZE as u64;
            let len = file_len.saturating_sub(page_start).min(PAGE_SIZE as u64) as usize;
            if len == 0 {
                continue;
            }
            let mut data = Vec::new();
            data.try_reserve_exact(len)
                .map_err(|_| VfsError::NoMemory)?;
            let generation = {
                let mut guard = self.page_cache.lock();
                let Some(page) = guard.get_mut(pn) else {
                    continue;
                };
                if !page.dirty {
                    continue;
                }
                data.extend_from_slice(&page.data()[..len]);
                page.dirty_generation
            };
            if data.len() != len {
                return Err(VfsError::BadState);
            }
            snapshots.push(DirtyPageSnapshot {
                pn: *pn,
                generation,
                data: data.into_boxed_slice(),
                len,
            });
        }
        Ok(snapshots)
    }

    fn cancel_writeback_tracking(&self, pns: &[u32]) {
        let _io = self.io_lock.lock();
        self.finish_writeback_tracking(pns);
    }

    fn finish_writeback_tracking(&self, pns: &[u32]) {
        let mut guard = self.page_cache.lock();
        for pn in pns {
            if let Some(page) = guard.get_mut(pn) {
                page.writeback_protecting = false;
                page.dirty_during_writeback = false;
            }
        }
    }
}
