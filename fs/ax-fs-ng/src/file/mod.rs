mod cache;
mod handle;
mod open;
mod page;

/// 诊断快照：`(身份按 VFS 节点复用, 身份按 inode 复用, 新建身份)` 次数。
///
/// 用于判定"重复 open 同一个文件时页缓存有没有被复用"——`created` 占比高就说明
/// 每次 open 都在重建缓存身份（页缓存不可能复用）。见 `cache` 模块里的计数器说明。
pub fn cached_file_identity_stats() -> (u64, u64, u64) {
    use core::sync::atomic::Ordering;

    (
        cache::IDENTITY_FROM_LOCATION.load(Ordering::Relaxed),
        cache::IDENTITY_FROM_INODE.load(Ordering::Relaxed),
        cache::IDENTITY_CREATED.load(Ordering::Relaxed),
    )
}

#[cfg(feature = "ext4")]
pub(crate) use cache::forget_cached_file_key;
#[cfg(feature = "ext4")]
pub(crate) use cache::retire_filesystem_cache;
pub use cache::{
    CacheMappingEndpoint, CacheMappingEvent, CacheMappingResult, CachePageIdentity,
    CachePageoutDeferred, CachePageoutResult, CachedFile, CachedFileIdentity, CachedFrameIdentity,
    CachedPagePin,
};
#[cfg(feature = "vfs")]
pub use cache::{page_cache_reclaim, sync_all_cached_files, sync_filesystem_cached_files};
pub use handle::{File, FileBackend};
pub use open::{FileFlags, OpenOptions, OpenResult};
pub use page::PageCache;
