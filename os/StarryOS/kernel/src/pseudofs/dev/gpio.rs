use core::{any::Any, str};

use ax_errno::AxError;
use ax_hal::mem::{PhysAddr, phys_to_virt};
use axfs_ng_vfs::{NodeFlags, VfsResult};
use bytemuck::AnyBitPattern;
use starry_vm::{VmMutPtr, VmPtr};

use crate::pseudofs::DeviceOps;

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
#[derive(Clone, Copy, AnyBitPattern)]
struct GPIOOp {
    group: u32,
    offset: u32,
    value: u32,
}

pub struct GPIODev;

impl GPIODev {
    fn parse_u32(text: &str) -> Result<u32, AxError> {
        let text = text.trim();
        if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16).map_err(|_| AxError::InvalidInput)
        } else {
            text.parse::<u32>().map_err(|_| AxError::InvalidInput)
        }
    }

    fn base_addr(group: u32) -> Result<usize, AxError> {
        GPIO_BASES
            .get(group as usize)
            .copied()
            .ok_or(AxError::InvalidInput)
    }

    fn reg_addr(group: u32, reg_offset: usize) -> Result<usize, AxError> {
        Ok(Self::base_addr(group)? + reg_offset)
    }

    fn register_ptr(group: u32, reg_offset: usize) -> Result<*mut u32, AxError> {
        let paddr = PhysAddr::from_usize(Self::reg_addr(group, reg_offset)?);
        Ok(phys_to_virt(paddr).as_usize() as *mut u32)
    }

    fn read_register(group: u32, reg_offset: usize) -> Result<u32, AxError> {
        let ptr = Self::register_ptr(group, reg_offset)?;
        Ok(unsafe { core::ptr::read_volatile(ptr) })
    }

    fn modify_register(
        group: u32,
        reg_offset: usize,
        mask: u32,
        value: u32,
    ) -> Result<(), AxError> {
        let ptr = Self::register_ptr(group, reg_offset)?;
        unsafe {
            let current = core::ptr::read_volatile(ptr);
            let new = (current & !mask) | (value & mask);
            core::ptr::write_volatile(ptr, new);
        }
        Ok(())
    }

    fn pin_bit(offset: u32) -> Result<u32, AxError> {
        if offset >= GPIO_PINS_PER_GROUP {
            return Err(AxError::InvalidInput);
        }
        Ok(1u32 << offset)
    }

    fn read_pin(group: u32, offset: u32) -> Result<bool, AxError> {
        let bit = Self::pin_bit(offset)?;
        let val = Self::read_register(group, GPIO_EXT_PORTA)?;
        Ok(val & bit != 0)
    }

    fn write_pin(group: u32, offset: u32, high: bool) -> Result<(), AxError> {
        let bit = Self::pin_bit(offset)?;
        let value = if high { bit } else { 0 };
        Self::modify_register(group, GPIO_SWPORTA_DR, bit, value)
    }

    fn set_direction(group: u32, offset: u32, output: bool) -> Result<(), AxError> {
        let bit = Self::pin_bit(offset)?;
        let value = if output { bit } else { 0 };
        Self::modify_register(group, GPIO_SWPORTA_DDR, bit, value)
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
        let input = str::from_utf8(buf).map_err(|_| AxError::InvalidInput)?;
        let mut parts = input.split_whitespace();
        let group = Self::parse_u32(parts.next().ok_or(AxError::InvalidInput)?)?;
        let offset = Self::parse_u32(parts.next().ok_or(AxError::InvalidInput)?)?;
        let third = parts.next().ok_or(AxError::InvalidInput)?;
        if parts.next().is_some() {
            return Err(AxError::InvalidInput);
        }
        match third {
            "in" | "input" => Self::set_direction(group, offset, false)?,
            "out" | "output" => Self::set_direction(group, offset, true)?,
            _ => {
                let value = Self::parse_u32(third)?;
                if value > 1 {
                    return Err(AxError::InvalidInput);
                }
                Self::write_pin(group, offset, value != 0)?;
            }
        }
        Ok(buf.len())
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> VfsResult<usize> {
        match cmd {
            GPIO_SET_DIR => {
                let op: GPIOOp = (arg as *const GPIOOp).vm_read()?;
                Self::set_direction(op.group, op.offset, op.value != 0)?;
                Ok(0)
            }
            GPIO_SET_VAL => {
                let op: GPIOOp = (arg as *const GPIOOp).vm_read()?;
                Self::write_pin(op.group, op.offset, op.value != 0)?;
                Ok(0)
            }
            GPIO_GET_VAL => {
                let mut op: GPIOOp = (arg as *const GPIOOp).vm_read()?;
                op.value = Self::read_pin(op.group, op.offset)? as u32;
                (arg as *mut GPIOOp).vm_write(op)?;
                Ok(0)
            }
            _ => Err(AxError::InvalidInput),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn flags(&self) -> NodeFlags {
        NodeFlags::NON_CACHEABLE | NodeFlags::STREAM
    }
}
