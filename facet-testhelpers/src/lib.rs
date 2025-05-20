#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

pub use color_eyre::eyre;
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

/// Installs color-backtrace (except on miri), and sets up a simple logger.
pub fn setup() {
    #[cfg(not(miri))]
    {
        use color_eyre::config::HookBuilder;
        use regex::Regex;
        use std::sync::LazyLock;

        /// This regex is used to filter out unwanted frames in error backtraces.
        /// It ignores panic frames, test runners, and a few threading details.
        ///
        /// Regex: ^(std::panic|core::panic|test::run_test|__pthread_cond_wait|Thread::new::thread_start)
        static IGNORE_FRAMES: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"^(std::panic|core::panic|test::run_test|__pthread_cond_wait|std::sys::(pal|backtrace)|std::thread::Builder|core::ops::function|test::__rust_begin_short_backtrace|<core::panic::|<alloc::boxed::Box<F,A> as core::ops::function::FnOnce<Args>>::call_once)")
                .unwrap()
        });

        // color-eyre filter
        let eyre_filter = {
            move |frames: &mut Vec<&color_eyre::config::Frame>| {
                frames.retain(|frame| {
                    frame
                        .name
                        .as_ref()
                        .map(|n| !IGNORE_FRAMES.is_match(&n.to_string()))
                        .unwrap_or(true)
                });
            }
        };

        HookBuilder::default()
            .add_frame_filter(Box::new(eyre_filter))
            .install()
            .expect("Failed to set up color-eyre");

        // color-backtrace filter
        {
            use color_backtrace::{BacktracePrinter, Frame};

            // The frame filter must be Fn(&mut Vec<&Frame>)
            let filter = move |frames: &mut Vec<&Frame>| {
                frames.retain(|frame| {
                    frame
                        .name
                        .as_ref()
                        .map(|name| !IGNORE_FRAMES.is_match(name))
                        .unwrap_or(true)
                });
            };

            // Build and install custom BacktracePrinter with our filter.
            // Use StandardStream to provide a WriteColor.
            let stderr = color_backtrace::termcolor::StandardStream::stderr(
                color_backtrace::termcolor::ColorChoice::Auto,
            );
            let printer = BacktracePrinter::new().add_frame_filter(Box::new(filter));
            printer.install(Box::new(stderr));
        }
    }

    let logger = Box::new(SimpleLogger);
    log::set_boxed_logger(logger).unwrap();
    log::set_max_level(LevelFilter::Trace);
}
