use crate::{
    arg::ArgType,
    error::{ArgsError, ArgsErrorKind, ArgsErrorWithInput, get_variants_from_shape},
    help::{HelpConfig, generate_help_for_shape},
    is_counted_field, is_supported_counted_type,
    span::Span,
};
use alloc::collections::BTreeMap;
use facet_core::{Def, EnumType, Facet, Field, Shape, StructKind, Type, UserType, Variant};
use facet_reflect::{HeapValue, Partial};
use heck::{ToKebabCase, ToSnakeCase};

/// Check if the given argument is a help flag
fn is_help_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "-help" | "/?")
}

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
    from_slice_with_config(args, &HelpConfig::default())
}

/// Parse command line arguments with custom help configuration
pub fn from_slice_with_config<'input, T: Facet<'static>>(
    args: &'input [&'input str],
    help_config: &HelpConfig,
) -> Result<T, ArgsErrorWithInput> {
    // Check for help flag as the only argument (or first argument for simplicity)
    if let Some(first_arg) = args.first()
        && is_help_flag(first_arg)
    {
        let help_text = generate_help_for_shape(T::SHAPE, help_config);
        let span = Span::new(0, first_arg.len());
        return Err(ArgsErrorWithInput {
            inner: ArgsError::new(ArgsErrorKind::HelpRequested { help_text }, span),
            flattened_args: args.join(" "),
        });
    }

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

    /// Stack of counted field maps (one per struct/variant nesting level).
    /// Maps field_index -> count for fields with `args::counted`.
    counted_stack: Vec<BTreeMap<usize, u64>>,
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
            counted_stack: Vec::new(),
        }
    }

    fn push_counted_scope(&mut self) {
        self.counted_stack.push(BTreeMap::new());
    }

    fn pop_and_apply_counted_fields(
        &mut self,
        mut p: Partial<'static>,
    ) -> Result<Partial<'static>, ArgsErrorKind> {
        let counts = self.counted_stack.pop().unwrap_or_default();
        for (field_index, count) in counts {
            p = p.begin_nth_field(field_index)?;
            p = p.parse_from_str(&count.to_string())?;
            p = p.end()?;
        }
        Ok(p)
    }

    fn increment_counted(&mut self, field_index: usize) {
        if let Some(counts) = self.counted_stack.last_mut() {
            let count = counts.entry(field_index).or_insert(0);
            *count = count.saturating_add(1);
        }
    }

    fn try_handle_counted_long_flag(&mut self, field: &'static Field, field_index: usize) -> bool {
        if is_counted_field(field) && is_supported_counted_type(field.shape()) {
            self.increment_counted(field_index);
            self.index += 1;
            true
        } else {
            false
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
            // For bool flags, check if a value was provided via `=`
            let bool_value = if let Some(value) = value {
                // Parse the value as a boolean
                match value.s.to_lowercase().as_str() {
                    "true" | "yes" | "1" | "on" => true,
                    "false" | "no" | "0" | "off" => false,
                    "" => true, // `--flag=` means true
                    other => {
                        tracing::warn!("Unknown boolean value '{other}', treating as true");
                        true
                    }
                }
            } else {
                // No value provided, presence of flag means true
                true
            };
            tracing::trace!("Flag is boolean, setting it to {bool_value}");
            p = p.set(bool_value)?;

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
        // Check if this is a subcommand field by looking at the current shape
        // If it's an enum and we're trying to parse it from a string, it's likely a subcommand
        if let Type::User(UserType::Enum(_)) = p.shape().ty {
            // This is an enum field being set via a flag value, which is likely a mistake
            // for subcommand fields. Provide a helpful error.
            return Err(ArgsErrorKind::ReflectError(
                facet_reflect::ReflectError::OperationFailed {
                    shape: p.shape(),
                    operation: "Subcommands must be provided as positional arguments, not as flag values. Use the subcommand name directly instead of --flag <value>.",
                },
            ));
        }

        let p = match p.shape().def {
            Def::List(_) => {
                // if it's a list, then we'll want to initialize the list first and push to it
                let mut p = p.init_list()?;
                p = p.begin_list_item()?;
                p = p.parse_from_str(value)?;
                p.end()?
            }
            Def::Option(_) => {
                // if it's an Option<T>, wrap the value in Some
                let mut p = p.begin_some()?;
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
            // Use precise span if the error kind has one, otherwise use the whole arg span
            let span = kind.precise_span().unwrap_or_else(|| {
                if self.index >= self.args.len() {
                    Span::new(self.flattened_args.len(), 0)
                } else {
                    let arg = self.args[self.index];
                    let index = self.arg_indices[self.index];
                    Span::new(index, arg.len())
                }
            });
            ArgsError::new(kind, span)
        })
    }

    #[allow(unsafe_code)]
    fn work_inner(&mut self) -> Result<HeapValue<'static>, ArgsErrorKind> {
        // SAFETY: self.shape comes from T::SHAPE where T: Facet<'static>,
        // which guarantees the shape accurately describes the type.
        let p = unsafe { Partial::alloc_shape(self.shape) }?;

        // Only parse structs at the top level
        // Enums should only be parsed as subcommands when explicitly marked with args::subcommand attribute
        match self.shape.ty {
            Type::User(UserType::Struct(_)) => self.parse_struct(p),
            Type::User(UserType::Enum(_)) => {
                // Enum at top level without explicit subcommand attribute is not supported
                Err(ArgsErrorKind::ReflectError(
                    facet_reflect::ReflectError::OperationFailed {
                        shape: self.shape,
                        operation: "Top-level enums must be wrapped in a struct with #[facet(args::subcommand)] attribute to be used as subcommands.",
                    },
                ))
            }
            _ => Err(ArgsErrorKind::NoFields { shape: self.shape }),
        }
    }

    /// Parse a struct type
    fn parse_struct(
        &mut self,
        mut p: Partial<'static>,
    ) -> Result<HeapValue<'static>, ArgsErrorKind> {
        self.push_counted_scope();

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
                    // Reject flags that start with `-` (e.g., `---verbose`)
                    if flag.starts_with('-') {
                        let fields = self.fields(&p)?;
                        return Err(ArgsErrorKind::UnknownLongFlag {
                            flag: flag.to_string(),
                            fields,
                        });
                    }

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
                            tracing::trace!("Looking up long flag {flag}");
                            let fields = self.fields(&p)?;
                            let Some(field_index) =
                                find_field_index_by_effective_name(fields, flag)
                            else {
                                return Err(ArgsErrorKind::UnknownLongFlag {
                                    flag: flag.to_string(),
                                    fields,
                                });
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            tracing::trace!("Looking up long flag {flag}");
                            let fields = self.fields(&p)?;
                            let Some(field_index) =
                                find_field_index_by_effective_name(fields, flag)
                            else {
                                return Err(ArgsErrorKind::UnknownLongFlag {
                                    flag: flag.to_string(),
                                    fields,
                                });
                            };
                            if !self.try_handle_counted_long_flag(&fields[field_index], field_index)
                            {
                                p = self.handle_field(p, field_index, None)?;
                            }
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

                            let short_char = key.s;
                            tracing::trace!("Looking up short flag {short_char}");
                            let fields = self.fields(&p)?;
                            let Some(field_index) =
                                find_field_index_with_short_char(fields, short_char)
                            else {
                                return Err(ArgsErrorKind::UnknownShortFlag {
                                    flag: short_char.to_string(),
                                    fields,
                                    precise_span: None, // Not chained, use default arg span
                                });
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            // No `=` in the flag. Use helper to handle chaining.
                            let fields = self.fields(&p)?;
                            p = self.process_short_flag(p, flag, flag_span, fields)?;
                        }
                    }
                }
                ArgType::Positional => {
                    let fields = self.fields(&p)?;

                    // First, check if there's a subcommand field that hasn't been set yet
                    if let Some((field_index, field)) = find_subcommand_field(fields)
                        && !p.is_field_set(field_index)?
                    {
                        p = self.handle_subcommand_field(p, field_index, field)?;
                        continue;
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
                        return Err(ArgsErrorKind::UnexpectedPositionalArgument { fields });
                    };

                    p = p.begin_nth_field(chosen_field_index)?;

                    let value = self.args[self.index];

                    // Check if this is an enum field without the subcommand attribute
                    if let Type::User(UserType::Enum(_)) = fields[chosen_field_index].shape().ty
                        && !fields[chosen_field_index].has_attr(Some("args"), "subcommand")
                    {
                        return Err(ArgsErrorKind::EnumWithoutSubcommandAttribute {
                            field: &fields[chosen_field_index],
                        });
                    }

                    p = self.handle_value(p, value)?;

                    p = p.end()?;
                    self.index += 1;
                }
                ArgType::None => todo!(),
            }
        }

        p = self.pop_and_apply_counted_fields(p)?;
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

        // Flatten newtype tuple variants: `Build(BuildArgs)` -> parse BuildArgs fields directly
        if variant.data.kind == StructKind::TupleStruct && fields.len() == 1 {
            let inner_shape = fields[0].shape();
            if let Type::User(UserType::Struct(struct_type)) = inner_shape.ty {
                p = p.begin_nth_field(0)?;
                p = self.parse_fields_loop(p, struct_type.fields)?;
                p = self.pop_and_apply_counted_fields(p)?;
                p = self.finalize_variant_fields(p, struct_type.fields)?;
                p = p.end()?;
                return Ok(p);
            }
        }

        self.push_counted_scope();

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
                    // Reject flags that start with `-` (e.g., `---verbose`)
                    if flag.starts_with('-') {
                        return Err(ArgsErrorKind::UnknownLongFlag {
                            flag: flag.to_string(),
                            fields,
                        });
                    }

                    let flag_span = Span::new(arg_span.start + 2, arg_span.len - 2);
                    match split(flag, flag_span) {
                        Some(tokens) => {
                            let mut tokens = tokens.into_iter();
                            let key = tokens.next().unwrap();
                            let value = tokens.next().unwrap();

                            let flag = key.s;
                            tracing::trace!("Looking up long flag {flag} in variant");
                            let Some(field_index) =
                                find_field_index_by_effective_name(fields, flag)
                            else {
                                return Err(ArgsErrorKind::UnknownLongFlag {
                                    flag: flag.to_string(),
                                    fields,
                                });
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            tracing::trace!("Looking up long flag {flag} in variant");
                            let Some(field_index) =
                                find_field_index_by_effective_name(fields, flag)
                            else {
                                return Err(ArgsErrorKind::UnknownLongFlag {
                                    flag: flag.to_string(),
                                    fields,
                                });
                            };
                            if !self.try_handle_counted_long_flag(&fields[field_index], field_index)
                            {
                                p = self.handle_field(p, field_index, None)?;
                            }
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

                            let short_char = key.s;
                            tracing::trace!("Looking up short flag {short_char} in variant");
                            let Some(field_index) =
                                find_field_index_with_short_char(fields, short_char)
                            else {
                                return Err(ArgsErrorKind::UnknownShortFlag {
                                    flag: short_char.to_string(),
                                    fields,
                                    precise_span: None, // Not chained, use default arg span
                                });
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            // No `=` in the flag. Use helper to handle chaining.
                            p = self.process_short_flag(p, flag, flag_span, fields)?;
                        }
                    }
                }
                ArgType::Positional => {
                    // Check for subcommand field first (for nested subcommands)
                    if let Some((field_index, field)) = find_subcommand_field(fields)
                        && !p.is_field_set(field_index)?
                    {
                        p = self.handle_subcommand_field(p, field_index, field)?;
                        continue;
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
                        return Err(ArgsErrorKind::UnexpectedPositionalArgument { fields });
                    };

                    p = p.begin_nth_field(chosen_field_index)?;
                    let value = self.args[self.index];

                    // Check if this is an enum field without the subcommand attribute
                    if let Type::User(UserType::Enum(_)) = fields[chosen_field_index].shape().ty
                        && !fields[chosen_field_index].has_attr(Some("args"), "subcommand")
                    {
                        return Err(ArgsErrorKind::EnumWithoutSubcommandAttribute {
                            field: &fields[chosen_field_index],
                        });
                    }

                    p = self.handle_value(p, value)?;
                    p = p.end()?;
                    self.index += 1;
                }
                ArgType::None => todo!(),
            }
        }

        // Finalize variant fields
        p = self.pop_and_apply_counted_fields(p)?;
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

        // Check if the next argument (if it exists) is a help flag for this subcommand
        if self.index < self.args.len() && is_help_flag(self.args[self.index]) {
            // Generate help for this specific subcommand variant
            let help_text = crate::help::generate_subcommand_help(
                variant,
                "command", // This would ideally be the program name, but we don't have it in Context
                &HelpConfig::default(),
            );
            return Err(ArgsErrorKind::HelpRequested { help_text });
        }

        if is_optional {
            // Set Option to Some(variant)
            p = p.begin_some()?;
        }

        // Select the variant
        p = p.select_variant_named(variant.effective_name())?;

        // Parse the variant's fields
        p = self.parse_variant_fields(p, variant)?;

        if is_optional {
            p = p.end()?; // end Some
        }

        p = p.end()?; // end field

        Ok(p)
    }

    /// Parse fields from an explicit slice (used for flattened tuple variant structs)
    fn parse_fields_loop(
        &mut self,
        mut p: Partial<'static>,
        fields: &'static [Field],
    ) -> Result<Partial<'static>, ArgsErrorKind> {
        self.push_counted_scope();

        while self.args.len() > self.index {
            let arg = self.args[self.index];
            let arg_span = Span::new(self.arg_indices[self.index], arg.len());
            let at = if self.positional_only {
                ArgType::Positional
            } else {
                ArgType::parse(arg)
            };
            tracing::trace!("Parsing flattened struct field, arg: {at:?}");

            match at {
                ArgType::DoubleDash => {
                    self.positional_only = true;
                    self.index += 1;
                }
                ArgType::LongFlag(flag) => {
                    if flag.starts_with('-') {
                        return Err(ArgsErrorKind::UnknownLongFlag {
                            flag: flag.to_string(),
                            fields,
                        });
                    }

                    let flag_span = Span::new(arg_span.start + 2, arg_span.len - 2);
                    match split(flag, flag_span) {
                        Some(tokens) => {
                            let mut tokens = tokens.into_iter();
                            let key = tokens.next().unwrap();
                            let value = tokens.next().unwrap();

                            let flag = key.s;
                            let Some(field_index) =
                                find_field_index_by_effective_name(fields, flag)
                            else {
                                return Err(ArgsErrorKind::UnknownLongFlag {
                                    flag: flag.to_string(),
                                    fields,
                                });
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            let Some(field_index) =
                                find_field_index_by_effective_name(fields, flag)
                            else {
                                return Err(ArgsErrorKind::UnknownLongFlag {
                                    flag: flag.to_string(),
                                    fields,
                                });
                            };
                            if !self.try_handle_counted_long_flag(&fields[field_index], field_index)
                            {
                                p = self.handle_field(p, field_index, None)?;
                            }
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

                            let short_char = key.s;
                            let Some(field_index) =
                                find_field_index_with_short_char(fields, short_char)
                            else {
                                return Err(ArgsErrorKind::UnknownShortFlag {
                                    flag: short_char.to_string(),
                                    fields,
                                    precise_span: None,
                                });
                            };
                            p = self.handle_field(p, field_index, Some(value))?;
                        }
                        None => {
                            p = self.process_short_flag(p, flag, flag_span, fields)?;
                        }
                    }
                }
                ArgType::Positional => {
                    // Look for a positional field
                    let mut chosen_field_index: Option<usize> = None;
                    for (field_index, field) in fields.iter().enumerate() {
                        let is_positional = field.has_attr(Some("args"), "positional");
                        if !is_positional {
                            continue;
                        }

                        // If it's a list, we can keep appending to it even if already set.
                        // Otherwise, skip fields that are already set.
                        if matches!(field.shape().def, Def::List(_)) {
                            // List field - can accept multiple positional arguments
                        } else if p.is_field_set(field_index)? {
                            continue;
                        }

                        chosen_field_index = Some(field_index);
                        break;
                    }

                    if let Some(field_index) = chosen_field_index {
                        let value = SplitToken {
                            s: arg,
                            span: arg_span,
                        };
                        p = self.handle_field(p, field_index, Some(value))?;
                    } else {
                        return Err(ArgsErrorKind::UnexpectedPositionalArgument { fields });
                    }
                }
                ArgType::None => todo!(),
            }
        }
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
                    return Err(ArgsErrorKind::MissingSubcommand {
                        variants: get_variants_from_shape(field_shape),
                    });
                }
            }

            if is_counted_field(field) && is_supported_counted_type(field.shape()) {
                // Counted fields default to 0 if not incremented
                p = p.begin_nth_field(field_index)?;
                p = p.parse_from_str("0")?;
                p = p.end()?;
            } else if field.has_default() {
                tracing::trace!("Setting #{field_index} field to default: {field:?}");
                p = p.set_nth_field_to_default(field_index)?;
            } else if field.shape().is_shape(bool::SHAPE) {
                // bools are just set to false
                p = p.set_nth_field(field_index, false)?;
            } else if let Def::Option(_) = field.shape().def {
                // Option<T> fields default to None
                p = p.set_nth_field_to_default(field_index)?;
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

            // Check if it's a subcommand field
            if field.has_attr(Some("args"), "subcommand") {
                let field_shape = field.shape();
                if let Def::Option(_) = field_shape.def {
                    // Optional subcommand, set to None using default
                    p = p.set_nth_field_to_default(field_index)?;
                    continue;
                } else {
                    // Required subcommand missing
                    return Err(ArgsErrorKind::MissingSubcommand {
                        variants: get_variants_from_shape(field_shape),
                    });
                }
            }

            if is_counted_field(field) && is_supported_counted_type(field.shape()) {
                // Counted fields default to 0 if not incremented
                p = p.begin_nth_field(field_index)?;
                p = p.parse_from_str("0")?;
                p = p.end()?;
            } else if field.has_default() {
                tracing::trace!("Setting variant field #{field_index} to default: {field:?}");
                p = p.set_nth_field_to_default(field_index)?;
            } else if field.shape().is_shape(bool::SHAPE) {
                p = p.set_nth_field(field_index, false)?;
            } else if let Def::Option(_) = field.shape().def {
                // Option<T> fields default to None
                p = p.set_nth_field_to_default(field_index)?;
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
        if let Some(attr) = variant.get_builtin_attr("rename")
            && let Some(rename) = attr.get_as::<&str>()
            && *rename == name
        {
            return Ok(variant);
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
        provided: name.to_string(),
        variants: enum_type.variants,
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

impl<'input> Context<'input> {
    /// Process a short flag that may contain chained flags or an attached value.
    ///
    /// This function handles three cases:
    /// 1. Single flag: `-v` → process as bool or look for value in next arg
    /// 2. Chained bool flags: `-abc` → recursively process `-a`, then `-bc`, then `-c`
    /// 3. Attached value: `-j4` → process `-j` with value `4`
    ///
    /// The function is recursive for chained flags, maintaining proper span tracking
    /// and index management. Only increments `self.index` at the leaf of recursion.
    fn process_short_flag(
        &mut self,
        mut p: Partial<'static>,
        flag: &'input str,
        flag_span: Span,
        fields: &'static [Field],
    ) -> Result<Partial<'static>, ArgsErrorKind> {
        // Get the first character as the flag
        let first_char = flag.chars().next().unwrap();
        let first_char_str = &flag[..first_char.len_utf8()];
        let rest = &flag[first_char.len_utf8()..];

        tracing::trace!("Looking up short flag '{first_char}' (rest: '{rest}')");

        // Look up the field for this character
        let Some(field_index) = find_field_index_with_short_char(fields, first_char_str) else {
            // Error: unknown flag, report just the first character with precise span
            let char_span = Span::new(flag_span.start, first_char.len_utf8());
            return Err(ArgsErrorKind::UnknownShortFlag {
                flag: first_char_str.to_string(),
                fields,
                precise_span: Some(char_span),
            });
        };

        let field = &fields[field_index];
        let field_shape = field.shape();

        // Check if the field is bool or Vec<bool>
        let is_bool = field_shape.is_shape(bool::SHAPE);
        let is_bool_list = if let facet_core::Def::List(list_def) = field_shape.def {
            list_def.t.is_shape(bool::SHAPE)
        } else {
            false
        };
        let is_counted = is_counted_field(field) && is_supported_counted_type(field_shape);

        if rest.is_empty() {
            // Leaf case: last character in the chain
            if is_counted {
                self.increment_counted(field_index);
                self.index += 1;
            } else if is_bool || is_bool_list {
                // Bool or Vec<bool> at the end of chain
                p = p.begin_nth_field(field_index)?;

                if is_bool_list {
                    // For Vec<bool> fields, initialize list and push an item
                    p = p.init_list()?;
                    p = p.begin_list_item()?;
                    p = p.set(true)?;
                    p = p.end()?; // end list item
                } else {
                    // For simple bool fields, just set to true
                    p = p.set(true)?;
                }

                p = p.end()?; // end field
                self.index += 1; // Move to next arg
            } else {
                // Non-bool field: use handle_field which looks for value in next arg
                p = self.handle_field(p, field_index, None)?;
            }
        } else if is_counted {
            // Counted flag with trailing chars: `-vvv` increments for each `v`
            self.increment_counted(field_index);
            let rest_span = Span::new(flag_span.start + first_char.len_utf8(), rest.len());
            p = self.process_short_flag(p, rest, rest_span, fields)?;
        } else if is_bool || is_bool_list {
            // Bool flag with trailing chars: could be chaining like `-abc` or `-vvv`
            // Process current bool flag without going through handle_field
            // (which would increment index and consume next arg)
            p = p.begin_nth_field(field_index)?;

            if is_bool_list {
                // For Vec<bool> fields, we need to initialize the list and push an item
                p = p.init_list()?;
                p = p.begin_list_item()?;
                p = p.set(true)?;
                p = p.end()?; // end list item
            } else {
                // For simple bool fields, just set to true
                p = p.set(true)?;
            }

            p = p.end()?; // end field

            // Recursively process remaining characters as a new short flag chain
            let rest_span = Span::new(flag_span.start + first_char.len_utf8(), rest.len());
            p = self.process_short_flag(p, rest, rest_span, fields)?;
            // Note: index increment happens in the leaf recursion
        } else {
            // Non-bool flag with attached value: `-j4`
            let value_span = Span::new(flag_span.start + first_char.len_utf8(), rest.len());
            p = self.handle_field(
                p,
                field_index,
                Some(SplitToken {
                    s: rest,
                    span: value_span,
                }),
            )?;
        }

        Ok(p)
    }
}

/// Given an array of fields, find the field with the given `args::short = 'a'`
/// annotation. Uses extension attribute syntax: #[facet(args::short = "j")]
/// The `short` parameter should be a single character (as a string slice).
fn find_field_index_with_short_char(fields: &'static [Field], short: &str) -> Option<usize> {
    let short_char = short.chars().next()?;
    fields.iter().position(|f| {
        if let Some(ext) = f.get_attr(Some("args"), "short") {
            // The attribute stores the full Attr enum
            if let Some(crate::Attr::Short(opt_char)) = ext.get_as::<crate::Attr>() {
                match opt_char {
                    Some(c) => *c == short_char,
                    None => {
                        // No explicit short specified, use first char of effective name
                        // (effective_name returns rename if set, otherwise the field name)
                        f.effective_name().starts_with(short_char)
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

/// Find the field index by matching against the effective name (respects rename attribute).
/// The `flag` parameter should be a kebab-case CLI flag name.
fn find_field_index_by_effective_name(fields: &'static [Field], flag: &str) -> Option<usize> {
    let snek = flag.to_snake_case();
    fields
        .iter()
        .position(|f| f.effective_name().to_snake_case() == snek)
}
