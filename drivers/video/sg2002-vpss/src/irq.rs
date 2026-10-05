//! Destructive interrupt-status ownership for VPSS.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};

use crate::{
    registers::{IRQ_OFFLINE_MASK, IRQ_PROGRAM_LATE, IRQ_SC_V1_END, RegisterIo, TOP_INTR_STATUS},
    types::Error,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RunState {
    Idle        = 0,
    Running     = 1,
    Done        = 2,
    ProgramLate = 3,
    Timeout     = 4,
}

impl RunState {
    fn from_raw(value: u8) -> Self {
        match value {
            1 => Self::Running,
            2 => Self::Done,
            3 => Self::ProgramLate,
            4 => Self::Timeout,
            _ => Self::Idle,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StatsSnapshot {
    pub irq_count: u64,
    pub completed_jobs: u64,
    pub program_late_errors: u64,
    pub timeout_errors: u64,
    pub spurious_irqs: u64,
    pub last_irq_status: u32,
}

/// Lock-free state shared by task and interrupt contexts.
pub struct CompletionState {
    state: AtomicU8,
    sequence: AtomicU64,
    irq_status: AtomicU32,
    /// Monotonic timestamp captured by the OS IRQ glue as soon as the
    /// terminal scaler interrupt has been acknowledged.
    finished_at_ns: AtomicU64,
    irq_count: AtomicU64,
    completed_jobs: AtomicU64,
    program_late_errors: AtomicU64,
    timeout_errors: AtomicU64,
    spurious_irqs: AtomicU64,
    last_irq_status: AtomicU32,
}

impl Default for CompletionState {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionState {
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(RunState::Idle as u8),
            sequence: AtomicU64::new(0),
            irq_status: AtomicU32::new(0),
            finished_at_ns: AtomicU64::new(0),
            irq_count: AtomicU64::new(0),
            completed_jobs: AtomicU64::new(0),
            program_late_errors: AtomicU64::new(0),
            timeout_errors: AtomicU64::new(0),
            spurious_irqs: AtomicU64::new(0),
            last_irq_status: AtomicU32::new(0),
        }
    }

    pub fn begin(&self, sequence: u64) -> Result<(), Error> {
        self.state
            .compare_exchange(
                RunState::Idle as u8,
                RunState::Running as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_| Error::Busy)?;
        self.sequence.store(sequence, Ordering::Release);
        self.irq_status.store(0, Ordering::Relaxed);
        self.finished_at_ns.store(0, Ordering::Release);
        Ok(())
    }

    pub fn state(&self) -> RunState {
        RunState::from_raw(self.state.load(Ordering::Acquire))
    }

    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::Acquire)
    }

    pub fn irq_status(&self) -> u32 {
        self.irq_status.load(Ordering::Acquire)
    }

    /// Records the completion instant supplied by the OS IRQ adapter.
    ///
    /// The core driver deliberately has no dependency on a platform clock.
    /// The IRQ adapter must call this before waking the task-side waiter.
    pub fn record_finished_at_ns(&self, timestamp_ns: u64) {
        self.finished_at_ns.store(timestamp_ns, Ordering::Release);
    }

    pub fn finished_at_ns(&self) -> u64 {
        self.finished_at_ns.load(Ordering::Acquire)
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.state(), RunState::Done | RunState::ProgramLate)
    }

    pub(crate) fn record_irq(&self, status: u32) -> bool {
        self.irq_count.fetch_add(1, Ordering::Relaxed);
        self.last_irq_status.store(status, Ordering::Relaxed);
        self.irq_status.fetch_or(status, Ordering::Relaxed);

        if self.state() != RunState::Running {
            self.record_spurious();
            return false;
        }

        if status & IRQ_PROGRAM_LATE != 0 {
            let completed = self
                .state
                .compare_exchange(
                    RunState::Running as u8,
                    RunState::ProgramLate as u8,
                    Ordering::Release,
                    Ordering::Relaxed,
                )
                .is_ok();
            if completed {
                self.program_late_errors.fetch_add(1, Ordering::Relaxed);
            } else {
                self.record_spurious();
            }
            return completed;
        }
        if status & IRQ_SC_V1_END != 0 {
            let completed = self
                .state
                .compare_exchange(
                    RunState::Running as u8,
                    RunState::Done as u8,
                    Ordering::Release,
                    Ordering::Relaxed,
                )
                .is_ok();
            if completed {
                self.completed_jobs.fetch_add(1, Ordering::Relaxed);
            } else {
                self.record_spurious();
            }
            return completed;
        }
        false
    }

    pub(crate) fn record_spurious(&self) {
        self.spurious_irqs.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_timeout(&self) {
        self.timeout_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn claim_timeout(&self) -> bool {
        self.state
            .compare_exchange(
                RunState::Running as u8,
                RunState::Timeout as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub(crate) fn reset_idle(&self) {
        self.state.store(RunState::Idle as u8, Ordering::Release);
    }

    pub fn stats(&self) -> StatsSnapshot {
        StatsSnapshot {
            irq_count: self.irq_count.load(Ordering::Relaxed),
            completed_jobs: self.completed_jobs.load(Ordering::Relaxed),
            program_late_errors: self.program_late_errors.load(Ordering::Relaxed),
            timeout_errors: self.timeout_errors.load(Ordering::Relaxed),
            spurious_irqs: self.spurious_irqs.load(Ordering::Relaxed),
            last_irq_status: self.last_irq_status.load(Ordering::Relaxed),
        }
    }

    pub fn reset_stats(&self) {
        self.irq_count.store(0, Ordering::Relaxed);
        self.completed_jobs.store(0, Ordering::Relaxed);
        self.program_late_errors.store(0, Ordering::Relaxed);
        self.timeout_errors.store(0, Ordering::Relaxed);
        self.spurious_irqs.store(0, Ordering::Relaxed);
        self.last_irq_status.store(0, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrqEvent {
    pub status: u32,
    pub wake_waiter: bool,
}

/// The sole runtime reader and W1C clearer of `TOP_INTR_STATUS`.
#[derive(Clone)]
pub struct IrqHandler<I: RegisterIo> {
    io: I,
    completion: Arc<CompletionState>,
}

impl<I: RegisterIo> IrqHandler<I> {
    pub fn new(io: I, completion: Arc<CompletionState>) -> Self {
        Self { io, completion }
    }

    pub fn handle(&self) -> Option<IrqEvent> {
        let status = self.io.read32(TOP_INTR_STATUS);
        if status == 0 {
            return None;
        }

        // The register is W1C. Clear exactly the single snapshot just read;
        // never synthesize a mask from a later read.
        self.io.write32(TOP_INTR_STATUS, status);
        if status & IRQ_OFFLINE_MASK == 0 {
            self.completion.record_spurious();
            return Some(IrqEvent {
                status,
                wake_waiter: false,
            });
        }
        let wake_waiter = self.completion.record_irq(status);
        Some(IrqEvent {
            status,
            wake_waiter,
        })
    }
}
