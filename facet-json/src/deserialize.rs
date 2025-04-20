use facet_core::Def;
use facet_core::Facet;
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
enum Instruction {
    Value,
    CommaThenObjectKeyOrObjectClose,
    ObjectKeyOrObjectClose,
    Pop, // pop after setting an object value
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
    let mut stack = vec![Instruction::Value];
    let mut tokenizer = Tokenizer::new(input);
    let mut last_span = Span { start: 0, len: 0 };

    macro_rules! bail {
        ($kind:expr) => {
            return Err(JsonError::new($kind, input, last_span, wip.path()))
        };
    }

    macro_rules! next_token {
        () => {
            match tokenizer.next_token() {
                Ok(token) => {
                    last_span = token.span;
                    token
                }
                Err(e) => {
                    last_span = e.span;
                    bail!(JsonErrorKind::SyntaxError(e.kind));
                }
            }
        };
    }

    macro_rules! reflect {
        ($($tt:tt)*) => {
            let path = wip.path();
            wip = match wip.$($tt)* {
                Ok(wip) => wip,
                Err(e) => {
                    return Err(JsonError::new(
                        JsonErrorKind::ReflectError(e),
                        input,
                        last_span,
                        path,
                    ));
                }
            }
        };
    }

    loop {
        let frame_count = wip.frames_count();
        let insn = match stack.pop() {
            Some(insn) => insn,
            None => {
                trace!("No instruction, building.");
                let path = wip.path();
                return Ok(match wip.build() {
                    Ok(hv) => hv,
                    Err(e) => {
                        return Err(JsonError::new(
                            JsonErrorKind::ReflectError(e),
                            input,
                            last_span,
                            path,
                        ));
                    }
                });
            }
        };
        trace!("[{frame_count}] Instruction {:?}", insn.yellow());

        match insn {
            Instruction::Value => {
                let token = next_token!();
                match token.node {
                    Token::LBrace => {
                        match wip.shape().def {
                            Def::Map(_md) => {
                                trace!("Object starting for map value ({})!", wip.shape().blue());
                                reflect!(put_default());
                            }
                            Def::Struct(_) => {
                                trace!(
                                    "Object starting for struct value ({})!",
                                    wip.shape().blue()
                                );
                                // nothing to do here
                            }
                            Def::Enum(ed) => {
                                trace!("Object starting for enum value ({})!", wip.shape().blue());
                                bail!(JsonErrorKind::Unimplemented("map object".to_owned()));
                            }
                            _ => {
                                bail!(JsonErrorKind::Unimplemented("Object".to_owned()));
                            }
                        }

                        stack.push(Instruction::ObjectKeyOrObjectClose)
                    }
                    Token::RBrace
                    | Token::LBracket
                    | Token::RBracket
                    | Token::Colon
                    | Token::Comma => {
                        bail!(JsonErrorKind::UnexpectedToken {
                            got: token.node,
                            wanted: "value"
                        });
                    }
                    Token::String(s) => {
                        reflect!(put::<String>(s));
                    }
                    Token::Number(n) => {
                        let path = wip.path();
                        wip = match wip.put::<u64>(n as u64) {
                            Ok(wip) => wip,
                            Err(e) => {
                                return Err(JsonError::new(
                                    JsonErrorKind::ReflectError(e),
                                    input,
                                    token.span,
                                    path,
                                ));
                            }
                        }
                    }
                    Token::True => todo!(),
                    Token::False => todo!(),
                    Token::Null => todo!(),
                    Token::EOF => todo!(),
                }
            }
            Instruction::CommaThenObjectKeyOrObjectClose => {
                let token = next_token!();
                match token.node {
                    Token::Comma => {
                        trace!("Object comma");
                        stack.push(Instruction::ObjectKeyOrObjectClose);
                    }
                    Token::RBrace => {
                        trace!("Object close");
                        stack.push(Instruction::Pop);
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedToken {
                            got: token.node,
                            wanted: "comma"
                        });
                    }
                }
            }
            Instruction::ObjectKeyOrObjectClose => {
                let token = next_token!();
                match token.node {
                    Token::String(key) => {
                        let index = match wip.field_index(&key) {
                            Some(index) => index,
                            None => bail!(JsonErrorKind::UnknownField(key)),
                        };
                        reflect!(field(index));

                        trace!("Object key: {}", key);
                        let colon = next_token!();
                        if colon.node != Token::Colon {
                            bail!(JsonErrorKind::UnexpectedToken {
                                got: colon.node,
                                wanted: "colon"
                            });
                        }
                        stack.push(Instruction::CommaThenObjectKeyOrObjectClose);
                        stack.push(Instruction::Pop);
                        stack.push(Instruction::Value);
                    }
                    Token::RBrace => {
                        trace!("Object closing");
                        stack.push(Instruction::Pop);
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedToken {
                            got: token.node,
                            wanted: "object key or closing brace"
                        });
                    }
                }
            }
            Instruction::Pop => {
                reflect!(pop());
            }
        }
    }
}
