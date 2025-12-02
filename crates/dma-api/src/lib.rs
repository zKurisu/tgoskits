#![cfg_attr(not(any(windows, unix)), no_std)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "alloc")]
extern crate alloc;

use core::{ptr::NonNull, sync::atomic::AtomicBool};

mod osal;
mod stream;

pub use stream::*;

/// DMA 传输方向
///
/// 参考 Linux `enum dma_data_direction`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum Direction {
    /// 数据从 CPU 传输到设备 (DMA_TO_DEVICE)
    ToDevice,
    /// 数据从设备传输到 CPU (DMA_FROM_DEVICE)
    FromDevice,
    /// 双向传输 (DMA_BIDIRECTIONAL)
    Bidirectional,
}

/// DMA 地址类型
pub type DmaAddr = u64;

/// 物理地址类型
pub type PhysAddr = u64;

/// DMA 错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaError {
    /// 无效地址
    InvalidAddress,
    /// 映射失败
    MappingFailed,
    /// 超出 DMA 地址范围
    AddressOutOfRange,
    /// 内存分配失败
    AllocationFailed,
}

impl core::fmt::Display for DmaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DmaError::InvalidAddress => write!(f, "Invalid DMA address"),
            DmaError::MappingFailed => write!(f, "DMA mapping failed"),
            DmaError::AddressOutOfRange => write!(f, "Address out of DMA range"),
            DmaError::AllocationFailed => write!(f, "DMA allocation failed"),
        }
    }
}

pub struct DmaHandle {
    pub virt_addr: NonNull<u8>,
    pub dma_addr: DmaAddr,
    pub layout: core::alloc::Layout,
}

/// 操作系统抽象层 trait
///
/// 用于适配不同的 OS/平台
pub trait Osal {
    fn page_size(&self) -> usize;

    /// 将虚拟地址映射到 DMA 地址
    /// 若返回的size小于请求的size，则需要分多次映射
    fn map(&self, addr: NonNull<u8>, size: usize, direction: Direction) -> DmaHandle;

    /// 解除 DMA 映射
    fn unmap(&self, handle: DmaHandle);

    /// 写回缓存到内存 (clean)
    fn flush(&self, addr: NonNull<u8>, size: usize) {
        osal::arch::flush(addr, size)
    }

    /// 使缓存无效 (invalidate)
    fn invalidate(&self, addr: NonNull<u8>, size: usize) {
        osal::arch::invalidate(addr, size)
    }

    /// 分配 DMA 可访问内存
    /// # Safety
    /// 调用者必须确保 layout 合法
    unsafe fn alloc(&self, dma_mask: u64, layout: core::alloc::Layout) -> DmaHandle;

    /// 释放 DMA 内存
    /// # Safety
    /// 调用者必须确保 ptr 和 layout 与 alloc 时匹配
    unsafe fn dealloc(&self, handle: DmaHandle);
}

static mut OSAL: &'static dyn Osal = &osal::NopOsal;
static INIT: AtomicBool = AtomicBool::new(false);

/// 初始化 DMA API
pub fn init(osal: &'static dyn Osal) {
    if INIT.load(core::sync::atomic::Ordering::Acquire) {
        return;
    }

    unsafe {
        OSAL = osal;
    }
    INIT.store(true, core::sync::atomic::Ordering::Release);
}

fn get_osal() -> &'static dyn Osal {
    if !INIT.load(core::sync::atomic::Ordering::Acquire) {
        panic!("dma-api not initialized");
    }
    unsafe { OSAL }
}

fn invalidate(addr: NonNull<u8>, size: usize) {
    get_osal().invalidate(addr, size)
}

fn flush(addr: NonNull<u8>, size: usize) {
    get_osal().flush(addr, size)
}

/// 分配 DMA 可访问内存
pub fn alloc(dma_mask: u64, layout: core::alloc::Layout) -> DmaHandle {
    unsafe { get_osal().alloc(dma_mask, layout) }
}

/// 释放 DMA 内存
#[allow(dead_code)]
pub fn dealloc(h: DmaHandle) {
    unsafe { get_osal().dealloc(h) }
}

fn map(addr: NonNull<u8>, size: usize, direction: Direction) -> DmaHandle {
    get_osal().map(addr, size, direction)
}

fn unmap(h: DmaHandle) {
    get_osal().unmap(h)
}
