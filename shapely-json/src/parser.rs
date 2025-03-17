//! The shapely-json parser.
//!
//! For now it is extremely naive, it's just a proof of concept, it doesn't use SIMD or anything,
//! it's not fast, it's nothing, it's just proving that we can use shapely types to deserialize something.

#[derive(Debug)]
pub struct JsonParseError {
    pub kind: JsonParseErrorKind,
    pub position: usize,
}

#[derive(Debug)]
pub enum JsonParseErrorKind {
    ExpectedOpeningQuote,
    UnterminatedString,
    InvalidEscapeSequence(char),
    IncompleteUnicodeEscape,
    InvalidUnicodeEscape,
    ExpectedNumber,
    ExpectedOpeningBrace,
    ExpectedColon,
    UnexpectedEndOfInput,
    InvalidValue,
    ExpectedClosingBrace,
    UnknownField(String),
    Custom(String),
}

impl JsonParseError {
    pub fn new(kind: JsonParseErrorKind, position: usize) -> Self {
        JsonParseError { kind, position }
    }
}

#[derive(Debug)]
pub struct JsonParseErrorWithContext<'input> {
    pub error: JsonParseError,
    pub input: &'input str,
}

impl JsonParseErrorWithContext<'_> {
    pub fn strip_context(self) -> JsonParseError {
        self.error
    }
}

impl std::fmt::Display for JsonParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let error_message = match &self.kind {
            JsonParseErrorKind::ExpectedOpeningQuote => "Expected opening quote for string",
            JsonParseErrorKind::UnterminatedString => "Unterminated string",
            JsonParseErrorKind::InvalidEscapeSequence(ch) => {
                return write!(f, "Invalid escape sequence: \\{}", ch);
            }
            JsonParseErrorKind::IncompleteUnicodeEscape => "Incomplete Unicode escape sequence",
            JsonParseErrorKind::InvalidUnicodeEscape => "Invalid Unicode escape sequence",
            JsonParseErrorKind::ExpectedNumber => "Expected a number",
            JsonParseErrorKind::ExpectedOpeningBrace => "Expected opening brace for object",
            JsonParseErrorKind::ExpectedColon => "Expected ':' after object key",
            JsonParseErrorKind::UnexpectedEndOfInput => "Unexpected end of input",
            JsonParseErrorKind::InvalidValue => "Invalid value",
            JsonParseErrorKind::ExpectedClosingBrace => "Expected closing brace for object",
            JsonParseErrorKind::UnknownField(field) => {
                return write!(f, "Unknown field: {}", field);
            }
            JsonParseErrorKind::Custom(msg) => msg,
        };

        write!(f, "{} at position {}", error_message, self.position)
    }
}

impl std::fmt::Display for JsonParseErrorWithContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let context_start = self.error.position.saturating_sub(20);
        let context_end = (self.error.position + 20).min(self.input.len());
        let context = &self.input[context_start..context_end];
        let arrow_position = self.error.position - context_start;

        writeln!(f, "{}", self.error)?;
        writeln!(f, "\x1b[36m{}\x1b[0m", context)?;
        write!(f, "{}\x1b[31m^\x1b[0m", " ".repeat(arrow_position))
    }
}

impl std::error::Error for JsonParseError {}

pub struct JsonParser<'input> {
    pub input: &'input str,
    pub position: usize,
}

impl<'a> JsonParser<'a> {
    pub fn new(input: &'a str) -> Self {
        JsonParser { input, position: 0 }
    }

    pub fn make_error(&self, kind: JsonParseErrorKind) -> JsonParseErrorWithContext<'a> {
        JsonParseErrorWithContext {
            error: JsonParseError::new(kind, self.position),
            input: self.input,
        }
    }

    pub fn parse_string(&mut self) -> Result<String, JsonParseErrorWithContext<'a>> {
        self.skip_whitespace();
        if self.position >= self.input.len() || self.input.as_bytes()[self.position] != b'"' {
            return Err(self.make_error(JsonParseErrorKind::ExpectedOpeningQuote));
        }
        self.position += 1;

        let mut result = String::new();
        let mut escaped = false;

        while self.position < self.input.len() {
            let ch = self.input.as_bytes()[self.position];
            self.position += 1;

            if escaped {
                match ch {
                    b'"' | b'\\' | b'/' => result.push(ch as char),
                    b'b' => result.push('\x08'),
                    b'f' => result.push('\x0C'),
                    b'n' => result.push('\n'),
                    b'r' => result.push('\r'),
                    b't' => result.push('\t'),
                    b'u' => {
                        // Parse 4-digit hex code
                        if self.position + 4 > self.input.len() {
                            return Err(
                                self.make_error(JsonParseErrorKind::IncompleteUnicodeEscape)
                            );
                        }
                        let hex = &self.input[self.position..self.position + 4];
                        self.position += 4;
                        if let Ok(code) = u16::from_str_radix(hex, 16) {
                            result.push(char::from_u32(code as u32).unwrap_or('\u{FFFD}'));
                        } else {
                            return Err(self.make_error(JsonParseErrorKind::InvalidUnicodeEscape));
                        }
                    }
                    _ => {
                        return Err(
                            self.make_error(JsonParseErrorKind::InvalidEscapeSequence(ch as char))
                        );
                    }
                }
                escaped = false;
            } else if ch == b'\\' {
                escaped = true;
            } else if ch == b'"' {
                return Ok(result);
            } else {
                result.push(ch as char);
            }
        }

        Err(self.make_error(JsonParseErrorKind::UnterminatedString))
    }

    pub fn parse_u64(&mut self) -> Result<u64, JsonParseErrorWithContext<'a>> {
        self.skip_whitespace();
        let start = self.position;
        while self.position < self.input.len()
            && self.input.as_bytes()[self.position].is_ascii_digit()
        {
            self.position += 1;
        }
        if start == self.position {
            return Err(self.make_error(JsonParseErrorKind::ExpectedNumber));
        }
        let num_str = &self.input[start..self.position];
        num_str
            .parse::<u64>()
            .map_err(|_| self.make_error(JsonParseErrorKind::ExpectedNumber))
    }

    pub fn skip_whitespace(&mut self) {
        while self.position < self.input.len() {
            match self.input.as_bytes()[self.position] {
                b' ' | b'\t' | b'\n' | b'\r' => self.position += 1,
                _ => break,
            }
        }
    }

    /// Expects the start of an object and returns the first key if present.
    /// Returns None if the object is empty.
    pub fn expect_object_start(&mut self) -> Result<Option<String>, JsonParseErrorWithContext<'a>> {
        self.skip_whitespace();
        if self.position >= self.input.len() || self.input.as_bytes()[self.position] != b'{' {
            return Err(self.make_error(JsonParseErrorKind::ExpectedOpeningBrace));
        }
        self.position += 1;
        self.skip_whitespace();

        if self.position < self.input.len() && self.input.as_bytes()[self.position] == b'"' {
            let key = self.parse_string()?;
            self.skip_whitespace();
            if self.position < self.input.len() && self.input.as_bytes()[self.position] == b':' {
                self.position += 1;
                Ok(Some(key))
            } else {
                Err(self.make_error(JsonParseErrorKind::ExpectedColon))
            }
        } else if self.position < self.input.len() && self.input.as_bytes()[self.position] == b'}' {
            self.position += 1;
            Ok(None)
        } else {
            Err(self.make_error(JsonParseErrorKind::InvalidValue))
        }
    }

    /// Expects the end of an object or a comma followed by the next key.
    /// Returns None if the object has ended, or Some(key) if there's another key-value pair.
    ///
    /// This function is used to parse the end of an object or to move to the next key-value pair.
    /// It handles three cases:
    /// 1. If it encounters a comma, it expects the next key-value pair and returns Some(key).
    /// 2. If it encounters a closing brace, it returns None to indicate the end of the object.
    /// 3. If it encounters anything else, it returns an error.
    ///
    /// The function also takes care of skipping whitespace before and after tokens.
    /// If it reaches the end of input unexpectedly, it returns an appropriate error.
    pub fn parse_object_key(&mut self) -> Result<Option<String>, JsonParseErrorWithContext<'a>> {
        self.skip_whitespace();
        if self.position >= self.input.len() {
            return Err(self.make_error(JsonParseErrorKind::UnexpectedEndOfInput));
        }
        match self.input.as_bytes()[self.position] {
            b',' => {
                self.position += 1;
                self.skip_whitespace();
                if self.position < self.input.len() && self.input.as_bytes()[self.position] == b'"'
                {
                    let key = self.parse_string()?;
                    self.skip_whitespace();
                    if self.position < self.input.len()
                        && self.input.as_bytes()[self.position] == b':'
                    {
                        self.position += 1;
                        Ok(Some(key))
                    } else {
                        Err(self.make_error(JsonParseErrorKind::ExpectedColon))
                    }
                } else {
                    Err(self.make_error(JsonParseErrorKind::InvalidValue))
                }
            }
            b'}' => {
                self.position += 1;
                Ok(None)
            }
            _ => Err(self.make_error(JsonParseErrorKind::InvalidValue)),
        }
    }
}
