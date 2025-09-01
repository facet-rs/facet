#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

// cf. facet-toml/facet-json for examples

use std::{
    error::Error,
    fmt::{self, Display},
    mem,
};

use facet_core::{
    Def, Facet, FieldAttribute, FieldFlags, NumericType, PrimitiveType, Shape, ShapeLayout, Type,
    UserType,
};
use facet_reflect::{Partial, ReflectError};
use kdl::{KdlDocument, KdlEntry, KdlError as KdlParseError, KdlNode, KdlValue};

// QUESTION: Any interest in making something a bit like `strum` with `facet`? Always nice to have an easy way to get
// the names of enum variants as strings!

// DESIGN: Like `facet-toml`, this crate currently fully parses KDL into an AST before doing any deserialization. In the
// long-term, I think it's important that the code in `facet-kdl` stays as minimally complex and easy to maintain as
// possible — I'd like to get "free" KDL format / parsing updates from `kdl-rs`, and a "free" derive macro from `facet`.
// For this prototype then, I'm really going to try to avoid any premature optimisation — I'll try to take inspiration
// from `facet-toml` and split things into easy-to-understand functions that I can call recursively as I crawl down the
// KDL AST. After I'm happy with the API and have a really solid set of tests, we can look into making some more
// optimisations, like flattening this recursive structure into something more iterative / imparative (as in
// `facet-json`) or parsing things more incrementally by using `KdlNode::parse()` or `KdlEntry::parse`.

// TODO: Need to actually add some shared information here so it's not just a useless wrapper...

/// Error type for KDL deserialization.
#[derive(Debug)]
pub struct KdlError {
    kind: KdlErrorKind,
}

impl Display for KdlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        let kind = &self.kind;
        write!(f, "{kind}")
    }
}
impl Error for KdlError {}

// FIXME: Replace this with a proper constructor once there is other information to put into `KdlError`!
impl<K: Into<KdlErrorKind>> From<K> for KdlError {
    fn from(value: K) -> Self {
        let kind = value.into();
        KdlError { kind }
    }
}

#[derive(Debug)]
enum KdlErrorKind {
    InvalidDocumentShape(&'static Def),
    Parse(KdlParseError),
    Reflect(ReflectError),
}

impl Display for KdlErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KdlErrorKind::InvalidDocumentShape(def) => {
                write!(f, "invalid shape {def:#?} — needed... TODO")
            }
            KdlErrorKind::Parse(kdl_error) => write!(f, "{kdl_error}"),
            KdlErrorKind::Reflect(reflect_error) => write!(f, "{reflect_error}"),
        }
    }
}

impl From<KdlParseError> for KdlErrorKind {
    fn from(value: KdlParseError) -> Self {
        Self::Parse(value)
    }
}

impl From<ReflectError> for KdlErrorKind {
    fn from(value: ReflectError) -> Self {
        Self::Reflect(value)
    }
}

// FIXME: I'm not sure what to name this...
#[allow(dead_code)]
struct KdlDeserializer<'input> {
    // FIXME: Also no clue what fields it should have, if it should exist at all...
    kdl: &'input str,
}

type Result<T> = std::result::Result<T, KdlError>;

impl<'input, 'facet> KdlDeserializer<'input> {
    fn from_str<T: Facet<'facet>>(kdl: &'input str) -> Result<T> {
        log::trace!("Entering `from_str` method");

        // PERF: This definitely isn't zero-copy, so it might be worth seeing if that's something that can be added to
        // `kdl-rs` at some point in the future?
        // PERF: Would be be better / quicker if I did this parsing incrementally? Using information from the `Partial` to
        // decide when to call `KdlNode::parse` and `KdlEntry::parse`? Probably would be if I'm only trying to parse
        // some of the KDL text, but I'm not so sure otherwise? Will need benchmarking...
        let document: KdlDocument = dbg!(kdl.parse()?);
        log::trace!("KDL parsed");

        let mut typed_partial = Partial::alloc::<T>().expect("failed to allocate");
        log::trace!(
            "Allocated WIP for type {}",
            typed_partial.inner_mut().shape()
        );

        {
            let wip = typed_partial.inner_mut();
            Self { kdl }.deserialize_toplevel_document(wip, document)?;
        }

        let boxed_value = typed_partial.build()?;
        log::trace!("WIP fully built");
        log::trace!("Type of WIP unerased");

        Ok(*boxed_value)
    }

    fn deserialize_toplevel_document(
        &mut self,
        wip: &mut Partial<'facet>,
        document: KdlDocument,
    ) -> Result<()> {
        log::trace!("Entering `deserialize_toplevel_document` method");

        // First check the type system (Type)
        if let Type::User(UserType::Struct(struct_def)) = &wip.shape().ty {
            log::trace!("Document `Partial` is a struct: {struct_def:#?}");
            // QUESTION: Would be be possible, once we allow custom types, to make all attributes arbitrary? With
            // the sort of general tool that `facet` is, I think it might actually be best if we didn't try to
            // "bake-in" anything like sensitive, default, skip, etc...
            let is_valid_toplevel = struct_def.fields.iter().all(|field| {
                field.flags.contains(FieldFlags::CHILD)
                    || field
                        .attributes
                        .contains(&FieldAttribute::Arbitrary("children"))
            });
            log::trace!("WIP represents a valid top-level: {is_valid_toplevel}");

            if is_valid_toplevel {
                return self.deserialize_document(wip, document);
            } else {
                return Err(KdlErrorKind::InvalidDocumentShape(&wip.shape().def).into());
            }
        }

        // Fall back to the def system for backward compatibility
        let def = wip.shape().def;
        match def {
            // TODO: Valid if the list contains only enums with single fields that can be parsed as entries?
            Def::List(_list_def) => todo!(),
            _ => todo!(),
        }
    }

    fn deserialize_document(
        &mut self,
        wip: &mut Partial<'facet>,
        mut document: KdlDocument,
    ) -> Result<()> {
        log::trace!("Entering `deserialize_document` method at {}", wip.path());

        let document_shape = wip.shape();

        let mut in_node_children_list = false;

        for node in document.nodes_mut().drain(..) {
            // log::trace!("Processing node: {node:#?}");
            self.deserialize_node(wip, node, document_shape, &mut in_node_children_list)?;
        }

        if in_node_children_list {
            wip.end()?;
        }

        log::trace!("Exiting `deserialize_document` method at {}", wip.path());

        Ok(())
    }

    fn deserialize_node(
        &mut self,
        wip: &mut Partial<'facet>,
        mut node: KdlNode,
        document_shape: &Shape,
        in_node_children_list: &mut bool,
    ) -> Result<()> {
        log::trace!("Entering `deserialize_node` method at {}", wip.path());

        match document_shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                if let Some(child_field) = struct_def.fields.iter().find(|field| {
                    field.flags.contains(FieldFlags::CHILD) && field.name == node.name().value()
                }) {
                    log::trace!("Node matched expected child {}", child_field.name);
                    if *in_node_children_list {
                        wip.end()?;
                        *in_node_children_list = false;
                    }
                    wip.begin_field(child_field.name)?;
                } else if let Some((children_field_index, children_field)) = struct_def
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_index, field)| {
                        field
                            .attributes
                            .contains(&FieldAttribute::Arbitrary("children"))
                    })
                {
                    log::trace!("Node matched children container");
                    if !*in_node_children_list {
                        if wip.is_field_set(children_field_index)? {
                            todo!("reopening children already completed")
                        }
                        wip.begin_field(children_field.name)?;
                        wip.begin_list()?;
                        *in_node_children_list = true;
                    }
                    wip.begin_list_item()?;
                } else {
                    log::debug!("No fields for child {}", node.name());
                    for field in struct_def.fields {
                        log::debug!(
                            "field {}\tflags {:?}\tattributes {:?}",
                            field.name,
                            field.flags,
                            field.attributes
                        );
                    }
                    todo!()
                }
            }
            ty => todo!("deserialize_node with shape {ty}"),
        }
        log::trace!("New def: {:#?}", wip.shape().def);

        if let Type::User(UserType::Struct(struct_def)) = wip.shape().ty {
            if let Some(node_name_field) = struct_def.fields.iter().find(|field| {
                field
                    .attributes
                    .contains(&FieldAttribute::Arbitrary("node_name"))
            }) {
                wip.set_field(node_name_field.name, node.name().value().to_string())?;
            }
        }

        let node_shape = wip.shape();
        let mut in_entry_arguments_list = false;

        for entry in node.entries_mut().drain(..) {
            log::trace!("Processing entry: {entry:?}");

            self.deserialize_entry(wip, entry, node_shape, &mut in_entry_arguments_list)?;
        }

        if in_entry_arguments_list {
            wip.end()?;
        }

        if let Some(children) = node.children_mut().take() {
            self.deserialize_document(wip, children)?;
        }

        wip.end()?;

        log::trace!("Exiting `deserialize_node` method");

        Ok(())
    }

    fn deserialize_entry(
        &mut self,
        wip: &mut Partial<'facet>,
        mut entry: KdlEntry,
        node_shape: &Shape,
        in_entry_arguments_list: &mut bool,
    ) -> Result<()> {
        log::trace!("Entering `deserialize_entry` method at {}", wip.path());

        if let Some(name) = entry.name() {
            // property
            match node_shape.ty {
                Type::User(UserType::Struct(struct_def)) => {
                    if let Some(matching_field) = struct_def.fields.iter().find(|field| {
                        field
                            .attributes
                            .contains(&FieldAttribute::Arbitrary("property"))
                            && field.name == name.value()
                    }) {
                        wip.begin_field(matching_field.name)?;
                    } else {
                        todo!()
                    }
                }
                _ => todo!(),
            }
        } else {
            // argument
            match node_shape.ty {
                Type::User(UserType::Struct(struct_def)) => {
                    if let Some((_, next_arg_field)) =
                        struct_def.fields.iter().enumerate().find(|(index, field)| {
                            field
                                .attributes
                                .contains(&FieldAttribute::Arbitrary("argument"))
                                && wip.is_field_set(*index).ok() == Some(false)
                        })
                    {
                        if *in_entry_arguments_list {
                            todo!("argument after arguments")
                        }
                        wip.begin_field(next_arg_field.name)?;
                    } else if let Some((args_field_index, args_field)) =
                        struct_def.fields.iter().enumerate().find(|(_, field)| {
                            field
                                .attributes
                                .contains(&FieldAttribute::Arbitrary("arguments"))
                        })
                    {
                        if !*in_entry_arguments_list {
                            if wip.is_field_set(args_field_index)? {
                                todo!("reopening arguments already completed")
                            }
                            wip.begin_field(args_field.name)?;
                            wip.begin_list()?;
                            *in_entry_arguments_list = true;
                        }
                        wip.begin_list_item()?;
                    } else {
                        log::debug!("No fields for argument");
                        for field in struct_def.fields {
                            log::debug!(
                                "field {}\tattributes {:?}\tis_field_set {:?}",
                                field.name,
                                field.attributes,
                                wip.is_field_set(field.offset)
                            );
                        }
                        todo!()
                    }
                }
                _ => todo!(),
            }
        }
        let self1 = entry.value_mut();
        self.deserialize_value(wip, mem::replace(self1, KdlValue::Null))?;
        wip.end()?;

        log::trace!("Exiting `deserialize_entry` method");

        Ok(())
    }

    fn deserialize_value(&mut self, wip: &mut Partial<'facet>, value: KdlValue) -> Result<()> {
        log::trace!("Entering `deserialize_value` method at {}", wip.path());

        log::trace!("Parsing {:?} into {}", &value, wip.path());

        enum Cleanup {
            None,
            End,
        }

        let cleanup = match wip.shape().def {
            Def::Scalar => Cleanup::None,
            Def::Option(_) => {
                if value == KdlValue::Null {
                    wip.set_default()?;
                    Cleanup::None
                } else {
                    wip.begin_some()?;
                    Cleanup::End
                }
            }
            def => todo!("handle {def:?}"),
        };

        match value {
            KdlValue::String(string) => {
                wip.set(string)?;
            }
            KdlValue::Integer(integer) => {
                let size = match wip.shape().layout {
                    ShapeLayout::Sized(layout) => layout.size(),
                    ShapeLayout::Unsized => todo!(),
                };
                let ty = match wip.shape().ty {
                    Type::Primitive(PrimitiveType::Numeric(ty)) => ty,
                    _ => todo!(),
                };
                match (ty, size) {
                    (NumericType::Integer { signed: false }, 1) => wip.set(integer as u8)?,
                    _ => todo!(),
                };
            }
            KdlValue::Float(float) => {
                wip.set(float)?;
            }
            KdlValue::Bool(bool) => {
                wip.set(bool)?;
            }
            KdlValue::Null => match wip.shape().def {
                Def::Option(_) => {}
                _ => todo!(),
            },
        };

        match cleanup {
            Cleanup::None => {}
            Cleanup::End => {
                wip.end()?;
            }
        }

        log::trace!("Exiting `deserialize_value` method");

        Ok(())
    }
}

/// Deserialize a value of type `T` from a KDL string.
///
/// Returns a [`KdlError`] if the input KDL is invalid or doesn't match `T`.
///
/// # Example
/// ```ignore
/// let kdl = r#"
/// my_struct {
///     field1 "value"
///     field2 42
/// }
/// "#;
/// let val: MyStruct = from_str(kdl)?;
/// ```
pub fn from_str<'input, 'facet: 'shape, 'shape, T>(kdl: &'input str) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    log::trace!("Entering `from_str` function");

    KdlDeserializer::from_str(kdl)
}
