extern crate alloc;

use alloc::borrow::Cow;

use facet_core::Facet;
use facet_deserialize_eventbased::{
    DeserError, DeserErrorKind, Expectation, Format, NextData, NextResult, Outcome, Scalar,
};
use log::trace;

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
        mut nd: NextData<'input, 'facet>,
        mut expectation: Expectation,
    ) -> NextResult<'input, 'facet, Outcome<'input>, DeserError<'input>> {
        loop {
            // Skip whitespace
            let mut n = 0;
            while let Some(&ch) = nd.runner.input.get(n) {
                if ch.is_ascii_whitespace() {
                    n += 1;
                } else {
                    break;
                }
            }

            // Update input to skip whitespace
            nd.runner.input = &nd.runner.input[n..];

            // Check if we've reached the end after skipping whitespace
            if nd.runner.input.is_empty() {
                let err = DeserError::new(
                    DeserErrorKind::UnexpectedEof {
                        wanted: "any value (got EOF after whitespace)",
                    },
                    nd.runner.input,
                    nd.runner.last_span,
                );
                return (nd, Err(err));
            }

            // Update 'next' with the new first character
            let next = nd.runner.input[0];
            let mut n = 0;
            let res = match next {
                b'0'..=b'9' => {
                    trace!("Found number");
                    while let Some(next) = nd.runner.input.get(n) {
                        if *next >= b'0' && *next <= b'9' {
                            n += 1;
                        } else {
                            break;
                        }
                    }
                    let num_slice = &nd.runner.input[0..n];
                    let num_str = core::str::from_utf8(num_slice).unwrap();
                    let number = num_str.parse::<u64>().unwrap();
                    nd.runner.input = &nd.runner.input[n..];
                    Ok(Outcome::GotScalar(Scalar::U64(number)))
                }
                b'"' => {
                    trace!("Found string");
                    n += 1; // Skip the opening quote
                    let start = n;

                    // Parse until closing quote
                    let mut escaped = false;
                    while let Some(&next) = nd.runner.input.get(n) {
                        if escaped {
                            escaped = false;
                        } else if next == b'\\' {
                            escaped = true;
                        } else if next == b'"' {
                            break;
                        }
                        n += 1;
                    }

                    // Skip the closing quote if found
                    if nd.runner.input.get(n) == Some(&b'"') {
                        n += 1;
                    }

                    let string_slice = &nd.runner.input[start..n - 1];
                    let string_content = core::str::from_utf8(string_slice).unwrap();
                    trace!("String content: {:?}", string_content);

                    nd.runner.input = &nd.runner.input[n..];
                    Ok(Outcome::GotScalar(Scalar::String(Cow::Borrowed(
                        string_content,
                    ))))
                }
                b't' => {
                    // Try to match "true"
                    let candidate = nd.runner.input.get(0..4);
                    if candidate == Some(b"true".as_ref()) {
                        nd.runner.input = &nd.runner.input[4..];
                        Ok(Outcome::GotScalar(Scalar::Bool(true)))
                    } else {
                        Err(DeserError {
                            input: nd.runner.input.into(),
                            span: nd.runner.last_span,
                            kind: DeserErrorKind::UnexpectedChar {
                                got: 't',
                                wanted: "a value, thought it was gonna be true, ngl",
                            },
                        })
                    }
                }
                b'f' => {
                    // Try to match "false"
                    let candidate = nd.runner.input.get(0..5);
                    if candidate == Some(b"false".as_ref()) {
                        nd.runner.input = &nd.runner.input[5..];
                        Ok(Outcome::GotScalar(Scalar::Bool(false)))
                    } else {
                        Err(DeserError {
                            input: nd.runner.input.into(),
                            span: nd.runner.last_span,
                            kind: DeserErrorKind::UnexpectedChar {
                                got: 'f',
                                wanted: "a value, thought it was gonna be false, ngl",
                            },
                        })
                    }
                }
                b':' => {
                    if expectation == Expectation::ObjectVal {
                        // makes sense, let's skip it and try again
                        nd.runner.input = &nd.runner.input[1..];
                        expectation = Expectation::Value;

                        continue;
                    } else {
                        trace!("Did not expect ObjectValue, expected {:?}", expectation);

                        Err(DeserError {
                            input: nd.runner.input.into(),
                            span: nd.runner.last_span,
                            kind: DeserErrorKind::UnexpectedChar {
                                got: ':',
                                wanted: "a value",
                            },
                        })
                    }
                }
                b',' => {
                    match expectation {
                        Expectation::ListItemOrListClose | Expectation::ObjectKeyOrObjectClose => {
                            // Let's skip the comma and try again
                            nd.runner.input = &nd.runner.input[1..];
                            expectation = Expectation::Value;
                            continue;
                        }
                        other => {
                            trace!("Did not expect comma, expected {:?}", other);
                            Err(DeserError {
                                input: nd.runner.input.into(),
                                span: nd.runner.last_span,
                                kind: DeserErrorKind::UnexpectedChar {
                                    got: ',',
                                    wanted: "a value",
                                },
                            })
                        }
                    }
                }
                b'{' => {
                    nd.runner.input = &nd.runner.input[1..];
                    Ok(Outcome::ObjectStarted)
                }
                b'}' => {
                    if expectation == Expectation::ObjectKeyOrObjectClose {
                        nd.runner.input = &nd.runner.input[1..];
                        Ok(Outcome::ObjectEnded)
                    } else {
                        trace!("Did not expect closing brace, expected {:?}", expectation);
                        Err(DeserError {
                            input: nd.runner.input.into(),
                            span: nd.runner.last_span,
                            kind: DeserErrorKind::UnexpectedChar {
                                got: '}',
                                wanted: "a value",
                            },
                        })
                    }
                }
                b'[' => {
                    nd.runner.input = &nd.runner.input[1..];
                    Ok(Outcome::ListStarted)
                }
                b']' => {
                    if expectation == Expectation::ListItemOrListClose {
                        nd.runner.input = &nd.runner.input[1..];
                        Ok(Outcome::ListEnded)
                    } else {
                        Err(DeserError {
                            input: nd.runner.input.into(),
                            span: nd.runner.last_span,
                            kind: DeserErrorKind::UnexpectedChar {
                                got: ']',
                                wanted: "a value",
                            },
                        })
                    }
                }
                c => Err(DeserError {
                    input: nd.runner.input.into(),
                    span: nd.runner.last_span,
                    kind: DeserErrorKind::UnexpectedChar {
                        got: c as char,
                        wanted: "a value",
                    },
                }),
            };
            return (nd, res);
        }
    }
}
