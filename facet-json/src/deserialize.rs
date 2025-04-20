use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
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
    Pop(PopReason),
    ObjectKeyOrObjectClose,
    CommaThenObjectKeyOrObjectClose,
    ArrayItemOrArrayClose,
    CommaThenArrayItemOrArrayClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopReason {
    ObjectVal,
    ArrayItem,
    Some,
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
    let mut unread_token: Option<Spanned<Token>> = None;

    macro_rules! bail {
        ($kind:expr) => {
            return Err(JsonError::new($kind, input, last_span, wip.path()))
        };
    }

    macro_rules! read_token {
        () => {
            if let Some(token) = unread_token.take() {
                last_span = token.span;
                token
            } else {
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
            }
        };
    }

    macro_rules! put_back_token {
        ($token:expr) => {
            assert!(
                unread_token.is_none(),
                "Cannot put back more than one token at a time"
            );
            unread_token = Some($token);
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
            Instruction::Pop(reason) => {
                trace!("Popping because {:?}", reason.yellow());
                reflect!(pop());
            }
            Instruction::Value => {
                let token = read_token!();
                match token.node {
                    Token::Null => {
                        reflect!(put_default());
                    }
                    _ => {
                        if matches!(wip.shape().def, Def::Option(_)) {
                            trace!("Starting Some(_) option for {}", wip.shape().blue());
                            reflect!(push_some());
                            stack.push(Instruction::Pop(PopReason::Some))
                        }

                        match token.node {
                            Token::Null => unreachable!(),
                            Token::LBrace => {
                                match wip.shape().def {
                                    Def::Map(_md) => {
                                        trace!(
                                            "Object starting for map value ({})!",
                                            wip.shape().blue()
                                        );
                                        reflect!(put_default());
                                    }
                                    Def::Enum(_ed) => {
                                        trace!(
                                            "Object starting for enum value ({})!",
                                            wip.shape().blue()
                                        );
                                        bail!(JsonErrorKind::Unimplemented("map object"));
                                    }
                                    Def::Struct(_) => {
                                        trace!(
                                            "Object starting for struct value ({})!",
                                            wip.shape().blue()
                                        );
                                        // nothing to do here
                                    }
                                    _ => {
                                        bail!(JsonErrorKind::UnsupportedType {
                                            got: wip.shape(),
                                            wanted: "map, enum, or struct"
                                        });
                                    }
                                }

                                stack.push(Instruction::ObjectKeyOrObjectClose)
                            }
                            Token::LBracket => {
                                match wip.shape().def {
                                    Def::Array(_) => {
                                        trace!(
                                            "Array starting for array ({})!",
                                            wip.shape().blue()
                                        );
                                    }
                                    Def::Slice(_) => {
                                        trace!(
                                            "Array starting for slice ({})!",
                                            wip.shape().blue()
                                        );
                                    }
                                    Def::List(_) => {
                                        trace!("Array starting for list ({})!", wip.shape().blue());
                                        reflect!(put_default());
                                    }
                                    _ => {
                                        bail!(JsonErrorKind::UnsupportedType {
                                            got: wip.shape(),
                                            wanted: "array, list, or slice"
                                        });
                                    }
                                }

                                trace!("Beginning pushback");
                                reflect!(begin_pushback());
                                stack.push(Instruction::ArrayItemOrArrayClose)
                            }
                            Token::RBrace | Token::RBracket | Token::Colon | Token::Comma => {
                                bail!(JsonErrorKind::UnexpectedToken {
                                    got: token.node,
                                    wanted: "value"
                                });
                            }
                            Token::String(s) => {
                                reflect!(put::<String>(s));
                            }
                            Token::F64(n) => {
                                reflect!(put(n));
                            }
                            Token::U64(n) => {
                                reflect!(put(n));
                            }
                            Token::I64(n) => {
                                reflect!(put(n));
                            }
                            Token::True => {
                                reflect!(put::<bool>(true));
                            }
                            Token::False => {
                                reflect!(put::<bool>(false));
                            }
                            Token::EOF => todo!(),
                        }
                    }
                }
            }
            Instruction::ObjectKeyOrObjectClose => {
                let token = read_token!();
                match token.node {
                    Token::String(key) => {
                        trace!("Object key: {}", key);

                        match wip.shape().def {
                            Def::Struct(_) => {
                                let index = match wip.field_index(&key) {
                                    Some(index) => index,
                                    None => bail!(JsonErrorKind::UnknownField {
                                        field_name: key.to_string(),
                                        shape: wip.shape(),
                                    }),
                                };
                                reflect!(field(index));
                            }
                            Def::Map(_) => {
                                reflect!(push_map_key());
                                reflect!(put(key));
                                reflect!(push_map_value());
                            }
                            _ => {
                                bail!(JsonErrorKind::Unimplemented(
                                    "object key for non-struct/map"
                                ));
                            }
                        }

                        let colon = read_token!();
                        if colon.node != Token::Colon {
                            bail!(JsonErrorKind::UnexpectedToken {
                                got: colon.node,
                                wanted: "colon"
                            });
                        }
                        stack.push(Instruction::CommaThenObjectKeyOrObjectClose);
                        stack.push(Instruction::Pop(PopReason::ObjectVal));
                        stack.push(Instruction::Value);
                    }
                    Token::RBrace => {
                        trace!("Object closing");
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedToken {
                            got: token.node,
                            wanted: "object key or closing brace"
                        });
                    }
                }
            }
            Instruction::CommaThenObjectKeyOrObjectClose => {
                let token = read_token!();
                match token.node {
                    Token::Comma => {
                        trace!("Object comma");
                        stack.push(Instruction::ObjectKeyOrObjectClose);
                    }
                    Token::RBrace => {
                        trace!("Object close");
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedToken {
                            got: token.node,
                            wanted: "comma"
                        });
                    }
                }
            }
            Instruction::ArrayItemOrArrayClose => {
                let token = read_token!();
                match token.node {
                    Token::RBracket => {
                        trace!("Array close");
                    }
                    _ => {
                        trace!("Array item");
                        put_back_token!(token);
                        reflect!(begin_pushback());
                        reflect!(push());

                        stack.push(Instruction::CommaThenArrayItemOrArrayClose);
                        stack.push(Instruction::Pop(PopReason::ArrayItem));
                        stack.push(Instruction::Value);
                    }
                }
            }
            Instruction::CommaThenArrayItemOrArrayClose => {
                let token = read_token!();
                match token.node {
                    Token::RBracket => {
                        trace!("Array close");
                    }
                    Token::Comma => {
                        trace!("Array comma");
                        reflect!(push());
                        stack.push(Instruction::CommaThenArrayItemOrArrayClose);
                        stack.push(Instruction::Pop(PopReason::ArrayItem));
                        stack.push(Instruction::Value);
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedToken {
                            got: token.node,
                            wanted: "comma or closing bracket"
                        });
                    }
                }
            }
        }
    }
}
