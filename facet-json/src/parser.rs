//! The facet-json parser.
//!
//! For now it is extremely naive, it's just a proof of concept, it doesn't use SIMD or anything,
//! it's not fast, it's nothing, it's just proving that we can use facet types to deserialize something.

#![allow(dead_code)]

use std::str::FromStr;

#[derive(Debug)]
pub struct JsonParseError {
    pub kind: JsonParseErrorKind,
    pub position: usize,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum JsonParseErrorKind {
    ExpectedOpeningQuote,
    UnterminatedString,
    InvalidEscapeSequence(char),
    IncompleteUnicodeEscape,
    InvalidUnicodeEscape,
    ExpectedNumber,
    InvalidNumberFormat,
    ExpectedOpeningBrace,
    ExpectedOpeningBracket,
    ExpectedColon,
    UnexpectedEndOfInput,
    InvalidValue,
    ExpectedClosingBrace,
    ExpectedClosingBracket,
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

pub struct JsonParser<'input> {
    input: &'input str,
    position: usize,
}

impl<'input> JsonParser<'input> {
    pub fn new(input: &'input str) -> Self {
        JsonParser { input, position: 0 }
    }

    pub fn make_error(&self, kind: JsonParseErrorKind) -> JsonParseErrorWithContext<'input> {
        JsonParseErrorWithContext {
            error: JsonParseError::new(kind, self.position),
            input: self.input,
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }

    fn next_char(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.position += c.len_utf8();
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() {
                self.next_char();
            } else {
                break;
            }
        }
    }

    pub fn parse_string(&mut self) -> Result<String, JsonParseErrorWithContext<'input>> {
        self.skip_whitespace();

        if self.next_char() != Some('"') {
            return Err(self.make_error(JsonParseErrorKind::ExpectedOpeningQuote));
        }

        let mut result = String::new();
        loop {
            match self.next_char() {
                Some('"') => break,
                Some('\\') => match self.next_char() {
                    Some('"') => result.push('"'),
                    Some('\\') => result.push('\\'),
                    Some('/') => result.push('/'),
                    Some('b') => result.push('\x08'),
                    Some('f') => result.push('\x0C'),
                    Some('n') => result.push('\n'),
                    Some('r') => result.push('\r'),
                    Some('t') => result.push('\t'),
                    Some('u') => {
                        let mut code_point = 0;
                        for _ in 0..4 {
                            let c = self.next_char().ok_or_else(|| {
                                self.make_error(JsonParseErrorKind::IncompleteUnicodeEscape)
                            })?;
                            code_point = code_point * 16
                                + c.to_digit(16).ok_or_else(|| {
                                    self.make_error(JsonParseErrorKind::InvalidUnicodeEscape)
                                })?;
                        }
                        result.push(char::from_u32(code_point).ok_or_else(|| {
                            self.make_error(JsonParseErrorKind::InvalidUnicodeEscape)
                        })?);
                    }
                    Some(c) => {
                        return Err(self.make_error(JsonParseErrorKind::InvalidEscapeSequence(c)));
                    }
                    None => return Err(self.make_error(JsonParseErrorKind::UnexpectedEndOfInput)),
                },
                Some(c) => result.push(c),
                None => return Err(self.make_error(JsonParseErrorKind::UnterminatedString)),
            }
        }

        Ok(result)
    }

    pub fn parse_number<T: FromStr>(&mut self) -> Result<T, JsonParseErrorWithContext<'input>>
    where
        <T as FromStr>::Err: std::fmt::Debug,
    {
        self.skip_whitespace();

        let start_position = self.position;
        let mut end_position = start_position;

        // Check if it's a negative number
        if self.peek_char() == Some('-') {
            self.next_char();
            end_position = self.position;
        }

        // Parse digits before decimal point
        let mut has_digits = false;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                self.next_char();
                end_position = self.position;
                has_digits = true;
            } else {
                break;
            }
        }

        if !has_digits {
            return Err(self.make_error(JsonParseErrorKind::ExpectedNumber));
        }

        // Parse decimal point and digits after it
        if self.peek_char() == Some('.') {
            self.next_char();

            let mut has_decimal_digits = false;
            while let Some(c) = self.peek_char() {
                if c.is_ascii_digit() {
                    self.next_char();
                    end_position = self.position;
                    has_decimal_digits = true;
                } else {
                    break;
                }
            }

            if !has_decimal_digits {
                return Err(self.make_error(JsonParseErrorKind::InvalidNumberFormat));
            }
        }

        // Parse exponent
        if let Some(e) = self.peek_char() {
            if e == 'e' || e == 'E' {
                self.next_char();

                // Check for sign
                if let Some(sign) = self.peek_char() {
                    if sign == '+' || sign == '-' {
                        self.next_char();
                    }
                }

                let mut has_exponent_digits = false;
                while let Some(c) = self.peek_char() {
                    if c.is_ascii_digit() {
                        self.next_char();
                        end_position = self.position;
                        has_exponent_digits = true;
                    } else {
                        break;
                    }
                }

                if !has_exponent_digits {
                    return Err(self.make_error(JsonParseErrorKind::InvalidNumberFormat));
                }
            }
        }

        let number_str = &self.input[start_position..end_position];
        number_str
            .parse::<T>()
            .map_err(|_| self.make_error(JsonParseErrorKind::InvalidNumberFormat))
    }

    pub fn parse_i64(&mut self) -> Result<i64, JsonParseErrorWithContext<'input>> {
        self.parse_number::<i64>()
    }

    pub fn parse_u64(&mut self) -> Result<u64, JsonParseErrorWithContext<'input>> {
        self.parse_number::<u64>()
    }

    pub fn parse_f64(&mut self) -> Result<f64, JsonParseErrorWithContext<'input>> {
        self.parse_number::<f64>()
    }

    pub fn parse_bool(&mut self) -> Result<bool, JsonParseErrorWithContext<'input>> {
        self.skip_whitespace();

        match self.peek_char() {
            Some('t') => {
                if self.position + 4 <= self.input.len()
                    && &self.input[self.position..self.position + 4] == "true"
                {
                    self.position += 4;
                    Ok(true)
                } else {
                    Err(self.make_error(JsonParseErrorKind::InvalidValue))
                }
            }
            Some('f') => {
                if self.position + 5 <= self.input.len()
                    && &self.input[self.position..self.position + 5] == "false"
                {
                    self.position += 5;
                    Ok(false)
                } else {
                    Err(self.make_error(JsonParseErrorKind::InvalidValue))
                }
            }
            _ => Err(self.make_error(JsonParseErrorKind::InvalidValue)),
        }
    }

    pub fn expect_object_start(
        &mut self,
    ) -> Result<Option<String>, JsonParseErrorWithContext<'input>> {
        self.skip_whitespace();

        if self.next_char() != Some('{') {
            return Err(self.make_error(JsonParseErrorKind::ExpectedOpeningBrace));
        }

        self.skip_whitespace();

        // Check if it's an empty object
        if self.peek_char() == Some('}') {
            self.next_char();
            return Ok(None);
        }

        // Parse the first key
        let first_key = self.parse_string()?;

        self.skip_whitespace();
        if self.next_char() != Some(':') {
            return Err(self.make_error(JsonParseErrorKind::ExpectedColon));
        }

        Ok(Some(first_key))
    }

    pub fn parse_object_key(
        &mut self,
    ) -> Result<Option<String>, JsonParseErrorWithContext<'input>> {
        self.skip_whitespace();

        if self.peek_char() == Some('}') {
            self.next_char();
            return Ok(None);
        }

        if self.next_char() != Some(',') {
            return Err(self.make_error(JsonParseErrorKind::ExpectedClosingBrace));
        }

        self.skip_whitespace();
        let key = self.parse_string()?;

        self.skip_whitespace();
        if self.next_char() != Some(':') {
            return Err(self.make_error(JsonParseErrorKind::ExpectedColon));
        }

        Ok(Some(key))
    }

    pub fn expect_array_start(&mut self) -> Result<(), JsonParseErrorWithContext<'input>> {
        self.skip_whitespace();

        if self.next_char() != Some('[') {
            return Err(self.make_error(JsonParseErrorKind::ExpectedOpeningBracket));
        }

        Ok(())
    }

    /// Parse an array element
    ///
    /// Some(true) -> an element was found
    /// Some(false) -> the end of the array was reached
    /// None -> more elements are expected
    pub fn parse_array_element(
        &mut self,
    ) -> Result<Option<bool>, JsonParseErrorWithContext<'input>> {
        self.skip_whitespace();

        if self.peek_char() == Some(']') {
            self.next_char();
            return Ok(Some(false));
        }

        if self.position > 0 && self.input.as_bytes()[self.position - 1] == b'[' {
            // First element
            return Ok(Some(true));
        }

        if self.next_char() != Some(',') {
            return Err(self.make_error(JsonParseErrorKind::ExpectedClosingBracket));
        }

        Ok(Some(true))
    }
}
