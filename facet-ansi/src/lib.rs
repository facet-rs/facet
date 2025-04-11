use std::fmt::{self, Debug, Display, Formatter};

/// A struct to represent a text style including color and formatting.
#[derive(Default, Clone, Copy)]
pub struct Style {
    color: Option<Color>,
    bold: bool,
    underline: bool,
    dimmed: bool,
}

/// Available colors for text styling.
#[derive(Clone, Copy)]
pub enum Color {
    Red,
    Green,
    Blue,
    Yellow,
    Magenta,
    Cyan,
    White,
    Black,
    BrightRed,
    BrightGreen,
    BrightBlue,
    BrightYellow,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl Style {
    /// Create a new style with no color or formatting.
    pub fn new() -> Self {
        Style {
            color: None,
            bold: false,
            underline: false,
            dimmed: false,
        }
    }

    /// Set the style color to red.
    pub fn red(self) -> Self {
        Style {
            color: Some(Color::Red),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to green.
    pub fn green(self) -> Self {
        Style {
            color: Some(Color::Green),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to blue.
    pub fn blue(self) -> Self {
        Style {
            color: Some(Color::Blue),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to yellow.
    pub fn yellow(self) -> Self {
        Style {
            color: Some(Color::Yellow),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to magenta.
    pub fn magenta(self) -> Self {
        Style {
            color: Some(Color::Magenta),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to cyan.
    pub fn cyan(self) -> Self {
        Style {
            color: Some(Color::Cyan),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to white.
    pub fn white(self) -> Self {
        Style {
            color: Some(Color::White),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to black.
    pub fn black(self) -> Self {
        Style {
            color: Some(Color::Black),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to bright red.
    pub fn bright_red(self) -> Self {
        Style {
            color: Some(Color::BrightRed),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to bright green.
    pub fn bright_green(self) -> Self {
        Style {
            color: Some(Color::BrightGreen),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to bright blue.
    pub fn bright_blue(self) -> Self {
        Style {
            color: Some(Color::BrightBlue),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to bright yellow.
    pub fn bright_yellow(self) -> Self {
        Style {
            color: Some(Color::BrightYellow),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to bright magenta.
    pub fn bright_magenta(self) -> Self {
        Style {
            color: Some(Color::BrightMagenta),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to bright cyan.
    pub fn bright_cyan(self) -> Self {
        Style {
            color: Some(Color::BrightCyan),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the style color to bright white.
    pub fn bright_white(self) -> Self {
        Style {
            color: Some(Color::BrightWhite),
            bold: self.bold,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the text style to bold.
    pub fn bold(self) -> Self {
        Style {
            color: self.color,
            bold: true,
            underline: self.underline,
            dimmed: self.dimmed,
        }
    }

    /// Set the text style to underlined.
    pub fn underline(self) -> Self {
        Style {
            color: self.color,
            bold: self.bold,
            underline: true,
            dimmed: self.dimmed,
        }
    }

    /// Set the text style to dimmed.
    pub fn dimmed(self) -> Self {
        Style {
            color: self.color,
            bold: self.bold,
            underline: self.underline,
            dimmed: true,
        }
    }
}

/// A struct that wraps a value and its style.
pub struct StyledDisplay<T> {
    value: T,
    style: Style,
}

impl Color {
    fn ansi_code(&self) -> &'static str {
        match self {
            Color::Red => "\x1b[31m",
            Color::Green => "\x1b[32m",
            Color::Blue => "\x1b[34m",
            Color::Yellow => "\x1b[33m",
            Color::Magenta => "\x1b[35m",
            Color::Cyan => "\x1b[36m",
            Color::White => "\x1b[37m",
            Color::Black => "\x1b[30m",
            Color::BrightRed => "\x1b[91m",
            Color::BrightGreen => "\x1b[92m",
            Color::BrightBlue => "\x1b[94m",
            Color::BrightYellow => "\x1b[93m",
            Color::BrightMagenta => "\x1b[95m",
            Color::BrightCyan => "\x1b[96m",
            Color::BrightWhite => "\x1b[97m",
        }
    }
}

impl Style {
    // Helper method to generate formatting codes
    fn format_codes(&self) -> String {
        let mut format_codes = String::new();

        if self.bold {
            format_codes.push_str("\x1b[1m");
        }

        if self.underline {
            format_codes.push_str("\x1b[4m");
        }

        if self.dimmed {
            format_codes.push_str("\x1b[2m");
        }

        if let Some(color) = self.color {
            format_codes.push_str(color.ansi_code());
        }

        format_codes
    }
}

impl<T: Display> Display for StyledDisplay<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let format_codes = self.style.format_codes();

        if format_codes.is_empty() {
            write!(f, "{}", self.value)
        } else {
            write!(f, "{}{}\x1b[0m", format_codes, self.value)
        }
    }
}

impl<T: Debug> Debug for StyledDisplay<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let format_codes = self.style.format_codes();

        if format_codes.is_empty() {
            write!(f, "{:?}", self.value)
        } else {
            write!(f, "{}{:?}\x1b[0m", format_codes, self.value)
        }
    }
}

/// Extension trait for styling any Display value.
pub trait Stylize {
    /// Apply a style to a value.
    fn style(self, style: Style) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply red color style to a value.
    fn red(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply green color style to a value.
    fn green(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply blue color style to a value.
    fn blue(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply yellow color style to a value.
    fn yellow(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply magenta color style to a value.
    fn magenta(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply cyan color style to a value.
    fn cyan(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply white color style to a value.
    fn white(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply black color style to a value.
    fn black(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply bright red color style to a value.
    fn bright_red(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply bright green color style to a value.
    fn bright_green(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply bright blue color style to a value.
    fn bright_blue(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply bright yellow color style to a value.
    fn bright_yellow(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply bright magenta color style to a value.
    fn bright_magenta(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply bright cyan color style to a value.
    fn bright_cyan(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply bright white color style to a value.
    fn bright_white(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply bold style to a value.
    fn bold(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply underline style to a value.
    fn underline(self) -> StyledDisplay<Self>
    where
        Self: Sized;

    /// Apply dimmed style to a value.
    fn dimmed(self) -> StyledDisplay<Self>
    where
        Self: Sized;
}

impl<T: Display> Stylize for T {
    fn style(self, style: Style) -> StyledDisplay<Self> {
        StyledDisplay { value: self, style }
    }

    fn red(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().red(),
        }
    }

    fn green(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().green(),
        }
    }

    fn blue(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().blue(),
        }
    }

    fn yellow(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().yellow(),
        }
    }

    fn magenta(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().magenta(),
        }
    }

    fn cyan(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().cyan(),
        }
    }

    fn white(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().white(),
        }
    }

    fn black(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().black(),
        }
    }

    fn bright_red(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().bright_red(),
        }
    }

    fn bright_green(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().bright_green(),
        }
    }

    fn bright_blue(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().bright_blue(),
        }
    }

    fn bright_yellow(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().bright_yellow(),
        }
    }

    fn bright_magenta(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().bright_magenta(),
        }
    }

    fn bright_cyan(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().bright_cyan(),
        }
    }

    fn bright_white(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().bright_white(),
        }
    }

    fn bold(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().bold(),
        }
    }

    fn underline(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().underline(),
        }
    }

    fn dimmed(self) -> StyledDisplay<Self> {
        StyledDisplay {
            value: self,
            style: Style::new().dimmed(),
        }
    }
}
