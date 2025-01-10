// use core::hint::spin_loop;
// use core::ptr;
// use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

// use pasts::prelude::*;

// pub fn init_exeutor() {
//     unsafe {
//         EXECUTOR = Some(Executor::default());
//     }
// }

// fn executor() -> &'static Executor {
//     unsafe { EXECUTOR.as_ref().unwrap() }
// }

// pub fn block_on(f: impl Future<Output = ()> + 'static) {
//     Executor::default().block_on(f);
// }

// pub fn spawn_boxed(f: impl Future<Output = ()> + 'static) {
//     executor().spawn_boxed(f)
// }

// static VTABLE: RawWakerVTable = RawWakerVTable::new(
//     |_| RawWaker::new(ptr::null(), &VTABLE),
//     |_| {},
//     |_| {},
//     |_| {},
// );

// pub fn block_on<F: Future>(mut fut: F) -> F::Output {
//     // safety: we don't move the future after this line.
//     let mut fut = unsafe { Pin::new_unchecked(&mut fut) };

//     let raw_waker = RawWaker::new(ptr::null(), &VTABLE);
//     let waker = unsafe { Waker::from_raw(raw_waker) };
//     let mut cx = Context::from_waker(&waker);
//     loop {
//         if let Poll::Ready(res) = fut.as_mut().poll(&mut cx) {
//             return res;
//         }
//         spin_loop();
//     }
// }
