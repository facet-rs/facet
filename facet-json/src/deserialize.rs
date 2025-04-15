use core::f32::consts::E;

use facet_ansi::Stylize as _;
use facet_core::Facet;
use facet_reflect::Wip;
use log::trace;

/// A JSON parse error, with context. Never would've guessed huh.
#[derive(Debug)]
pub struct JsonParseErrorWithContext<'input> {
    input: &'input [u8],
    pos: usize,
    kind: JsonErrorKind,
}

impl<'input> JsonParseErrorWithContext<'input> {
    pub fn new(kind: JsonErrorKind, input: &'input [u8], pos: usize) -> Self {
        Self { input, pos, kind }
    }
}

/// An error kind for JSON parsing.
#[derive(Debug)]
pub enum JsonErrorKind {
    UnexpectedEof,
    UnexpectedCharacter(char),
}

impl core::fmt::Display for JsonParseErrorWithContext<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "JSON parse error:\n{}\n{:width$}{} {:?}",
            core::str::from_utf8(self.input)
                .unwrap_or("invalid UTF-8")
                .yellow(),
            "",
            "↑".red(),
            (&self.kind).bright_blue(),
            width = self.pos,
        )
    }
}

/// Deserializes a JSON string into a value of type `T` that implements `Facet`.
///
/// This function takes a JSON string representation and converts it into a Rust
/// value of the specified type `T`. The type must implement the `Facet` trait
/// to provide the necessary type information for deserialization.
pub fn from_str<T: Facet>(json: &str) -> Result<T, JsonParseErrorWithContext<'_>> {
    from_slice(json.as_bytes())
}

/// Deserialize JSON from a slice
pub fn from_slice<T: Facet>(json: &[u8]) -> Result<T, JsonParseErrorWithContext<'_>> {
    let wip = Wip::alloc::<T>();
    let wip = from_slice_wip(wip, json)?;
    let heap_value = wip.build().unwrap();
    Ok(heap_value.materialize::<T>().unwrap())
}

/// Deserialize a JSON string into a Wip object.
pub fn from_slice_wip<'input, 'a>(
    mut wip: Wip<'a>,
    input: &'input [u8],
) -> Result<Wip<'a>, JsonParseErrorWithContext<'input>> {
    let mut pos = 0;
    macro_rules! err {
        ($kind:expr) => {
            Err(JsonParseErrorWithContext::new($kind, input, pos))
        };
    }
    macro_rules! bail {
        ($kind:expr) => {
            return err!($kind);
        };
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum WhyValue {
        TopLevel,
        ObjectKey,
        ObjectValue,
        ArrayElement,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum WhyComma {
        Object,
        Array,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Separator {
        Colon,
        Comma(WhyComma),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Expect {
        Value(WhyValue),
        Separator(Separator),
    }

    let mut stack: Vec<Expect> = Vec::new();
    stack.push(Expect::Value(WhyValue::TopLevel));

    'main: loop {
        let frame_count = wip.frames_count();
        let expect = match stack.pop() {
            Some(expect) => expect,
            None => bail!(JsonErrorKind::UnexpectedEof),
        };
        trace!("[{frame_count}] Expecting {expect:?}");

        let Some(c) = input.get(pos).copied() else {
            if frame_count == 1 {
                // alright, we're done!
                break;
            } else {
                bail!(JsonErrorKind::UnexpectedEof);
            }
        };
        pos += 1;

        match expect {
            Expect::Value(_why_value) => {
                match c {
                    b'{' => {
                        // we definitely expect some next character
                        let Some(c) = input.get(pos).copied() else {
                            bail!(JsonErrorKind::UnexpectedEof);
                        };
                        match c {
                            b'}' => {
                                pos += 1;
                                if frame_count == 1 {
                                    // alright, we're done!
                                    break 'main;
                                } else {
                                    // just finished reading a value I guess
                                    wip = wip.pop().unwrap();
                                }
                            }
                            _ => {
                                // okay, next we expect a "key: value"
                                stack.push(Expect::Value(WhyValue::ObjectValue));
                                stack.push(Expect::Separator(Separator::Colon));
                                stack.push(Expect::Value(WhyValue::ObjectKey));
                            }
                        }
                    }
                    b'"' => {
                        // our value is a string
                        let mut value = String::new();
                        loop {
                            let Some(c) = input.get(pos).copied() else {
                                bail!(JsonErrorKind::UnexpectedEof);
                            };
                            match c {
                                b'"' => {
                                    pos += 1;
                                    break;
                                }
                                b'\\' => {
                                    pos += 2;
                                    value.push('\\');
                                }
                                _ => {
                                    pos += 1;
                                    value.push(c as char);
                                }
                            }
                        }
                    }
                    c => {
                        bail!(JsonErrorKind::UnexpectedCharacter(c as char));
                    }
                }
            }
            Expect::Separator(separator) => match separator {
                Separator::Colon => match c {
                    b':' => {
                        pos += 1;
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedCharacter(c as char));
                    }
                },
                Separator::Comma(_why) => match c {
                    b',' => {
                        pos += 1;
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedCharacter(c as char));
                    }
                },
            },
        }
    }
    Ok(wip)
}
