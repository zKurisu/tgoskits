use core::{cell::UnsafeCell, ops::Deref};

pub struct OnceStatic<T>(UnsafeCell<T>);

unsafe impl<T> Send for OnceStatic<T> {}
unsafe impl<T> Sync for OnceStatic<T> {}

impl<T> OnceStatic<T> {
    pub const fn new(v: T) -> Self {
        OnceStatic(UnsafeCell::new(v))
    }

    /// 设置值
    /// # Safety
    ///
    /// 仅在core0初始化时调用
    pub unsafe fn set(&self, v: T) {
        unsafe {
            *self.0.get() = v;
        }
    }

    /// 获取mut 引用
    /// # Safety
    ///
    /// 仅在core0初始化时调用
    pub unsafe fn get(&self) -> *mut T {
        self.0.get()
    }
}

impl<T> Deref for OnceStatic<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0.get() }
    }
}
