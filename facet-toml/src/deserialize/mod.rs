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
use toml_edit::{ImDocument, Item, TomlError, Value};
use yansi::Paint as _;

/// TOML deserialization format.
pub struct Toml<'doc> {
    /// Document.
    document: &'doc ImDocument<String>,
    /// Current stack of where we are in the tree.
    ///
    /// Number represents the index into a table.
    stack: Vec<usize>,
}

impl<'doc> Toml<'doc> {
    /// Instantiate the format from a parsed TOML document.
    pub fn new(document: &'doc ImDocument<String>) -> Self {
        let stack = Vec::new();

        Self { document, stack }
    }

    /// Get the last item.
    fn item(&self) -> (&'doc str, &'doc Item) {
        // TODO: don't use numeric iterators
        let mut item = self.document.as_item();
        let mut key = "";
        for index in &self.stack {
            (key, item) = item.as_table_like().unwrap().iter().nth(*index).unwrap();
        }

        (key, item)
    }

    /// Get the next sibling to the last item.
    fn next_sibling(&self) -> Option<(&'doc str, &'doc Item)> {
        let mut item = self.document.as_item();

        // Go up until the second last item
        for index in &self.stack[0..self.stack.len() - 1] {
            item = item.as_table_like().unwrap().iter().nth(*index).unwrap().1;
        }

        // Take the last item with the incremented index
        item.as_table_like().and_then(|table| {
            table
                .iter()
                .nth(self.stack.last().cloned().unwrap_or_default() + 1)
        })
    }

    /// Get the span of an item.
    fn item_span<'input, 'facet>(&self, item: &'doc Item, next: &NextData<'input, 'facet>) -> Span {
        item.span().map_or_else(
            || Span::new(next.start(), next.input().len()),
            |range| Span::new(range.start, range.end),
        )
    }
}

impl Format for Toml<'_> {
    fn next<'input, 'facet>(
        &mut self,
        next: NextData<'input, 'facet>,
        expectation: Expectation,
    ) -> NextResult<'input, 'facet, Spanned<Outcome<'input>>, Spanned<DeserErrorKind>> {
        let (key, item) = self.item();

        // Convert the TOML span to a facet span
        let span = self.item_span(item, &next);

        eprintln!("{}, {expectation:?}", item.type_name());

        let res = match (&item, expectation) {
            (Item::Value(_value), Expectation::ObjectKeyOrObjectClose) => {
                match self.next_sibling() {
                    None => {
                        // Object is closed
                        self.stack.pop();

                        Spanned {
                            node: Outcome::ObjectEnded,
                            span,
                        }
                    }
                    Some((next_key, next_item)) => {
                        // Key
                        Spanned {
                            node: Scalar::String(next_key.to_owned().into()).into(),
                            span: self.item_span(next_item, &next),
                        }
                    }
                }
            }
            (Item::Value(value), Expectation::ObjectVal) => {
                // There is a another field, go to it
                *self.stack.last_mut().unwrap() += 1;

                match value {
                    Value::String(formatted) => Spanned {
                        node: Scalar::String(formatted.value().to_owned().into()).into(),
                        span,
                    },
                    Value::Integer(formatted) => Spanned {
                        node: Scalar::I64(*formatted.value()).into(),
                        span,
                    },
                    value => panic!("Unimplemented {}", value.type_name()),
                }
            }
            (Item::Table(_table), Expectation::Value) => {
                self.stack.push(0);

                Spanned {
                    node: Outcome::ObjectStarted,
                    span,
                }
            }
            (Item::Table(table), Expectation::ObjectKeyOrObjectClose) => {
                // Key
                Spanned {
                    node: Scalar::String(key.to_owned().into()).into(),
                    span,
                }
            }
            (item, expectation) => panic!("Unimplemented {}/{:?}", item.type_name(), expectation),
        };

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

    facet_deserialize::deserialize(input.as_bytes(), Toml::new(&document)).map_err(|err| {
        eprintln!("{err}");
        TomlDeError::new(
            input,
            TomlDeErrorKind::ExpectedExactlyOneField,
            None,
            String::new(),
        )
    })
}
