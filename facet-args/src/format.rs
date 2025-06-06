use crate::arg::{ArgType, extract_subspan};
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

/// Parse command line arguments provided by std::env::args() into a Facet-compatible type
pub fn from_std_args<'input, 'facet, 'shape, T: Facet<'facet>>()
-> Result<T, DeserError<'input, 'shape>>
where
    'input: 'facet + 'shape,
{
    let args = std::env::args().skip(1).collect::<Vec<String>>();
    let args_str: Vec<&'static str> = args
        .into_iter()
        .map(|s| Box::leak(s.into_boxed_str()) as &str)
        .collect();

    from_slice(Box::leak(args_str.into_boxed_slice()))
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
        let subspans = nd.substack().get();
        let has_subspans = !subspans.is_empty();

        let stay_put = Span::new(arg_idx, 0);
        let step_forth = Span::new(arg_idx, 1);

        let span = match expectation {
            Expectation::Value => stay_put,
            Expectation::ObjectKeyOrObjectClose
            | Expectation::ObjectVal
            | Expectation::ListItemOrListClose => step_forth,
        };

        let result = match expectation {
            // Top-level value
            Expectation::Value => {
                // Check if it's a struct type
                wrap_outcome_result(validate_struct_type(shape), Outcome::ObjectStarted, span)
            }

            // Object key (or finished)
            Expectation::ObjectKeyOrObjectClose => {
                /* Check if we have more arguments */
                if arg_idx < args.len() {
                    let arg = args[arg_idx];

                    // Check if we need to resegment an arg with '='
                    if arg.starts_with("-") && arg.contains('=') && !has_subspans {
                        // This is an argument with '=' that needs resegmentation
                        if let Some(key_value_subspans) = create_key_value_subspans(arg) {
                            return (nd, wrap_resegmented_result(key_value_subspans, stay_put));
                        }
                    }

                    // Regular argument or subspan processing
                    let effective_arg = if has_subspans {
                        extract_subspan(&subspans[0], arg)
                    } else {
                        arg
                    };

                    // Parse the argument type
                    match ArgType::parse(effective_arg) {
                        ArgType::LongFlag(key) => {
                            // Validate field exists
                            wrap_string_result(
                                validate_field(&key, shape, &nd.wip).map(|_| key),
                                if has_subspans { stay_put } else { span },
                            )
                        }
                        ArgType::ShortFlag(key) => {
                            // Convert short argument to field name via shape
                            wrap_field_result(
                                find_field_by_short_flag(key, shape),
                                if has_subspans { stay_put } else { span },
                            )
                        }
                        ArgType::Positional => {
                            // Handle positional argument
                            wrap_field_result(find_positional_field(shape, &nd.wip), stay_put)
                        }
                        ArgType::None => {
                            // Handle empty argument (shouldn't happen normally)
                            let err = create_unknown_field_error("empty argument", shape);
                            Err(Spanned { node: err, span })
                        }
                    }
                } else {
                    // EOF: inject implicit-false-if-absent bool flags, if there are any
                    handle_unset_bool_field_error(find_unset_bool_field(shape, &nd.wip), span)
                }
            }

            // Value for the current key
            Expectation::ObjectVal => {
                // Determine what to do based on the type and available arguments
                if shape.is_type::<bool>() {
                    // Handle boolean values (true if we have an arg, false if EOF)
                    let has_arg = arg_idx < args.len();
                    wrap_result(handle_bool_value(has_arg), Outcome::Scalar, stay_put)
                } else {
                    // For non-boolean types, check if we have subspans
                    let result = if has_subspans && arg_idx < args.len() {
                        let arg = args[arg_idx];
                        let subspan = &subspans[1];
                        let arg_type: ArgType = (subspan, arg).into();

                        // If this isn't a flag type (neither ShortFlag nor LongFlag), use it as a value
                        match arg_type {
                            ArgType::ShortFlag(_) | ArgType::LongFlag(_) => {
                                // It's a flag, not a value - continue to validation
                                None
                            }
                            _ => {
                                // Extract the actual substring to use
                                let part = extract_subspan(subspan, arg);
                                Some(Ok(parse_scalar(part, span)))
                            }
                        }
                    } else {
                        None
                    };

                    // Use the result from above if available, otherwise fall back to regular validation
                    result.unwrap_or_else(|| {
                        // No usable subspans, fall back to regular validation
                        match validate_value_available(arg_idx, args) {
                            Ok(arg) => Ok(parse_scalar(arg, span)),
                            Err(err) => Err(Spanned {
                                node: err,
                                span: Span::new(arg_idx.saturating_sub(1), 0),
                            }),
                        }
                    })
                }
            }

            // List items
            Expectation::ListItemOrListClose => {
                // End the list if we're out of arguments, or if it's a new flag
                if is_list_ended(arg_idx, args) {
                    // End the list
                    Ok(Spanned {
                        node: Outcome::ListEnded,
                        span,
                    })
                } else {
                    // Process the next item
                    Ok(Spanned {
                        node: Outcome::Scalar(Scalar::String(Cow::Borrowed(args[arg_idx]))),
                        span: step_forth,
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
        let span = Span::new(arg_idx, 1);

        let result = if arg_idx < args.len() {
            // Simply skip one position
            Ok(span)
        } else {
            // No argument to skip
            Err(Spanned {
                node: DeserErrorKind::UnexpectedEof {
                    wanted: "argument to skip",
                },
                span,
            })
        };

        (nd, result)
    }
}
