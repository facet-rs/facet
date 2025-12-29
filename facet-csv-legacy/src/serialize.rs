use facet_core::{Def, Facet, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek, ScalarType};
use std::io::{self, Write};

/// Serializes a value to CSV
pub fn to_string<'facet, T: Facet<'facet>>(value: &'facet T) -> String {
    let peek = Peek::new(value);
    let mut output = Vec::new();
    let mut serializer = CsvSerializer::new(&mut output);
    serialize_value(peek, &mut serializer).unwrap();
    String::from_utf8(output).unwrap()
}

/// Serializes a Peek instance to CSV
pub fn peek_to_string<'facet>(peek: Peek<'_, 'facet>) -> String {
    let mut output = Vec::new();
    let mut serializer = CsvSerializer::new(&mut output);
    serialize_value(peek, &mut serializer).unwrap();
    String::from_utf8(output).unwrap()
}

/// Serializes a value to a writer in CSV format
pub fn to_writer<'a, T: Facet<'a>, W: Write>(value: &'a T, writer: &mut W) -> io::Result<()> {
    let peek = Peek::new(value);
    let mut serializer = CsvSerializer::new(writer);
    serialize_value(peek, &mut serializer)
}

/// Serializes a Peek instance to a writer in CSV format
pub fn peek_to_writer<'facet, W: Write>(peek: Peek<'_, 'facet>, writer: &mut W) -> io::Result<()> {
    let mut serializer = CsvSerializer::new(writer);
    serialize_value(peek, &mut serializer)
}

/// A struct to handle the CSV serializer logic
pub struct CsvSerializer<W> {
    /// Owned writer
    writer: W,

    /// The current position in a row
    pos: usize,

    /// Initialized by `start_object`
    n_fields: usize,

    /// Delimeter used to separate values
    delim: &'static [u8],

    /// Newline encoding
    newline: &'static [u8],
}

impl<W> CsvSerializer<W>
where
    W: Write,
{
    /// Initializes a new CSV Serializer
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            pos: 0,
            n_fields: 0,
            delim: b",",
            newline: b"\n",
        }
    }

    fn set_n_fields(&mut self, n_fields: usize) {
        self.n_fields = n_fields;
    }

    /// Conditionally prefix the value with the required delimeter
    fn start_value(&mut self) -> Result<(), io::Error> {
        if self.pos == 0 {
            // no prefix
            Ok(())
        } else {
            self.writer.write_all(self.delim)
        }
    }

    /// Conditionally suffix the value with the required newline
    fn end_value(&mut self) -> Result<(), io::Error> {
        if self.pos == self.n_fields - 1 {
            // Reset the position to zero
            self.pos = 0;
            self.writer.write_all(self.newline)
        } else {
            // Increment the position
            self.pos += 1;
            // no suffix
            Ok(())
        }
    }

    fn write_empty(&mut self) -> io::Result<()> {
        self.start_value()?;
        self.end_value()
    }
}

fn serialize_value<W: Write>(peek: Peek<'_, '_>, ser: &mut CsvSerializer<W>) -> io::Result<()> {
    match (peek.shape().def, peek.shape().ty) {
        (Def::Scalar, _) => {
            let peek = peek.innermost_peek();
            serialize_scalar(peek, ser)
        }
        (Def::Option(_), _) => {
            let opt = peek.into_option().unwrap();
            if let Some(inner) = opt.value() {
                serialize_value(inner, ser)
            } else {
                ser.write_empty()
            }
        }
        (Def::Pointer(_), _) => {
            let ptr = peek.into_pointer().unwrap();
            if let Some(inner) = ptr.borrow_inner() {
                serialize_value(inner, ser)
            } else {
                ser.write_empty()
            }
        }
        (_, Type::User(UserType::Struct(sd))) => {
            match sd.kind {
                StructKind::Unit => {
                    // Unit structs serialize as empty
                    ser.write_empty()
                }
                StructKind::Tuple | StructKind::TupleStruct | StructKind::Struct => {
                    let ps = peek.into_struct().unwrap();
                    let fields: Vec<_> = ps.fields_for_serialize().collect();
                    ser.set_n_fields(fields.len());
                    for (_, field_value) in fields {
                        serialize_value(field_value, ser)?;
                    }
                    Ok(())
                }
            }
        }
        (_, Type::User(UserType::Enum(_))) => {
            // Unit variants should not serialize to anything
            ser.write_empty()
        }
        (_, Type::Pointer(_)) => {
            // Handle string types
            if let Some(s) = peek.as_str() {
                ser.start_value()?;
                write!(ser.writer, "{s}")?;
                ser.end_value()
            } else {
                let innermost = peek.innermost_peek();
                if innermost.shape() != peek.shape() {
                    serialize_value(innermost, ser)
                } else {
                    ser.write_empty()
                }
            }
        }
        _ => {
            // Unsupported types serialize as empty
            ser.write_empty()
        }
    }
}

fn serialize_scalar<W: Write>(peek: Peek<'_, '_>, ser: &mut CsvSerializer<W>) -> io::Result<()> {
    match peek.scalar_type() {
        Some(ScalarType::Unit) => ser.write_empty(),
        Some(ScalarType::Bool) => {
            let v = *peek.get::<bool>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{}", if v { "true" } else { "false" })?;
            ser.end_value()
        }
        Some(ScalarType::Char) => {
            let c = *peek.get::<char>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{c}")?;
            ser.end_value()
        }
        Some(ScalarType::Str) => {
            let s = peek.get::<str>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{s}")?;
            ser.end_value()
        }
        Some(ScalarType::String) => {
            let s = peek.get::<String>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{s}")?;
            ser.end_value()
        }
        Some(ScalarType::CowStr) => {
            let s = peek.get::<alloc::borrow::Cow<'_, str>>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{s}")?;
            ser.end_value()
        }
        Some(ScalarType::F32) => {
            let v = *peek.get::<f32>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::F64) => {
            let v = *peek.get::<f64>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::U8) => {
            let v = *peek.get::<u8>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::U16) => {
            let v = *peek.get::<u16>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::U32) => {
            let v = *peek.get::<u32>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::U64) => {
            let v = *peek.get::<u64>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::U128) => {
            let v = *peek.get::<u128>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::USize) => {
            let v = *peek.get::<usize>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::I8) => {
            let v = *peek.get::<i8>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::I16) => {
            let v = *peek.get::<i16>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::I32) => {
            let v = *peek.get::<i32>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::I64) => {
            let v = *peek.get::<i64>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::I128) => {
            let v = *peek.get::<i128>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(ScalarType::ISize) => {
            let v = *peek.get::<isize>().unwrap();
            ser.start_value()?;
            write!(ser.writer, "{v}")?;
            ser.end_value()
        }
        Some(_) | None => {
            // Unknown scalar - try to display it
            ser.start_value()?;
            write!(ser.writer, "{peek}")?;
            ser.end_value()
        }
    }
}
