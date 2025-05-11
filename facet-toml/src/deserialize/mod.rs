//! Parse TOML strings into Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod error;
mod to_scalar;

use core::borrow::Borrow as _;

use alloc::{
    borrow::Cow,
    string::{String, ToString},
};
pub use error::{TomlDeError, TomlDeErrorKind};
use facet_core::Facet;
use facet_deserialize::{
    DeserError, DeserErrorKind, Expectation, Format, NextData, NextResult, Outcome, Scalar, Span,
    Spanned,
};
use log::trace;
use toml_edit::{DocumentMut, ImDocument, Item, TableLike, TomlError, Value};
use yansi::Paint as _;

/// TOML deserialization format.
pub struct Toml {
    /// Root item.
    root: Item,
    /// Current stack of where we are in the tree.
    stack: Vec<String>,
    /// Whether the current item is uninitialized.
    first: bool,
}

impl Toml {
    /// Instantiate the format from a parsed TOML document.
    pub fn new(root: Item) -> Self {
        let stack = Vec::new();
        let first = false;

        Self { root, stack, first }
    }

    /// Get the last item.
    fn item(&self) -> &Item {
        self.stack.iter().fold(&self.root, move |item, key| {
            item.as_table_like().unwrap().get(key).unwrap()
        })
    }

    /// Get a mutable reference to the last item.
    fn item_mut(&mut self) -> &mut Item {
        self.stack.iter().fold(&mut self.root, move |item, key| {
            item.as_table_like_mut().unwrap().get_mut(key).unwrap()
        })
    }

    /// Pop the last item and remove it from the document.
    fn pop_and_remove(&mut self) {
        let last_key = self.stack.pop().unwrap();

        trace!("Removing key '{last_key}' from table");

        let parent_item = self.item_mut();
        parent_item
            .as_table_like_mut()
            .unwrap()
            .remove(&last_key)
            .unwrap();
    }

    /// Pop the last item and remove it from the document, then push the next child of the parent item if there's still one.
    ///
    /// # Returns
    ///
    /// - `true` if a sibling is pushed.
    /// - `false` if the parent item is the new current item.
    fn pop_and_push_next_sibling_if_exists(&mut self) -> bool {
        // Pop the last item
        let last_key = self.stack.pop().unwrap();

        // Remove the last item from the parent item
        let parent_item = self.item_mut();
        let parent_table = parent_item.as_table_like_mut().unwrap();
        parent_table.remove(&last_key).unwrap();

        // Push the next child if there's still one
        let maybe_next_field_key = parent_table
            .iter()
            .next()
            .map(|(key, _item)| key.to_string());
        if let Some(key) = maybe_next_field_key {
            self.stack.push(key);

            true
        } else {
            false
        }
    }

    /// Push the next child of the current item if there's still one.
    ///
    /// # Returns
    ///
    /// - `true` if a child is pushed.
    /// - `false` if nothing is done.
    fn push_child_if_exists(&mut self) -> bool {
        // Remove the last item from the parent item
        let parent_item = self.item_mut();
        let Some(parent_table) = parent_item.as_table_like_mut() else {
            return false;
        };

        // Push the next child if there's still one
        let maybe_next_field_key = parent_table
            .iter()
            .next()
            .map(|(key, _item)| key.to_string());
        if let Some(key) = maybe_next_field_key {
            self.stack.push(key);

            self.first = true;

            true
        } else {
            false
        }
    }

    /// Whether the last item is a table type and has more than zero fields.
    fn is_table_with_fields(&self) -> bool {
        self.item()
            .as_table_like()
            .is_some_and(|table| !table.is_empty())
    }

    /// Get the span of an item or the whole document when it doesn't have one.
    fn item_span(&self, item: &Item, next: &NextData<'_, '_>) -> Span {
        item.span().map_or_else(
            || next.document_span(),
            |range| Span::new(range.start, range.end),
        )
    }
}

impl Format for Toml {
    fn next<'input, 'facet>(
        &mut self,
        next: NextData<'input, 'facet>,
        expectation: Expectation,
    ) -> NextResult<'input, 'facet, Spanned<Outcome<'input>>, Spanned<DeserErrorKind>> {
        let item = self.item();
        // Convert the TOML span to a facet span
        let span = self.item_span(item, &next);

        eprint!("{}, {expectation:?}", item.type_name());

        let res = match (item, expectation) {
            (Item::Value(value), Expectation::ObjectVal) => {
                let node = match value {
                    Value::String(formatted) => {
                        Scalar::String(formatted.value().to_owned().into()).into()
                    }
                    Value::Integer(formatted) => Scalar::I64(*formatted.value()).into(),
                    Value::Float(formatted) => Scalar::F64(*formatted.value()).into(),
                    Value::Boolean(formatted) => Scalar::Bool(*formatted.value()).into(),
                    value => {
                        // Throw unimplemented error
                        return (
                            next,
                            Err(Spanned {
                                node: DeserErrorKind::Unimplemented(value.type_name()),
                                span,
                            }),
                        );
                    }
                };

                Spanned { node, span }
            }
            // Push the child of the current table as a new child object value
            (Item::Table(_table), Expectation::ObjectVal) => {
                assert!(self.push_child_if_exists());

                Spanned {
                    node: Outcome::ObjectStarted,
                    span,
                }
            }
            (Item::Table(table), Expectation::Value) => {
                // Try to get the next field
                let key = table.iter().next().map(|(key, _)| key.to_string());

                let node = if let Some(key) = key {
                    // If there is a field push the key for it on the stack
                    self.stack.push(key);
                    self.first = true;

                    Outcome::ObjectStarted
                } else {
                    // No field, that means the object is finished
                    todo!()
                };

                Spanned { node, span }
            }
            // Push the key, or close the object when done
            (_, Expectation::ObjectKeyOrObjectClose) => {
                let node = if self.first || self.pop_and_push_next_sibling_if_exists() {
                    // It's a field, push the key
                    self.first = false;

                    Scalar::String(self.stack.last().unwrap().clone().into()).into()
                } else {
                    // No more fields
                    Outcome::ObjectEnded
                };

                Spanned { node, span }
            }
            (item, expectation) => panic!("Unimplemented {}/{:?}", item.type_name(), expectation),
        };

        eprintln!("-> {}", res.node);

        (next, Ok(res))
    }

    fn skip<'input, 'facet>(
        &mut self,
        nd: NextData<'input, 'facet>,
    ) -> NextResult<'input, 'facet, Span, Spanned<DeserErrorKind>> {
        todo!()
    }
}

/// Deserializes a TOML string into a value of type `T` that implements `Facet`.
pub fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input str,
) -> Result<T, TomlDeError<'input>> {
    // Parse the TOML document
    let document: ImDocument<String> = input
        .parse()
        .map_err(|e: TomlError| {
            TomlDeError::new(
                input,
                TomlDeErrorKind::GenericTomlError(e.message().to_string()),
                e.span(),
                String::new(),
            )
        })
        // TODO: handle error
        .unwrap();

    // TODO: remove this clone
    let item = document.as_item().clone();

    facet_deserialize::deserialize(input.as_bytes(), Toml::new(item)).map_err(|err| {
        eprintln!("{err}");
        TomlDeError::new(
            input,
            TomlDeErrorKind::ExpectedExactlyOneField,
            None,
            String::new(),
        )
    })
}
