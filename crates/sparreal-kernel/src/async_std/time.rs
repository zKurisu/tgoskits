use core::future::Future;
use core::time::Duration;

use crate::time::since_boot;

pub fn sleep(duration: Duration) -> FutureSleep {
    let now = since_boot();
    FutureSleep {
        wake_at: now + duration,
    }
}

pub struct FutureSleep {
    wake_at: Duration,
}

impl Future for FutureSleep {
    type Output = ();

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let now = since_boot();
        if now >= self.wake_at {
            core::task::Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    }
}
