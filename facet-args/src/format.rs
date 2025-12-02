use crate::{
    arg::ArgType,
    error::{ArgsError, ArgsErrorKind, ArgsErrorWithInput},
    span::Span,
};
use facet_core::{Def, EnumType, Facet, Field, Shape, Type, UserType, Variant};
use facet_reflect::{HeapValue, Partial};
use heck::{ToKebabCase, ToSnakeCase};

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
        tracing::trace!("flattened args: {flattened_args:?}");
        tracing::trace!("arg_indices: {arg_indices:?}");

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
        tracing::trace!("Handling field at index {field_index}");

        let mut p = p.begin_nth_field(field_index)?;

        tracing::trace!("After begin_field, shape is {}", p.shape());
        if p.shape().is_shape(bool::SHAPE) {
            tracing::trace!("Flag is boolean, setting it to true");
            p = p.set(true)?;

            self.index += 1;
        } else {
            tracing::trace!("Flag isn't boolean, expecting a {} value", p.shape());

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
        let p = Partial::alloc_shape(self.shape)?;

        // Check if we're parsing an enum (subcommand) or a struct
        match self.shape.ty {
            Type::User(UserType::Enum(enum_type)) => self.parse_enum(p, enum_type),
            Type::User(UserType::Struct(_)) => self.parse_struct(p),
            _ => Err(ArgsErrorKind::NoFields { shape: self.shape }),
        }
    }

    /// Parse an enum type as a subcommand
    fn parse_enum(
        &mut self,
        p: Partial<'static>,
        enum_type: EnumType,
    ) -> Result<HeapValue<'static>, ArgsErrorKind> {
        // The first positional argument should be the subcommand name
        if self.index >= self.args.len() {
            return Err(ArgsErrorKind::MissingSubcommand { shape: self.shape });
        }

        let subcommand_name = self.args[self.index];
        tracing::trace!("Looking for subcommand: {subcommand_name}");

        // Find matching variant (convert variant names to kebab-case for matching)
        let variant = find_variant_by_name(enum_type, subcommand_name)?;
        tracing::trace!("Found variant: {}", variant.name);

        self.index += 1;

        // Select the variant and parse its fields
        let mut p = p.select_variant_named(variant.name)?;

        // Parse the variant's fields like a struct
        p = self.parse_variant_fields(p, variant)?;

        Ok(p.build()?)
    }

    /// Parse a struct type
    fn parse_struct(
        &mut self,
        mut p: Partial<'static>,
    ) -> Result<HeapValue<'static>, ArgsErrorKind> {
        while self.args.len() > self.index {
            let arg = self.args[self.index];
            let arg_span = Span::new(self.arg_indices[self.index], arg.len());
            let at = if self.positional_only {
                ArgType::Positional
            } else {
                ArgType::parse(arg)
            };
            tracing::trace!("Parsed {at:?}");

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
                            tracing::trace!("Looking up long flag {flag} (field name: {snek})");
                            let Some(field_index) = p.field_index(&snek) else {
                                return Err(ArgsErrorKind::UnknownLongFlag);
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            let snek = flag.to_snake_case();
                            tracing::trace!("Looking up long flag {flag} (field name: {snek})");
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
                            // We have something like `-k=value`
                            let mut tokens = tokens.into_iter();
                            let Some(key) = tokens.next() else {
                                unreachable!()
                            };
                            let Some(value) = tokens.next() else {
                                unreachable!()
                            };

                            let flag = key.s;
                            tracing::trace!("Looking up short flag {flag}");
                            let fields = self.fields(&p)?;
                            let Some(field_index) = find_field_index_with_short(fields, flag)
                            else {
                                return Err(ArgsErrorKind::UnknownShortFlag);
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            tracing::trace!("Looking up short flag {flag}");
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

                    // First, check if there's a subcommand field that hasn't been set yet
                    if let Some((field_index, field)) = find_subcommand_field(fields) {
                        if !p.is_field_set(field_index)? {
                            p = self.handle_subcommand_field(p, field_index, field)?;
                            continue;
                        }
                    }

                    // Otherwise, look for a positional field
                    let mut chosen_field_index: Option<usize> = None;

                    for (field_index, field) in fields.iter().enumerate() {
                        let is_positional = field.has_attr(Some("args"), "positional");
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

                        tracing::trace!("found field, it's not a list {field:?}");
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

        // Finalize: set defaults for unset fields
        p = self.finalize_struct(p)?;

        Ok(p.build()?)
    }

    /// Parse fields of an enum variant (similar to struct parsing)
    fn parse_variant_fields(
        &mut self,
        mut p: Partial<'static>,
        variant: &'static Variant,
    ) -> Result<Partial<'static>, ArgsErrorKind> {
        let fields = variant.data.fields;

        while self.args.len() > self.index {
            let arg = self.args[self.index];
            let arg_span = Span::new(self.arg_indices[self.index], arg.len());
            let at = if self.positional_only {
                ArgType::Positional
            } else {
                ArgType::parse(arg)
            };
            tracing::trace!("Parsing variant field, arg: {at:?}");

            match at {
                ArgType::DoubleDash => {
                    self.positional_only = true;
                    self.index += 1;
                }
                ArgType::LongFlag(flag) => {
                    let flag_span = Span::new(arg_span.start + 2, arg_span.len - 2);
                    match split(flag, flag_span) {
                        Some(tokens) => {
                            let mut tokens = tokens.into_iter();
                            let key = tokens.next().unwrap();
                            let value = tokens.next().unwrap();

                            let snek = key.s.to_snake_case();
                            tracing::trace!(
                                "Looking up long flag {flag} in variant (field name: {snek})"
                            );
                            let Some(field_index) = fields.iter().position(|f| f.name == snek)
                            else {
                                return Err(ArgsErrorKind::UnknownLongFlag);
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            let snek = flag.to_snake_case();
                            tracing::trace!(
                                "Looking up long flag {flag} in variant (field name: {snek})"
                            );
                            let Some(field_index) = fields.iter().position(|f| f.name == snek)
                            else {
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
                            let mut tokens = tokens.into_iter();
                            let key = tokens.next().unwrap();
                            let value = tokens.next().unwrap();

                            tracing::trace!("Looking up short flag {flag} in variant");
                            let Some(field_index) = find_field_index_with_short(fields, key.s)
                            else {
                                return Err(ArgsErrorKind::UnknownShortFlag);
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            tracing::trace!("Looking up short flag {flag} in variant");
                            let Some(field_index) = find_field_index_with_short(fields, flag)
                            else {
                                return Err(ArgsErrorKind::UnknownShortFlag);
                            };
                            p = self.handle_field(p, field_index, None)?;
                        }
                    }
                }
                ArgType::Positional => {
                    // Check for subcommand field first (for nested subcommands)
                    if let Some((field_index, field)) = find_subcommand_field(fields) {
                        if !p.is_field_set(field_index)? {
                            p = self.handle_subcommand_field(p, field_index, field)?;
                            continue;
                        }
                    }

                    // Look for positional field
                    let mut chosen_field_index: Option<usize> = None;

                    for (field_index, field) in fields.iter().enumerate() {
                        let is_positional = field.has_attr(Some("args"), "positional");
                        if !is_positional {
                            continue;
                        }

                        if matches!(field.shape().def, Def::List(_)) {
                            // list field, keep going
                        } else if p.is_field_set(field_index)? {
                            continue;
                        }

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

        // Finalize variant fields
        p = self.finalize_variant_fields(p, fields)?;

        Ok(p)
    }

    /// Handle a field marked with args::subcommand
    fn handle_subcommand_field(
        &mut self,
        p: Partial<'static>,
        field_index: usize,
        field: &'static Field,
    ) -> Result<Partial<'static>, ArgsErrorKind> {
        let field_shape = field.shape();
        tracing::trace!(
            "Handling subcommand field: {} with shape {}",
            field.name,
            field_shape
        );

        let mut p = p.begin_nth_field(field_index)?;

        // Check if the field is an Option<Enum> or just an Enum
        // IMPORTANT: Check Def::Option FIRST because Option is represented as an enum internally
        let (is_optional, _enum_shape, enum_type) = if let Def::Option(option_def) = field_shape.def
        {
            // It's Option<T>, get the inner type
            let inner_shape = option_def.t;
            if let Type::User(UserType::Enum(enum_type)) = inner_shape.ty {
                (true, inner_shape, enum_type)
            } else {
                return Err(ArgsErrorKind::NoFields { shape: field_shape });
            }
        } else if let Type::User(UserType::Enum(enum_type)) = field_shape.ty {
            // It's a direct enum
            (false, field_shape, enum_type)
        } else {
            return Err(ArgsErrorKind::NoFields { shape: field_shape });
        };

        // Get the subcommand name from current argument
        let subcommand_name = self.args[self.index];
        tracing::trace!("Looking for subcommand variant: {subcommand_name}");

        // Find matching variant
        let variant = match find_variant_by_name(enum_type, subcommand_name) {
            Ok(v) => v,
            Err(e) => {
                if is_optional {
                    // For optional subcommand, if we can't find a variant, leave it as None
                    // But first we need to "undo" begin_nth_field... we can't easily do that
                    // So instead we should check if it's a valid subcommand BEFORE calling begin_nth_field
                    // For now, return the error
                    return Err(e);
                } else {
                    return Err(e);
                }
            }
        };

        self.index += 1;

        if is_optional {
            // Set Option to Some(variant)
            p = p.begin_some()?;
        }

        // Select the variant
        p = p.select_variant_named(variant.name)?;

        // Parse the variant's fields
        p = self.parse_variant_fields(p, variant)?;

        if is_optional {
            p = p.end()?; // end Some
        }

        p = p.end()?; // end field

        Ok(p)
    }

    /// Finalize struct fields (set defaults, check required)
    fn finalize_struct(&self, mut p: Partial<'static>) -> Result<Partial<'static>, ArgsErrorKind> {
        let fields = self.fields(&p)?;
        for (field_index, field) in fields.iter().enumerate() {
            if p.is_field_set(field_index)? {
                continue;
            }

            // Check if it's an optional subcommand field
            if field.has_attr(Some("args"), "subcommand") {
                let field_shape = field.shape();
                if let Def::Option(_) = field_shape.def {
                    // Optional subcommand, set to None using default
                    // Option<T> has a default_in_place that sets it to None
                    p = p.set_nth_field_to_default(field_index)?;
                    continue;
                } else {
                    // Required subcommand missing
                    return Err(ArgsErrorKind::MissingSubcommand { shape: field_shape });
                }
            }

            if field.has_default() {
                tracing::trace!("Setting #{field_index} field to default: {field:?}");
                p = p.set_nth_field_to_default(field_index)?;
            } else if (field.shape)().is_shape(bool::SHAPE) {
                // bools are just set to false
                p = p.set_nth_field(field_index, false)?;
            } else {
                return Err(ArgsErrorKind::MissingArgument { field });
            }
        }
        Ok(p)
    }

    /// Finalize variant fields (set defaults, check required)
    fn finalize_variant_fields(
        &self,
        mut p: Partial<'static>,
        fields: &'static [Field],
    ) -> Result<Partial<'static>, ArgsErrorKind> {
        for (field_index, field) in fields.iter().enumerate() {
            if p.is_field_set(field_index)? {
                continue;
            }

            if field.has_default() {
                tracing::trace!("Setting variant field #{field_index} to default: {field:?}");
                p = p.set_nth_field_to_default(field_index)?;
            } else if (field.shape)().is_shape(bool::SHAPE) {
                p = p.set_nth_field(field_index, false)?;
            } else {
                return Err(ArgsErrorKind::MissingArgument { field });
            }
        }
        Ok(p)
    }
}

/// Find a variant by its CLI name (kebab-case) or its actual name
fn find_variant_by_name(
    enum_type: EnumType,
    name: &str,
) -> Result<&'static Variant, ArgsErrorKind> {
    tracing::trace!(
        "find_variant_by_name: looking for '{}' among variants: {:?}",
        name,
        enum_type
            .variants
            .iter()
            .map(|v| v.name)
            .collect::<Vec<_>>()
    );

    // First check for rename attribute
    for variant in enum_type.variants {
        if let Some(attr) = variant.get_builtin_attr("rename") {
            if let Some(rename) = attr.get_as::<&str>() {
                if *rename == name {
                    return Ok(variant);
                }
            }
        }
    }

    // Then check kebab-case conversion of variant name
    for variant in enum_type.variants {
        let kebab_name = variant.name.to_kebab_case();
        tracing::trace!(
            "  checking variant '{}' -> kebab '{}' against '{}'",
            variant.name,
            kebab_name,
            name
        );
        if kebab_name == name {
            return Ok(variant);
        }
    }

    // Finally check exact name match
    for variant in enum_type.variants {
        if variant.name == name {
            return Ok(variant);
        }
    }

    Err(ArgsErrorKind::UnknownSubcommand {
        shape: enum_type
            .variants
            .first()
            .map(|v| v.data.fields)
            .unwrap_or(&[])
            .first()
            .map(|f| f.shape())
            .unwrap_or(<()>::SHAPE),
    })
}

/// Find a field marked with args::subcommand
fn find_subcommand_field(fields: &'static [Field]) -> Option<(usize, &'static Field)> {
    fields
        .iter()
        .enumerate()
        .find(|(_, f)| f.has_attr(Some("args"), "subcommand"))
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
        if let Some(ext) = f.get_attr(Some("args"), "short") {
            // The attribute stores the full Attr enum
            if let Some(crate::Attr::Short(opt_char)) = ext.get_as::<crate::Attr>() {
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
