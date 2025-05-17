use alloc::{borrow::Cow, format};
use core::str;

use facet_core::Facet;
use facet_deserialize::{
    DeserError, DeserErrorKind, Expectation, Format, NextData, NextResult, Outcome, Scalar, Span,
    Spannable, Spanned,
};
use log::{debug, trace};

use crate::tokenizer::{Token, TokenError, TokenErrorKind, Tokenizer};

/// Deserialize JSON from a given byte slice using recursive descent parser
/// Falls back to iterative parser when recursion_depth exceeds MAX_RECURSION_DEPTH
pub(crate) fn from_slice<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input [u8],
    recursion_depth: usize,
) -> Result<T, DeserError<'input>> {
    // If starting recursion depth is already at or above the threshold, delegate to iterative implementation
    if recursion_depth >= crate::MAX_RECURSION_DEPTH {
        // Fall back to iterative implementation when recursion depth exceeds threshold
        trace!(
            "Initial recursion depth {} exceeds threshold, using iterative parser",
            recursion_depth
        );
        return crate::iterative::from_slice(input);
    }

    facet_deserialize::deserialize(
        input,
        RecursiveJson {
            recursion_depth,
            current_depth: 0,
        },
    )
}

/// Deserialize JSON from a given string
pub(crate) fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input str,
    recursion_depth: usize,
) -> Result<T, DeserError<'input>> {
    let input = input.as_bytes();
    if recursion_depth >= crate::MAX_RECURSION_DEPTH {
        trace!(
            "Initial recursion depth {} exceeds threshold, using iterative parser",
            recursion_depth
        );
        return crate::iterative::from_slice(input);
    }

    facet_deserialize::deserialize(
        input,
        RecursiveJson {
            recursion_depth,
            current_depth: 0,
        },
    )
}

/// Deserialize JSON from a given string, converting any dynamic error into a static one.
///
/// This function attempts to deserialize a type `T` implementing `Facet` from the input string slice.
/// If deserialization fails, the error is converted into an owned, static error type to avoid lifetime issues.
pub(crate) fn from_str_static_error<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input str,
    recursion_depth: usize,
) -> Result<T, DeserError<'input>> {
    let input = input.as_bytes();
    facet_deserialize::deserialize(
        input,
        RecursiveJson {
            recursion_depth,
            current_depth: 0,
        },
    )
    .map_err(|e| e.into_owned())
}

/// Recursive JSON format implementation
struct RecursiveJson {
    recursion_depth: usize,
    current_depth: usize,
}

impl Format for RecursiveJson {
    type Input<'input> = [u8];

    fn source(&self) -> &'static str {
        "json"
    }

    fn next<'input, 'facet>(
        &mut self,
        nd: NextData<'input, 'facet>,
        mut expectation: Expectation,
    ) -> NextResult<'input, 'facet, Spanned<Outcome<'input>>, Spanned<DeserErrorKind>> {
        // If we've gone too deep in recursion, delegate to the iterative implementation
        if self.current_depth + self.recursion_depth >= crate::MAX_RECURSION_DEPTH {
            debug!(
                "Recursion depth exceeded in next(): depth={}, threshold={}, delegating to iterative parser",
                self.current_depth + self.recursion_depth,
                crate::MAX_RECURSION_DEPTH
            );
            return crate::Json.next(nd, expectation);
        }

        let input = &nd.input()[nd.start()..];
        let mut tokenizer = Tokenizer::new(input);

        loop {
            let token = match tokenizer.next_token() {
                Ok(token) => token,
                Err(err) => {
                    trace!("Tokenizer error in next: {:?}", err.kind);
                    return (nd, Err(convert_token_error(err)));
                }
            };

            // Adjust token span to be relative to the beginning of the overall input
            let token_offset = nd.start();
            let span = Span::new(token.span.start() + token_offset, token.span.len());

            let res = match token.node {
                Token::String(s) => Ok(Spanned {
                    node: Outcome::Scalar(Scalar::String(Cow::Owned(s))),
                    span,
                }),
                Token::F64(n) => Ok(Spanned {
                    node: Outcome::Scalar(Scalar::F64(n)),
                    span,
                }),
                Token::I64(n) => Ok(Spanned {
                    node: Outcome::Scalar(Scalar::I64(n)),
                    span,
                }),
                Token::U64(n) => Ok(Spanned {
                    node: Outcome::Scalar(Scalar::U64(n)),
                    span,
                }),
                Token::True => Ok(Spanned {
                    node: Outcome::Scalar(Scalar::Bool(true)),
                    span,
                }),
                Token::False => Ok(Spanned {
                    node: Outcome::Scalar(Scalar::Bool(false)),
                    span,
                }),
                Token::Null => Ok(Spanned {
                    node: Outcome::Scalar(Scalar::Null),
                    span,
                }),
                Token::LBrace => {
                    self.current_depth += 1;
                    trace!(
                        "Object started, recursion depth now: {}",
                        self.current_depth
                    );
                    Ok(Spanned {
                        node: Outcome::ObjectStarted,
                        span,
                    })
                }
                Token::RBrace => {
                    if expectation == Expectation::ObjectKeyOrObjectClose {
                        self.current_depth = self.current_depth.saturating_sub(1);
                        trace!("Object ended, recursion depth now: {}", self.current_depth);
                        Ok(Spanned {
                            node: Outcome::ObjectEnded,
                            span,
                        })
                    } else {
                        trace!("Did not expect closing brace, expected {:?}", expectation);
                        Err(DeserErrorKind::UnexpectedChar {
                            got: '}',
                            wanted: "a value",
                        }
                        .with_span(span))
                    }
                }
                Token::LBracket => {
                    self.current_depth += 1;
                    trace!("List started, recursion depth now: {}", self.current_depth);
                    Ok(Spanned {
                        node: Outcome::ListStarted,
                        span,
                    })
                }
                Token::RBracket => {
                    if expectation == Expectation::ListItemOrListClose {
                        self.current_depth = self.current_depth.saturating_sub(1);
                        trace!("List ended, recursion depth now: {}", self.current_depth);
                        Ok(Spanned {
                            node: Outcome::ListEnded,
                            span,
                        })
                    } else {
                        Err(DeserErrorKind::UnexpectedChar {
                            got: ']',
                            wanted: "a value",
                        }
                        .with_span(span))
                    }
                }
                Token::Colon => {
                    if expectation == Expectation::ObjectVal {
                        expectation = Expectation::Value;
                        continue;
                    } else {
                        trace!("Did not expect ObjectValue, expected {:?}", expectation);
                        Err(DeserErrorKind::UnexpectedChar {
                            got: ':',
                            wanted: "a value, not a colon",
                        }
                        .with_span(span))
                    }
                }
                Token::Comma => match expectation {
                    Expectation::ListItemOrListClose | Expectation::ObjectKeyOrObjectClose => {
                        expectation = Expectation::Value;
                        continue;
                    }
                    other => {
                        trace!("Did not expect comma, expected {:?}", other);
                        Err(DeserErrorKind::UnexpectedChar {
                            got: ',',
                            wanted: "<value or key>",
                        }
                        .with_span(span))
                    }
                },
                Token::Eof => {
                    return (
                        nd,
                        Err(DeserErrorKind::UnexpectedEof {
                            wanted: "any value (got EOF)",
                        }
                        .with_span(span)),
                    );
                }
            };

            return (nd, res);
        }
    }

    fn skip<'input, 'facet>(
        &mut self,
        nd: NextData<'input, 'facet>,
    ) -> NextResult<'input, 'facet, Span, Spanned<DeserErrorKind>> {
        // Increment current depth when skipping
        self.current_depth += 1;
        trace!("Skip increased recursion depth to: {}", self.current_depth);

        // If we've gone too deep in recursion, delegate to the iterative implementation
        if self.current_depth + self.recursion_depth >= crate::MAX_RECURSION_DEPTH {
            debug!(
                "Recursion depth exceeded in skip(): depth={}, threshold={}, delegating to iterative parser",
                self.current_depth + self.recursion_depth,
                crate::MAX_RECURSION_DEPTH
            );
            self.current_depth -= 1; // Restore depth before delegating
            return crate::Json.skip(nd);
        }

        trace!("Starting recursive skip at offset {}", nd.start());
        let input = &nd.input()[nd.start()..];
        let mut tokenizer = Tokenizer::new(input);

        loop {
            let token = match tokenizer.next_token() {
                Ok(token) => token,
                Err(err) => {
                    trace!("Tokenizer error on initial token: {:?}", err.kind);
                    return (nd, Err(convert_token_error(err)));
                }
            };

            let res = match token.node {
                Token::LBrace | Token::LBracket => {
                    let mut depth = 1;
                    let mut last_span = token.span;
                    while depth > 0 {
                        let token = match tokenizer.next_token() {
                            Ok(token) => token,
                            Err(err) => {
                                trace!("Tokenizer error while skipping container: {:?}", err.kind);
                                return (nd, Err(convert_token_error(err)));
                            }
                        };

                        match token.node {
                            Token::LBrace | Token::LBracket => {
                                depth += 1;
                                last_span = token.span;
                            }
                            Token::RBrace | Token::RBracket => {
                                depth -= 1;
                                last_span = token.span;
                            }
                            _ => {
                                last_span = token.span;
                            }
                        }
                    }
                    (nd, Ok(last_span))
                }
                Token::String(_)
                | Token::F64(_)
                | Token::I64(_)
                | Token::U64(_)
                | Token::True
                | Token::False
                | Token::Null => (nd, Ok(token.span)),
                Token::Colon => {
                    // Skip colon token
                    continue;
                }
                other => (
                    nd,
                    Err(DeserErrorKind::UnexpectedChar {
                        got: format!("{:?}", other).chars().next().unwrap_or('?'),
                        wanted: "value",
                    }
                    .with_span(Span::new(token.span.start(), token.span.len()))),
                ),
            };
            let (nd, mut span) = res;
            if let Ok(valid_span) = &mut span {
                let offset = nd.start();
                valid_span.start += offset;
            }

            // Decrement current depth counter when done
            self.current_depth = self.current_depth.saturating_sub(1);
            trace!("Exiting skip, recursion depth now: {}", self.current_depth);

            return (nd, span);
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
