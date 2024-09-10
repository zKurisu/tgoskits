use core::{
    hint::spin_loop,
    sync::atomic::{AtomicUsize, Ordering},
};

use lock_api::GuardSend;
use log::warn;

const READER: usize = 1 << 2;
const UPGRADED: usize = 1 << 1;
const WRITER: usize = 1;
const READERS_MASK: usize = !(WRITER | UPGRADED);

pub type RwLock<T> = lock_api::RwLock<RawRwLock, T>;
/// RAII structure used to release the shared read access of a lock when
/// dropped.
pub type RwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, RawRwLock, T>;

/// RAII structure used to release the exclusive write access of a lock when
/// dropped.
pub type RwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RawRwLock, T>;

/// Raw reader-writer lock type backed by the parking lot.
pub struct RawRwLock {
    lock: AtomicUsize,
}

unsafe impl lock_api::RawRwLock for RawRwLock {
    const INIT: RawRwLock = RawRwLock {
        lock: AtomicUsize::new(0),
    };

    type GuardMarker = GuardSend;

    #[inline]
    fn lock_exclusive(&self) {
        // Note: This isn't the best way of implementing a spinlock, but it
        // suffices for the sake of this example.
        while !self.try_lock_exclusive() {
            spin_loop();
        }
    }

    #[inline]
    fn try_lock_exclusive(&self) -> bool {
        if self
            .lock
            .compare_exchange(0, WRITER, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            true
        } else {
            false
        }
    }

    #[inline]
    unsafe fn unlock_exclusive(&self) {
        // Writer is responsible for clearing both WRITER and UPGRADED bits.
        // The UPGRADED bit may be set if an upgradeable lock attempts an upgrade while this lock is held.
        self.lock.fetch_and(!(WRITER | UPGRADED), Ordering::Release);
    }

    #[inline]
    fn lock_shared(&self) {
        // Note: This isn't the best way of implementing a spinlock, but it
        // suffices for the sake of this example.
        while !self.try_lock_shared() {
            spin_loop();
        }
    }

    #[inline]
    fn try_lock_shared(&self) -> bool {
        let value = match self.acquire_reader() {
            Some(v) => v,
            None => return false,
        };
        // We check the UPGRADED bit here so that new readers are prevented when an UPGRADED lock is held.
        // This helps reduce writer starvation.
        if value & (WRITER | UPGRADED) != 0 {
            // Lock is taken, undo.
            self.lock.fetch_sub(READER, Ordering::Release);
            false
        } else {
            true
        }
    }

    #[inline]
    unsafe fn unlock_shared(&self) {
        self.lock.fetch_sub(READER, Ordering::Release);
    }

    #[inline]
    fn is_locked(&self) -> bool {
        let state = self.lock.load(Ordering::Relaxed);
        state & (WRITER | READERS_MASK) != 0
    }

    #[inline]
    fn is_locked_exclusive(&self) -> bool {
        let state = self.lock.load(Ordering::Relaxed);
        state & (WRITER) != 0
    }
}
impl RawRwLock {
    // Acquire a read lock, returning the new lock value.
    fn acquire_reader(&self) -> Option<usize> {
        // An arbitrary cap that allows us to catch overflows long before they happen
        const MAX_READERS: usize = core::usize::MAX / READER / 2;

        let value = self.lock.fetch_add(READER, Ordering::Acquire);

        if value > MAX_READERS * READER {
            self.lock.fetch_sub(READER, Ordering::Relaxed);
            warn!("Too many lock readers, cannot safely proceed");
            None
        } else {
            Some(value)
        }
    }

    unsafe fn force_write_unlock(&self) {
        debug_assert_eq!(self.lock.load(Ordering::Relaxed) & !(WRITER | UPGRADED), 0);
        self.lock.fetch_and(!(WRITER | UPGRADED), Ordering::Release);
    }
}
