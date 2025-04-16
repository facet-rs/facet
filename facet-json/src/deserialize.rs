use core::num::{
    NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroIsize, NonZeroU8, NonZeroU16, NonZeroU32,
    NonZeroU64, NonZeroUsize,
};

#[cfg(feature = "rich-diagnostics")]
use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source};
use facet_core::{Def, Facet, ScalarAffinity};
use facet_reflect::{HeapValue, Wip};
use log::trace;
use yansi::Paint as _;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// A JSON parse error, with context. Never would've guessed huh.
#[derive(Debug)]
pub struct JsonParseErrorWithContext<'input> {
    input: &'input [u8],
    pos: usize,
    kind: JsonErrorKind,
    path: String,
}

impl<'input> JsonParseErrorWithContext<'input> {
    /// Creates a new `JsonParseErrorWithContext`.
    ///
    /// # Arguments
    ///
    /// * `kind` - The kind of JSON error encountered.
    /// * `input` - The original input being parsed.
    /// * `pos` - The position in the input where the error occurred.
    pub fn new(kind: JsonErrorKind, input: &'input [u8], pos: usize, path: String) -> Self {
        Self {
            input,
            pos,
            kind,
            path,
        }
    }

    /// Returns a human-readable error message for this JSON error.
    pub fn message(&self) -> String {
        match &self.kind {
            JsonErrorKind::UnexpectedEof => "Unexpected end of file".to_string(),
            JsonErrorKind::UnexpectedCharacter(c) => format!("Unexpected character: '{}'", c),
            JsonErrorKind::NumberOutOfRange(n) => format!("Number out of range: {}", n),
            JsonErrorKind::StringAsNumber(s) => format!("Expected a string but got number: {}", s),
        }
    }
}

/// An error kind for JSON parsing.
#[derive(Debug)]
pub enum JsonErrorKind {
    /// The input ended unexpectedly while parsing JSON.
    UnexpectedEof,
    /// An unexpected character was encountered in the input.
    UnexpectedCharacter(char),
    /// A number is out of range.
    NumberOutOfRange(f64),
    /// An unexpected String was encountered in the input.
    StringAsNumber(String),
}

#[cfg(not(feature = "rich-diagnostics"))]
impl core::fmt::Display for JsonParseErrorWithContext<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} at byte {} in path {}",
            self.message(),
            self.pos,
            self.path
        )
    }
}

#[cfg(feature = "rich-diagnostics")]
impl core::fmt::Display for JsonParseErrorWithContext<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Ok(input_str) = core::str::from_utf8(self.input) else {
            return write!(f, "(JSON input was invalid UTF-8)");
        };

        let source_id = "json";

        let (span_start, span_end) = match &self.kind {
            JsonErrorKind::StringAsNumber(s) => (self.pos - s.len(), self.pos),
            _ => {
                let span_end = if self.pos < self.input.len() {
                    self.pos + 1
                } else {
                    self.input.len()
                };
                (self.pos, span_end)
            }
        };

        let mut report = Report::build(ReportKind::Error, (source_id, span_start..span_end))
            .with_message(format!("Error at {}", self.path.fg(Color::Yellow)))
            .with_config(Config::new().with_index_type(IndexType::Byte));

        let label = Label::new((source_id, span_start..span_end))
            .with_message(self.message())
            .with_color(Color::Red);

        report = report.with_label(label);

        let source = Source::from(input_str);

        let mut writer = Vec::new();
        let cache = (source_id, &source);

        if report.finish().write(cache, &mut writer).is_err() {
            return write!(f, "Error formatting with ariadne");
        }

        if let Ok(output) = String::from_utf8(writer) {
            write!(f, "{}", output)
        } else {
            write!(f, "Error converting ariadne output to string")
        }
    }
}

impl core::error::Error for JsonParseErrorWithContext<'_> {}

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
    let heap_value = from_slice_wip(wip, json)?;
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
) -> Result<HeapValue<'a>, JsonParseErrorWithContext<'input>> {
    let mut pos = 0;

    macro_rules! err {
        ($kind:expr) => {
            Err(JsonParseErrorWithContext::new(
                $kind,
                input,
                pos,
                wip.path(),
            ))
        };
    }
    macro_rules! bail {
        ($kind:expr) => {
            return err!($kind);
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
        /// We did `push_some` and now we need to pop it
        PopOption,
    }

    let mut stack: Vec<Expect> = Vec::new();
    stack.push(Expect::Value(WhyValue::TopLevel));

    loop {
        // skip over whitespace
        while let Some(c) = input.get(pos).copied() {
            match c {
                b' ' | b'\t' | b'\n' | b'\r' => {
                    pos += 1;
                }
                _ => break,
            }
        }

        let frame_count = wip.frames_count();
        let expect = match stack.pop() {
            Some(expect) => expect,
            None => {
                if frame_count == 1 {
                    return Ok(wip.build().unwrap());
                } else {
                    bail!(JsonErrorKind::UnexpectedEof);
                }
            }
        };
        trace!("[{frame_count}] Expecting {expect:?}");

        let Some(c) = input.get(pos).copied() else {
            bail!(JsonErrorKind::UnexpectedEof);
        };

        match expect {
            Expect::PopOption => {
                // that's all, carry on
                trace!("Popping option");
                wip = wip.pop().unwrap();
            }
            Expect::Value(why) => {
                if let Def::Option(_) = wip.shape().def {
                    wip = wip.push_some().unwrap();
                    stack.push(Expect::PopOption);
                }

                match c {
                    b'{' => {
                        pos += 1;
                        let Some(c) = input.get(pos).copied() else {
                            bail!(JsonErrorKind::UnexpectedEof);
                        };
                        match c {
                            b'}' => {
                                pos += 1;
                                if frame_count > 1 {
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
                        pos += 1;
                        let Some(c) = input.get(pos).copied() else {
                            bail!(JsonErrorKind::UnexpectedEof);
                        };

                        wip = wip.begin_pushback().unwrap();
                        match c {
                            b']' => {
                                // an array just closed, somewhere
                                pos += 1;
                            }
                            _ => {
                                // okay, next we expect an item and a separator (or the end of the array)
                                stack.push(Expect::Separator(Separator::Comma(WhyComma::Array)));
                                stack.push(Expect::Value(WhyValue::ArrayElement));
                                wip = wip.push().unwrap();
                            }
                        }
                    }
                    b'"' => {
                        pos += 1;
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
                        trace!(
                            "Parsed string value: {:?} for shape {}",
                            value.yellow(),
                            wip.shape()
                        );

                        match why {
                            WhyValue::TopLevel => {
                                wip = wip.parse(&value).unwrap();
                            }
                            WhyValue::ArrayElement => {
                                wip = wip.parse(&value).unwrap();
                                wip = wip.pop().unwrap();
                            }
                            WhyValue::ObjectValue => {
                                wip = wip.parse(&value).unwrap();
                                wip = wip.pop().unwrap();
                            }
                            WhyValue::ObjectKey => {
                                wip = wip.field_named(&value).unwrap();
                            }
                        }
                    }
                    b'0'..=b'9' | b'-' => {
                        pos += 1;
                        let start = pos - 1;
                        while let Some(c) = input.get(pos) {
                            match c {
                                b'0'..=b'9' | b'.' => {
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

                        let shape = wip.shape();
                        match shape.def {
                            Def::Scalar(sd) => match sd.affinity {
                                ScalarAffinity::Number(_na) => {
                                    if shape.is_type::<u8>() {
                                        if number >= 0.0 && number <= u8::MAX as f64 {
                                            let value = number as u8;
                                            wip = wip.put::<u8>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<u16>() {
                                        if number >= 0.0 && number <= u16::MAX as f64 {
                                            let value = number as u16;
                                            wip = wip.put::<u16>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<u32>() {
                                        if number >= 0.0 && number <= u32::MAX as f64 {
                                            let value = number as u32;
                                            wip = wip.put::<u32>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<u64>() {
                                        if number >= 0.0 && number <= u64::MAX as f64 {
                                            let value = number as u64;
                                            wip = wip.put::<u64>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<i8>() {
                                        if number >= i8::MIN as f64 && number <= i8::MAX as f64 {
                                            let value = number as i8;
                                            wip = wip.put::<i8>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<i16>() {
                                        if number >= i16::MIN as f64 && number <= i16::MAX as f64 {
                                            let value = number as i16;
                                            wip = wip.put::<i16>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<i32>() {
                                        if number >= i32::MIN as f64 && number <= i32::MAX as f64 {
                                            let value = number as i32;
                                            wip = wip.put::<i32>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<i64>() {
                                        // Note: f64 might lose precision for large i64 values, but this is a common limitation.
                                        if number >= i64::MIN as f64 && number <= i64::MAX as f64 {
                                            let value = number as i64;
                                            wip = wip.put::<i64>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<f32>() {
                                        if number >= f32::MIN as f64 && number <= f32::MAX as f64 {
                                            let value = number as f32;
                                            wip = wip.put::<f32>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<f64>() {
                                        wip = wip.put::<f64>(number).unwrap();
                                    } else if shape.is_type::<NonZeroU8>() {
                                        if number >= 1.0 && number <= u8::MAX as f64 {
                                            let value = NonZeroU8::new(number as u8).unwrap();
                                            wip = wip.put::<NonZeroU8>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<NonZeroU16>() {
                                        if number >= 1.0 && number <= u16::MAX as f64 {
                                            let value = NonZeroU16::new(number as u16).unwrap();
                                            wip = wip.put::<NonZeroU16>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<NonZeroU32>() {
                                        if number >= 1.0 && number <= u32::MAX as f64 {
                                            let value = NonZeroU32::new(number as u32).unwrap();
                                            wip = wip.put::<NonZeroU32>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<NonZeroU64>() {
                                        if number >= 1.0 && number <= u64::MAX as f64 {
                                            let value = NonZeroU64::new(number as u64).unwrap();
                                            wip = wip.put::<NonZeroU64>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<NonZeroUsize>() {
                                        if number >= 1.0 && number <= usize::MAX as f64 {
                                            let value = NonZeroUsize::new(number as usize).unwrap();
                                            wip = wip.put::<NonZeroUsize>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<NonZeroI8>() {
                                        if number >= 1.0 && number <= i8::MAX as f64 {
                                            let value = NonZeroI8::new(number as i8).unwrap();
                                            wip = wip.put::<NonZeroI8>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<NonZeroI16>() {
                                        if number >= 1.0 && number <= i16::MAX as f64 {
                                            let value = NonZeroI16::new(number as i16).unwrap();
                                            wip = wip.put::<NonZeroI16>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<NonZeroI32>() {
                                        if number >= 1.0 && number <= i32::MAX as f64 {
                                            let value = NonZeroI32::new(number as i32).unwrap();
                                            wip = wip.put::<NonZeroI32>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<NonZeroI64>() {
                                        if number >= 1.0 && number <= i64::MAX as f64 {
                                            let value = NonZeroI64::new(number as i64).unwrap();
                                            wip = wip.put::<NonZeroI64>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else if shape.is_type::<NonZeroIsize>() {
                                        if number >= 1.0 && number <= isize::MAX as f64 {
                                            let value = NonZeroIsize::new(number as isize).unwrap();
                                            wip = wip.put::<NonZeroIsize>(value).unwrap();
                                        } else {
                                            bail!(JsonErrorKind::NumberOutOfRange(number));
                                        }
                                    } else {
                                        todo!("number type, but unknown")
                                    }
                                }
                                ScalarAffinity::String(_sa) => {
                                    if shape.is_type::<String>() {
                                        let value = number.to_string();
                                        bail!(JsonErrorKind::StringAsNumber(value));
                                    } else {
                                        todo!()
                                    }
                                }
                                _ => {
                                    todo!("saw number in JSON but expected.. shape {}?", shape)
                                }
                            },
                            _ => {
                                todo!("saw number in JSON but expected.. shape {}?", shape)
                            }
                        }

                        match why {
                            WhyValue::TopLevel => {}
                            WhyValue::ObjectKey => todo!(),
                            WhyValue::ObjectValue => {
                                wip = wip.pop().unwrap();
                            }
                            WhyValue::ArrayElement => {
                                wip = wip.pop().unwrap();
                            }
                        }
                    }
                    b'n' => {
                        // wow it's a null — probably
                        let slice_rest = &input[pos..];
                        if slice_rest.starts_with(b"null") {
                            pos += 4;

                            // ok but we already pushed some! luckily wip has the method for us
                            wip = wip.pop_some_push_none().unwrap();

                            match why {
                                WhyValue::TopLevel => {}
                                WhyValue::ObjectKey => todo!(),
                                WhyValue::ObjectValue => {
                                    // these are all super messy, they should be expect on the stack
                                    wip = wip.pop().unwrap();
                                }
                                WhyValue::ArrayElement => {
                                    wip = wip.pop().unwrap();
                                }
                            }
                        } else {
                            bail!(JsonErrorKind::UnexpectedCharacter('n'));
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
                Separator::Comma(why) => match c {
                    b',' => {
                        pos += 1;
                        match why {
                            WhyComma::Array => {
                                stack.push(Expect::Separator(Separator::Comma(WhyComma::Array)));
                                stack.push(Expect::Value(WhyValue::ArrayElement));
                                wip = wip.push().unwrap();
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
                                pos += 1;

                                // we finished the object, neat
                                if frame_count > 1 {
                                    wip = wip.pop().unwrap();
                                }
                            }
                            _ => {
                                bail!(JsonErrorKind::UnexpectedCharacter(c as char));
                            }
                        }
                    }
                    b']' => {
                        pos += 1;
                        match why {
                            WhyComma::Array => {
                                // we finished the array, neat
                                if frame_count > 1 {
                                    wip = wip.pop().unwrap();
                                }
                            }
                            _ => {
                                bail!(JsonErrorKind::UnexpectedCharacter(c as char));
                            }
                        }
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedCharacter(c as char));
                    }
                },
            },
        }
    }
}
