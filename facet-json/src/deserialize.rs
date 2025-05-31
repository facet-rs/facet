use alloc::format;

use facet_core::Facet;
use facet_deserialize::{
    Cooked, Expectation, Format, NextData, NextResult, Outcome, Scalar, Span, Spannable, Spanned,
};
pub use facet_deserialize::{DeserError, DeserErrorKind};
use log::trace;

use crate::tokenizer::{Token, TokenError, TokenErrorKind, Tokenizer};

/// Deserialize JSON from a given byte slice
pub fn from_slice<'input, 'facet, 'shape, T: Facet<'facet>>(
    input: &'input [u8],
) -> Result<T, DeserError<'input, 'shape>>
where
    'input: 'facet,
{
    facet_deserialize::deserialize(input, crate::Json)
}

/// Deserialize JSON from a UTF-8 string slice
pub fn from_str<'input, 'facet, 'shape, T: Facet<'facet>>(
    input: &'input str,
) -> Result<T, DeserError<'input, 'shape>>
where
    'input: 'facet,
{
    from_slice(input.as_bytes())
}

impl Format for crate::Json {
    type Input<'input> = [u8];
    type SpanType = Cooked;

    fn source(&self) -> &'static str {
        "json"
    }

    fn next<'input, 'facet, 'shape>(
        &mut self,
        nd: NextData<'input, 'facet, 'shape>,
        mut expectation: Expectation,
    ) -> NextResult<
        'input,
        'facet,
        'shape,
        Spanned<Outcome<'input>, Self::SpanType>,
        Spanned<DeserErrorKind<'shape>, Self::SpanType>,
        Self::SpanType,
        Self::Input<'input>,
    >
    where
        'shape: 'input,
    {
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
                    node: Outcome::Scalar(Scalar::String(s)),
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
                Token::LBrace => Ok(Spanned {
                    node: Outcome::ObjectStarted,
                    span,
                }),
                Token::RBrace => {
                    if expectation == Expectation::ObjectKeyOrObjectClose {
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
                Token::LBracket => Ok(Spanned {
                    node: Outcome::ListStarted,
                    span,
                }),
                Token::RBracket => {
                    if expectation == Expectation::ListItemOrListClose {
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

    fn skip<'input, 'facet, 'shape>(
        &mut self,
        nd: NextData<'input, 'facet, 'shape>,
    ) -> NextResult<
        'input,
        'facet,
        'shape,
        Span,
        Spanned<DeserErrorKind<'shape>>,
        Self::SpanType,
        Self::Input<'input>,
    >
    where
        'shape: 'input,
    {
        trace!("Starting skip at offset {}", nd.start());
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
            return (nd, span);
        }
    }
}

fn convert_token_error(err: TokenError) -> Spanned<DeserErrorKind<'static>> {
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
