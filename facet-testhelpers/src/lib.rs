#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

use log::{Level, LevelFilter, Log, Metadata, Record};
use std::io::Write;
use yansi::{Paint as _, Style};

struct SimpleLogger;

impl Log for SimpleLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        // Create style based on log level
        let level_style = match record.level() {
            Level::Error => Style::new().rgb(243, 139, 168), // Catppuccin red (Maroon)
            Level::Warn => Style::new().rgb(249, 226, 175),  // Catppuccin yellow (Peach)
            Level::Info => Style::new().rgb(166, 227, 161),  // Catppuccin green (Green)
            Level::Debug => Style::new().rgb(137, 180, 250), // Catppuccin blue (Blue)
            Level::Trace => Style::new().rgb(148, 226, 213), // Catppuccin teal (Teal)
        };

        // Convert level to styled display
        let level = record.level();

        eprintln!(
            "{} - {}: {}",
            level.paint(level_style),
            record.target().blue(),
            record.args()
        );
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

/// Installs color-backtrace (except on miri), and sets up a simple logger.
pub fn setup() {
    #[cfg(not(miri))]
    color_eyre::install().expect("Failed to set up color-eyre");
    #[cfg(not(miri))]
    color_backtrace::install();

    let logger = Box::new(SimpleLogger);
    log::set_boxed_logger(logger).unwrap();
    log::set_max_level(LevelFilter::Trace);
}
