use alloc::{sync::Arc, vec::Vec as AllocVec};
use core::sync::atomic::{AtomicBool, Ordering};

use axfs_ng_vfs::VfsResult;
use heapless::Vec as InlineVec;

use super::{CachedFileShared, PageCache};

const MAX_RECLAIM_BATCH: usize = 256;

/// 关闭后仍然保留页缓存的"小文件"个数上限。
///
/// 这是 H5 的核心：原来的裁剪条件是"没有别人引用就摘掉"，而缓存身份的唯一强引用
/// 来自打开的文件对象 —— 于是**每一次 close 都把整片页缓存丢掉**，下次 open 只能
/// 回 SD 卡读。板上实测 ext4 上 `open+read(0 KB)+close` 中位 10.0 ms（procfs 0.4 ms），
/// 而且每次都重复；exec 一个 busybox ≈36 ms、模型加载 ≈3 s、cat ~32 MB/s 全是这一个原因。
///
/// 保留是有代价的（内存随"被访问过的文件数"增长），所以这里用**可证明的上界**：
/// 一个文件的缓存页数不可能超过它的长度，于是只要限制"保留几个文件"以及"文件多大"，
/// 就能给出总上界，不需要新增任何页计数：
///   4 个文件 × 4 MiB = 16 MiB（本板 236 MB 内存的 7%）。
/// busybox（803 KB）、常用动态库、akars 模型（3.63 MB）都落在窗口内。
const RETAIN_CLOSED_CACHE_FILES: usize = 4;
const RETAIN_CLOSED_CACHE_MAX_BYTES: u64 = 4 * 1024 * 1024;

struct ReclaimGuard;

impl Drop for ReclaimGuard {
    fn drop(&mut self) {
        RECLAIM_IN_PROGRESS.store(false, Ordering::Release);
    }
}

struct CachedFileRegistry {
    files: ax_sync::SpinRwLock<AllocVec<Arc<CachedFileShared>>>,
}

impl CachedFileRegistry {
    const fn new() -> Self {
        Self {
            files: ax_sync::SpinRwLock::new(AllocVec::new()),
        }
    }

    #[cfg(feature = "ext4")]
    fn release(&self, file: &Arc<CachedFileShared>) {
        let removed = {
            let mut registry = self.files.write();
            registry
                .iter()
                .position(|cached| Arc::ptr_eq(cached, file))
                .map(|index| registry.remove(index))
        };
        drop(removed);
    }

    fn prune(&self) {
        self.prune_with(|| {});
    }

    fn prune_with(&self, before_restore: impl FnOnce()) {
        // 一次加锁 + 线性扫描：把"没有别人引用且没有脏页"的缓存身份摘掉。
        // 被摘掉的 Arc 收集到局部 Vec，**锁外**再析构（析构可能取可睡眠的
        // 文件系统锁）。
        //
        // 原实现是 `mem::take` 出整个注册表 → `retain` → 再**逐个**加写锁 +
        // `iter().any(ptr_eq)` 查重后放回：那是 O(n²)，而它在每次创建新的
        // 缓存身份时都会跑（打开一个还没被缓存的文件）。板上实测：
        // `open+read(64K)+close` 中位数 10.3 ms（其中读数据只 ~0.9 ms），
        // `execve("/bin/true")` ≈36 ms —— 都是这 O(n²) 在付账；而且探针的
        // max(44 ms) ≫ median(10 ms) 正说明注册表在增长。
        //
        // 换成原地 retain 之后不再存在"注册表被整体取出"的窗口，因此原来那种
        // "unlink 先置位、恢复时在锁内复查"的补偿也就不需要了：unlink 与本
        // 扫描在同一把写锁下互斥。
        let mut doomed: AllocVec<Arc<CachedFileShared>> = AllocVec::new();
        {
            let mut registry = self.files.write();
            // 注册表按插入顺序排列，保留"最近 N 个"才有意义（保留最旧的一批
            // 等于保留一堆再也不会被访问的文件，而被反复访问的那个反而被裁掉）。
            let tail_start = registry.len().saturating_sub(RETAIN_CLOSED_CACHE_FILES);
            let mut position = 0usize;
            registry.retain(|cached| {
                let index = position;
                position += 1;
                // 已经 unlink / 已被退休的注册必须消失（原来的语义：它们不得被恢复）。
                if cached.unlinked.load(Ordering::Acquire) || cached.retired.load(Ordering::Acquire) {
                    doomed.push(cached.clone());
                    return false;
                }
                if Arc::strong_count(cached) > 1 || cached.has_dirty_pages() {
                    true
                } else if index >= tail_start && cached.len() <= RETAIN_CLOSED_CACHE_MAX_BYTES {
                    // H5：关闭后按上界保留页缓存，见常量说明。
                    true
                } else {
                    // 先计数、后 clone：判定看到的是"注册表自己那一个引用"之外
                    // 还有没有人持有。
                    doomed.push(cached.clone());
                    false
                }
            });
        }
        before_restore();
        drop(doomed);
    }
}

static GLOBAL_CACHED_FILES: CachedFileRegistry = CachedFileRegistry::new();
static RECLAIM_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

fn visit_registered_cached_file<R>(
    index: usize,
    visit: impl FnOnce(&Arc<CachedFileShared>) -> R,
) -> Option<R> {
    // Retain registry read ownership instead of cloning an Arc that could
    // become the last file owner during concurrent pruning. The visitor only
    // uses try-lock clean eviction and never allocates or invokes callbacks.
    let registry = GLOBAL_CACHED_FILES.files.try_read()?;
    registry.get(index).map(visit)
}

/// Reclaims clean disk-backed cache pages without holding listener callbacks
/// under the page-cache lock.
pub fn page_cache_reclaim(num_pages: usize) -> usize {
    if RECLAIM_IN_PROGRESS.swap(true, Ordering::AcqRel) {
        return 0;
    }
    let _guard = ReclaimGuard;

    let mut reclaimed = 0;
    let target = num_pages.max(16).saturating_mul(2);
    let mut visited_files = 0;
    let scan_len = {
        let Some(registry) = GLOBAL_CACHED_FILES.files.try_read() else {
            return 0;
        };
        registry.len()
    };

    // Borrow each registry owner while trying the file locks; pressure reclaim
    // cannot become a cached file's final owner or allocate a snapshot Vec.
    // Concurrent pruning may move an entry between indices; reclaim is a
    // best-effort scan, so a later allocator retry can revisit a skipped entry.
    for index in 0..scan_len {
        let Some(freed) = visit_registered_cached_file(index, |file| {
            file.try_evict_clean_pages(target - reclaimed)
        }) else {
            continue;
        };

        reclaimed += freed;
        visited_files += 1;
        if reclaimed >= target {
            break;
        }
    }
    // The remaining quota goes to the block-layer cache trees; like the
    // page cache above, only clean folios are reclaimable here.
    #[cfg(any(feature = "ext4", feature = "fat"))]
    if reclaimed < target {
        let freed = crate::block::cache::reclaim_clean_folios(target - reclaimed);
        if freed > 0 {
            debug!("page_cache_reclaim: evicted {freed} clean block-cache folios");
        }
        reclaimed += freed;
    }

    if reclaimed > 0 {
        debug!(
            "page_cache_reclaim: evicted {} clean pages across {} files",
            reclaimed, visited_files
        );
    } else if num_pages > 0 {
        // 诊断：分配器已经来求救了，我们却一页都没腾出来 —— 这正是板子上
        // "内存慢慢被吃光最后 OOM panic" 的前兆。打一条有上限的 warn，把
        // 现场信息（注册表里有多少文件、目标页数、回收是否重入）留下来，
        // 免得事故只能靠事后猜。
        static STARVED_REPORTS: core::sync::atomic::AtomicU64 =
            core::sync::atomic::AtomicU64::new(0);
        if STARVED_REPORTS.fetch_add(1, Ordering::Relaxed) < 8 {
            warn!(
                "page_cache_reclaim: freed 0 of {num_pages} requested pages \
                 (registered_files={scan_len}, reentrant_or_empty)",
            );
        }
    }
    reclaimed
}

pub(super) fn register_cached_file(file: &Arc<CachedFileShared>) {
    prune_cached_files();
    let mut registry = GLOBAL_CACHED_FILES.files.write();
    if !registry.iter().any(|cached| Arc::ptr_eq(cached, file)) {
        registry.push(file.clone());
    }
}

/// Drops reclaim ownership after inode reaping or final mount-cache writeback.
///
/// The removed `Arc` is dropped only after releasing the registry spin lock:
/// destroying a cached file can take its sleepable page-cache lock.
#[cfg(feature = "ext4")]
pub(super) fn release_cached_file(file: &Arc<CachedFileShared>) {
    GLOBAL_CACHED_FILES.release(file);
}

pub fn sync_all_cached_files(_data_only: bool) -> VfsResult<()> {
    sync_cached_files(None)
}

fn sync_cached_files(filesystem: Option<&dyn axfs_ng_vfs::FilesystemOps>) -> VfsResult<()> {
    let files = GLOBAL_CACHED_FILES.files.read().clone();
    let mut first_error = None;
    for file in &files {
        if let Some(filesystem) = filesystem
            && !file.backing.as_ref().is_some_and(|backing| {
                super::filesystem_key(backing.filesystem()) == super::filesystem_key(filesystem)
            })
        {
            continue;
        }
        if let Err(error) = file.writeback_dirty_for_global_sync()
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }

    drop(files);
    prune_cached_files();
    first_error.map_or(Ok(()), Err)
}

/// Writes back cached files belonging to one filesystem before its unmount.
pub fn sync_filesystem_cached_files(filesystem: &dyn axfs_ng_vfs::FilesystemOps) -> VfsResult<()> {
    sync_cached_files(Some(filesystem))
}

fn prune_cached_files() {
    GLOBAL_CACHED_FILES.prune();
}

impl CachedFileShared {
    /// Scans the LRU and evicts up to `max` clean pages.
    ///
    /// This allocator-pressure path is allocation-free and only detaches pages
    /// from files without a live mapping endpoint.
    fn try_evict_clean_pages(&self, max: usize) -> usize {
        self.try_evict_clean_pages_with(max, || {})
    }

    fn try_evict_clean_pages_with(&self, max: usize, before_detach: impl FnOnce()) -> usize {
        // Hold endpoint exclusion until every victim has left the cache.
        // A Weak with zero strong refs cannot be resurrected; inspecting it
        // avoids acquiring an Arc whose last Drop might run arbitrary code in
        // allocator-pressure context. Leave tombstone cleanup to normal I/O.
        let Some(installed) = self.mapping_endpoint.try_lock() else {
            return 0;
        };
        if installed
            .as_ref()
            .is_some_and(|endpoint| endpoint.strong_count() != 0)
        {
            return 0;
        }
        before_detach();

        let limit = max.min(MAX_RECLAIM_BATCH);
        let mut pending: InlineVec<PageCache, MAX_RECLAIM_BATCH> = InlineVec::new();
        let Some(mut cache) = self.page_cache.try_lock() else {
            return 0;
        };
        let mut to_pop = [0u32; MAX_RECLAIM_BATCH];
        let mut count = 0;
        for (&pn, page) in cache.iter().rev() {
            if !page.dirty && page.pins == 0 && count < limit {
                to_pop[count] = pn;
                count += 1;
            }
        }
        for &pn in &to_pop[..count] {
            if let Some(page) = cache.pop(&pn) {
                // There is one push per selected key and count <= capacity.
                if pending.push(page).is_err() {
                    unreachable!("reclaim batch exceeds its selected victim count");
                }
            }
        }

        let evicted = pending.len();
        drop(cache);
        drop(installed);
        drop(pending);
        evicted
    }
}

#[cfg(test)]
struct ReclaimTestEndpoint {
    invoked: Arc<AtomicBool>,
}

#[cfg(test)]
impl super::CacheMappingEndpoint for ReclaimTestEndpoint {
    fn publish(&self, _event: super::CacheMappingEvent) -> super::CacheMappingResult {
        self.invoked.store(true, Ordering::Release);
        super::CacheMappingResult::Retired
    }
}

#[cfg(test)]
fn pressure_reclaim_is_allocation_free_and_skips_live_mappings_for_test() -> bool {
    const RECLAIM_PAGES: usize = 32;

    let file = Arc::new(CachedFileShared::new_unbounded(
        (RECLAIM_PAGES * crate::os::memory::PAGE_SIZE) as u64,
    ));
    for page_number in 0..RECLAIM_PAGES as u32 {
        file.page_cache
            .lock()
            .put(page_number, PageCache::detached_for_test());
    }

    let invoked = Arc::new(AtomicBool::new(false));
    let endpoint: Arc<dyn super::CacheMappingEndpoint> = Arc::new(ReclaimTestEndpoint {
        invoked: Arc::clone(&invoked),
    });
    *file.mapping_endpoint.lock() = Some(Arc::downgrade(&endpoint));

    let protected = file.try_evict_clean_pages(RECLAIM_PAGES);
    let protected_pages = file.page_cache.lock().len();
    drop(endpoint);
    let reclaimed = file.try_evict_clean_pages(RECLAIM_PAGES);
    protected == 0
        && protected_pages == RECLAIM_PAGES
        && reclaimed == RECLAIM_PAGES
        && !invoked.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };

    use super::*;

    std::thread_local! {
        static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
    }

    struct ObservedAllocator;

    // SAFETY: all allocation semantics are delegated unchanged to System.
    // The const thread-local Cell cannot allocate or observe another thread.
    unsafe impl GlobalAlloc for ObservedAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|count| {
                if let Some(value) = count.get() {
                    count.set(Some(value + 1));
                }
            });
            // SAFETY: preserve the caller's GlobalAlloc layout contract.
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: this allocation came from System with the same layout.
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: ObservedAllocator = ObservedAllocator;

    #[test]
    fn pressure_reclaim_endpoint_race_never_reallocates_cache_nodes() {
        let file = CachedFileShared::new_unbounded(4096);
        file.page_cache
            .lock()
            .put(0, PageCache::detached_for_test());
        let endpoint: Arc<dyn super::super::CacheMappingEndpoint> = Arc::new(ReclaimTestEndpoint {
            invoked: Arc::new(AtomicBool::new(false)),
        });
        let mut installed_during_reclaim = false;
        ALLOCATIONS.with(|count| count.set(Some(0)));
        let reclaimed = file.try_evict_clean_pages_with(1, || {
            if let Some(mut installed) = file.mapping_endpoint.try_lock() {
                *installed = Some(Arc::downgrade(&endpoint));
                installed_during_reclaim = true;
            }
        });
        let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
        assert_eq!(
            allocations, 0,
            "allocator-pressure rollback must not allocate new LRU nodes"
        );
        assert!(
            !installed_during_reclaim,
            "endpoint publication must be excluded until detachment is complete"
        );
        assert_eq!(reclaimed, 1);
        assert!(file.page_cache.lock().is_empty());
    }

    #[test]
    fn allocator_pressure_reclaim_skips_mapped_pages_without_callbacks() {
        assert!(pressure_reclaim_is_allocation_free_and_skips_live_mappings_for_test());
    }

    #[cfg(feature = "ext4")]
    #[test]
    fn registry_does_not_restore_retired_cache_owners_after_pruning() {
        // Force retirement while pruning temporarily owns the registry entries.
        for unlinked in [true, false] {
            let registry = CachedFileRegistry::new();
            let cached = Arc::new(CachedFileShared::new_unbounded(0));
            let lifetime = Arc::downgrade(&cached);
            registry.files.write().push(cached.clone());
            registry.prune_with(|| {
                if unlinked {
                    cached.mark_unlinked();
                } else {
                    cached.retired.store(true, Ordering::Release);
                }
                registry.release(&cached);
            });
            drop(cached);
            assert!(
                lifetime.upgrade().is_none(),
                "pruning restored retired cache ownership: unlinked={unlinked}"
            );
        }
    }
}
