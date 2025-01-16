use core::fmt::Display;
use core::marker::PhantomData;
use core::ops::{Add, Sub};
use core::ptr::NonNull;

use super::va_offset_now;

#[derive(Debug, Clone, Copy)]
pub struct Address {
    pub cpu: usize,
    pub virt: Option<usize>,
    pub bus: Option<u64>,
}

impl Address {
    pub fn new(cpu: usize, virt: Option<*mut u8>, bus: Option<u64>) -> Self {
        Self {
            cpu,
            virt: virt.map(|s| s as usize),
            bus,
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        match self.virt {
            Some(virt) => virt as *const u8,
            None => self.cpu as *const u8,
        }
    }

    pub fn bus(&self) -> u64 {
        match self.bus {
            Some(bus) => bus,
            None => self.cpu as _,
        }
    }

    pub fn physical(&self) -> usize {
        self.cpu
    }
}

impl Add<usize> for Address {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self {
            cpu: self.cpu + rhs,
            virt: self.virt.map(|s| s + rhs),
            bus: self.bus.map(|s| s + rhs as u64),
        }
    }
}

impl Sub<usize> for Address {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self {
            cpu: self.cpu - rhs,
            virt: self.virt.map(|s| s - rhs),
            bus: self.bus.map(|s| s - rhs as u64),
        }
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Virt<T>(usize, PhantomData<T>);
unsafe impl<T> Send for Virt<T> {}

impl<T> From<*const T> for Virt<T> {
    fn from(value: *const T) -> Self {
        Self(value as _, PhantomData)
    }
}
impl<T> From<*mut T> for Virt<T> {
    fn from(value: *mut T) -> Self {
        Self(value as _, PhantomData)
    }
}
impl<T> From<NonNull<T>> for Virt<T> {
    fn from(value: NonNull<T>) -> Self {
        Self(value.as_ptr() as _, PhantomData)
    }
}

impl<T> From<Virt<T>> for *const T {
    fn from(value: Virt<T>) -> Self {
        value.0 as *const T
    }
}

pub type VirtAddr = Virt<u8>;

impl<T> Virt<T> {
    pub const fn new() -> Self {
        Self(0, PhantomData)
    }

    pub fn as_mut_ptr(self) -> *mut T {
        self.0 as *mut T
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl<T> Default for Virt<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<usize> for Virt<T> {
    fn from(value: usize) -> Self {
        Self(value, PhantomData)
    }
}
impl<T> From<Virt<T>> for usize {
    fn from(value: Virt<T>) -> Self {
        value.0
    }
}

impl<T> From<PhysAddr> for Virt<T> {
    fn from(value: PhysAddr) -> Self {
        Self((value.0 + va_offset_now()) as _, PhantomData)
    }
}

impl From<PhysAddr> for usize {
    fn from(value: PhysAddr) -> Self {
        value.0
    }
}

impl<T> From<Virt<T>> for PhysAddr {
    fn from(value: Virt<T>) -> Self {
        Self(value.as_usize() - va_offset_now())
    }
}

#[derive(Default, Clone, Copy, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct PhysAddr(usize);

unsafe impl Send for PhysAddr {}

impl From<usize> for PhysAddr {
    fn from(value: usize) -> Self {
        Self(value as _)
    }
}

impl PhysAddr {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn as_usize(&self) -> usize {
        self.0
    }
}

impl Sub<PhysAddr> for PhysAddr {
    type Output = usize;

    fn sub(self, rhs: PhysAddr) -> Self::Output {
        self.as_usize() - rhs.as_usize()
    }
}

pub trait Align: Clone + Copy
where
    usize: From<Self>,
{
    fn align_down(self, align: usize) -> Self
    where
        Self: From<usize>,
    {
        align_down(self.into(), align).into()
    }

    fn align_up(self, align: usize) -> Self
    where
        Self: From<usize>,
    {
        align_up(self.into(), align).into()
    }

    fn is_aligned_4k(self) -> bool {
        self.is_aligned_to(0x1000)
    }

    fn is_aligned_to(self, align: usize) -> bool {
        align_offset(self.into(), align) == 0
    }
}

impl<T> Align for T
where
    T: Into<usize> + From<usize> + Copy,
    usize: From<T>,
{
}

impl<T> Add<usize> for Virt<T> {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        let lhs: usize = self.into();
        (lhs + rhs).into()
    }
}

pub const fn align_offset(addr: usize, align: usize) -> usize {
    addr & (align - 1)
}

pub const fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

pub const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

impl Add<usize> for PhysAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        (self.as_usize() + rhs).into()
    }
}

impl<T> Sub<usize> for Virt<T> {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        (self.as_usize() - rhs).into()
    }
}

impl<T> Sub<Virt<T>> for Virt<T> {
    type Output = usize;

    fn sub(self, rhs: Virt<T>) -> Self::Output {
        self.as_usize() - rhs.as_usize()
    }
}

impl Display for PhysAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}
impl<T> Display for Virt<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl core::fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}
impl<T> core::fmt::Debug for Virt<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}
