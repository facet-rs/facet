use std::fmt::{self, Debug, Display, Formatter};

// Re-export Style from anstyle
pub use anstyle::Style;

/// Provides easy access to common styles
pub mod styles {
    use super::*;

    /// Get a red style
    pub fn red() -> Style {
        Style::new().with_red()
    }

    /// Get a green style
    pub fn green() -> Style {
        Style::new().with_green()
    }

    /// Get a blue style
    pub fn blue() -> Style {
        Style::new().with_blue()
    }

    /// Get a yellow style
    pub fn yellow() -> Style {
        Style::new().with_yellow()
    }

    /// Get a magenta style
    pub fn magenta() -> Style {
        Style::new().with_magenta()
    }

    /// Get a cyan style
    pub fn cyan() -> Style {
        Style::new().with_cyan()
    }

    /// Get a white style
    pub fn white() -> Style {
        Style::new().with_white()
    }

    /// Get a black style
    pub fn black() -> Style {
        Style::new().with_black()
    }

    /// Get a bright red style
    pub fn bright_red() -> Style {
        Style::new().with_bright_red()
    }

    /// Get a bright green style
    pub fn bright_green() -> Style {
        Style::new().with_bright_green()
    }

    /// Get a bright blue style
    pub fn bright_blue() -> Style {
        Style::new().with_bright_blue()
    }

    /// Get a bright yellow style
    pub fn bright_yellow() -> Style {
        Style::new().with_bright_yellow()
    }

    /// Get a bright magenta style
    pub fn bright_magenta() -> Style {
        Style::new().with_bright_magenta()
    }

    /// Get a bright cyan style
    pub fn bright_cyan() -> Style {
        Style::new().with_bright_cyan()
    }

    /// Get a bright white style
    pub fn bright_white() -> Style {
        Style::new().with_bright_white()
    }

    /// Get a bold style
    pub fn bold() -> Style {
        Style::new().bold()
    }

    /// Get an underlined style
    pub fn underline() -> Style {
        Style::new().underline()
    }

    /// Get a dimmed style
    pub fn dimmed() -> Style {
        Style::new().dimmed()
    }
}

/// Extensions for creating styles with common colors
pub trait ColorStyle {
    /// Create a new style with red foreground color
    fn with_red(self) -> Style;
    /// Create a new style with green foreground color
    fn with_green(self) -> Style;
    /// Create a new style with blue foreground color
    fn with_blue(self) -> Style;
    /// Create a new style with yellow foreground color
    fn with_yellow(self) -> Style;
    /// Create a new style with magenta foreground color
    fn with_magenta(self) -> Style;
    /// Create a new style with cyan foreground color
    fn with_cyan(self) -> Style;
    /// Create a new style with white foreground color
    fn with_white(self) -> Style;
    /// Create a new style with black foreground color
    fn with_black(self) -> Style;
    /// Create a new style with bright red foreground color
    fn with_bright_red(self) -> Style;
    /// Create a new style with bright green foreground color
    fn with_bright_green(self) -> Style;
    /// Create a new style with bright blue foreground color
    fn with_bright_blue(self) -> Style;
    /// Create a new style with bright yellow foreground color
    fn with_bright_yellow(self) -> Style;
    /// Create a new style with bright magenta foreground color
    fn with_bright_magenta(self) -> Style;
    /// Create a new style with bright cyan foreground color
    fn with_bright_cyan(self) -> Style;
    /// Create a new style with bright white foreground color
    fn with_bright_white(self) -> Style;
}

impl ColorStyle for Style {
    fn with_red(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red)))
    }

    fn with_green(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green)))
    }

    fn with_blue(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Blue)))
    }

    fn with_yellow(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow)))
    }

    fn with_magenta(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Magenta)))
    }

    fn with_cyan(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Cyan)))
    }

    fn with_white(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::White)))
    }

    fn with_black(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Black)))
    }

    fn with_bright_red(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::BrightRed)))
    }

    fn with_bright_green(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::BrightGreen)))
    }

    fn with_bright_blue(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::BrightBlue)))
    }

    fn with_bright_yellow(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::BrightYellow)))
    }

    fn with_bright_magenta(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(
            anstyle::AnsiColor::BrightMagenta,
        )))
    }

    fn with_bright_cyan(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::BrightCyan)))
    }

    fn with_bright_white(self) -> Style {
        self.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::BrightWhite)))
    }
}

/// A struct that wraps a value and its style.
pub struct Styled<T> {
    value: T,
    style: Style,
}

impl<T: Display> Display for Styled<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.style == Style::new() {
            write!(f, "{}", self.value)
        } else {
            // anstyle's Style implements Display which handles all the formatting
            write!(f, "{}{}{}", self.style, self.value, anstyle::Reset)
        }
    }
}

impl<T: Debug> Debug for Styled<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.style == Style::new() {
            write!(f, "{:?}", self.value)
        } else {
            write!(f, "{}{:?}{}", self.style, self.value, anstyle::Reset)
        }
    }
}

/// Extension trait for styling any Display value.
pub trait Stylize {
    /// Apply a style to a value.
    fn style(self, style: Style) -> Styled<Self>
    where
        Self: Sized;

    /// Apply red color style to a value.
    fn red(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply green color style to a value.
    fn green(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply blue color style to a value.
    fn blue(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply yellow color style to a value.
    fn yellow(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply magenta color style to a value.
    fn magenta(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply cyan color style to a value.
    fn cyan(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply white color style to a value.
    fn white(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply black color style to a value.
    fn black(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply bright red color style to a value.
    fn bright_red(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply bright green color style to a value.
    fn bright_green(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply bright blue color style to a value.
    fn bright_blue(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply bright yellow color style to a value.
    fn bright_yellow(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply bright magenta color style to a value.
    fn bright_magenta(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply bright cyan color style to a value.
    fn bright_cyan(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply bright white color style to a value.
    fn bright_white(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply bold style to a value.
    fn bold(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply underline style to a value.
    fn underline(self) -> Styled<Self>
    where
        Self: Sized;

    /// Apply dimmed style to a value.
    fn dimmed(self) -> Styled<Self>
    where
        Self: Sized;
}

impl<T: Display> Stylize for T {
    fn style(self, style: Style) -> Styled<Self> {
        Styled { value: self, style }
    }

    fn red(self) -> Styled<Self> {
        self.style(Style::new().with_red())
    }

    fn green(self) -> Styled<Self> {
        self.style(Style::new().with_green())
    }

    fn blue(self) -> Styled<Self> {
        self.style(Style::new().with_blue())
    }

    fn yellow(self) -> Styled<Self> {
        self.style(Style::new().with_yellow())
    }

    fn magenta(self) -> Styled<Self> {
        self.style(Style::new().with_magenta())
    }

    fn cyan(self) -> Styled<Self> {
        self.style(Style::new().with_cyan())
    }

    fn white(self) -> Styled<Self> {
        self.style(Style::new().with_white())
    }

    fn black(self) -> Styled<Self> {
        self.style(Style::new().with_black())
    }

    fn bright_red(self) -> Styled<Self> {
        self.style(Style::new().with_bright_red())
    }

    fn bright_green(self) -> Styled<Self> {
        self.style(Style::new().with_bright_green())
    }

    fn bright_blue(self) -> Styled<Self> {
        self.style(Style::new().with_bright_blue())
    }

    fn bright_yellow(self) -> Styled<Self> {
        self.style(Style::new().with_bright_yellow())
    }

    fn bright_magenta(self) -> Styled<Self> {
        self.style(Style::new().with_bright_magenta())
    }

    fn bright_cyan(self) -> Styled<Self> {
        self.style(Style::new().with_bright_cyan())
    }

    fn bright_white(self) -> Styled<Self> {
        self.style(Style::new().with_bright_white())
    }

    fn bold(self) -> Styled<Self> {
        self.style(Style::new().bold())
    }

    fn underline(self) -> Styled<Self> {
        self.style(Style::new().underline())
    }

    fn dimmed(self) -> Styled<Self> {
        self.style(Style::new().dimmed())
    }
}
