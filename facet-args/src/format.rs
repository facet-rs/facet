use crate::{
    arg::ArgType,
    error::{ArgsError, ArgsErrorKind, ArgsErrorWithInput},
    span::Span,
};
use facet_core::{Def, Facet, Field, FieldFlags, Shape, Type, UserType};
use facet_reflect::{HeapValue, Partial};
use heck::ToSnakeCase;

/// Parse command line arguments provided by std::env::args() into a Facet-compatible type
pub fn from_std_args<T: Facet<'static>>() -> Result<T, ArgsErrorWithInput> {
    let args = std::env::args().skip(1).collect::<Vec<String>>();
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    from_slice(&args_str[..])
}

/// Parse command line arguments into a Facet-compatible type
pub fn from_slice<'input, T: Facet<'static>>(
    args: &'input [&'input str],
) -> Result<T, ArgsErrorWithInput> {
    let mut cx = Context::new(args, T::SHAPE);
    let hv = cx.work_add_input()?;

    // TODO: proper error handling
    Ok(hv.materialize::<T>().unwrap())
}

struct Context<'input> {
    /// The shape we're building
    shape: &'static Shape,

    /// Input arguments (already tokenized)
    args: &'input [&'input str],

    /// Argument we're currently parsing
    index: usize,

    /// Flips to true after `--`, which makes us only look for positional args
    positional_only: bool,

    /// Index of every argument in `flattened_args`
    arg_indices: Vec<usize>,

    /// Essentially `input.join(" ")`
    flattened_args: String,
}

impl<'input> Context<'input> {
    fn new(args: &'input [&'input str], shape: &'static Shape) -> Self {
        let mut arg_indices = vec![];
        let mut flattened_args = String::new();

        for arg in args {
            arg_indices.push(flattened_args.len());
            flattened_args.push_str(arg);
            flattened_args.push(' ');
        }
        log::trace!("flattened args: {flattened_args:?}");
        log::trace!("arg_indices: {arg_indices:?}");

        Self {
            shape,
            args,
            index: 0,
            positional_only: false,
            arg_indices,
            flattened_args,
        }
    }

    /// Returns fields for the current shape, errors out if it's not a struct
    fn fields(&self, p: &Partial<'static>) -> Result<&'static [Field], ArgsErrorKind> {
        let shape = p.shape();
        match &shape.ty {
            Type::User(UserType::Struct(struct_type)) => Ok(struct_type.fields),
            _ => Err(ArgsErrorKind::NoFields { shape }),
        }
    }

    /// Once we have found the struct field that corresponds to a `--long` or `-s` short flag,
    /// this is where we toggle something on, look for a value, etc.
    fn handle_field(
        &mut self,
        p: Partial<'static>,
        field_index: usize,
        value: Option<SplitToken<'input>>,
    ) -> Result<Partial<'static>, ArgsErrorKind> {
        let fields = self.fields(&p)?;
        let field = fields[field_index];
        log::trace!("Found field {field:?}");

        let mut p = p.begin_nth_field(field_index)?;

        log::trace!("After begin_field, shape is {}", p.shape());
        if p.shape().is_shape(bool::SHAPE) {
            log::trace!("Flag is boolean, setting it to true");
            p = p.set(true)?;

            self.index += 1;
        } else {
            log::trace!("Flag isn't boolean, expecting a {} value", p.shape());

            if let Some(value) = value {
                p = self.handle_value(p, value.s)?;
            } else {
                if self.index + 1 >= self.args.len() {
                    return Err(ArgsErrorKind::ExpectedValueGotEof { shape: p.shape() });
                }
                let value = self.args[self.index + 1];

                self.index += 1;
                p = self.handle_value(p, value)?;
            }

            self.index += 1;
        }

        p = p.end()?;

        Ok(p)
    }

    fn handle_value(
        &mut self,
        p: Partial<'static>,
        value: &'input str,
    ) -> Result<Partial<'static>, ArgsErrorKind> {
        let p = match p.shape().def {
            Def::List(_) => {
                // if it's a list, then we'll want to initialize the list first and push to it
                let mut p = p.begin_list()?;
                p = p.begin_list_item()?;
                p = p.parse_from_str(value)?;
                p.end()?
            }
            _ => {
                // TODO: this surely won't be enough eventually
                p.parse_from_str(value)?
            }
        };

        Ok(p)
    }

    fn work_add_input(&mut self) -> Result<HeapValue<'static>, ArgsErrorWithInput> {
        self.work().map_err(|e| ArgsErrorWithInput {
            inner: e,
            flattened_args: self.flattened_args.clone(),
        })
    }

    /// Forward to `work_inner`, converts `ArgsErrorKind` to `ArgsError` (with span)
    fn work(&mut self) -> Result<HeapValue<'static>, ArgsError> {
        self.work_inner().map_err(|kind| {
            let span = if self.index >= self.args.len() {
                Span::new(self.flattened_args.len(), 0)
            } else {
                let arg = self.args[self.index];
                let index = self.arg_indices[self.index];
                Span::new(index, arg.len())
            };
            ArgsError::new(kind, span)
        })
    }

    fn work_inner(&mut self) -> Result<HeapValue<'static>, ArgsErrorKind> {
        let mut p = Partial::alloc_shape(self.shape)?;

        while self.args.len() > self.index {
            let arg = self.args[self.index];
            let arg_span = Span::new(self.arg_indices[self.index], arg.len());
            let at = if self.positional_only {
                ArgType::Positional
            } else {
                ArgType::parse(arg)
            };
            log::trace!("Parsed {at:?}");

            match at {
                ArgType::DoubleDash => {
                    self.positional_only = true;
                    self.index += 1;
                }
                ArgType::LongFlag(flag) => {
                    let flag_span = Span::new(arg_span.start + 2, arg_span.len - 2);
                    match split(flag, flag_span) {
                        Some(tokens) => {
                            // We have something like `--key=value`
                            let mut tokens = tokens.into_iter();
                            let Some(key) = tokens.next() else {
                                unreachable!()
                            };
                            let Some(value) = tokens.next() else {
                                unreachable!()
                            };

                            let flag = key.s;
                            let snek = key.s.to_snake_case();
                            log::trace!("Looking up long flag {flag} (field name: {snek})");
                            let Some(field_index) = p.field_index(&snek) else {
                                return Err(ArgsErrorKind::UnknownLongFlag);
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            let snek = flag.to_snake_case();
                            log::trace!("Looking up long flag {flag} (field name: {snek})");
                            let Some(field_index) = p.field_index(&snek) else {
                                return Err(ArgsErrorKind::UnknownLongFlag);
                            };
                            p = self.handle_field(p, field_index, None)?;
                        }
                    }
                }
                ArgType::ShortFlag(flag) => {
                    let flag_span = Span::new(arg_span.start + 1, arg_span.len - 1);
                    match split(flag, flag_span) {
                        Some(tokens) => {
                            // We have something like `--key=value`
                            let mut tokens = tokens.into_iter();
                            let Some(key) = tokens.next() else {
                                unreachable!()
                            };
                            let Some(value) = tokens.next() else {
                                unreachable!()
                            };

                            let flag = key.s;
                            log::trace!("Looking up short flag {flag}");
                            let fields = self.fields(&p)?;
                            let Some(field_index) = find_field_index_with_short(fields, flag)
                            else {
                                return Err(ArgsErrorKind::UnknownShortFlag);
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            log::trace!("Looking up short flag {flag}");
                            let fields = self.fields(&p)?;
                            let Some(field_index) = find_field_index_with_short(fields, flag)
                            else {
                                return Err(ArgsErrorKind::UnknownShortFlag);
                            };
                            p = self.handle_field(p, field_index, None)?;
                        }
                    }
                }
                ArgType::Positional => {
                    let fields = self.fields(&p)?;
                    let mut chosen_field_index: Option<usize> = None;

                    for (field_index, field) in fields.iter().enumerate() {
                        let is_positional = field.has_extension_attr("args", "positional");
                        if !is_positional {
                            continue;
                        }

                        // we've found a positional field. if it's a list, then we're done: every
                        // positional argument will just be pushed to it.
                        if matches!(field.shape().def, Def::List(_list_def)) {
                            // cool, keep going
                        } else if p.is_field_set(field_index)? {
                            // field is already set, continue
                            continue;
                        }

                        log::trace!("found field, it's not a list {field:?}");
                        chosen_field_index = Some(field_index);
                        break;
                    }

                    let Some(chosen_field_index) = chosen_field_index else {
                        return Err(ArgsErrorKind::UnexpectedPositionalArgument);
                    };

                    p = p.begin_nth_field(chosen_field_index)?;

                    let value = self.args[self.index];
                    p = self.handle_value(p, value)?;

                    p = p.end()?;
                    self.index += 1;
                }
                ArgType::None => todo!(),
            }
        }

        {
            let fields = self.fields(&p)?;
            for (field_index, field) in fields.iter().enumerate() {
                if p.is_field_set(field_index)? {
                    // cool
                    continue;
                }

                if field.flags.contains(FieldFlags::DEFAULT) {
                    log::trace!("Setting #{field_index} field to default: {field:?}");
                    p = p.set_nth_field_to_default(field_index)?;
                } else if (field.shape)().is_shape(bool::SHAPE) {
                    // bools are just set to false
                    p = p.set_nth_field(field_index, false)?;
                } else {
                    return Err(ArgsErrorKind::MissingArgument { field });
                }
            }
        }

        Ok(p.build()?)
    }
}

/// Result of `split`
#[derive(Debug, PartialEq)]
struct SplitToken<'input> {
    s: &'input str,
    span: Span,
}

/// Split on `=`, e.g. `a=b` returns (`a`, `b`).
/// Span-aware. If `=` is not contained in the input string,
/// returns None
fn split<'input>(input: &'input str, span: Span) -> Option<Vec<SplitToken<'input>>> {
    let equals_index = input.find('=')?;

    let l = &input[0..equals_index];
    let l_span = Span::new(span.start, l.len());

    let r = &input[equals_index + 1..];
    let r_span = Span::new(equals_index + 1, r.len());

    Some(vec![
        SplitToken { s: l, span: l_span },
        SplitToken { s: r, span: r_span },
    ])
}

#[test]
fn test_split() {
    assert_eq!(split("ababa", Span::new(5, 5)), None);
    assert_eq!(
        split("foo=bar", Span::new(0, 7)),
        Some(vec![
            SplitToken {
                s: "foo",
                span: Span::new(0, 3)
            },
            SplitToken {
                s: "bar",
                span: Span::new(4, 3)
            },
        ])
    );
    assert_eq!(
        split("foo=", Span::new(0, 4)),
        Some(vec![
            SplitToken {
                s: "foo",
                span: Span::new(0, 3)
            },
            SplitToken {
                s: "",
                span: Span::new(4, 0)
            },
        ])
    );
    assert_eq!(
        split("=bar", Span::new(0, 4)),
        Some(vec![
            SplitToken {
                s: "",
                span: Span::new(0, 0)
            },
            SplitToken {
                s: "bar",
                span: Span::new(1, 3)
            },
        ])
    );
}

/// Given an array of fields, find the field with the given `args::short = 'a'`
/// annotation. Uses extension attribute syntax: #[facet(args::short = "j")]
fn find_field_index_with_short(fields: &'static [Field], short: &str) -> Option<usize> {
    let short_char = short.chars().next()?;
    fields.iter().position(|f| {
        if let Some(ext) = f.get_extension_attr("args", "short") {
            // The short attribute returns Option<char>
            if let Some(opt_char) = ext.get_as::<Option<char>>() {
                match opt_char {
                    Some(c) => *c == short_char,
                    None => {
                        // No explicit short specified, use first char of field name
                        f.name.starts_with(short_char)
                    }
                }
            } else {
                false
            }
        } else {
            false
        }
    })
}
