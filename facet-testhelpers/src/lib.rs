use facet_ansi::{Style, Stylize as _};
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::io::Write;

struct SimpleLogger;

impl Log for SimpleLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let style = match record.level() {
            Level::Error => Style::new().red(),
            Level::Warn => Style::new().yellow(),
            Level::Info => Style::new().green(),
            Level::Debug => Style::new().cyan(),
            Level::Trace => Style::new().dimmed(),
        };

        eprintln!(
            "{} - {}: {}",
            record.level().style(style),
            record.target().blue(),
            record.args()
        );
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

pub fn setup() {
    #[cfg(not(miri))]
    color_backtrace::install();
    let logger = Box::new(SimpleLogger);
    _ = log::set_boxed_logger(logger);
    log::set_max_level(LevelFilter::Trace);
}
