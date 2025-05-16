// Integration tests for the Format trait's ability to handle different input types.
// Verify that the deserialization engine correctly processes various input types
// (e.g. `u8` bytes for JSON or `&str` for CLI args) with the same reflection machinery.

#[cfg(test)]
mod tests {
    use facet::Facet;
    use facet_deserialize::*;

    #[derive(Facet, Debug, PartialEq)]
    struct TestConfig {
        nom: String,
    }

    /// Mock formatter that processes byte slices ([u8]).
    ///
    /// This implementation provides a minimal verification that the deserialization
    /// system correctly handles the traditional byte-slice input format.
    struct MockByteFormat;

    impl Format for MockByteFormat {
        type Input<'input> = [u8];
        type SpanType = Cooked;

        fn source(&self) -> &'static str {
            "bin"
        }

        /// Generate tokens for deserialization in a predetermined sequence.
        ///
        /// Rather than actually parsing input bytes, this implementation simulates
        /// a parsing process by returning a fixed sequence of tokens based on the
        /// current position in the input.
        fn next<'input, 'facet, 'shape>(
            &mut self,
            nd: NextData<'input, 'facet, 'shape, Self::SpanType, Self::Input<'input>>,
            _exp: Expectation,
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
            // Use the start position to determine which token to return
            let position = nd.start();
            let input = nd.input();
            eprintln!("POSITION: {:?}", &position);
            eprintln!("INPUT: {:?}", &input);
            // Try to get the current byte
            let rel_pos = (position + 1) % 2; // pos. 1 -> 0, pos. 4 -> 2 -> 1
            if position == 0 {
                eprintln!("CURRENT BYTE: None (object start)");
            } else if rel_pos < input.len() && position < 8 {
                eprintln!(
                    "CURRENT BYTE: {:?} (ASCII: '{}')",
                    input[rel_pos],
                    char::from(input[rel_pos])
                );
            } else {
                eprintln!("CURRENT BYTE: None (object end)");
            }

            // Very rudimentary deserialisation routine: if we get 2 bytes we read the first as
            // "nom" (span=3) and the second as "test" (span=4). When the 2nd byte's span is put
            // on the runner we reach position 8 and the object ends. Anything unexpected errors.
            match position {
                0 => {
                    // Object start
                    let span = Span::new(position, 1);
                    (
                        nd,
                        Ok(Spanned {
                            node: Outcome::ObjectStarted,
                            span,
                        }),
                    )
                }
                1 => {
                    // Field name "nom"
                    let span = Span::new(position, 3);
                    (
                        nd,
                        Ok(Spanned {
                            node: Outcome::Scalar(Scalar::String("nom".into())),
                            span,
                        }),
                    )
                }
                4 => {
                    // Field value "test"
                    let span = Span::new(position, 4);
                    (
                        nd,
                        Ok(Spanned {
                            node: Outcome::Scalar(Scalar::String("test".into())),
                            span,
                        }),
                    )
                }
                8 => {
                    // Object end
                    let span = Span::new(position, 1);
                    (
                        nd,
                        Ok(Spanned {
                            node: Outcome::ObjectEnded,
                            span,
                        }),
                    )
                }
                _ => {
                    // Unexpected position
                    (
                        nd,
                        Err(Spanned {
                            node: DeserErrorKind::UnexpectedEof {
                                wanted: "no more input expected",
                            },
                            span: Span::new(position, 0),
                        }),
                    )
                }
            }
        }

        /// Minimal implementation of the skip method required by the Format trait.
        fn skip<'input, 'facet, 'shape>(
            &mut self,
            nd: NextData<'input, 'facet, 'shape, Self::SpanType, Self::Input<'input>>,
        ) -> NextResult<
            'input,
            'facet,
            'shape,
            Span<Self::SpanType>,
            Spanned<DeserErrorKind<'shape>, Self::SpanType>,
            Self::SpanType,
            Self::Input<'input>,
        >
        where
            'shape: 'input,
        {
            // Simply advance the position by 1
            let position = nd.start();
            let span = Span::new(position, 1);
            (nd, Ok(span))
        }
    }

    #[test]
    fn test_byte_slice_input() {
        // Explicit slice type annotation to avoid array type inference
        let dummy_bytes: &[u8] = b"xy";

        // Deserialize using the byte-based format
        let result: TestConfig = deserialize(dummy_bytes, MockByteFormat)
            .expect("Failed to deserialize from byte slice");

        // Verify expected field value
        assert_eq!(
            result,
            TestConfig {
                nom: "test".to_string()
            }
        );
    }

    /// Mock formatter that processes string slices ([&str]).
    ///
    /// This implementation verifies that the deserialization system
    /// can process string-based inputs like CLI arguments.
    struct MockCliFormat;

    impl Format for MockCliFormat {
        type Input<'input> = [&'input str];
        type SpanType = Raw;

        fn source(&self) -> &'static str {
            "cli"
        }

        /// Generate tokens for processing CLI-like arguments.
        ///
        /// Simulates parsing of arguments in the pattern:
        /// ["--nom", "test"]
        fn next<'input, 'facet, 'shape>(
            &mut self,
            nd: NextData<'input, 'facet, 'shape, Self::SpanType, Self::Input<'input>>,
            exp: Expectation,
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
            // Use the start position and expectation to determine which token to return
            let position = nd.start();
            let input = nd.input();
            eprintln!("POSITION: {:?}", &position);
            eprintln!("INPUT: {:?}", &input);

            if position == 0 {
                eprintln!("CURRENT ARG: None (object start)");
            } else if position <= input.len() {
                eprintln!("CURRENT ARG: {:?}", input[position - 1]);
            } else {
                eprintln!("CURRENT ARG: None (object end)");
            }

            match exp {
                Expectation::Value => {
                    if position == 0 {
                        // Start with object
                        let span = Span::new(position, 1); // Length 1 to advance position
                        (
                            nd,
                            Ok(Spanned {
                                node: Outcome::ObjectStarted,
                                span,
                            }),
                        )
                    } else {
                        // Unexpected value request
                        (
                            nd,
                            Err(Spanned {
                                node: DeserErrorKind::UnexpectedEof {
                                    wanted: "value at unexpected position",
                                },
                                span: Span::new(position, 0),
                            }),
                        )
                    }
                }
                Expectation::ObjectKeyOrObjectClose => {
                    if position == 1 {
                        // Field name "nom"
                        let field_name = input[position - 1].strip_prefix("--").unwrap();
                        let span = Span::new(position, 1); // Length 1 to advance position
                        if field_name != "nom" {
                            return (
                                nd,
                                Err(Spanned {
                                    node: DeserErrorKind::UnknownField {
                                        field_name: field_name.to_string(),
                                        shape: <TestConfig as Facet>::SHAPE,
                                    },
                                    span,
                                }),
                            );
                        }
                        (
                            nd,
                            Ok(Spanned {
                                node: Outcome::Scalar(Scalar::String(field_name.into())),
                                span,
                            }),
                        )
                    } else if position == 3 {
                        // End object
                        let span = Span::new(position, 1); // Length 1 to advance position
                        (
                            nd,
                            Ok(Spanned {
                                node: Outcome::ObjectEnded,
                                span,
                            }),
                        )
                    } else {
                        // Unexpected position
                        (
                            nd,
                            Err(Spanned {
                                node: DeserErrorKind::UnexpectedEof {
                                    wanted: "field or object end",
                                },
                                span: Span::new(position, 0),
                            }),
                        )
                    }
                }
                Expectation::ObjectVal => {
                    if position == 2 {
                        // Field value "test"
                        let field_value = input[position - 1];
                        let span = Span::new(position, 1); // Length 1 to advance position
                        (
                            nd,
                            Ok(Spanned {
                                node: Outcome::Scalar(Scalar::String(field_value.into())),
                                span,
                            }),
                        )
                    } else {
                        // Unexpected position
                        (
                            nd,
                            Err(Spanned {
                                node: DeserErrorKind::UnexpectedEof {
                                    wanted: "object value",
                                },
                                span: Span::new(position, 0),
                            }),
                        )
                    }
                }
                _ => {
                    // Unexpected expectation
                    (
                        nd,
                        Err(Spanned {
                            node: DeserErrorKind::UnexpectedEof {
                                wanted: "unsupported expectation",
                            },
                            span: Span::new(position, 0),
                        }),
                    )
                }
            }
        }

        fn skip<'input, 'facet, 'shape>(
            &mut self,
            nd: NextData<'input, 'facet, 'shape, Self::SpanType, Self::Input<'input>>,
        ) -> NextResult<
            'input,
            'facet,
            'shape,
            Span<Self::SpanType>,
            Spanned<DeserErrorKind<'shape>, Self::SpanType>,
            Self::SpanType,
            Self::Input<'input>,
        >
        where
            'shape: 'input,
        {
            // Simply return a span that advances the position
            let position = nd.start();
            let span = Span::new(position, 1); // Length 1 to advance position
            (nd, Ok(span))
        }
    }

    #[test]
    fn test_string_slice_input() {
        // Sample CLI args (content doesn't matter for this mock)
        let args: &[&str] = &["--nom", "test"];

        // Deserialize using the string-based format
        let result: TestConfig =
            deserialize(args, MockCliFormat).expect("Failed to deserialize from string slices");

        // Verify expected field value
        assert_eq!(
            result,
            TestConfig {
                nom: "test".to_string()
            }
        );
    }

    #[test]
    fn test_error_handling_with_raw_spans() {
        // Use invalid input to trigger an error to observe Raw span processing
        let invalid_args: &[&str] = &["--invalid-field", "value"];
        println!("The deserialize func should get called next");
        let result: Result<TestConfig, _> = deserialize(invalid_args, MockCliFormat);
        assert!(result.is_err());
        println!("{:?}", result);

        // Check that the error contains a properly converted span (from Raw to Cooked)
        if let Err(error) = result {
            // First, check that the error kind is what we expect for an unknown field
            match &error.kind {
                DeserErrorKind::UnknownField { field_name, .. } => {
                    assert_eq!(
                        field_name, "invalid-field",
                        "Error should indicate unknown field"
                    );
                }
                _ => panic!("Unexpected error kind: {:?}", error.kind),
            }

            // A real implementation would do more useful conversion e.g. count arg char lengths

            // Initial check: verify the error source_id matches our format
            assert_eq!(
                error.source_id, "cli",
                "Error source should match the format source"
            );
            // Check that the span has been properly converted from Raw to Cooked
            println!("Got error span {:?}", error.span);
            assert_eq!(
                error.span.start(),
                16,
                "Span should point to the start of the 2nd arg (index 1)"
            );
            assert_eq!(
                error.span.len(),
                5,
                "Span should have length of the 2nd arg (5 chars long)"
            );
        }
    }
}
