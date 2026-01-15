#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

pub use facet_testhelpers_macros::test;

use std::sync::LazyLock;
use std::time::Instant;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

struct Uptime;

impl FormatTime for Uptime {
    fn format_time(&self, w: &mut Writer<'_>) -> core::fmt::Result {
        let elapsed = START_TIME.elapsed();
        let secs = elapsed.as_secs();
        let millis = elapsed.subsec_millis();
        write!(w, "{:4}.{:03}s", secs, millis)
    }
}

/// Lazy initialization of the global tracing subscriber.
///
/// This ensures the subscriber is set up exactly once, regardless of how many
/// tests run in the same process.
static SUBSCRIBER_INIT: LazyLock<()> = LazyLock::new(|| {
    // Force start time initialization
    let _ = *START_TIME;

    #[cfg(miri)]
    let verbosity = color_backtrace::Verbosity::Medium;

    #[cfg(not(miri))]
    let verbosity = color_backtrace::Verbosity::Full;

    // Install color-backtrace for better panic output (with forced backtraces and colors)
    color_backtrace::BacktracePrinter::new()
        .verbosity(verbosity)
        .add_frame_filter(Box::new(|frames| {
            frames.retain(|frame| {
                let dominated_by_noise = |name: &str| {
                    // Test harness internals
                    name.starts_with("test::run_test")
                        || name.starts_with("test::__rust_begin_short_backtrace")
                        // Panic/unwind machinery
                        || name.starts_with("std::panicking::")
                        || name.starts_with("std::panic::")
                        || name.starts_with("core::panicking::")
                        // Thread spawning
                        || name.starts_with("std::thread::Builder::spawn_unchecked_")
                        || name.starts_with("std::sys::thread::")
                        || name.starts_with("std::sys::backtrace::")
                        // FnOnce::call_once trampolines in std/core/alloc
                        || name.starts_with("core::ops::function::FnOnce::call_once")
                        || name.starts_with("<alloc::boxed::Box<F,A> as core::ops::function::FnOnce<Args>>::call_once")
                        // AssertUnwindSafe wrapper
                        || name.starts_with("<core::panic::unwind_safe::AssertUnwindSafe<F> as core::ops::function::FnOnce<()>>::call_once")
                        // Low-level threading primitives
                        || name.starts_with("__pthread")
                };
                match &frame.name {
                    Some(name) => !dominated_by_noise(name),
                    None => true,
                }
            })
        }))
        .install(Box::new(termcolor::StandardStream::stderr(
            termcolor::ColorChoice::AlwaysAnsi,
        )));

    let filter = std::env::var("FACET_LOG")
        .ok()
        .and_then(|s| s.parse::<Targets>().ok())
        .unwrap_or_else(|| Targets::new().with_default(tracing::Level::TRACE));

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(true)
                .with_timer(Uptime)
                .with_target(false)
                .with_level(true)
                .with_file(true)
                .with_line_number(true)
                .compact(),
        )
        .with(filter)
        .try_init()
        .ok();
});

/// Set up a tracing subscriber for tests.
///
/// This function ensures the subscriber is initialized exactly once using
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
        #[allow(clippy::let_unit_value)]
        let _ = *NEXTEST_WARNING;
    }

    // Ensure the subscriber is initialized
    #[allow(clippy::let_unit_value)]
    let _ = *SUBSCRIBER_INIT;
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
