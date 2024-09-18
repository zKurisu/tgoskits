use core::{fmt::Write, marker::PhantomData, time::Duration};

use ansi_rgb::{red, yellow, Foreground};
use log::{Level, Log};
use rgb::{Rgb, RGB8};

use crate::{time::TimeSource, Kernel, Platform};

fn level_to_rgb(level: Level) -> RGB8 {
    match level {
        Level::Error => red(),
        Level::Warn => yellow(),
        Level::Info => Rgb::new(0x00, 0xBC, 0x12),
        Level::Debug => Rgb::new(0x16, 0x85, 0xA9),
        Level::Trace => Rgb::new(128, 128, 128),
    }
}

fn level_icon(level: Level) -> &'static str {
    match level {
        Level::Error => "💥",
        Level::Warn => "⚠️ ",
        Level::Info => "💡",
        Level::Debug => "🐛",
        Level::Trace => "🔍",
    }
}

macro_rules! format_record {
    ($record:expr, $d: expr) => {{
        format_args!(
            "{}",
            format_args!(
                "{} {:.3?} [{path}:{line}] {args}\n",
                // "{} [{path}:{line}] {args}\n",
                level_icon($record.level()),
                $d,
                path = $record.target(),
                line = $record.line().unwrap_or(0),
                args = $record.args()
            )
            .fg(level_to_rgb($record.level()))
        )
    }};
}

impl<P: Platform> Log for Kernel<P> {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let duration = self.module_time().since_boot();
            // let _ = self.print(format_record!(record, duration));

            OutFmt {}.write_fmt(format_record!(record, duration));
        }
    }
    fn flush(&self) {}
}

pub struct KernelLogger<P: Platform> {
    _marker: PhantomData<P>,
}

impl<P: Platform> KernelLogger<P> {
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<P: Platform> Log for KernelLogger<P> {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let duration = P::since_boot();
            OutFmt {}.write_fmt(format_record!(record, duration));
        }
    }
    fn flush(&self) {}
}

struct OutFmt;

impl Write for OutFmt {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for ch in s.chars() {
            unsafe {
                STDOUT.write_char(ch);
            }
        }
        Ok(())
    }
}

struct NopWrite;

impl StdoutWrite for NopWrite {
    fn write_char(&self, ch: char) {}
}

static mut STDOUT: &dyn StdoutWrite = &NopWrite;

pub trait StdoutWrite: Sync + Send {
    fn write_char(&self, ch: char);
}

pub fn set_stdout(stdout: &'static dyn StdoutWrite) {
    unsafe { STDOUT = stdout }
}
