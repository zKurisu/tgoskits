use core::{num::NonZeroUsize, ptr::NonNull};

use crate::{Direction, DmaError, DmaHandle};

cfg_if::cfg_if! {
    if #[cfg(target_arch = "aarch64")] {
        #[path = "aarch64.rs"]
        pub mod arch;
    } else{
        #[path = "nop.rs"]
        pub mod arch;
    }
}

pub trait DmaOp: Sync + Send + 'static {
    fn page_size(&self) -> usize;

    /// 将虚拟地址映射到 DMA 地址
    ///
    /// # Safety
    /// 只能是单个连续内存块
    unsafe fn map_single(
        &self,
        dma_mask: u64,
        addr: NonNull<u8>,
        size: NonZeroUsize,
        direction: Direction,
    ) -> Result<DmaHandle, DmaError>;

    /// 解除 DMA 映射
    ///
    /// # Safety
    /// 必须与 map_single 配对使用
    unsafe fn unmap_single(&self, handle: DmaHandle);

    /// 写回缓存到内存 (clean)
    fn flush(&self, addr: NonNull<u8>, size: usize) {
        arch::flush(addr, size)
    }

    /// 使缓存无效 (invalidate)
    fn invalidate(&self, addr: NonNull<u8>, size: usize) {
        arch::invalidate(addr, size)
    }

    /// 分配 DMA 可访问内存
    /// # Safety
    ///
    /// - 调用者必须确保 layout 合法
    /// - 返回的内存必须保证连续
    unsafe fn alloc_coherent(
        &self,
        dma_mask: u64,
        layout: core::alloc::Layout,
    ) -> Option<DmaHandle>;

    /// 释放 DMA 内存
    /// # Safety
    /// 调用者必须确保 ptr 和 layout 与 alloc 时匹配
    unsafe fn dealloc_coherent(&self, handle: DmaHandle);

    fn prepare_read(&self, handle: &DmaHandle, offset: usize, size: usize, direction: Direction) {
        if matches!(direction, Direction::FromDevice | Direction::Bidirectional) {
            let ptr = unsafe { handle.as_ptr().add(offset) };

            self.invalidate(unsafe { NonNull::new_unchecked(ptr) }, size);
            
            log::trace!("invalidate addr={ptr:#p}, size={size}");

            if let Some(virt) = handle.alloc_virt
                && virt != handle.origin_virt
            {
                unsafe {
                    let src = core::slice::from_raw_parts(virt.add(offset).as_ptr(), size);
                    let dst = core::slice::from_raw_parts_mut(
                        handle.origin_virt.as_ptr().add(offset),
                        size,
                    );
                    log::trace!("\ncopy\n  @{src:#p} {src:?} ->\n  @{dst:#p} {dst:?}");

                    dst.copy_from_slice(src);
                }
            }
        }
    }

    fn confirm_write(&self, handle: &DmaHandle, offset: usize, size: usize, direction: Direction) {
        if matches!(direction, Direction::ToDevice | Direction::Bidirectional) {
            if let Some(virt) = handle.alloc_virt
                && virt != handle.origin_virt
            {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        handle.origin_virt.as_ptr().add(offset),
                        virt.as_ptr().add(offset),
                        size,
                    );
                }
            }

            self.flush(
                unsafe { NonNull::new_unchecked(handle.as_ptr().add(offset)) },
                size,
            )
        }
    }
}
