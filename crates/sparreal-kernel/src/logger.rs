use ansi_rgb::{red, yellow, Foreground};
use log::{Level, LevelFilter, Log};
use rgb::{Rgb, RGB8};

use crate::stdout::print;

pub struct BootLogger;
unsafe impl Sync for BootLogger {}

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
    ($record:expr) => {{
        format_args!(
            "{}",
            format_args!(
                // "{} {:.3?} [{path}:{line}] {args}\n",
                "{} [{path}:{line}] {args}\n",
                level_icon($record.level()),
                // since_boot(),
                path = $record.target(),
                line = $record.line().unwrap_or(0),
                args = $record.args()
            )
            .fg(level_to_rgb($record.level()))
        )
    }};
}

impl Log for BootLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let _ = print(format_record!(record));
        }
    }

    fn flush(&self) {}
}

static LOGGER_BOOT: BootLogger = BootLogger {};

pub fn init_boot_log() {
    let _ = log::set_logger(&LOGGER_BOOT).map(|()| log::set_max_level(LevelFilter::Trace));
}
