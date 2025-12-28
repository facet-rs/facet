//! SVG points attribute parsing and structured representation.
//!
//! Used for polygon and polyline elements.

use facet::Facet;

/// A single point in SVG coordinates
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Structured SVG points list (for polygon/polyline)
#[derive(Debug, Clone, PartialEq, Default, Facet)]
#[facet(traits(Default, Display))]
pub struct Points {
    pub points: Vec<Point>,
}

impl Points {
    pub fn new() -> Self {
        Self { points: Vec::new() }
    }

    /// Add a point
    pub fn push(mut self, x: f64, y: f64) -> Self {
        self.points.push(Point { x, y });
        self
    }

    /// Parse points from an SVG points attribute string
    pub fn parse(s: &str) -> Result<Self, PointsParseError> {
        let mut points = Vec::new();
        let s = s.trim();
        if s.is_empty() {
            return Ok(Points { points });
        }

        // Points are space or comma separated pairs like "x1,y1 x2,y2 x3,y3"
        // or "x1 y1 x2 y2 x3 y3" (all whitespace/comma separated)
        let mut chars = s.chars().peekable();

        loop {
            skip_wsp_comma(&mut chars);
            if chars.peek().is_none() {
                break;
            }

            let x = parse_number(&mut chars)?;
            skip_wsp_comma(&mut chars);
            let y = parse_number(&mut chars)?;
            points.push(Point { x, y });
        }

        Ok(Points { points })
    }

    /// Serialize points to an SVG points attribute string
    fn serialize(&self) -> String {
        self.points
            .iter()
            .map(|p| format!("{},{}", fmt_num(p.x), fmt_num(p.y)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
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

/// Error parsing points
#[derive(Debug, Clone, PartialEq)]
pub enum PointsParseError {
    ExpectedNumber,
    InvalidNumber(String),
}

impl std::fmt::Display for PointsParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PointsParseError::ExpectedNumber => write!(f, "expected number"),
            PointsParseError::InvalidNumber(s) => write!(f, "invalid number: {}", s),
        }
    }
}

impl std::error::Error for PointsParseError {}

impl std::fmt::Display for Points {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.serialize())
    }
}

/// Proxy type for Points - serializes as a string
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct PointsProxy(pub String);

impl TryFrom<PointsProxy> for Points {
    type Error = PointsParseError;
    fn try_from(proxy: PointsProxy) -> Result<Self, Self::Error> {
        Points::parse(&proxy.0)
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<&Points> for PointsProxy {
    type Error = std::convert::Infallible;
    fn try_from(v: &Points) -> Result<Self, Self::Error> {
        Ok(PointsProxy(v.to_string()))
    }
}

// Option impls for facet proxy support
impl From<PointsProxy> for Option<Points> {
    fn from(proxy: PointsProxy) -> Self {
        Points::parse(&proxy.0).ok()
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<&Option<Points>> for PointsProxy {
    type Error = std::convert::Infallible;
    fn try_from(v: &Option<Points>) -> Result<Self, Self::Error> {
        match v {
            Some(data) => Ok(PointsProxy(data.to_string())),
            None => Ok(PointsProxy(String::new())),
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

fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<f64, PointsParseError> {
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
        return Err(PointsParseError::ExpectedNumber);
    }

    num_str
        .parse()
        .map_err(|_| PointsParseError::InvalidNumber(num_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let points = Points::parse("10,20 30,40 50,60").unwrap();
        assert_eq!(points.points.len(), 3);
        assert_eq!(points.points[0], Point { x: 10.0, y: 20.0 });
        assert_eq!(points.points[1], Point { x: 30.0, y: 40.0 });
        assert_eq!(points.points[2], Point { x: 50.0, y: 60.0 });
    }

    #[test]
    fn test_parse_with_decimals() {
        let points = Points::parse("402.528,63.6158 397.436,74.8164").unwrap();
        assert_eq!(points.points.len(), 2);
        assert!((points.points[0].x - 402.528).abs() < 0.0001);
        assert!((points.points[0].y - 63.6158).abs() < 0.0001);
    }

    #[test]
    fn test_serialize() {
        let points = Points::new().push(10.0, 20.0).push(30.5, 40.0);
        assert_eq!(points.to_string(), "10,20 30.5,40");
    }

    #[test]
    fn test_roundtrip() {
        let original = "10,20 30.5,40 50,60.123";
        let points = Points::parse(original).unwrap();
        let serialized = points.to_string();
        let reparsed = Points::parse(&serialized).unwrap();
        assert_eq!(points, reparsed);
    }

    #[test]
    fn test_empty() {
        let points = Points::parse("").unwrap();
        assert!(points.is_empty());
        assert_eq!(points.to_string(), "");
    }
}
