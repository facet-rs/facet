//! SVG path data parsing and structured representation.

use facet::Facet;

/// A single SVG path command
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum PathCommand {
    /// Move to (absolute)
    MoveTo { x: f64, y: f64 },
    /// Move to (relative)
    MoveToRel { dx: f64, dy: f64 },
    /// Line to (absolute)
    LineTo { x: f64, y: f64 },
    /// Line to (relative)
    LineToRel { dx: f64, dy: f64 },
    /// Horizontal line to (absolute)
    HorizontalLineTo { x: f64 },
    /// Horizontal line to (relative)
    HorizontalLineToRel { dx: f64 },
    /// Vertical line to (absolute)
    VerticalLineTo { y: f64 },
    /// Vertical line to (relative)
    VerticalLineToRel { dy: f64 },
    /// Cubic Bezier curve (absolute)
    CurveTo {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    /// Cubic Bezier curve (relative)
    CurveToRel {
        dx1: f64,
        dy1: f64,
        dx2: f64,
        dy2: f64,
        dx: f64,
        dy: f64,
    },
    /// Smooth cubic Bezier curve (absolute)
    SmoothCurveTo { x2: f64, y2: f64, x: f64, y: f64 },
    /// Smooth cubic Bezier curve (relative)
    SmoothCurveToRel {
        dx2: f64,
        dy2: f64,
        dx: f64,
        dy: f64,
    },
    /// Quadratic Bezier curve (absolute)
    QuadTo { x1: f64, y1: f64, x: f64, y: f64 },
    /// Quadratic Bezier curve (relative)
    QuadToRel {
        dx1: f64,
        dy1: f64,
        dx: f64,
        dy: f64,
    },
    /// Smooth quadratic Bezier curve (absolute)
    SmoothQuadTo { x: f64, y: f64 },
    /// Smooth quadratic Bezier curve (relative)
    SmoothQuadToRel { dx: f64, dy: f64 },
    /// Arc (absolute)
    Arc {
        rx: f64,
        ry: f64,
        x_rotation: f64,
        large_arc: bool,
        sweep: bool,
        x: f64,
        y: f64,
    },
    /// Arc (relative)
    ArcRel {
        rx: f64,
        ry: f64,
        x_rotation: f64,
        large_arc: bool,
        sweep: bool,
        dx: f64,
        dy: f64,
    },
    /// Close path
    ClosePath,
}

/// Structured SVG path data
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(traits(Default, Display))]
pub struct PathData {
    pub commands: Vec<PathCommand>,
}

impl PathData {
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Move to absolute position (M command)
    pub fn m(mut self, x: f64, y: f64) -> Self {
        self.commands.push(PathCommand::MoveTo { x, y });
        self
    }

    /// Move to relative position (m command)
    pub fn m_rel(mut self, dx: f64, dy: f64) -> Self {
        self.commands.push(PathCommand::MoveToRel { dx, dy });
        self
    }

    /// Line to absolute position (L command)
    pub fn l(mut self, x: f64, y: f64) -> Self {
        self.commands.push(PathCommand::LineTo { x, y });
        self
    }

    /// Line to relative position (l command)
    pub fn l_rel(mut self, dx: f64, dy: f64) -> Self {
        self.commands.push(PathCommand::LineToRel { dx, dy });
        self
    }

    /// Horizontal line to absolute x position (H command)
    pub fn h(mut self, x: f64) -> Self {
        self.commands.push(PathCommand::HorizontalLineTo { x });
        self
    }

    /// Horizontal line to relative x position (h command)
    pub fn h_rel(mut self, dx: f64) -> Self {
        self.commands.push(PathCommand::HorizontalLineToRel { dx });
        self
    }

    /// Vertical line to absolute y position (V command)
    pub fn v(mut self, y: f64) -> Self {
        self.commands.push(PathCommand::VerticalLineTo { y });
        self
    }

    /// Vertical line to relative y position (v command)
    pub fn v_rel(mut self, dy: f64) -> Self {
        self.commands.push(PathCommand::VerticalLineToRel { dy });
        self
    }

    /// Cubic Bezier curve to absolute position (C command)
    pub fn c(mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) -> Self {
        self.commands.push(PathCommand::CurveTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        });
        self
    }

    /// Cubic Bezier curve to relative position (c command)
    pub fn c_rel(mut self, dx1: f64, dy1: f64, dx2: f64, dy2: f64, dx: f64, dy: f64) -> Self {
        self.commands.push(PathCommand::CurveToRel {
            dx1,
            dy1,
            dx2,
            dy2,
            dx,
            dy,
        });
        self
    }

    /// Smooth cubic Bezier curve to absolute position (S command)
    pub fn s(mut self, x2: f64, y2: f64, x: f64, y: f64) -> Self {
        self.commands
            .push(PathCommand::SmoothCurveTo { x2, y2, x, y });
        self
    }

    /// Smooth cubic Bezier curve to relative position (s command)
    pub fn s_rel(mut self, dx2: f64, dy2: f64, dx: f64, dy: f64) -> Self {
        self.commands
            .push(PathCommand::SmoothCurveToRel { dx2, dy2, dx, dy });
        self
    }

    /// Quadratic Bezier curve to absolute position (Q command)
    pub fn q(mut self, x1: f64, y1: f64, x: f64, y: f64) -> Self {
        self.commands.push(PathCommand::QuadTo { x1, y1, x, y });
        self
    }

    /// Quadratic Bezier curve to relative position (q command)
    pub fn q_rel(mut self, dx1: f64, dy1: f64, dx: f64, dy: f64) -> Self {
        self.commands
            .push(PathCommand::QuadToRel { dx1, dy1, dx, dy });
        self
    }

    /// Smooth quadratic Bezier curve to absolute position (T command)
    pub fn t(mut self, x: f64, y: f64) -> Self {
        self.commands.push(PathCommand::SmoothQuadTo { x, y });
        self
    }

    /// Smooth quadratic Bezier curve to relative position (t command)
    pub fn t_rel(mut self, dx: f64, dy: f64) -> Self {
        self.commands.push(PathCommand::SmoothQuadToRel { dx, dy });
        self
    }

    /// Arc to absolute position (A command)
    #[allow(clippy::too_many_arguments)]
    pub fn a(
        mut self,
        rx: f64,
        ry: f64,
        x_rotation: f64,
        large_arc: bool,
        sweep: bool,
        x: f64,
        y: f64,
    ) -> Self {
        self.commands.push(PathCommand::Arc {
            rx,
            ry,
            x_rotation,
            large_arc,
            sweep,
            x,
            y,
        });
        self
    }

    /// Arc to relative position (a command)
    #[allow(clippy::too_many_arguments)]
    pub fn a_rel(
        mut self,
        rx: f64,
        ry: f64,
        x_rotation: f64,
        large_arc: bool,
        sweep: bool,
        dx: f64,
        dy: f64,
    ) -> Self {
        self.commands.push(PathCommand::ArcRel {
            rx,
            ry,
            x_rotation,
            large_arc,
            sweep,
            dx,
            dy,
        });
        self
    }

    /// Close path (Z command)
    pub fn z(mut self) -> Self {
        self.commands.push(PathCommand::ClosePath);
        self
    }

    /// Parse path data from a string
    pub fn parse(s: &str) -> Result<Self, PathParseError> {
        let mut commands = Vec::new();
        let mut chars = s.chars().peekable();

        while let Some(&c) = chars.peek() {
            // Skip whitespace and commas
            if c.is_whitespace() || c == ',' {
                chars.next();
                continue;
            }

            // Parse command
            let cmd = chars.next().unwrap();
            match cmd {
                'M' => {
                    let (x, y) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::MoveTo { x, y });
                    // Subsequent coordinate pairs are implicit LineTo
                    while let Some((x, y)) = try_parse_coord_pair(&mut chars) {
                        commands.push(PathCommand::LineTo { x, y });
                    }
                }
                'm' => {
                    let (dx, dy) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::MoveToRel { dx, dy });
                    while let Some((dx, dy)) = try_parse_coord_pair(&mut chars) {
                        commands.push(PathCommand::LineToRel { dx, dy });
                    }
                }
                'L' => {
                    let (x, y) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::LineTo { x, y });
                    while let Some((x, y)) = try_parse_coord_pair(&mut chars) {
                        commands.push(PathCommand::LineTo { x, y });
                    }
                }
                'l' => {
                    let (dx, dy) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::LineToRel { dx, dy });
                    while let Some((dx, dy)) = try_parse_coord_pair(&mut chars) {
                        commands.push(PathCommand::LineToRel { dx, dy });
                    }
                }
                'H' => {
                    let x = parse_number(&mut chars)?;
                    commands.push(PathCommand::HorizontalLineTo { x });
                    while let Some(x) = try_parse_number(&mut chars) {
                        commands.push(PathCommand::HorizontalLineTo { x });
                    }
                }
                'h' => {
                    let dx = parse_number(&mut chars)?;
                    commands.push(PathCommand::HorizontalLineToRel { dx });
                    while let Some(dx) = try_parse_number(&mut chars) {
                        commands.push(PathCommand::HorizontalLineToRel { dx });
                    }
                }
                'V' => {
                    let y = parse_number(&mut chars)?;
                    commands.push(PathCommand::VerticalLineTo { y });
                    while let Some(y) = try_parse_number(&mut chars) {
                        commands.push(PathCommand::VerticalLineTo { y });
                    }
                }
                'v' => {
                    let dy = parse_number(&mut chars)?;
                    commands.push(PathCommand::VerticalLineToRel { dy });
                    while let Some(dy) = try_parse_number(&mut chars) {
                        commands.push(PathCommand::VerticalLineToRel { dy });
                    }
                }
                'C' => {
                    let (x1, y1) = parse_coord_pair(&mut chars)?;
                    let (x2, y2) = parse_coord_pair(&mut chars)?;
                    let (x, y) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::CurveTo {
                        x1,
                        y1,
                        x2,
                        y2,
                        x,
                        y,
                    });
                }
                'c' => {
                    let (dx1, dy1) = parse_coord_pair(&mut chars)?;
                    let (dx2, dy2) = parse_coord_pair(&mut chars)?;
                    let (dx, dy) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::CurveToRel {
                        dx1,
                        dy1,
                        dx2,
                        dy2,
                        dx,
                        dy,
                    });
                }
                'S' => {
                    let (x2, y2) = parse_coord_pair(&mut chars)?;
                    let (x, y) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::SmoothCurveTo { x2, y2, x, y });
                }
                's' => {
                    let (dx2, dy2) = parse_coord_pair(&mut chars)?;
                    let (dx, dy) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::SmoothCurveToRel { dx2, dy2, dx, dy });
                }
                'Q' => {
                    let (x1, y1) = parse_coord_pair(&mut chars)?;
                    let (x, y) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::QuadTo { x1, y1, x, y });
                }
                'q' => {
                    let (dx1, dy1) = parse_coord_pair(&mut chars)?;
                    let (dx, dy) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::QuadToRel { dx1, dy1, dx, dy });
                }
                'T' => {
                    let (x, y) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::SmoothQuadTo { x, y });
                }
                't' => {
                    let (dx, dy) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::SmoothQuadToRel { dx, dy });
                }
                'A' => {
                    let rx = parse_number(&mut chars)?;
                    let ry = parse_number(&mut chars)?;
                    let x_rotation = parse_number(&mut chars)?;
                    let large_arc = parse_flag(&mut chars)?;
                    let sweep = parse_flag(&mut chars)?;
                    let (x, y) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::Arc {
                        rx,
                        ry,
                        x_rotation,
                        large_arc,
                        sweep,
                        x,
                        y,
                    });
                }
                'a' => {
                    let rx = parse_number(&mut chars)?;
                    let ry = parse_number(&mut chars)?;
                    let x_rotation = parse_number(&mut chars)?;
                    let large_arc = parse_flag(&mut chars)?;
                    let sweep = parse_flag(&mut chars)?;
                    let (dx, dy) = parse_coord_pair(&mut chars)?;
                    commands.push(PathCommand::ArcRel {
                        rx,
                        ry,
                        x_rotation,
                        large_arc,
                        sweep,
                        dx,
                        dy,
                    });
                }
                'Z' | 'z' => {
                    commands.push(PathCommand::ClosePath);
                }
                _ => {
                    return Err(PathParseError::UnknownCommand(cmd));
                }
            }
        }

        Ok(PathData { commands })
    }

    /// Serialize path data to a string
    fn serialize(&self) -> String {
        let mut result = String::new();
        for cmd in &self.commands {
            if !result.is_empty() {
                // No separator needed - commands are self-delimiting
            }
            match cmd {
                PathCommand::MoveTo { x, y } => {
                    result.push_str(&format!("M{},{}", fmt_num(*x), fmt_num(*y)));
                }
                PathCommand::MoveToRel { dx, dy } => {
                    result.push_str(&format!("m{},{}", fmt_num(*dx), fmt_num(*dy)));
                }
                PathCommand::LineTo { x, y } => {
                    result.push_str(&format!("L{},{}", fmt_num(*x), fmt_num(*y)));
                }
                PathCommand::LineToRel { dx, dy } => {
                    result.push_str(&format!("l{},{}", fmt_num(*dx), fmt_num(*dy)));
                }
                PathCommand::HorizontalLineTo { x } => {
                    result.push_str(&format!("H{}", fmt_num(*x)));
                }
                PathCommand::HorizontalLineToRel { dx } => {
                    result.push_str(&format!("h{}", fmt_num(*dx)));
                }
                PathCommand::VerticalLineTo { y } => {
                    result.push_str(&format!("V{}", fmt_num(*y)));
                }
                PathCommand::VerticalLineToRel { dy } => {
                    result.push_str(&format!("v{}", fmt_num(*dy)));
                }
                PathCommand::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    result.push_str(&format!(
                        "C{},{} {},{} {},{}",
                        fmt_num(*x1),
                        fmt_num(*y1),
                        fmt_num(*x2),
                        fmt_num(*y2),
                        fmt_num(*x),
                        fmt_num(*y)
                    ));
                }
                PathCommand::CurveToRel {
                    dx1,
                    dy1,
                    dx2,
                    dy2,
                    dx,
                    dy,
                } => {
                    result.push_str(&format!(
                        "c{},{} {},{} {},{}",
                        fmt_num(*dx1),
                        fmt_num(*dy1),
                        fmt_num(*dx2),
                        fmt_num(*dy2),
                        fmt_num(*dx),
                        fmt_num(*dy)
                    ));
                }
                PathCommand::SmoothCurveTo { x2, y2, x, y } => {
                    result.push_str(&format!(
                        "S{},{} {},{}",
                        fmt_num(*x2),
                        fmt_num(*y2),
                        fmt_num(*x),
                        fmt_num(*y)
                    ));
                }
                PathCommand::SmoothCurveToRel { dx2, dy2, dx, dy } => {
                    result.push_str(&format!(
                        "s{},{} {},{}",
                        fmt_num(*dx2),
                        fmt_num(*dy2),
                        fmt_num(*dx),
                        fmt_num(*dy)
                    ));
                }
                PathCommand::QuadTo { x1, y1, x, y } => {
                    result.push_str(&format!(
                        "Q{},{} {},{}",
                        fmt_num(*x1),
                        fmt_num(*y1),
                        fmt_num(*x),
                        fmt_num(*y)
                    ));
                }
                PathCommand::QuadToRel { dx1, dy1, dx, dy } => {
                    result.push_str(&format!(
                        "q{},{} {},{}",
                        fmt_num(*dx1),
                        fmt_num(*dy1),
                        fmt_num(*dx),
                        fmt_num(*dy)
                    ));
                }
                PathCommand::SmoothQuadTo { x, y } => {
                    result.push_str(&format!("T{},{}", fmt_num(*x), fmt_num(*y)));
                }
                PathCommand::SmoothQuadToRel { dx, dy } => {
                    result.push_str(&format!("t{},{}", fmt_num(*dx), fmt_num(*dy)));
                }
                PathCommand::Arc {
                    rx,
                    ry,
                    x_rotation,
                    large_arc,
                    sweep,
                    x,
                    y,
                } => {
                    result.push_str(&format!(
                        "A{},{} {} {} {} {},{}",
                        fmt_num(*rx),
                        fmt_num(*ry),
                        fmt_num(*x_rotation),
                        if *large_arc { 1 } else { 0 },
                        if *sweep { 1 } else { 0 },
                        fmt_num(*x),
                        fmt_num(*y)
                    ));
                }
                PathCommand::ArcRel {
                    rx,
                    ry,
                    x_rotation,
                    large_arc,
                    sweep,
                    dx,
                    dy,
                } => {
                    result.push_str(&format!(
                        "a{},{} {} {} {} {},{}",
                        fmt_num(*rx),
                        fmt_num(*ry),
                        fmt_num(*x_rotation),
                        if *large_arc { 1 } else { 0 },
                        if *sweep { 1 } else { 0 },
                        fmt_num(*dx),
                        fmt_num(*dy)
                    ));
                }
                PathCommand::ClosePath => {
                    result.push('Z');
                }
            }
        }
        result
    }
}

/// Format a number with up to 3 decimal places (sufficient for SVG coordinates).
/// Trims trailing zeros and decimal point.
fn fmt_num(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    s.to_string()
}

/// Error parsing path data
#[derive(Debug, Clone, PartialEq)]
pub enum PathParseError {
    UnknownCommand(char),
    ExpectedNumber,
    ExpectedFlag,
    InvalidNumber(String),
}

impl std::fmt::Display for PathParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathParseError::UnknownCommand(c) => write!(f, "unknown path command: {}", c),
            PathParseError::ExpectedNumber => write!(f, "expected number"),
            PathParseError::ExpectedFlag => write!(f, "expected flag (0 or 1)"),
            PathParseError::InvalidNumber(s) => write!(f, "invalid number: {}", s),
        }
    }
}

impl std::error::Error for PathParseError {}

impl std::fmt::Display for PathData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.serialize())
    }
}

/// Proxy type for PathData - serializes as a string
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct PathDataProxy(pub String);

impl TryFrom<PathDataProxy> for PathData {
    type Error = PathParseError;
    fn try_from(proxy: PathDataProxy) -> Result<Self, Self::Error> {
        PathData::parse(&proxy.0)
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<&PathData> for PathDataProxy {
    type Error = std::convert::Infallible;
    fn try_from(v: &PathData) -> Result<Self, Self::Error> {
        Ok(PathDataProxy(v.to_string()))
    }
}

// Option impls for facet proxy support
impl From<PathDataProxy> for Option<PathData> {
    fn from(proxy: PathDataProxy) -> Self {
        PathData::parse(&proxy.0).ok()
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<&Option<PathData>> for PathDataProxy {
    type Error = std::convert::Infallible;
    fn try_from(v: &Option<PathData>) -> Result<Self, Self::Error> {
        match v {
            Some(data) => Ok(PathDataProxy(data.to_string())),
            None => Ok(PathDataProxy(String::new())),
        }
    }
}

// Helper parsing functions

fn skip_wsp_comma(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || c == ',' {
            chars.next();
        } else {
            break;
        }
    }
}

fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<f64, PathParseError> {
    skip_wsp_comma(chars);
    let mut num_str = String::new();

    // Handle optional sign
    if let Some(&c) = chars.peek()
        && (c == '-' || c == '+')
    {
        num_str.push(chars.next().unwrap());
    }

    // Parse digits and decimal point
    let mut has_digits = false;
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' {
            num_str.push(chars.next().unwrap());
            has_digits = true;
        } else if c == 'e' || c == 'E' {
            // Scientific notation
            num_str.push(chars.next().unwrap());
            if let Some(&sign) = chars.peek()
                && (sign == '-' || sign == '+')
            {
                num_str.push(chars.next().unwrap());
            }
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    num_str.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            break;
        } else {
            break;
        }
    }

    if !has_digits {
        return Err(PathParseError::ExpectedNumber);
    }

    num_str
        .parse()
        .map_err(|_| PathParseError::InvalidNumber(num_str))
}

fn try_parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<f64> {
    skip_wsp_comma(chars);
    if let Some(&c) = chars.peek()
        && (c.is_ascii_digit() || c == '-' || c == '+' || c == '.')
    {
        return parse_number(chars).ok();
    }
    None
}

fn parse_coord_pair(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> Result<(f64, f64), PathParseError> {
    let x = parse_number(chars)?;
    let y = parse_number(chars)?;
    Ok((x, y))
}

fn try_parse_coord_pair(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<(f64, f64)> {
    skip_wsp_comma(chars);
    if let Some(&c) = chars.peek()
        && (c.is_ascii_digit() || c == '-' || c == '+' || c == '.')
        && let Ok(pair) = parse_coord_pair(chars)
    {
        return Some(pair);
    }
    None
}

fn parse_flag(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<bool, PathParseError> {
    skip_wsp_comma(chars);
    match chars.next() {
        Some('0') => Ok(false),
        Some('1') => Ok(true),
        _ => Err(PathParseError::ExpectedFlag),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_path() {
        let path = PathData::parse("M10,20L30,40Z").unwrap();
        assert_eq!(path.commands.len(), 3);
        assert_eq!(path.commands[0], PathCommand::MoveTo { x: 10.0, y: 20.0 });
        assert_eq!(path.commands[1], PathCommand::LineTo { x: 30.0, y: 40.0 });
        assert_eq!(path.commands[2], PathCommand::ClosePath);
    }

    #[test]
    fn test_parse_box_path() {
        // C pikchr box format
        let path =
            PathData::parse("M118.239,208.239L226.239,208.239L226.239,136.239L118.239,136.239Z")
                .unwrap();
        assert_eq!(path.commands.len(), 5);
    }

    #[test]
    fn test_roundtrip() {
        let original = "M10,20L30,40Z";
        let path = PathData::parse(original).unwrap();
        let serialized = path.to_string();
        let reparsed = PathData::parse(&serialized).unwrap();
        assert_eq!(path.commands, reparsed.commands);
    }

    #[test]
    fn test_arc() {
        let path = PathData::parse("A10,10 0 0,1 20,20").unwrap();
        assert_eq!(path.commands.len(), 1);
        match &path.commands[0] {
            PathCommand::Arc {
                rx,
                ry,
                x_rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                assert_eq!(*rx, 10.0);
                assert_eq!(*ry, 10.0);
                assert_eq!(*x_rotation, 0.0);
                assert!(!*large_arc);
                assert!(*sweep);
                assert_eq!(*x, 20.0);
                assert_eq!(*y, 20.0);
            }
            _ => panic!("expected Arc command"),
        }
    }

    #[test]
    fn test_float_tolerance_in_diff() {
        use facet_assert::{SameOptions, SameReport, check_same_with_report};

        // Simulate C vs Rust precision difference
        // C: "118.239" parses to this f64
        // Rust: "118.2387401575" parses to this f64
        let c_path = PathData::parse("M118.239,208.239L226.239,208.239Z").unwrap();
        let rust_path =
            PathData::parse("M118.2387401575,208.2387401575L226.2387401575,208.2387401575Z")
                .unwrap();

        // The difference is about 0.0003, well under 0.002 tolerance
        let tolerance = 0.002;
        let options = SameOptions::new().float_tolerance(tolerance);

        eprintln!("C path: {:?}", c_path);
        eprintln!("Rust path: {:?}", rust_path);

        // Check if they're the same within tolerance
        let result = check_same_with_report(&c_path, &rust_path, options);

        match &result {
            SameReport::Same => eprintln!("Result: Same"),
            SameReport::Different(report) => {
                eprintln!("Result: Different");
                eprintln!("XML diff:\n{}", report.render_ansi_xml());
            }
            SameReport::Opaque { type_name } => eprintln!("Result: Opaque({})", type_name),
        }

        assert!(
            matches!(result, SameReport::Same),
            "PathData values within float tolerance should be considered Same"
        );
    }
}
