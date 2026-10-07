//! `/dev/gpio` — SG2002 GPIO character device.
//!
//! The SG2002 exposes four 32-bit GPIO groups at `0x0302_0000 + 0x1000 * n`.
//! Each group has a data register, a direction register and a pad-input
//! register; there is no pin-controller driver in this tree, so the board's
//! bring-up scripts drive individual pins straight from userspace:
//!
//! * `read()` dumps every group's `DR`/`DDR`/`EXT` registers (debug aid);
//! * `write()` takes `"<group> <offset> in|out|<0|1>"`;
//! * `ioctl()` takes [`GPIOOp`] for the three canonical operations
//!   ([`GPIO_SET_DIR`], [`GPIO_SET_VAL`], [`GPIO_GET_VAL`]).
//!
//! Register work is deliberately minimal: only the three registers above are
//! touched, and every access is a volatile 32-bit read/modify/write so a
//! concurrent access to the same group cannot lose unrelated bits.

use core::{
    any::Any,
    str,
    sync::atomic::{AtomicUsize, Ordering},
};

use ax_hal::mem::{PAGE_SIZE_4K, PhysAddr};
use axfs_ng_vfs::{NodeFlags, VfsError, VfsResult};

use crate::{
    mm::{VmMutPtr, VmPtr},
    pseudofs::DeviceOps,
};

const GPIO0_BASE: usize = 0x0302_0000;
const GPIO1_BASE: usize = 0x0302_1000;
const GPIO2_BASE: usize = 0x0302_2000;
const GPIO3_BASE: usize = 0x0302_3000;

const GPIO_SWPORTA_DR: usize = 0x0000;
const GPIO_SWPORTA_DDR: usize = 0x0004;
const GPIO_EXT_PORTA: usize = 0x0050;

const GPIO_GROUP_COUNT: usize = 4;
const GPIO_PINS_PER_GROUP: u32 = 32;

const GPIO_BASES: [usize; GPIO_GROUP_COUNT] = [GPIO0_BASE, GPIO1_BASE, GPIO2_BASE, GPIO3_BASE];

const GPIO_SET_DIR: u32 = 0x01;
const GPIO_SET_VAL: u32 = 0x02;
const GPIO_GET_VAL: u32 = 0x03;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GPIOOp {
    group: u32,
    offset: u32,
    value: u32,
}

pub struct GPIODev;

impl GPIODev {
    fn parse_u32(text: &str) -> Result<u32, VfsError> {
        let text = text.trim();
        let parsed = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16)
        } else {
            text.parse::<u32>()
        };
        parsed.map_err(|_| VfsError::InvalidInput)
    }

    fn base_addr(group: u32) -> Result<usize, VfsError> {
        GPIO_BASES
            .get(group as usize)
            .copied()
            .ok_or(VfsError::InvalidInput)
    }

    /// Returns the kernel alias of one GPIO group's register page.
    ///
    /// The blocks live in device MMIO and this platform has no static linear
    /// MMIO window, so the raw physical address is not dereferenceable — it must
    /// go through the kernel ioremap path. Each group is mapped once and the
    /// translation memoized; a benign race can at worst map the same page twice.
    fn group_alias(group: u32) -> Result<usize, VfsError> {
        static ALIASES: [AtomicUsize; GPIO_GROUP_COUNT] =
            [const { AtomicUsize::new(0) }; GPIO_GROUP_COUNT];

        let index = group as usize;
        let paddr = Self::base_addr(group)?;
        let cached = ALIASES[index].load(Ordering::Acquire);
        if cached != 0 {
            return Ok(cached);
        }
        let vaddr = ax_mm::iomap(PhysAddr::from_usize(paddr), PAGE_SIZE_4K)
            .map_err(|_| VfsError::Io)?
            .as_usize();
        ALIASES[index].store(vaddr, Ordering::Release);
        Ok(vaddr)
    }

    fn register_ptr(group: u32, reg_offset: usize) -> Result<*mut u32, VfsError> {
        Ok((Self::group_alias(group)? + reg_offset) as *mut u32)
    }

    fn read_register(group: u32, reg_offset: usize) -> Result<u32, VfsError> {
        let ptr = Self::register_ptr(group, reg_offset)?;
        // SAFETY: the GPIO blocks are fixed MMIO windows in the kernel's
        // linear mapping; volatile access keeps the compiler from folding the
        // reads and the write below is a matched read-modify-write.
        Ok(unsafe { core::ptr::read_volatile(ptr) })
    }

    fn modify_register(group: u32, reg_offset: usize, mask: u32, value: u32) -> Result<(), VfsError> {
        let ptr = Self::register_ptr(group, reg_offset)?;
        // SAFETY: as above; only the masked bits are replaced.
        unsafe {
            let current = core::ptr::read_volatile(ptr);
            core::ptr::write_volatile(ptr, (current & !mask) | (value & mask));
        }
        Ok(())
    }

    fn pin_bit(offset: u32) -> Result<u32, VfsError> {
        if offset >= GPIO_PINS_PER_GROUP {
            return Err(VfsError::InvalidInput);
        }
        Ok(1u32 << offset)
    }

    fn read_pin(group: u32, offset: u32) -> Result<bool, VfsError> {
        let bit = Self::pin_bit(offset)?;
        Ok(Self::read_register(group, GPIO_EXT_PORTA)? & bit != 0)
    }

    fn write_pin(group: u32, offset: u32, high: bool) -> Result<(), VfsError> {
        let bit = Self::pin_bit(offset)?;
        Self::modify_register(group, GPIO_SWPORTA_DR, bit, if high { bit } else { 0 })
    }

    fn set_direction(group: u32, offset: u32, output: bool) -> Result<(), VfsError> {
        let bit = Self::pin_bit(offset)?;
        Self::modify_register(group, GPIO_SWPORTA_DDR, bit, if output { bit } else { 0 })
    }
}

impl DeviceOps for GPIODev {
    fn read_at(&self, buf: &mut [u8], _offset: u64) -> VfsResult<usize> {
        let mut output = alloc::string::String::with_capacity(256);
        for group in 0..GPIO_GROUP_COUNT as u32 {
            let dr = Self::read_register(group, GPIO_SWPORTA_DR).unwrap_or(0);
            let ddr = Self::read_register(group, GPIO_SWPORTA_DDR).unwrap_or(0);
            let ext = Self::read_register(group, GPIO_EXT_PORTA).unwrap_or(0);
            let _ = core::fmt::write(
                &mut output,
                format_args!("GPIO{group}: DR=0x{dr:08X} DDR=0x{ddr:08X} EXT=0x{ext:08X}\n"),
            );
        }
        let bytes = output.as_bytes();
        let len = bytes.len().min(buf.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }

    fn write_at(&self, buf: &[u8], _offset: u64) -> VfsResult<usize> {
        if buf.is_empty() || buf.iter().all(|b| b.is_ascii_whitespace()) {
            return Ok(0);
        }
        let input = str::from_utf8(buf).map_err(|_| VfsError::InvalidInput)?;
        let mut parts = input.split_whitespace();
        let group = Self::parse_u32(parts.next().ok_or(VfsError::InvalidInput)?)?;
        let offset = Self::parse_u32(parts.next().ok_or(VfsError::InvalidInput)?)?;
        let third = parts.next().ok_or(VfsError::InvalidInput)?;
        if parts.next().is_some() {
            return Err(VfsError::InvalidInput);
        }
        match third {
            "in" | "input" => Self::set_direction(group, offset, false)?,
            "out" | "output" => Self::set_direction(group, offset, true)?,
            _ => {
                let value = Self::parse_u32(third)?;
                if value > 1 {
                    return Err(VfsError::InvalidInput);
                }
                Self::write_pin(group, offset, value != 0)?;
            }
        }
        Ok(buf.len())
    }

    fn ioctl(
        &self,
        current: &crate::task::UserTaskRef,
        cmd: u32,
        arg: usize,
    ) -> VfsResult<usize> {
        let read_op = || -> VfsResult<GPIOOp> {
            (arg as *const GPIOOp)
                .vm_read(current)
                .map_err(|_| VfsError::BadAddress)
        };
        match cmd {
            GPIO_SET_DIR => {
                let op = read_op()?;
                Self::set_direction(op.group, op.offset, op.value != 0)?;
                Ok(0)
            }
            GPIO_SET_VAL => {
                let op = read_op()?;
                Self::write_pin(op.group, op.offset, op.value != 0)?;
                Ok(0)
            }
            GPIO_GET_VAL => {
                let mut op = read_op()?;
                op.value = if Self::read_pin(op.group, op.offset)? { 1 } else { 0 };
                (arg as *mut GPIOOp)
                    .vm_write(current, op)
                    .map_err(|_| VfsError::BadAddress)?;
                Ok(0)
            }
            _ => Err(VfsError::InvalidInput),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn flags(&self) -> NodeFlags {
        NodeFlags::NON_CACHEABLE | NodeFlags::STREAM
    }
}
