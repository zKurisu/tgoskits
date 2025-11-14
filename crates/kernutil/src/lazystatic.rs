use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ops::Deref,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct LazyStatic<T> {
    init: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Send> Sync for LazyStatic<T> {}
unsafe impl<T: Send> Send for LazyStatic<T> {}

impl<T> LazyStatic<T> {
    pub const fn uninit() -> Self {
        LazyStatic {
            init: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    pub fn init(&self, val: T) {
        if self.init.swap(true, Ordering::AcqRel) {
            panic!(
                "LazyStatic {} already initialized",
                core::any::type_name::<T>()
            );
        }
        unsafe { (*self.value.get()).as_mut_ptr().write(val) };
        self.init.store(true, Ordering::Release);
    }
}

impl<T> Deref for LazyStatic<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        if !self.init.load(Ordering::Acquire) {
            panic!("LazyStatic {} not initialized", core::any::type_name::<T>());
        }
        unsafe { &*(*self.value.get()).as_ptr() }
    }
}
