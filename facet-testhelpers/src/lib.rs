#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

pub use facet_testhelpers_macros::test;

use log::{Level, LevelFilter, Log, Metadata, Record};
use owo_colors::{OwoColorize, Style};
use std::io::Write;
use std::sync::LazyLock;

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

/// Lazy initialization of the global logger.
///
/// This ensures the logger is set up exactly once, regardless of how many
/// tests run in the same process.
static LOGGER_INIT: LazyLock<()> = LazyLock::new(|| {
    let logger = Box::new(SimpleLogger);
    let _ = log::set_boxed_logger(logger);
    log::set_max_level(LevelFilter::Trace);
});

/// Set up a simple logger for tests.
///
/// This function ensures the logger is initialized exactly once using
/// [`LazyLock`], making it safe to use with both `cargo test` and
/// `cargo nextest run`.
///
/// # Recommendation
///
/// While this works with regular `cargo test`, we recommend using
/// `cargo nextest run` for:
/// - Process-per-test isolation
/// - Faster parallel test execution
/// - Better test output and reporting
///
/// Install nextest with: `cargo install cargo-nextest`
///
/// For more information, visit: <https://nexte.st>
pub fn setup() {
    // Print a helpful message if not using nextest
    let is_nextest = std::env::var("NEXTEST").as_deref() == Ok("1");
    if !is_nextest {
        static NEXTEST_WARNING: LazyLock<()> = LazyLock::new(|| {
            eprintln!(
                "💡 Tip: Consider using `cargo nextest run` for better test output and performance."
            );
            eprintln!("   Install with: cargo install cargo-nextest");
            eprintln!("   More info: https://nexte.st");
            eprintln!();
        });
        *NEXTEST_WARNING;
    }

    // Ensure the logger is initialized
    *LOGGER_INIT;
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
