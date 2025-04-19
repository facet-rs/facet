use facet_core::{Characteristic, Def, Facet, FieldAttribute, ScalarAffinity, ShapeAttribute};
use facet_reflect::{HeapValue, Wip};
use log::trace;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use owo_colors::OwoColorize;

mod error;
pub use error::*;

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
            return err!($kind)
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
                    trace!("frame_count isn't 1, it's {}", frame_count);
                    bail!(JsonErrorKind::UnexpectedEof("frame_count isn't 1"));
                }
            }
        };
        trace!("[{frame_count}] Expecting {:?}", expect.yellow());

        let Some(c) = input.get(pos).copied() else {
            bail!(JsonErrorKind::UnexpectedEof("no input at pos"));
        };

        let mut finished_value: Option<WhyValue> = None;

        match expect {
            Expect::PopOption => {
                // that's all, carry on
                trace!("Popping option");
                finished_value = Some(WhyValue::ObjectValue);
            }
            Expect::Value(why) => {
                if let Def::Option(_) = wip.shape().def {
                    wip = wip.push_some().unwrap();
                    stack.push(Expect::PopOption);
                }

                match c {
                    b'{' => {
                        trace!("Object starting");
                        pos += 1;
                        let Some(c) = input.get(pos).copied() else {
                            bail!(JsonErrorKind::UnexpectedEof("nothing after {"));
                        };
                        match c {
                            b'}' => {
                                trace!("Empty object ended");
                                pos += 1;
                                finished_value = Some(why);
                            }
                            _ => {
                                trace!("Object's not empty, let's do `key: value ,` next");
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
                            bail!(JsonErrorKind::UnexpectedEof("nothing after ["));
                        };

                        wip = wip.begin_pushback().unwrap();
                        match c {
                            b']' => {
                                // an array just closed, somewhere
                                pos += 1;
                                trace!("Got empty array");
                                finished_value = Some(why);
                            }
                            _ => {
                                // okay, next we expect an item and a separator (or the end of the array)
                                stack.push(Expect::Separator(Separator::Comma(WhyComma::Array)));
                                stack.push(Expect::Value(WhyValue::ArrayElement));
                                wip = wip.push().unwrap();
                                continue; // we didn't finish a value so don't pop yet
                            }
                        }
                    }
                    b'"' => {
                        pos += 1;
                        let start = pos;
                        // Our value is a string: collect bytes first
                        let mut bytes = Vec::new();
                        loop {
                            let Some(c) = input.get(pos).copied() else {
                                bail!(JsonErrorKind::UnexpectedEof("nothing after \""));
                            };
                            match c {
                                b'"' => {
                                    break;
                                }
                                b'\\' => {
                                    // Handle escape sequences
                                    if let Some(&next) = input.get(pos + 1) {
                                        if bytes.is_empty() {
                                            bytes.extend(&input[start..pos]);
                                        }
                                        bytes.push(next);
                                        pos += 2;
                                    } else {
                                        bail!(JsonErrorKind::UnexpectedEof("nothing after \\"));
                                    }
                                }
                                _ => {
                                    if !bytes.is_empty() {
                                        bytes.push(c);
                                    }
                                    pos += 1;
                                }
                            }
                        }
                        let end = pos;
                        pos += 1;

                        let value = if bytes.is_empty() {
                            match core::str::from_utf8(&input[start..end]) {
                                Ok(it) => alloc::borrow::Cow::Borrowed(it),
                                Err(e) => bail!(JsonErrorKind::InvalidUtf8(format!(
                                    "Invalid UTF-8 sequence: {}",
                                    e
                                ))),
                            }
                        } else {
                            // Convert collected bytes to string at once
                            match alloc::string::String::from_utf8(bytes) {
                                Ok(it) => alloc::borrow::Cow::Owned(it),
                                Err(e) => bail!(JsonErrorKind::InvalidUtf8(format!(
                                    "Invalid UTF-8 sequence: {}",
                                    e
                                ))),
                            }
                        };

                        trace!(
                            "Parsed string {:?} for {} (why? {:?})",
                            value.yellow(),
                            wip.shape().blue(),
                            why.cyan()
                        );

                        match why {
                            WhyValue::TopLevel | WhyValue::ArrayElement | WhyValue::ObjectValue => {
                                // skip the string parse impl
                                if wip.current_is_type::<String>() {
                                    wip = wip.put::<String>(value.into_owned()).unwrap();
                                } else {
                                    wip = wip.parse(&value).unwrap();
                                }
                                finished_value = Some(why);
                            }
                            WhyValue::ObjectKey => {
                                // Look for field with matching name or rename attribute
                                let shape = wip.shape();

                                if let Def::Struct(struct_def) = shape.def {
                                    let field = struct_def.fields.iter().find(|f| {
                                        // Check original name
                                        if f.name == value {
                                            return true;
                                        }

                                        // Check rename attribute
                                        f.attributes.iter().any(|attr| {
                                            if let &FieldAttribute::Rename(rename) = attr {
                                                rename == value
                                            } else {
                                                false
                                            }
                                        })
                                    });

                                    if let Some(field) = field {
                                        trace!("found field {:?}", field.blue());
                                        wip = wip.field_named(field.name).unwrap();
                                    } else if shape.attributes.iter().any(|attr| {
                                        matches!(attr, ShapeAttribute::DenyUnknownFields)
                                    }) {
                                        // Field not found - original or renamed, and unknown fields denied
                                        bail!(JsonErrorKind::UnknownField(value.into_owned()));
                                    } else {
                                        // pop Expect::Colon (assert)
                                        let expect_colon = stack.pop();
                                        assert!(matches!(
                                            expect_colon,
                                            Some(Expect::Separator(Separator::Colon))
                                        ));
                                        // skip over whitespace
                                        while let Some(b' ' | b'\t' | b'\n' | b'\r') =
                                            input.get(pos).copied()
                                        {
                                            pos += 1;
                                        }
                                        // skip over colon
                                        if let Some(b':') = input.get(pos) {
                                            pos += 1;
                                        } else {
                                            bail!(JsonErrorKind::UnexpectedCharacter(
                                                input
                                                    .get(pos)
                                                    .copied()
                                                    .map(|c| c as char)
                                                    .unwrap_or('\0')
                                            ));
                                        }
                                        // skip over whitespace
                                        while let Some(b' ' | b'\t' | b'\n' | b'\r') =
                                            input.get(pos).copied()
                                        {
                                            pos += 1;
                                        }
                                        // pop Expect::Value
                                        let expect_value = stack.pop();
                                        assert!(matches!(
                                            expect_value,
                                            Some(Expect::Value(WhyValue::ObjectValue))
                                        ));
                                        // skip over value
                                        skip_over_value(&mut pos, input).map_err(|e| {
                                            JsonParseErrorWithContext::new(
                                                e,
                                                input,
                                                pos,
                                                wip.path(),
                                            )
                                        })?;
                                        trace!(
                                            "immediately after skip over value, we're at pos {}, char is {}",
                                            pos,
                                            input.get(pos).copied().unwrap_or(b'$') as char
                                        );
                                    }
                                } else {
                                    trace!(
                                        "Getting field {}, not in a Struct, but in a... {}",
                                        value.blue(),
                                        wip.shape()
                                    );
                                    wip = wip.field_named(&value).expect("assuming only structs have a fixed set of fields (which is not true, cf. enums)");
                                }
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
                        let number = number.parse::<f64>().unwrap();
                        trace!("Parsed {:?}", number.yellow());

                        // Prefer try_put_f64 only if actually supported (can_put_f64)
                        if wip.can_put_f64() {
                            wip = wip.try_put_f64(number).unwrap();
                        } else {
                            // If string affinity (eg, expecting a string, but got number)
                            let shape = wip.shape();
                            if let Def::Scalar(sd) = shape.def {
                                if let ScalarAffinity::String(_) = sd.affinity {
                                    if shape.is_type::<String>() {
                                        let value = number.to_string();
                                        bail!(JsonErrorKind::StringAsNumber(value));
                                    }
                                }
                            }
                            // fallback for other shape mismatch (todo! or parse error)
                            bail!(JsonErrorKind::NumberOutOfRange(number));
                        }
                        finished_value = Some(why);
                    }
                    b't' | b'f' => {
                        // Boolean: true or false
                        if input[pos..].starts_with(b"true") {
                            pos += 4;
                            let shape = wip.shape();
                            match shape.def {
                                Def::Scalar(sd) => match sd.affinity {
                                    ScalarAffinity::Boolean(_) => {
                                        wip = wip.put::<bool>(true).unwrap();
                                    }
                                    _ => {
                                        bail!(JsonErrorKind::UnexpectedCharacter('t'));
                                    }
                                },
                                _ => {
                                    bail!(JsonErrorKind::UnexpectedCharacter('t'));
                                }
                            }
                            finished_value = Some(why);
                        } else if input[pos..].starts_with(b"false") {
                            pos += 5;
                            let shape = wip.shape();
                            match shape.def {
                                Def::Scalar(sd) => match sd.affinity {
                                    ScalarAffinity::Boolean(_) => {
                                        wip = wip.put::<bool>(false).unwrap();
                                    }
                                    _ => {
                                        bail!(JsonErrorKind::UnexpectedCharacter('f'));
                                    }
                                },
                                _ => {
                                    bail!(JsonErrorKind::UnexpectedCharacter('f'));
                                }
                            }
                            finished_value = Some(why);
                        } else {
                            bail!(JsonErrorKind::UnexpectedCharacter(c as char));
                        }
                    }
                    b'n' => {
                        // wow it's a null — probably
                        let slice_rest = &input[pos..];
                        if slice_rest.starts_with(b"null") {
                            pos += 4;

                            // ok but we already pushed some! luckily wip has the method for us
                            wip = wip.pop_some_push_none().unwrap();
                            finished_value = Some(why);
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
                    b'}' => match why {
                        WhyComma::Object => {
                            pos += 1;
                            finished_value = Some(WhyValue::ObjectValue);
                        }
                        _ => {
                            bail!(JsonErrorKind::UnexpectedCharacter(c as char));
                        }
                    },
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

        if let Some(why) = finished_value {
            trace!("Just finished value because of {:?}", why.green());
            match why {
                WhyValue::ObjectKey => {}
                WhyValue::TopLevel | WhyValue::ObjectValue | WhyValue::ArrayElement => {
                    trace!("Shape before popping: {}", wip.shape());

                    let struct_has_default = wip
                        .shape()
                        .attributes
                        .iter()
                        .any(|attr| matches!(attr, ShapeAttribute::Default));
                    let mut has_missing_fields = false;

                    // Ensure all struct fields are set before popping
                    if let Def::Struct(sd) = wip.shape().def {
                        for i in 0..sd.fields.len() {
                            if !wip.is_field_set(i).unwrap() {
                                let missing_field: &'static str = sd.fields[i].name;
                                if struct_has_default {
                                    has_missing_fields = true;
                                } else {
                                    let field = sd.fields[i];
                                    if let Some(attr) =
                                        field.attributes.iter().find_map(|attr| match attr {
                                            FieldAttribute::Default(d) => Some(d),
                                            _ => None,
                                        })
                                    {
                                        match attr {
                                            Some(fun) => {
                                                wip = wip
                                                    .field(i)
                                                    .unwrap()
                                                    .put_from_fn(*fun)
                                                    .unwrap()
                                                    .pop()
                                                    .unwrap();
                                            }
                                            None => {
                                                wip = wip
                                                    .field(i)
                                                    .unwrap()
                                                    .put_default()
                                                    .unwrap()
                                                    .pop()
                                                    .unwrap();
                                            }
                                        }
                                    } else {
                                        bail!(JsonErrorKind::MissingField(missing_field));
                                    }
                                }
                            }
                        }
                    }

                    if has_missing_fields {
                        trace!("struct has missing fields but we have default");
                        if !wip.shape().is(Characteristic::Default) {
                            todo!(
                                "Default struct has missing fields, the `default` impl but it does not implement Default"
                            )
                        }
                        let default_struct_val = Wip::alloc_shape(wip.shape())
                            .put_default()
                            .unwrap()
                            .build()
                            .unwrap();
                        let peek = default_struct_val.peek().into_struct().unwrap();

                        // For every missing field, take it from the peek and copy it into the wip
                        if let Def::Struct(sd) = wip.shape().def {
                            for i in 0..sd.fields.len() {
                                if !wip.is_field_set(i).unwrap() {
                                    let field_value = peek.field(i).unwrap();
                                    wip = wip
                                        .field(i)
                                        .unwrap()
                                        .put_peek(field_value)
                                        .unwrap()
                                        .pop()
                                        .unwrap();
                                }
                            }
                        }
                    }

                    if frame_count == 1 {
                        return Ok(wip.build().unwrap());
                    } else {
                        wip = wip.pop().unwrap();
                    }
                }
            }
        }
    }
}

fn skip_over_value(pos: &mut usize, input: &[u8]) -> Result<(), JsonErrorKind> {
    let bytes = input;

    // Helper for skipping whitespace
    let skip_whitespace = |pos: &mut usize| {
        while *pos < bytes.len() {
            match bytes[*pos] {
                b' ' | b'\t' | b'\n' | b'\r' => *pos += 1,
                _ => break,
            }
        }
    };

    skip_whitespace(pos);

    if *pos >= bytes.len() {
        return Err(JsonErrorKind::UnexpectedEof(
            "while skipping over value: input ended unexpectedly at root",
        ));
    }

    match bytes[*pos] {
        b'{' => {
            // Skip a full object, recursively
            *pos += 1;
            skip_whitespace(pos);
            if *pos < bytes.len() && bytes[*pos] == b'}' {
                *pos += 1;
                return Ok(());
            }
            loop {
                // Skip key
                skip_over_value(pos, input)?;
                skip_whitespace(pos);
                // Expect colon between key and value
                if *pos >= bytes.len() || bytes[*pos] != b':' {
                    return Err(JsonErrorKind::UnexpectedEof(
                        "while skipping over value: object key with no colon or input ended",
                    ));
                }
                *pos += 1;
                skip_whitespace(pos);
                // Skip value
                skip_over_value(pos, input)?;
                skip_whitespace(pos);
                if *pos >= bytes.len() {
                    return Err(JsonErrorKind::UnexpectedEof(
                        "while skipping over value: object value with EOF after",
                    ));
                }
                if bytes[*pos] == b'}' {
                    *pos += 1;
                    break;
                } else if bytes[*pos] == b',' {
                    *pos += 1;
                    skip_whitespace(pos);
                    continue;
                } else {
                    return Err(JsonErrorKind::UnexpectedCharacter(bytes[*pos] as char));
                }
            }
        }
        b'[' => {
            // Skip a full array, recursively
            *pos += 1;
            skip_whitespace(pos);
            if *pos < bytes.len() && bytes[*pos] == b']' {
                *pos += 1;
                return Ok(());
            }
            loop {
                skip_over_value(pos, input)?;
                skip_whitespace(pos);
                if *pos >= bytes.len() {
                    return Err(JsonErrorKind::UnexpectedEof(
                        "while skipping over value: EOF inside array",
                    ));
                }
                if bytes[*pos] == b']' {
                    *pos += 1;
                    break;
                } else if bytes[*pos] == b',' {
                    *pos += 1;
                    skip_whitespace(pos);
                    continue;
                } else {
                    return Err(JsonErrorKind::UnexpectedCharacter(bytes[*pos] as char));
                }
            }
        }
        b'"' => {
            // Skip a string, with escape processing
            *pos += 1;
            while *pos < bytes.len() {
                match bytes[*pos] {
                    b'\\' => {
                        // Could have EOF after backslash
                        if *pos + 1 >= bytes.len() {
                            return Err(JsonErrorKind::UnexpectedEof(
                                "while skipping over value: EOF after backslash in string",
                            ));
                        }
                        *pos += 2; // Skip backslash and the next character (escaped)
                    }
                    b'"' => {
                        *pos += 1;
                        break;
                    }
                    _ => {
                        *pos += 1;
                    }
                }
            }
            if *pos > bytes.len() {
                return Err(JsonErrorKind::UnexpectedEof(
                    "while skipping over value: string ended unexpectedly",
                ));
            }
        }
        b't' => {
            // Expect "true"
            if bytes.len() >= *pos + 4 && &bytes[*pos..*pos + 4] == b"true" {
                *pos += 4;
            } else {
                return Err(JsonErrorKind::UnexpectedCharacter('t'));
            }
        }
        b'f' => {
            // Expect "false"
            if bytes.len() >= *pos + 5 && &bytes[*pos..*pos + 5] == b"false" {
                *pos += 5;
            } else {
                return Err(JsonErrorKind::UnexpectedCharacter('f'));
            }
        }
        b'n' => {
            // Expect "null"
            if bytes.len() >= *pos + 4 && &bytes[*pos..*pos + 4] == b"null" {
                *pos += 4;
            } else {
                return Err(JsonErrorKind::UnexpectedCharacter('n'));
            }
        }
        b'-' | b'0'..=b'9' => {
            // Skip a number: -?\d+(\.\d+)?([eE][+-]?\d+)?
            let start = *pos;
            if bytes[*pos] == b'-' {
                *pos += 1;
            }
            if *pos < bytes.len() && bytes[*pos] == b'0' {
                *pos += 1;
            } else {
                while *pos < bytes.len() && (bytes[*pos] as char).is_ascii_digit() {
                    *pos += 1;
                }
            }
            if *pos < bytes.len() && bytes[*pos] == b'.' {
                *pos += 1;
                let mut has_digit = false;
                while *pos < bytes.len() && (bytes[*pos] as char).is_ascii_digit() {
                    *pos += 1;
                    has_digit = true;
                }
                if !has_digit {
                    return Err(JsonErrorKind::UnexpectedCharacter('.'));
                }
            }
            if *pos < bytes.len() && (bytes[*pos] == b'e' || bytes[*pos] == b'E') {
                *pos += 1;
                if *pos < bytes.len() && (bytes[*pos] == b'+' || bytes[*pos] == b'-') {
                    *pos += 1;
                }
                let mut has_digit = false;
                while *pos < bytes.len() && (bytes[*pos] as char).is_ascii_digit() {
                    *pos += 1;
                    has_digit = true;
                }
                if !has_digit {
                    return Err(JsonErrorKind::UnexpectedCharacter('e'));
                }
            }
            if *pos == start {
                return Err(JsonErrorKind::UnexpectedCharacter(bytes[start] as char));
            }
        }
        _ => {
            return Err(JsonErrorKind::UnexpectedCharacter(bytes[*pos] as char));
        }
    }
    Ok(())
}
