// Copyright 2025 The Axvisor Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Runtime library of [ArceOS](https://github.com/arceos-org/arceos).
//!
//! Any application uses ArceOS should link this library. It does some
//! initialization work before entering the application's `main` function.
//!
//! # Cargo Features
//!
//! - `paging`: Enable page table manipulation support.
//! - `smp`: Enable SMP (symmetric multiprocessing) support.
//! - `fs`: Enable filesystem support.
//! - `net`: Enable networking support.
//! - `display`: Enable graphics support.
//!
//!
//! Interrupt handling and task scheduling are mandatory runtime capabilities.

#![cfg_attr(not(test), no_std)]
#![feature(allocator_api)]
#![allow(missing_abi)]

#[cfg(all(feature = "host-test", not(target_os = "none")))]
extern crate std;
#[cfg(all(feature = "std-compat", not(feature = "host-test")))]
extern crate std;

#[macro_use]
extern crate ax_log;

extern crate ax_driver as _;

#[cfg(all(target_os = "none", not(feature = "std-compat"), not(test)))]
mod lang_items;
#[cfg(all(
    feature = "stack-protector",
    any(target_os = "none", target_env = "musl"),
    not(test)
))]
mod stack_protector;

#[cfg(feature = "smp")]
mod mp;

mod boot_memory;
mod bootstrap;
mod guard;
#[cfg(feature = "irq-time-accounting")]
mod irq_time;
#[cfg(feature = "paging")]
pub mod kernel_mapping;
mod klib;
#[cfg(any(feature = "std-compat", target_os = "none"))]
mod panic_output;
mod structured_log;

/// Host-only adapters for testing runtime-owned capability providers.
#[cfg(all(feature = "host-test", not(target_os = "none")))]
pub mod host_test {
    pub use crate::klib::{HostIomapOverride, try_install_iomap_override};
}
mod clock_event;

mod clock_event_runtime;
pub mod console;
mod devices;
pub mod emergency_console;
mod error;
mod fs;
mod interrupt_bootstrap;
#[cfg(any(feature = "ipi", feature = "wake-ipi", test))]
mod ipi_delivery;
pub mod irq;
mod raw_console;
mod registers;
pub mod serial;
mod sync_provider;

/// OS-independent scheduler capabilities.
pub use ax_task as task;
pub mod thread;

#[cfg(all(feature = "net", feature = "fs"))]
mod unix_ns;

pub use ax_hal as hal;
pub use error::{RuntimeError, RuntimeResult};

/// Drains task-console output before shutting down the whole system.
///
/// Fatal paths must bypass this task-context transaction and use the
/// emergency console plus [`ax_hal::power::system_off`] directly.
pub fn terminate() -> ! {
    if let Ok(output) = console::output() {
        let _ = output.drain();
    }
    clock_event_runtime::take_current_clock_event_offline();
    ax_hal::power::system_off()
}

pub(crate) mod build_info {
    include!(concat!(env!("OUT_DIR"), "/build_info.rs"));
}

/// Maximum logical CPU count represented by runtime-sized CPU masks.
#[cfg(feature = "smp")]
pub const CPU_CAPACITY: usize = build_info::CPU_CAPACITY;

/// A uniprocessor runtime represents only CPU zero.
#[cfg(not(feature = "smp"))]
pub const CPU_CAPACITY: usize = 1;

pub use bootstrap::rust_main;

#[cfg(feature = "smp")]
pub use self::mp::rust_main_secondary;

extern crate alloc;

#[cfg(feature = "fs")]
pub(crate) fn runtime_default_task_stack_size() -> usize {
    build_info::TASK_STACK_SIZE
}

fn ax_app_entry() {
    #[cfg(all(feature = "std-compat", not(test)))]
    {
        unsafe extern "C" {
            safe fn __axstd_std_check_entry();
        }
        __axstd_std_check_entry();
    }

    #[cfg(all(not(feature = "std-compat"), not(test)))]
    {
        unsafe extern "C" {
            /// Legacy application's entry point.
            safe fn main();
        }
        main();
    }
}

struct LogIfImpl;

/// Additional consumer of every published log record.
///
/// The OS layer registers a sink here (see `starry-kernel`'s `entry::init`) so
/// records reach the kernel `dmesg` ring as well as the console. The hook is
/// stored as a plain `fn` pointer and is a no-op while unregistered, so an
/// ArceOS application that never sets it pays one relaxed atomic load.
static AUX_LOG_WRITER: core::sync::atomic::AtomicPtr<()> =
    core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Registers the auxiliary log sink used for `dmesg`-style ring buffering.
pub fn set_aux_log_writer(sink: fn(&str)) {
    AUX_LOG_WRITER.store(sink as *mut (), core::sync::atomic::Ordering::Release);
}

/// Forwards one fully formatted record to the auxiliary sink, if registered.
///
/// Formatting goes through the same structured writer the console uses, so the
/// ring buffer sees byte-identical lines (timestamp, CPU id, task id, level,
/// target). Runs after the record has been handed to the console pipeline, and
/// never allocates.
fn publish_aux_record(meta: ax_log::RecordMeta, args: core::fmt::Arguments<'_>) {
    let raw = AUX_LOG_WRITER.load(core::sync::atomic::Ordering::Acquire);
    if raw.is_null() {
        return;
    }
    // SAFETY: `set_aux_log_writer` is the only writer of this slot and it
    // stores a `fn(&str)` value whose lifetime is that of the program.
    let sink: fn(&str) = unsafe { core::mem::transmute(raw) };
    let context = structured_log::with_runtime_log_context(core::convert::identity)
        .unwrap_or_else(|_| structured_log::fallback_runtime_log_context(meta));
    let mut writer = AuxLogWriter { sink };
    let _ = structured_log::write_record(&mut writer, meta, context, args);
}

struct AuxLogWriter {
    sink: fn(&str),
}

impl core::fmt::Write for AuxLogWriter {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        (self.sink)(text);
        Ok(())
    }
}

#[ax_crate_interface::impl_interface]
impl ax_log::LogIf for LogIfImpl {
    fn try_publish(
        meta: ax_log::RecordMeta,
        args: core::fmt::Arguments<'_>,
    ) -> ax_log::PublishStatus {
        if let Some(status) = serial::try_publish_record(meta, args) {
            publish_aux_record(meta, args);
            return status;
        }
        let context = structured_log::with_runtime_log_context(core::convert::identity)
            .unwrap_or_else(|_| structured_log::fallback_runtime_log_context(meta));
        if let Some(status) = console::try_publish_without_runtime(meta, context, args) {
            publish_aux_record(meta, args);
            return status;
        }
        let mut writer = PlatformConsoleWriter::default();
        let published = structured_log::write_record(&mut writer, meta, context, args).is_ok();
        publish_aux_record(meta, args);
        if published {
            ax_log::PublishStatus::Published
        } else {
            ax_log::PublishStatus::Dropped
        }
    }

    fn emergency_write(args: core::fmt::Arguments<'_>) -> usize {
        emergency_console::write_fmt(args)
    }
}

#[derive(Default)]
struct PlatformConsoleWriter {
    written: usize,
}

impl core::fmt::Write for PlatformConsoleWriter {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        ax_hal::console::write_text_bytes(text.as_bytes());
        self.written = self.written.saturating_add(text.len());
        Ok(())
    }
}

use core::sync::atomic::{AtomicUsize, Ordering};

/// Number of CPUs that have completed initialization.
static INITED_CPUS: AtomicUsize = AtomicUsize::new(0);

fn is_init_ok() -> bool {
    INITED_CPUS.load(Ordering::Acquire) == ax_hal::cpu_num()
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(not(feature = "fs"))]
    fn fs_init_accepts_bootargs_without_fs_feature() {
        crate::fs::init(Some("root=/dev/nvme0n1"));
    }
}

pub mod diagnostics;
