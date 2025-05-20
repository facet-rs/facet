use crate::arg::ArgType;
use crate::fields::*;
use crate::parse::parse_scalar;
use crate::results::*;
use alloc::borrow::Cow;
use core::fmt;
use facet_core::Facet;
use facet_deserialize::{
    DeserError, DeserErrorKind, Expectation, Format, NextData, NextResult, Outcome, Raw, Scalar,
    Span, Spanned,
};

/// Command-line argument format for Facet deserialization
pub struct Cli;

impl fmt::Display for Cli {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cli")
    }
}

/// Parse command line arguments into a Facet-compatible type
pub fn from_slice<'input, 'facet, 'shape, T: Facet<'facet>>(
    args: &'input [&'input str],
) -> Result<T, DeserError<'input, 'shape>>
where
    'input: 'facet + 'shape,
{
    facet_deserialize::deserialize(args, Cli)
}

impl Format for Cli {
    type Input<'input> = [&'input str];
    type SpanType = Raw;

    fn source(&self) -> &'static str {
        "args"
    }

    fn next<'input, 'facet, 'shape>(
        &mut self,
        nd: NextData<'input, 'facet, 'shape, Self::SpanType, Self::Input<'input>>,
        expectation: Expectation,
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
        let arg_idx = nd.start();
        let shape = nd.wip.shape();
        let args = nd.input();

        let result = match expectation {
            // Top-level value
            Expectation::Value => {
                let span = Span::new(arg_idx, 0);

                // Check if it's a struct type
                wrap_outcome_result(validate_struct_type(shape), Outcome::ObjectStarted, span)
            }

            // Object key (or finished)
            Expectation::ObjectKeyOrObjectClose => {
                /* Check if we have more arguments */
                if arg_idx < args.len() {
                    let arg = args[arg_idx];
                    let span = Span::new(arg_idx, 1);
                    let error_span = Span::new(arg_idx, 0);

                    // Parse the argument type
                    match ArgType::parse(arg) {
                        ArgType::LongFlag(key) => {
                            // Validate field exists
                            wrap_string_result(
                                validate_field(&key, shape, &nd.wip).map(|_| key),
                                span,
                                error_span,
                            )
                        }
                        ArgType::ShortFlag(key) => {
                            // Convert short argument to field name via shape
                            wrap_field_result(
                                find_field_by_short_flag(key, shape),
                                span,
                                error_span,
                            )
                        }
                        ArgType::Positional => {
                            // Handle positional argument
                            wrap_field_result(
                                find_positional_field(shape, &nd.wip),
                                Span::new(arg_idx, 0), // TODO: just pass as span?
                                error_span,
                            )
                        }
                        ArgType::None => {
                            // Handle empty argument (shouldn't happen normally)
                            let err = create_unknown_field_error("empty argument", shape);
                            Err(Spanned {
                                node: err,
                                span: error_span,
                            })
                        }
                    }
                } else {
                    // EOF: inject implicit-false-if-absent bool flags, if there are any
                    let span = Span::new(arg_idx, 0);
                    handle_unset_bool_field(find_unset_bool_field(shape, &nd.wip), span)
                }
            }

            // Value for the current key
            Expectation::ObjectVal => {
                // Determine what to do based on the type and available arguments
                if shape.is_type::<bool>() {
                    // Handle boolean values (true if we have an arg, false if EOF)
                    let has_arg = arg_idx < args.len();
                    wrap_result(
                        handle_bool_value(has_arg),
                        Outcome::Scalar,
                        Span::new(arg_idx, 0),
                        Span::new(arg_idx, 0),
                    )
                } else {
                    // For non-boolean types, validate and parse the value
                    match validate_value_available(arg_idx, args) {
                        Ok(arg) => {
                            let span = Span::new(arg_idx, 1);
                            Ok(parse_scalar(arg, span))
                        }
                        Err(err) => Err(Spanned {
                            node: err,
                            span: Span::new(arg_idx.saturating_sub(1), 0),
                        }),
                    }
                }
            }

            // List items
            Expectation::ListItemOrListClose => {
                // End the list if we're out of arguments, or if it's a new flag
                if is_list_ended(arg_idx, args) {
                    // End the list
                    Ok(Spanned {
                        node: Outcome::ListEnded,
                        span: Span::new(arg_idx, 0),
                    })
                } else {
                    // Process the next item
                    Ok(Spanned {
                        node: Outcome::Scalar(Scalar::String(Cow::Borrowed(args[arg_idx]))),
                        span: Span::new(arg_idx, 1),
                    })
                }
            }
        };

        (nd, result)
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
        let arg_idx = nd.start();
        let args = nd.input();

        if arg_idx < args.len() {
            // Simply skip one position
            (nd, Ok(Span::new(arg_idx, 1)))
        } else {
            // No argument to skip
            (
                nd,
                Err(Spanned {
                    node: DeserErrorKind::UnexpectedEof {
                        wanted: "argument to skip",
                    },
                    span: Span::new(arg_idx, 1),
                }),
            )
        }
    }
}
