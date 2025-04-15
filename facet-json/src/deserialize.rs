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
    /// Creates a new `JsonParseErrorWithContext`.
    ///
    /// # Arguments
    ///
    /// * `kind` - The kind of JSON error encountered.
    /// * `input` - The original input being parsed.
    /// * `pos` - The position in the input where the error occurred.
    pub fn new(kind: JsonErrorKind, input: &'input [u8], pos: usize) -> Self {
        Self { input, pos, kind }
    }
}

/// An error kind for JSON parsing.
#[derive(Debug)]
pub enum JsonErrorKind {
    /// The input ended unexpectedly while parsing JSON.
    UnexpectedEof,
    /// An unexpected character was encountered in the input.
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
///
/// # Arguments
///
/// * `json` - A slice of bytes representing the JSON input.
///
/// # Returns
///
/// A result containing the deserialized value of type `T` or a `JsonParseErrorWithContext`.
pub fn from_slice<T: Facet>(json: &[u8]) -> Result<T, JsonParseErrorWithContext<'_>> {
    let wip = Wip::alloc::<T>();
    let wip = from_slice_wip(wip, json)?;
    let heap_value = wip.build().unwrap();
    Ok(heap_value.materialize::<T>().unwrap())
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

    /// err "previous char"
    macro_rules! errp {
        ($kind:expr) => {
            Err(JsonParseErrorWithContext::new($kind, input, pos - 1))
        };
    }
    /// bail "previous char"
    macro_rules! bailp {
        ($kind:expr) => {
            return errp!($kind);
        };
    }

    /// Indicates why we are expecting a value in the parsing stack.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum WhyValue {
        /// At the top level of the JSON input.
        TopLevel,
        /// Expecting an object key.
        ObjectKey,
        /// Expecting an object value.
        ObjectValue,
        /// Expecting an array element.
        ArrayElement,
    }

    /// Indicates the context for a comma separator in JSON (object or array).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum WhyComma {
        /// A comma in an object context.
        Object,
        /// A comma in an array context.
        Array,
    }

    /// Indicates the type of separator expected (colon or comma).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Separator {
        /// Expecting a colon separator in object key-value pairs.
        Colon,
        /// Expecting a comma separator (in objects or arrays).
        Comma(WhyComma),
    }

    /// Represents the next expected token or structure while parsing.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Expect {
        /// Expecting a value, with its reason/context.
        Value(WhyValue),
        /// Expecting a separator (colon or comma).
        Separator(Separator),
    }

    let mut stack: Vec<Expect> = Vec::new();
    stack.push(Expect::Value(WhyValue::TopLevel));

    'main: loop {
        let frame_count = wip.frames_count();
        let expect = match stack.pop() {
            Some(expect) => expect,
            None => {
                if frame_count == 1 {
                    // we're done!
                    break;
                } else {
                    bail!(JsonErrorKind::UnexpectedEof);
                }
            }
        };
        trace!("[{frame_count}] Expecting {expect:?}");

        let Some(c) = input.get(pos).copied() else {
            bail!(JsonErrorKind::UnexpectedEof);
        };
        pos += 1;

        match expect {
            Expect::Value(_why_value) => {
                match c {
                    b'{' => {
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
                                stack.push(Expect::Separator(Separator::Comma(WhyComma::Object)));
                                stack.push(Expect::Value(WhyValue::ObjectValue));
                                stack.push(Expect::Separator(Separator::Colon));
                                stack.push(Expect::Value(WhyValue::ObjectKey));
                            }
                        }
                    }
                    b'[' => {
                        let Some(c) = input.get(pos).copied() else {
                            bail!(JsonErrorKind::UnexpectedEof);
                        };
                        match c {
                            b']' => {
                                // an array just closed, somewhere
                                pos += 1;
                            }
                            _ => {
                                // okay, next we expect an item and a separator (or the end of the array)
                                stack.push(Expect::Separator(Separator::Comma(WhyComma::Array)));
                                stack.push(Expect::Value(WhyValue::ArrayElement));
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
                        trace!("Parsed string value: {:?}", value.yellow());
                    }
                    b'0'..=b'9' => {
                        let start = pos - 1;
                        while let Some(c) = input.get(pos) {
                            match c {
                                b'0'..=b'9' => {
                                    pos += 1;
                                }
                                _ => break,
                            }
                        }
                        let number = &input[start..pos];
                        let number = core::str::from_utf8(number).unwrap();
                        trace!("Parsed number value: {:?}", number.yellow());
                        let number = number.parse::<f64>().unwrap();
                        trace!("Parsed number value: {:?}", number.yellow());
                    }
                    c => {
                        bailp!(JsonErrorKind::UnexpectedCharacter(c as char));
                    }
                }
            }
            Expect::Separator(separator) => match separator {
                Separator::Colon => match c {
                    b':' => {
                        pos += 1;
                    }
                    _ => {
                        bailp!(JsonErrorKind::UnexpectedCharacter(c as char));
                    }
                },
                Separator::Comma(why) => match c {
                    b',' => {
                        pos += 1;
                        match why {
                            WhyComma::Array => {
                                stack.push(Expect::Separator(Separator::Comma(WhyComma::Array)));
                                stack.push(Expect::Value(WhyValue::ArrayElement));
                            }
                            WhyComma::Object => {
                                // looks like we're in for another round of object parsing
                                stack.push(Expect::Separator(Separator::Comma(WhyComma::Object)));
                                stack.push(Expect::Value(WhyValue::ObjectValue));
                                stack.push(Expect::Separator(Separator::Colon));
                                stack.push(Expect::Value(WhyValue::ObjectKey));
                            }
                        }
                    }
                    b'}' => {
                        match why {
                            WhyComma::Object => {
                                // we finished the object, neat
                            }
                            _ => {
                                bailp!(JsonErrorKind::UnexpectedCharacter(c as char));
                            }
                        }
                    }
                    b']' => {
                        match why {
                            WhyComma::Array => {
                                // we finished the array, neat
                            }
                            _ => {
                                bailp!(JsonErrorKind::UnexpectedCharacter(c as char));
                            }
                        }
                    }
                    _ => {
                        bailp!(JsonErrorKind::UnexpectedCharacter(c as char));
                    }
                },
            },
        }
    }
    Ok(wip)
}
