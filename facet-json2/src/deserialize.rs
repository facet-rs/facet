extern crate alloc;

use alloc::borrow::Cow;

use facet_core::Facet;
use facet_deserialize_eventbased::{
    DeserError, DeserErrorKind, Expectation, Format, NextData, NextResult, Outcome, Scalar, Span,
    Spannable, Spanned,
};
use log::trace;

mod tokenizer;
use tokenizer::{Token, TokenError, TokenErrorKind, Tokenizer};

/// Deserialize JSON from a given byte slice
pub fn from_slice<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input [u8],
) -> Result<T, DeserError<'input>> {
    facet_deserialize_eventbased::deserialize::<T, Json>(input)
}

/// Deserialize JSON from a given string
pub fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input str,
) -> Result<T, DeserError<'input>> {
    let input = input.as_bytes();
    facet_deserialize_eventbased::deserialize::<T, Json>(input)
}

/// Deserialize JSON from a given string, converting any dynamic error into a static one.
///
/// This function attempts to deserialize a type `T` implementing `Facet` from the input string slice.
/// If deserialization fails, the error is converted into an owned, static error type to avoid lifetime issues.
pub fn from_str_static_error<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input str,
) -> Result<T, DeserError<'input>> {
    let input = input.as_bytes();
    facet_deserialize_eventbased::deserialize::<T, Json>(input).map_err(|e| e.into_owned())
}

/// The JSON format
pub struct Json;

impl Format for Json {
    fn next<'input, 'facet>(
        nd: NextData<'input, 'facet>,
        mut expectation: Expectation,
    ) -> NextResult<'input, 'facet, Spanned<Outcome<'input>>, Spanned<DeserErrorKind>> {
        let input = nd.input();
        let mut n = nd.start();

        'loopy: loop {
            // SKip whitespace
            while let Some(&ch) = input.get(n) {
                if ch.is_ascii_whitespace() {
                    n += 1;
                } else {
                    break;
                }
            }

            // Check if we've reached the end after skipping whitespace
            if input[n..].is_empty() {
                return (
                    nd,
                    Err(DeserErrorKind::UnexpectedEof {
                        wanted: "any value (got EOF after whitespace)",
                    }
                    .with_span(Span::new(n, 0))),
                );
            }

            // Track span start at the original offset before parsing value
            let token_start = n;

            // Update 'next' with the new first character
            let next = input[n];

            let res = 'body: {
                match next {
                    b'0'..=b'9' | b'-' | b'.' => {
                        trace!("Found number");
                        let mut has_decimal = next == b'.';
                        let is_negative = next == b'-';

                        n += 1; // Move past the first character

                        // Parse the rest of the number
                        while let Some(next) = nd.input().get(n) {
                            if *next >= b'0' && *next <= b'9' {
                                n += 1;
                            } else if *next == b'.' && !has_decimal {
                                has_decimal = true;
                                n += 1;
                            } else {
                                break;
                            }
                        }

                        let num_slice = &nd.input()[token_start..n];
                        match core::str::from_utf8(num_slice) {
                            Ok(num_str) => {
                                if has_decimal {
                                    // Parse as f64 for decimal numbers
                                    match num_str.parse::<f64>() {
                                        Ok(number) => Ok(Outcome::from(Scalar::F64(number))
                                            .with_span(Span::new(token_start, n - token_start))),
                                        Err(_) => Err(DeserErrorKind::NumberOutOfRange(f64::NAN)
                                            .with_span(Span::new(token_start, n - token_start))),
                                    }
                                } else if is_negative {
                                    // Parse as i64 for negative integers
                                    match num_str.parse::<i64>() {
                                        Ok(number) => Ok(Outcome::from(Scalar::I64(number))
                                            .with_span(Span::new(token_start, n - token_start))),
                                        Err(_) => Err(DeserErrorKind::NumberOutOfRange(
                                            num_str.parse::<f64>().unwrap_or(f64::NAN),
                                        )
                                        .with_span(Span::new(token_start, n - token_start))),
                                    }
                                } else {
                                    // Parse as u64 for positive integers
                                    match num_str.parse::<u64>() {
                                        Ok(number) => Ok(Outcome::from(Scalar::U64(number))
                                            .with_span(Span::new(token_start, n - token_start))),
                                        Err(_) => Err(DeserErrorKind::NumberOutOfRange(
                                            num_str.parse::<f64>().unwrap_or(f64::NAN),
                                        )
                                        .with_span(Span::new(token_start, n - token_start))),
                                    }
                                }
                            }
                            Err(e) => Err(DeserErrorKind::InvalidUtf8(e.to_string())
                                .with_span(Span::new(token_start, n - token_start))),
                        }
                    }
                    b'"' => {
                        trace!("Found string");
                        n += 1; // Skip opening quote
                        let start = n;
                        let len = input.len();
                        let mut escaped = false;
                        let mut i = n;

                        // Walk the input to find the closing quote
                        while i < len {
                            let b = input[i];
                            if escaped {
                                escaped = false;
                            } else if b == b'\\' {
                                escaped = true;
                            } else if b == b'"' {
                                break;
                            }
                            i += 1;
                        }

                        if i >= len || input[i] != b'"' {
                            // Unterminated string
                            break 'body Err(DeserErrorKind::UnexpectedEof {
                                wanted: "closing '\"' of string",
                            }
                            .with_span(Span::new(token_start, i)));
                        }
                        // Extract slice inside the string quotes, i points at closing quote
                        let string_slice = &input[start..i];

                        // Validate utf8, else emit proper error like error.rs does
                        let string_content = match core::str::from_utf8(string_slice) {
                            Ok(s) => s,
                            Err(e) => {
                                break 'body Err(DeserErrorKind::InvalidUtf8(e.to_string())
                                    .with_span(Span::new(token_start, i)));
                            }
                        };
                        trace!("String content: {:?}", string_content);

                        let span = facet_deserialize_eventbased::Spanned {
                            node: Outcome::Scalar(Scalar::String(Cow::Borrowed(string_content))),
                            span: Span::new(token_start, (i + 1) - token_start),
                        };
                        Ok(span)
                    }
                    b't' => {
                        // Try to match "true"
                        let input = nd.input();
                        if input.len() >= token_start + 4
                            && &input[token_start..token_start + 4] == b"true"
                        {
                            let span = facet_deserialize_eventbased::Spanned {
                                node: Outcome::Scalar(Scalar::Bool(true)),
                                span: Span::new(token_start, 4),
                            };
                            Ok(span)
                        } else {
                            Err(DeserErrorKind::UnexpectedChar {
                                got: 't',
                                wanted: "\"true\"",
                            }
                            .with_span(Span::new(token_start, 1)))
                        }
                    }
                    b'f' => {
                        // Try to match "false"
                        let input = nd.input();
                        if input.len() >= token_start + 5
                            && &input[token_start..token_start + 5] == b"false"
                        {
                            let span = facet_deserialize_eventbased::Spanned {
                                node: Outcome::Scalar(Scalar::Bool(false)),
                                span: Span::new(token_start, 5),
                            };
                            Ok(span)
                        } else {
                            Err(DeserErrorKind::UnexpectedChar {
                                got: 'f',
                                wanted: "\"false\"",
                            }
                            .with_span(Span::new(token_start, 1)))
                        }
                    }
                    b'n' => {
                        // Try to match "null"
                        let input = nd.input();
                        if input.len() >= token_start + 4
                            && &input[token_start..token_start + 4] == b"null"
                        {
                            let span = facet_deserialize_eventbased::Spanned {
                                node: Outcome::Scalar(Scalar::Null),
                                span: Span::new(token_start, 4),
                            };
                            Ok(span)
                        } else {
                            Err(DeserErrorKind::UnexpectedChar {
                                got: 'n',
                                wanted: "\"null\"",
                            }
                            .with_span(Span::new(token_start, 1)))
                        }
                    }
                    b':' => {
                        if expectation == Expectation::ObjectVal {
                            n += 1;
                            expectation = Expectation::Value;
                            continue 'loopy;
                        } else {
                            trace!("Did not expect ObjectValue, expected {:?}", expectation);
                            Err(DeserErrorKind::UnexpectedChar {
                                got: ':',
                                wanted: "a value, not a colon",
                            }
                            .with_span(Span::new(token_start, 1)))
                        }
                    }
                    b',' => match expectation {
                        Expectation::ListItemOrListClose | Expectation::ObjectKeyOrObjectClose => {
                            n += 1;
                            expectation = Expectation::Value;
                            continue 'loopy;
                        }
                        other => {
                            trace!("Did not expect comma, expected {:?}", other);
                            Err(DeserErrorKind::UnexpectedChar {
                                got: ',',
                                wanted: "<value or key>",
                            }
                            .with_span(Span::new(token_start, 1)))
                        }
                    },
                    b'{' => {
                        let span = facet_deserialize_eventbased::Spanned {
                            node: Outcome::ObjectStarted,
                            span: Span::new(token_start, 1),
                        };
                        Ok(span)
                    }
                    b'}' => {
                        if expectation == Expectation::ObjectKeyOrObjectClose {
                            let span = facet_deserialize_eventbased::Spanned {
                                node: Outcome::ObjectEnded,
                                span: Span::new(token_start, 1),
                            };
                            Ok(span)
                        } else {
                            trace!("Did not expect closing brace, expected {:?}", expectation);
                            Err(DeserErrorKind::UnexpectedChar {
                                got: '}',
                                wanted: "a value",
                            }
                            .with_span(Span::new(token_start, 1)))
                        }
                    }
                    b'[' => {
                        let span = facet_deserialize_eventbased::Spanned {
                            node: Outcome::ListStarted,
                            span: Span::new(token_start, 1),
                        };
                        Ok(span)
                    }
                    b']' => {
                        if expectation == Expectation::ListItemOrListClose {
                            let span = facet_deserialize_eventbased::Spanned {
                                node: Outcome::ListEnded,
                                span: Span::new(token_start, 1),
                            };
                            Ok(span)
                        } else {
                            Err(DeserErrorKind::UnexpectedChar {
                                got: ']',
                                wanted: "a value",
                            }
                            .with_span(Span::new(token_start, 1)))
                        }
                    }
                    c => Err(DeserErrorKind::UnexpectedChar {
                        got: c as char,
                        wanted: "value",
                    }
                    .with_span(Span::new(token_start, 1))),
                }
            };
            return (nd, res);
        }
    }

    fn skip<'input, 'facet>(
        nd: NextData<'input, 'facet>,
    ) -> NextResult<'input, 'facet, Span, Spanned<DeserErrorKind>> {
        trace!("Starting skip at offset {}", nd.start());
        let input = &nd.input()[nd.start()..];
        let mut tokenizer = Tokenizer::new(input);

        loop {
            let token = match tokenizer.next_token() {
                Ok(token) => {
                    trace!("Initial token for skip: {:?}", token.node);
                    token
                }
                Err(err) => {
                    trace!("Tokenizer error on initial token: {:?}", err.kind);
                    return (nd, Err(convert_token_error(err)));
                }
            };

            let res = match token.node {
                Token::LBrace | Token::LBracket => {
                    trace!(
                        "Skip: found container start ({:?}), entering depth parse",
                        token.node
                    );
                    let mut depth = 1;
                    let mut last_span = token.span;
                    while depth > 0 {
                        let token = match tokenizer.next_token() {
                            Ok(token) => {
                                trace!(
                                    "Skip: depth {}, next token in container: {:?}",
                                    depth, token.node
                                );
                                token
                            }
                            Err(err) => {
                                trace!("Tokenizer error while skipping container: {:?}", err.kind);
                                return (nd, Err(convert_token_error(err)));
                            }
                        };

                        match token.node {
                            Token::LBrace | Token::LBracket => {
                                depth += 1;
                                last_span = token.span;
                                trace!("Container nested incremented, depth now {}", depth);
                            }
                            Token::RBrace | Token::RBracket => {
                                depth -= 1;
                                last_span = token.span;
                                trace!("Container closed, depth now {}", depth);
                            }
                            _ => {
                                last_span = token.span;
                                trace!("Skipping non-container token: {:?}", token.node);
                            }
                        }
                    }
                    trace!("Skip complete, span {:?}", last_span);
                    (nd, Ok(last_span))
                }
                Token::String(_)
                | Token::F64(_)
                | Token::I64(_)
                | Token::U64(_)
                | Token::True
                | Token::False
                | Token::Null => {
                    trace!("Skip found primitive: {:?}", token.node);
                    (nd, Ok(token.span))
                }
                Token::Colon => {
                    // Skip colon token
                    continue;
                }
                other => {
                    trace!(
                        "Skip encountered unexpected token kind: {:?} at span {:?}",
                        other, token.span
                    );
                    (
                        nd,
                        Err(DeserErrorKind::UnexpectedChar {
                            got: format!("{:?}", other).chars().next().unwrap_or('?'),
                            wanted: "value",
                        }
                        .with_span(Span::new(token.span.start(), token.span.len()))),
                    )
                }
            };
            let (nd, mut span) = res;
            if let Ok(valid_span) = &mut span {
                let offset = nd.start();
                valid_span.start += offset;
            }
            let res = (nd, span);
            trace!("Returning {:?}", res.1);
            return res;
        }
    }
}

fn convert_token_error(err: TokenError) -> Spanned<DeserErrorKind> {
    match err.kind {
        TokenErrorKind::UnexpectedCharacter(c) => DeserErrorKind::UnexpectedChar {
            got: c,
            wanted: "valid JSON character",
        }
        .with_span(err.span),
        TokenErrorKind::UnexpectedEof(why) => {
            DeserErrorKind::UnexpectedEof { wanted: why }.with_span(err.span)
        }
        TokenErrorKind::InvalidUtf8(s) => DeserErrorKind::InvalidUtf8(s).with_span(err.span),
        TokenErrorKind::NumberOutOfRange(number) => {
            DeserErrorKind::NumberOutOfRange(number).with_span(err.span)
        }
    }
}
