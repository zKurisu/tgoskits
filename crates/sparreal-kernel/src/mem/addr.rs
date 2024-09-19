use core::{
    fmt::Display,
    ops::{Add, Sub},
};

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Virt<T>(*const T);
unsafe impl<T> Send for Virt<T> {}
unsafe impl<T> Sync for Virt<T> {}

impl<T> From<*const T> for Virt<T> {
    fn from(value: *const T) -> Self {
        Self(value as *const T)
    }
}
impl<T> From<*mut T> for Virt<T> {
    fn from(value: *mut T) -> Self {
        Self(value as *const T)
    }
}
impl<T> From<Virt<T>> for *const T {
    fn from(value: Virt<T>) -> Self {
        value.0 as *const T
    }
}

pub type VirtAddr<T = u8> = Virt<T>;

impl<T> Virt<T> {
    pub const fn new() -> Self {
        Self(0 as *const T)
    }

    pub fn convert_to_phys(self, va_offset: usize) -> Phys<T> {
        Phys::from(self.0 as usize - va_offset)
    }

    pub fn as_mut_ptr(self) -> *mut T {
        self.0 as *mut T
    }

    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl<T> From<usize> for Virt<T> {
    fn from(value: usize) -> Self {
        Self(value as *const T)
    }
}
impl<T> From<Virt<T>> for usize {
    fn from(value: Virt<T>) -> Self {
        value.0 as usize
    }
}
impl<T> From<Phys<T>> for usize {
    fn from(value: Phys<T>) -> Self {
        value.0 as usize
    }
}
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Phys<T>(*const T);

impl<T> PartialEq for Phys<T> {
    fn eq(&self, other: &Self) -> bool {
        unsafe { (self.0 as *const u8 as usize) == (other.0 as *const u8 as usize) }
    }
}

unsafe impl<T> Send for Phys<T> {}
unsafe impl<T> Sync for Phys<T> {}

pub type PhysAddr<T = u8> = Phys<T>;

impl<T> From<usize> for Phys<T> {
    fn from(value: usize) -> Self {
        unsafe { Self(value as _) }
    }
}
impl<T> From<*const T> for Phys<T> {
    fn from(value: *const T) -> Self {
        unsafe { Self(value as _) }
    }
}
impl<T> Phys<T> {
    pub const fn new() -> Self {
        Self(0 as *const T)
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

impl<T> Sub<Phys<T>> for Phys<T> {
    type Output = usize;

    fn sub(self, rhs: Phys<T>) -> Self::Output {
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

impl<T> Add<usize> for Phys<T> {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        (self.as_usize() + rhs).into()
    }
}

impl<T> Display for Phys<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PA({:p})", self.0)
    }
}
