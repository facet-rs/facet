#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

// cf. facet-toml/facet-json for examples

use std::{
    error::Error,
    fmt::{self, Display},
};

use facet_core::{Def, Facet, FieldFlags};
use facet_reflect::{ReflectError, Wip};
use kdl::{KdlDocument, KdlError as KdlParseError};

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
    InvalidDocumentShape(Def),
    MissingNodes(Vec<String>),
    Parse(KdlParseError),
    Reflect(ReflectError),
}

impl Display for KdlErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KdlErrorKind::InvalidDocumentShape(def) => {
                write!(f, "invalid shape {def:#?} — needed... TODO")
            }
            KdlErrorKind::MissingNodes(expected) => write!(f, "failed to find node {expected:?}"),
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
        // PERF: Would be be better / quicker if I did this parsing incrementally? Using information from the `Wip` to
        // decide when to call `KdlNode::parse` and `KdlEntry::parse`? Probably would be if I'm only trying to parse
        // some of the KDL text, but I'm not so sure otherwise? Will need benchmarking...
        let document: KdlDocument = dbg!(kdl.parse()?);
        log::trace!("KDL parsed");

        let mut wip = Wip::alloc::<T>().expect("failed to allocate");
        log::trace!("Allocated WIP for type {}", wip.shape());

        wip = Self { kdl }.deserialize_document(wip, document)?;

        let heap_value = wip.build()?;
        log::trace!("WIP fully built");
        let result = heap_value.materialize::<T>()?;
        log::trace!("Type of WIP unerased");

        Ok(result)
    }

    fn deserialize_document(
        &mut self,
        wip: Wip<'facet>,
        document: KdlDocument,
    ) -> Result<Wip<'facet>> {
        log::trace!("Entering `deserialize_document` method");
        let def = wip.shape().def;
        match def {
            // FIXME: Refactor this checking code out into some functions!
            Def::Struct(struct_def) => {
                log::trace!("Document `Wip` is a struct: {struct_def:#?}");
                // QUESTION: Would be be possible, once we allow custom types, to make all attributes arbitrary? With
                // the sort of general tool that `facet` is, I think it might actually be best if we didn't try to
                // "bake-in" anything like sensitive, default, skip, etc...
                let is_valid_toplevel = struct_def
                    .fields
                    .iter()
                    .all(|field| field.flags.contains(FieldFlags::CHILD));
                log::trace!("WIP represents a valid top-level: {is_valid_toplevel}");

                if is_valid_toplevel {
                    // FIXME: At this point I'm really not sure where function boundaries should be... It's a messy disaster
                    // whilst I try to work that out...
                    // FIXME: For example, this feels like maybe it should take a `KdlNode` and not a `KdlDocument`?
                    self.deserialize_node(wip, document)
                } else {
                    Err(KdlErrorKind::InvalidDocumentShape(def).into())
                }
            }
            // TODO: Valid if the list contains only enums with single fields that can be parsed as entries?
            Def::List(_list_def) => todo!(),
            _ => todo!(),
        }
    }

    fn deserialize_node(
        &mut self,
        mut wip: Wip<'facet>,
        mut document: KdlDocument,
    ) -> Result<Wip<'facet>> {
        log::trace!("Entering `deserialize_node` method");

        // TODO: Correctly generate that error and write a constructor that gets rid of the `.to_owned()`?
        let node = document
            .nodes_mut()
            .pop()
            .ok_or_else(|| KdlErrorKind::MissingNodes(vec!["TODO".to_owned()]))?;
        log::trace!("Popped node from `KdlDocument`: {node:#?}");

        wip = wip.field_named(&node.name().to_string())?;
        log::trace!(
            "Node matched expected child; New def: {:#?}",
            wip.shape().def
        );

        // TODO: Planning to step through those entries one at a time then dispatch a method like
        // `deserialize_argument()` or `deserialize_property()` depending on which it is. Then I need a way to keep
        // track of which `Wip` fields have already been filled? I think that shouldn't be too bad, then I can just
        // grab the next "unfilled" argument field if it's an argument, or search all of the (filled or unfilled) fields
        // if it's a parameter?
        for entry in node.entries() {
            log::trace!("Processing entry: {entry:#?}");
        }

        todo!()
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
pub fn from_str<'input, 'facet, T>(kdl: &'input str) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    log::trace!("Entering `from_str` function");

    KdlDeserializer::from_str(kdl)
}
