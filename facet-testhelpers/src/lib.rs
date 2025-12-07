#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

pub use facet_testhelpers_macros::test;

use log::{Level, LevelFilter, Log, Metadata, Record};
use owo_colors::{OwoColorize, Style};
use std::io::Write;

struct SimpleLogger;

impl Log for SimpleLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        // Create style based on log level
        let level_style = match record.level() {
            Level::Error => Style::new().fg_rgb::<243, 139, 168>(), // Catppuccin red (Maroon)
            Level::Warn => Style::new().fg_rgb::<249, 226, 175>(),  // Catppuccin yellow (Peach)
            Level::Info => Style::new().fg_rgb::<166, 227, 161>(),  // Catppuccin green (Green)
            Level::Debug => Style::new().fg_rgb::<137, 180, 250>(), // Catppuccin blue (Blue)
            Level::Trace => Style::new().fg_rgb::<148, 226, 213>(), // Catppuccin teal (Teal)
        };

        // Convert level to styled display
        eprintln!(
            "{} - {}: {}",
            record.level().style(level_style),
            record
                .target()
                .style(Style::new().fg_rgb::<137, 180, 250>()), // Blue for the target
            record.args()
        );
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

/// Set up a simple logger.
///
/// # Panics
///
/// Panics if not running under `cargo-nextest`. This crate requires nextest
/// for proper test isolation and logger setup.
pub fn setup() {
    let is_nextest = std::env::var("NEXTEST").as_deref() == Ok("1");
    if !is_nextest {
        let command = if cfg!(miri) {
            "cargo miri nextest run"
        } else {
            "cargo nextest run"
        };
        let message = format!(
            "This test suite requires cargo-nextest to run.\n\
            \n\
            cargo-nextest provides:\n\
              • Process-per-test isolation (required for our logger setup)\n\
              • Faster parallel test execution\n\
              • Better test output and reporting\n\
            \n\
            Install it with:\n\
              cargo install cargo-nextest\n\
            \n\
            Then run tests with:\n\
              {command}\n\
            \n\
            For more information, visit: https://nexte.st"
        );
        let boxed = boxen::builder()
            .border_style(boxen::BorderStyle::Round)
            .padding(1)
            .border_color("red")
            .render(&message)
            .unwrap();
        panic!("\n{boxed}");
    }

    let logger = Box::new(SimpleLogger);
    // Ignore SetLoggerError - logger may already be set if running multiple tests
    // in the same process (e.g., under valgrind with --test-threads=1)
    let _ = log::set_boxed_logger(logger);
    log::set_max_level(LevelFilter::Trace);
}

/// An error type that panics when it's built (such as when you use `?`
/// to coerce to it)
#[derive(Debug)]
pub struct IPanic;

impl<E> From<E> for IPanic
where
    E: core::error::Error + Send + Sync,
{
    #[track_caller]
    fn from(value: E) -> Self {
        panic!("from: {}: {value}", core::panic::Location::caller())
    }
}
