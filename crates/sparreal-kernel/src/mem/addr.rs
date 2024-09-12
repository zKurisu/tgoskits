use core::ops::Add;

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

impl<T> Addr for Virt<T> {}

impl<T> Virt<T> {
    pub fn convert_to_phys(self, va_offset: usize) -> Phys<T> {
        Phys::from(self.0 as usize - va_offset)
    }

    pub fn as_mut_ptr(self) -> *mut T {
        self.0 as *mut T
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
unsafe impl<T> Send for Phys<T> {}
unsafe impl<T> Sync for Phys<T> {}

pub type PhysAddr<T = u8> = Phys<T>;

impl<T> Addr for Phys<T> {}

impl<T> From<usize> for Phys<T> {
    fn from(value: usize) -> Self {
        unsafe { Self(value as _) }
    }
}
impl<T> Phys<T> {
    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}
pub trait Addr: Into<usize> + From<usize> {
    fn is_aligned_to(self, align: usize) -> bool {
        align_offset(self.into(), align) == 0
    }

    fn is_aligned_4k(self) -> bool {
        self.is_aligned_to(0x1000)
    }

    fn align_down(self, align: usize) -> Self {
        align_down(self.into(), align).into()
    }
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
