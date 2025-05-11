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
}

impl Toml {
    /// Instantiate the format from a parsed TOML document.
    pub fn new(root: Item) -> Self {
        let stack = Vec::new();

        Self { root, stack }
    }

    /// Get the last item.
    fn item(&self) -> &'_ Item {
        self.stack.iter().fold(&self.root, move |item, key| {
            item.as_table_like().unwrap().get(key).unwrap()
        })
    }

    /// Apply a function on a the current item.
    ///
    /// Can't return a mutable reference due to lifetime issues.
    fn apply_on_item(&mut self, func: impl Fn(&mut Item)) {
        todo!()
    }

    /// Pop the last item and remove it from the document.
    fn pop(&mut self) {
        let last_key = self.stack.pop().unwrap();

        self.apply_on_item(|item| {
            item.as_table_like_mut().unwrap().remove(&last_key).unwrap();
        });
    }

    /// Get the span of an item.
    fn item_span<'input, 'facet>(&self, item: &Item, next: &NextData<'input, 'facet>) -> Span {
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

        eprintln!("{}, {expectation:?}", item.type_name());

        let res = match (&item, expectation) {
            (Item::Value(_value), Expectation::ObjectKeyOrObjectClose) => {
                self.pop();

                Spanned {
                    node: Outcome::ObjectEnded,
                    span,
                }
            }
            (Item::Value(value), Expectation::ObjectVal) => {
                // There is a another field, go to it
                // *self.stack.last_mut().unwrap() += 1;

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
            (Item::Table(table), Expectation::Value) => {
                if let Some((key, _)) = table.iter().next() {
                    self.stack.push(key.to_string());
                }

                Spanned {
                    node: Outcome::ObjectStarted,
                    span,
                }
            }
            (Item::Table(table), Expectation::ObjectKeyOrObjectClose) => {
                // TODO: get next item
                let key = self.stack.last().unwrap();
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
