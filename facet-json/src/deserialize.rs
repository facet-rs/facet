use facet_core::{Def, Facet};
use facet_reflect::{HeapValue, Wip};
use log::trace;
use owo_colors::OwoColorize;

mod tokenizer;
pub use tokenizer::*;

mod error;
pub use error::*;

/// Deserializes a JSON string into a value of type `T` that implements `Facet`.
///
/// This function takes a JSON string representation and converts it into a Rust
/// value of the specified type `T`. The type must implement the `Facet` trait
/// to provide the necessary type information for deserialization.
pub fn from_str<T: Facet>(json: &str) -> Result<T, JsonError<'_>> {
    from_slice(json.as_bytes())
}

/// Deserialize JSON from a slice
///
/// # Arguments
///
/// * `json` - A slice of bytes representing the JSON input.
///
/// # Returns
///
/// A result containing the deserialized value of type `T` or a `JsonParseErrorWithContext`.
pub fn from_slice<T: Facet>(json: &[u8]) -> Result<T, JsonError<'_>> {
    let wip = Wip::alloc::<T>();
    let heap_value = from_slice_wip(wip, json)?;
    Ok(heap_value.materialize::<T>().unwrap())
}

/// Represents the next expected token or structure while parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    Value,
    ObjectKeyOrObjectClose,
}

/// Deserialize a JSON string into a Wip object.
///
/// # Arguments
///
/// * `wip` - A mutable Wip object to deserialize into.
/// * `input` - A byte slice representing the JSON input.
///
/// # Returns
///
/// A result containing the updated `Wip` or a `JsonParseErrorWithContext`.
pub fn from_slice_wip<'input, 'a>(
    mut wip: Wip<'a>,
    input: &'input [u8],
) -> Result<HeapValue<'a>, JsonError<'input>> {
    let mut span = Span { start: 0, len: 0 };
    let mut stack = vec![Expect::Value];
    let mut tokenizer = Tokenizer::new(input);

    macro_rules! bail {
        ($span:expr, $kind:expr) => {
            return Err(JsonError::new($kind, input, $span, wip.path()))
        };
    }

    macro_rules! next_token {
        () => {
            match tokenizer.next_token() {
                Ok(token) => token,
                Err(e) => {
                    bail!(e.span, JsonErrorKind::SyntaxError(e.kind));
                }
            }
        };
    }

    loop {
        let frame_count = wip.frames_count();
        let expect = match stack.pop() {
            Some(expect) => expect,
            None => {
                return Ok(wip.build().unwrap());
            }
        };
        trace!("[{frame_count}] Expecting {:?}", expect.yellow());

        let token = next_token!();
        span = token.span;

        match expect {
            Expect::Value => match token.node {
                Token::LBrace => {
                    trace!("Object starting");
                    stack.push(Expect::ObjectKeyOrObjectClose)
                }
                Token::RBrace => todo!(),
                Token::LBracket => todo!(),
                Token::RBracket => todo!(),
                Token::Colon => todo!(),
                Token::Comma => todo!(),
                Token::String(_) => todo!(),
                Token::Number(_) => todo!(),
                Token::True => todo!(),
                Token::False => todo!(),
                Token::Null => todo!(),
                Token::EOF => todo!(),
            },
            Expect::ObjectKeyOrObjectClose => match token.node {
                Token::String(key) => {
                    trace!("Object key: {}", key);
                    let colon = next_token!();
                    if colon.node != Token::Colon {
                        bail!(
                            colon.span,
                            JsonErrorKind::UnexpectedToken {
                                got: colon.node,
                                wanted: "colon"
                            }
                        );
                    }
                    stack.push(Expect::Value);
                    stack.push(Expect::ObjectKeyOrObjectClose);
                }
                Token::RBrace => {
                    trace!("Object closing");
                }
                _ => {
                    bail!(
                        span,
                        JsonErrorKind::UnexpectedToken {
                            got: token.node,
                            wanted: "object key or closing brace"
                        }
                    );
                }
            },
            _ => {
                todo!()
            }
        }
    }
}
