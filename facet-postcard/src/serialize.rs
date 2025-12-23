use crate::error::SerializeError;
use facet_core::{Def, Facet, Shape, StructKind, Type, UserType};
use facet_path::{Path, PathStep};
use facet_reflect::{HasFields, Peek, ScalarType};
use log::trace;

#[cfg(feature = "alloc")]
use alloc::{borrow::Cow, string::String, vec::Vec};

/// Serializes any Facet type to postcard bytes.
///
/// # Example
/// ```
/// use facet::Facet;
/// use facet_postcard::to_vec;
///
/// #[derive(Debug, Facet)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let point = Point { x: 10, y: 20 };
/// let bytes = to_vec(&point).unwrap();
/// ```
#[cfg(feature = "alloc")]
pub fn to_vec<T: Facet<'static>>(value: &T) -> Result<Vec<u8>, SerializeError> {
    let peek = Peek::new(value);
    ptr_to_vec(peek)
}

/// Serializes any Facet Reflect Peek to postcard bytes.
#[cfg(feature = "alloc")]
pub fn ptr_to_vec<'mem>(peek: Peek<'mem, 'static>) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    let mut ctx = SerializeContext::new(peek.shape());
    serialize_value(peek, &mut buffer, &mut ctx)?;
    Ok(buffer)
}

/// Serializes any Facet type to a provided byte slice.
///
/// Returns the number of bytes written.
///
/// # Example
/// ```
/// use facet::Facet;
/// use facet_postcard::to_slice;
///
/// #[derive(Debug, Facet)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let point = Point { x: 10, y: 20 };
/// let mut buffer = [0u8; 64];
/// let len = to_slice(&point, &mut buffer).unwrap();
/// let bytes = &buffer[..len];
/// ```
pub fn to_slice<T: Facet<'static>>(value: &T, buffer: &mut [u8]) -> Result<usize, SerializeError> {
    let peek = Peek::new(value);
    let mut writer = SliceWriter::new(buffer);
    let mut ctx = SerializeContext::new(peek.shape());
    serialize_value(peek, &mut writer, &mut ctx)?;
    Ok(writer.pos)
}

/// Context for tracking serialization state including the current path
struct SerializeContext {
    /// The path through the type structure
    path: Path,
    /// The root shape for error formatting
    root_shape: &'static Shape,
}

impl SerializeContext {
    fn new(root_shape: &'static Shape) -> Self {
        Self {
            path: Path::new(),
            root_shape,
        }
    }

    fn push(&mut self, step: PathStep) {
        self.path.push(step);
    }

    fn pop(&mut self) {
        self.path.pop();
    }

    fn unsupported_scalar(&self, scalar_type: ScalarType) -> SerializeError {
        SerializeError::UnsupportedScalar {
            scalar_type,
            path: self.path.clone(),
            root_shape: self.root_shape,
        }
    }

    fn unknown_scalar(&self, type_name: &'static str) -> SerializeError {
        SerializeError::UnknownScalar {
            type_name,
            path: self.path.clone(),
            root_shape: self.root_shape,
        }
    }
}

/// A trait for writing bytes during serialization
trait Writer {
    fn write_byte(&mut self, byte: u8) -> Result<(), SerializeError>;
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError>;
}

#[cfg(feature = "alloc")]
impl Writer for Vec<u8> {
    fn write_byte(&mut self, byte: u8) -> Result<(), SerializeError> {
        self.push(byte);
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

struct SliceWriter<'a> {
    buffer: &'a mut [u8],
    pos: usize,
}

impl<'a> SliceWriter<'a> {
    fn new(buffer: &'a mut [u8]) -> Self {
        Self { buffer, pos: 0 }
    }
}

impl Writer for SliceWriter<'_> {
    fn write_byte(&mut self, byte: u8) -> Result<(), SerializeError> {
        if self.pos >= self.buffer.len() {
            return Err(SerializeError::BufferTooSmall);
        }
        self.buffer[self.pos] = byte;
        self.pos += 1;
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError> {
        if self.pos + bytes.len() > self.buffer.len() {
            return Err(SerializeError::BufferTooSmall);
        }
        self.buffer[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        Ok(())
    }
}

fn serialize_value<W: Writer>(
    peek: Peek<'_, '_>,
    writer: &mut W,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    trace!("Serializing value, shape is {}", peek.shape());

    match (peek.shape().def, peek.shape().ty) {
        (Def::Scalar, _) => {
            let peek = peek.innermost_peek();
            serialize_scalar(peek, writer, ctx)
        }
        (Def::List(ld), _) => {
            // Special case for Vec<u8> - serialize as bytes
            if ld.t().is_type::<u8>() && peek.shape().is_type::<Vec<u8>>() {
                let bytes = peek.get::<Vec<u8>>().unwrap();
                write_varint(bytes.len() as u64, writer)?;
                return writer.write_bytes(bytes);
            }
            // Special case for Bytes - serialize as bytes
            #[cfg(feature = "bytes")]
            if ld.t().is_type::<u8>() && peek.shape().type_identifier == "Bytes" {
                use bytes::Bytes;
                let bytes = peek.get::<Bytes>().unwrap();
                write_varint(bytes.len() as u64, writer)?;
                return writer.write_bytes(bytes);
            }
            // Special case for BytesMut - serialize as bytes
            #[cfg(feature = "bytes")]
            if ld.t().is_type::<u8>() && peek.shape().type_identifier == "BytesMut" {
                use bytes::BytesMut;
                let bytes_mut = peek.get::<BytesMut>().unwrap();
                write_varint(bytes_mut.len() as u64, writer)?;
                return writer.write_bytes(bytes_mut);
            }
            {
                let list = peek.into_list_like().unwrap();
                let items: Vec<_> = list.iter().collect();
                write_varint(items.len() as u64, writer)?;
                for (i, item) in items.into_iter().enumerate() {
                    ctx.push(PathStep::Index(i as u32));
                    serialize_value(item, writer, ctx)?;
                    ctx.pop();
                }
                Ok(())
            }
        }
        (Def::Array(ad), _) => {
            if ad.t().is_type::<u8>() {
                // Serialize byte arrays directly (length already known from type)
                let bytes: Vec<u8> = peek
                    .into_list_like()
                    .unwrap()
                    .iter()
                    .map(|p| *p.get::<u8>().unwrap())
                    .collect();
                writer.write_bytes(&bytes)
            } else {
                // For fixed-size arrays, postcard doesn't write length
                let list = peek.into_list_like().unwrap();
                for (i, item) in list.iter().enumerate() {
                    ctx.push(PathStep::Index(i as u32));
                    serialize_value(item, writer, ctx)?;
                    ctx.pop();
                }
                Ok(())
            }
        }
        (Def::Slice(sd), _) => {
            if sd.t().is_type::<u8>() {
                let bytes = peek.get::<[u8]>().unwrap();
                write_varint(bytes.len() as u64, writer)?;
                writer.write_bytes(bytes)
            } else {
                let list = peek.into_list_like().unwrap();
                let items: Vec<_> = list.iter().collect();
                write_varint(items.len() as u64, writer)?;
                for (i, item) in items.into_iter().enumerate() {
                    ctx.push(PathStep::Index(i as u32));
                    serialize_value(item, writer, ctx)?;
                    ctx.pop();
                }
                Ok(())
            }
        }
        (Def::Map(_), _) => {
            let map = peek.into_map().unwrap();
            let entries: Vec<_> = map.iter().collect();
            write_varint(entries.len() as u64, writer)?;
            for (key, value) in entries {
                ctx.push(PathStep::MapKey);
                serialize_value(key, writer, ctx)?;
                ctx.pop();
                ctx.push(PathStep::MapValue);
                serialize_value(value, writer, ctx)?;
                ctx.pop();
            }
            Ok(())
        }
        (Def::Set(_), _) => {
            let set = peek.into_set().unwrap();
            let items: Vec<_> = set.iter().collect();
            write_varint(items.len() as u64, writer)?;
            for (i, item) in items.into_iter().enumerate() {
                ctx.push(PathStep::Index(i as u32));
                serialize_value(item, writer, ctx)?;
                ctx.pop();
            }
            Ok(())
        }
        (Def::Option(_), _) => {
            let opt = peek.into_option().unwrap();
            if let Some(inner) = opt.value() {
                writer.write_byte(1)?; // Some
                ctx.push(PathStep::OptionSome);
                let result = serialize_value(inner, writer, ctx);
                ctx.pop();
                result
            } else {
                writer.write_byte(0) // None
            }
        }
        (Def::Pointer(_), _) => {
            let ptr = peek.into_pointer().unwrap();
            if let Some(inner) = ptr.borrow_inner() {
                ctx.push(PathStep::Deref);
                let result = serialize_value(inner, writer, ctx);
                ctx.pop();
                result
            } else {
                Err(SerializeError::UnsupportedType(
                    "Smart pointer without borrow support",
                ))
            }
        }
        (_, Type::User(UserType::Struct(sd))) => {
            match sd.kind {
                StructKind::Unit => {
                    // Unit structs serialize as nothing
                    Ok(())
                }
                StructKind::Tuple => {
                    let ps = peek.into_struct().unwrap();
                    for (i, (_, field_value)) in ps.fields().enumerate() {
                        ctx.push(PathStep::Field(i as u32));
                        serialize_value(field_value, writer, ctx)?;
                        ctx.pop();
                    }
                    Ok(())
                }
                StructKind::TupleStruct => {
                    let ps = peek.into_struct().unwrap();
                    for (i, (_, field_value)) in ps.fields_for_serialize().enumerate() {
                        ctx.push(PathStep::Field(i as u32));
                        serialize_value(field_value, writer, ctx)?;
                        ctx.pop();
                    }
                    Ok(())
                }
                StructKind::Struct => {
                    // Postcard serializes structs in field order without names
                    let ps = peek.into_struct().unwrap();
                    for (i, (_, field_value)) in ps.fields_for_serialize().enumerate() {
                        ctx.push(PathStep::Field(i as u32));
                        serialize_value(field_value, writer, ctx)?;
                        ctx.pop();
                    }
                    Ok(())
                }
            }
        }
        (_, Type::User(UserType::Enum(et))) => {
            let pe = peek.into_enum().unwrap();
            let variant = pe.active_variant().expect("Failed to get active variant");
            let variant_idx = et
                .variants
                .iter()
                .position(|v| v.name == variant.name)
                .unwrap_or(0);
            trace!(
                "Serializing enum variant {} at index {}",
                variant.name, variant_idx
            );

            // Write variant index as varint
            write_varint(variant_idx as u64, writer)?;

            // Push variant onto path
            ctx.push(PathStep::Variant(variant_idx as u32));

            let result = if variant.data.fields.is_empty() {
                // Unit variant - nothing more to write
                Ok(())
            } else if variant.data.kind == StructKind::Tuple
                || variant.data.kind == StructKind::TupleStruct
            {
                // Tuple variant - serialize fields in order
                for (i, (_, field_value)) in pe.fields_for_serialize().enumerate() {
                    ctx.push(PathStep::Field(i as u32));
                    serialize_value(field_value, writer, ctx)?;
                    ctx.pop();
                }
                Ok(())
            } else {
                // Struct variant - serialize fields in order (no names)
                for (i, (_, field_value)) in pe.fields_for_serialize().enumerate() {
                    ctx.push(PathStep::Field(i as u32));
                    serialize_value(field_value, writer, ctx)?;
                    ctx.pop();
                }
                Ok(())
            };

            ctx.pop();
            result
        }
        (_, Type::Pointer(_)) => {
            // Handle string types
            if let Some(s) = peek.as_str() {
                write_varint(s.len() as u64, writer)?;
                writer.write_bytes(s.as_bytes())
            } else if let Some(bytes) = peek.as_bytes() {
                write_varint(bytes.len() as u64, writer)?;
                writer.write_bytes(bytes)
            } else {
                let innermost = peek.innermost_peek();
                if innermost.shape() != peek.shape() {
                    ctx.push(PathStep::Deref);
                    let result = serialize_value(innermost, writer, ctx);
                    ctx.pop();
                    result
                } else {
                    Err(SerializeError::UnsupportedType("Unknown pointer type"))
                }
            }
        }
        _ => {
            trace!("Unhandled type: {:?}", peek.shape().ty);
            Err(SerializeError::UnsupportedType("Unknown type"))
        }
    }
}

fn serialize_scalar<W: Writer>(
    peek: Peek<'_, '_>,
    writer: &mut W,
    ctx: &SerializeContext,
) -> Result<(), SerializeError> {
    // Check for opaque scalar types that need special handling

    // Camino types (UTF-8 paths)
    #[cfg(feature = "camino")]
    if peek.shape().type_identifier == "Utf8PathBuf" {
        use camino::Utf8PathBuf;
        let path = peek.get::<Utf8PathBuf>().unwrap();
        let s = path.as_str();
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }
    #[cfg(feature = "camino")]
    if peek.shape().type_identifier == "Utf8Path" {
        use camino::Utf8Path;
        let path = peek.get::<Utf8Path>().unwrap();
        let s = path.as_str();
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }

    // UUID - serialize as 16 bytes (native format)
    #[cfg(feature = "uuid")]
    if peek.shape().type_identifier == "Uuid" {
        use uuid::Uuid;
        let uuid = peek.get::<Uuid>().unwrap();
        return writer.write_bytes(uuid.as_bytes());
    }

    // ULID - serialize as 16 bytes (native format)
    #[cfg(feature = "ulid")]
    if peek.shape().type_identifier == "Ulid" {
        use ulid::Ulid;
        let ulid = peek.get::<Ulid>().unwrap();
        return writer.write_bytes(&ulid.to_bytes());
    }

    // Jiff date/time types - serialize as RFC3339 strings
    #[cfg(feature = "jiff02")]
    if peek.shape().type_identifier == "Zoned" {
        use jiff::Zoned;
        let zoned = peek.get::<Zoned>().unwrap();
        let s = zoned.to_string();
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }
    #[cfg(feature = "jiff02")]
    if peek.shape().type_identifier == "Timestamp" {
        use jiff::Timestamp;
        let ts = peek.get::<Timestamp>().unwrap();
        let s = ts.to_string();
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }
    #[cfg(feature = "jiff02")]
    if peek.shape().type_identifier == "DateTime" {
        use jiff::civil::DateTime;
        let dt = peek.get::<DateTime>().unwrap();
        let s = dt.to_string();
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }

    // Chrono date/time types - serialize as RFC3339 strings
    #[cfg(feature = "chrono")]
    if peek.shape().type_identifier == "DateTime<Utc>" {
        use chrono::{DateTime, SecondsFormat, Utc};
        let dt = peek.get::<DateTime<Utc>>().unwrap();
        let s = dt.to_rfc3339_opts(SecondsFormat::AutoSi, true);
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }
    #[cfg(feature = "chrono")]
    if peek.shape().type_identifier == "DateTime<Local>" {
        use chrono::{DateTime, Local, SecondsFormat};
        let dt = peek.get::<DateTime<Local>>().unwrap();
        let s = dt.to_rfc3339_opts(SecondsFormat::AutoSi, false);
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }
    #[cfg(feature = "chrono")]
    if peek.shape().type_identifier == "DateTime<FixedOffset>" {
        use chrono::{DateTime, FixedOffset, SecondsFormat};
        let dt = peek.get::<DateTime<FixedOffset>>().unwrap();
        let s = dt.to_rfc3339_opts(SecondsFormat::AutoSi, false);
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }
    #[cfg(feature = "chrono")]
    if peek.shape().type_identifier == "NaiveDateTime" {
        use chrono::NaiveDateTime;
        let dt = peek.get::<NaiveDateTime>().unwrap();
        // Use same format as facet-core: RFC3339-like without timezone and fractional seconds
        let s = dt.format("%Y-%m-%dT%H:%M:%S").to_string();
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }
    #[cfg(feature = "chrono")]
    if peek.shape().type_identifier == "NaiveDate" {
        use chrono::NaiveDate;
        let date = peek.get::<NaiveDate>().unwrap();
        let s = date.to_string();
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }
    #[cfg(feature = "chrono")]
    if peek.shape().type_identifier == "NaiveTime" {
        use chrono::NaiveTime;
        let time = peek.get::<NaiveTime>().unwrap();
        let s = time.to_string();
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }

    // Time crate date/time types - serialize as RFC3339 strings
    #[cfg(feature = "time")]
    if peek.shape().type_identifier == "UtcDateTime" {
        use time::UtcDateTime;
        let dt = peek.get::<UtcDateTime>().unwrap();
        let s = dt
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "<invalid>".to_string());
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }
    #[cfg(feature = "time")]
    if peek.shape().type_identifier == "OffsetDateTime" {
        use time::OffsetDateTime;
        let dt = peek.get::<OffsetDateTime>().unwrap();
        let s = dt
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "<invalid>".to_string());
        write_varint(s.len() as u64, writer)?;
        return writer.write_bytes(s.as_bytes());
    }

    // OrderedFloat - serialize as the inner float
    #[cfg(feature = "ordered-float")]
    if peek.shape().type_identifier == "OrderedFloat" {
        // Check if it's OrderedFloat<f32> or OrderedFloat<f64> by looking at the inner shape
        if let Some(inner_shape) = peek.shape().inner {
            if inner_shape.is_type::<f32>() {
                use ordered_float::OrderedFloat;
                let val = peek.get::<OrderedFloat<f32>>().unwrap();
                return writer.write_bytes(&val.0.to_le_bytes());
            } else if inner_shape.is_type::<f64>() {
                use ordered_float::OrderedFloat;
                let val = peek.get::<OrderedFloat<f64>>().unwrap();
                return writer.write_bytes(&val.0.to_le_bytes());
            }
        }
    }

    // NotNan - serialize as the inner float
    #[cfg(feature = "ordered-float")]
    if peek.shape().type_identifier == "NotNan" {
        // Check if it's NotNan<f32> or NotNan<f64> by looking at the inner shape
        if let Some(inner_shape) = peek.shape().inner {
            if inner_shape.is_type::<f32>() {
                use ordered_float::NotNan;
                let val = peek.get::<NotNan<f32>>().unwrap();
                return writer.write_bytes(&val.into_inner().to_le_bytes());
            } else if inner_shape.is_type::<f64>() {
                use ordered_float::NotNan;
                let val = peek.get::<NotNan<f64>>().unwrap();
                return writer.write_bytes(&val.into_inner().to_le_bytes());
            }
        }
    }

    match peek.scalar_type() {
        Some(ScalarType::Unit) => Ok(()),
        Some(ScalarType::Bool) => {
            let v = *peek.get::<bool>().unwrap();
            writer.write_byte(if v { 1 } else { 0 })
        }
        Some(ScalarType::Char) => {
            let c = *peek.get::<char>().unwrap();
            let mut buf = [0; 4];
            let s = c.encode_utf8(&mut buf);
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::Str) => {
            let s = peek.get::<str>().unwrap();
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::String) => {
            let s = peek.get::<String>().unwrap();
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::CowStr) => {
            let s = peek.get::<Cow<'_, str>>().unwrap();
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::F32) => {
            let v = *peek.get::<f32>().unwrap();
            writer.write_bytes(&v.to_le_bytes())
        }
        Some(ScalarType::F64) => {
            let v = *peek.get::<f64>().unwrap();
            writer.write_bytes(&v.to_le_bytes())
        }
        Some(ScalarType::U8) => {
            let v = *peek.get::<u8>().unwrap();
            writer.write_byte(v)
        }
        Some(ScalarType::U16) => {
            let v = *peek.get::<u16>().unwrap();
            write_varint(v as u64, writer)
        }
        Some(ScalarType::U32) => {
            let v = *peek.get::<u32>().unwrap();
            write_varint(v as u64, writer)
        }
        Some(ScalarType::U64) => {
            let v = *peek.get::<u64>().unwrap();
            write_varint(v, writer)
        }
        Some(ScalarType::U128) => {
            let v = *peek.get::<u128>().unwrap();
            write_varint_u128(v, writer)
        }
        Some(ScalarType::USize) => {
            let v = *peek.get::<usize>().unwrap();
            write_varint(v as u64, writer)
        }
        Some(ScalarType::I8) => {
            let v = *peek.get::<i8>().unwrap();
            writer.write_byte(v as u8)
        }
        Some(ScalarType::I16) => {
            let v = *peek.get::<i16>().unwrap();
            write_varint_signed(v as i64, writer)
        }
        Some(ScalarType::I32) => {
            let v = *peek.get::<i32>().unwrap();
            write_varint_signed(v as i64, writer)
        }
        Some(ScalarType::I64) => {
            let v = *peek.get::<i64>().unwrap();
            write_varint_signed(v, writer)
        }
        Some(ScalarType::I128) => {
            let v = *peek.get::<i128>().unwrap();
            write_varint_signed_i128(v, writer)
        }
        Some(ScalarType::ISize) => {
            let v = *peek.get::<isize>().unwrap();
            write_varint_signed(v as i64, writer)
        }
        #[cfg(feature = "net")]
        Some(ScalarType::Ipv4Addr) => {
            use core::net::Ipv4Addr;
            let addr = *peek.get::<Ipv4Addr>().unwrap();
            writer.write_bytes(&addr.octets())
        }
        #[cfg(feature = "net")]
        Some(ScalarType::Ipv6Addr) => {
            use core::net::Ipv6Addr;
            let addr = *peek.get::<Ipv6Addr>().unwrap();
            writer.write_bytes(&addr.octets())
        }
        #[cfg(feature = "net")]
        Some(ScalarType::IpAddr) => {
            use core::net::IpAddr;
            let addr = *peek.get::<IpAddr>().unwrap();
            match addr {
                IpAddr::V4(v4) => {
                    writer.write_byte(0)?; // V4 tag
                    writer.write_bytes(&v4.octets())
                }
                IpAddr::V6(v6) => {
                    writer.write_byte(1)?; // V6 tag
                    writer.write_bytes(&v6.octets())
                }
            }
        }
        #[cfg(feature = "net")]
        Some(ScalarType::SocketAddr) => {
            use core::net::SocketAddr;
            let addr = *peek.get::<SocketAddr>().unwrap();
            match addr {
                SocketAddr::V4(v4) => {
                    writer.write_byte(0)?; // V4 tag
                    writer.write_bytes(&v4.ip().octets())?;
                    writer.write_bytes(&v4.port().to_le_bytes())
                }
                SocketAddr::V6(v6) => {
                    writer.write_byte(1)?; // V6 tag
                    writer.write_bytes(&v6.ip().octets())?;
                    writer.write_bytes(&v6.port().to_le_bytes())
                }
            }
        }
        Some(scalar_type) => Err(ctx.unsupported_scalar(scalar_type)),
        None => Err(ctx.unknown_scalar(peek.shape().type_identifier)),
    }
}

/// Write an unsigned varint (LEB128-like encoding used by postcard)
fn write_varint<W: Writer>(mut value: u64, writer: &mut W) -> Result<(), SerializeError> {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        writer.write_byte(byte)?;
        if value == 0 {
            break;
        }
    }
    Ok(())
}

/// Write an unsigned 128-bit varint
fn write_varint_u128<W: Writer>(mut value: u128, writer: &mut W) -> Result<(), SerializeError> {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        writer.write_byte(byte)?;
        if value == 0 {
            break;
        }
    }
    Ok(())
}

/// Write a signed varint using zigzag encoding
fn write_varint_signed<W: Writer>(value: i64, writer: &mut W) -> Result<(), SerializeError> {
    // Zigzag encoding: (value << 1) ^ (value >> 63)
    let encoded = ((value << 1) ^ (value >> 63)) as u64;
    write_varint(encoded, writer)
}

/// Write a signed 128-bit varint using zigzag encoding
fn write_varint_signed_i128<W: Writer>(value: i128, writer: &mut W) -> Result<(), SerializeError> {
    // Zigzag encoding: (value << 1) ^ (value >> 127)
    let encoded = ((value << 1) ^ (value >> 127)) as u128;
    write_varint_u128(encoded, writer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use postcard::to_allocvec as postcard_to_vec;
    use serde::Serialize;

    #[derive(Facet, Serialize, PartialEq, Debug)]
    struct SimpleStruct {
        a: u32,
        b: String,
        c: bool,
    }

    #[test]
    fn test_simple_struct() {
        facet_testhelpers::setup();

        let value = SimpleStruct {
            a: 123,
            b: "hello".to_string(),
            c: true,
        };

        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();

        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_u8() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U8Struct {
            value: u8,
        }

        let value = U8Struct { value: 42 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_u16() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U16Struct {
            value: u16,
        }

        let value = U16Struct { value: 1000 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_u32() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U32Struct {
            value: u32,
        }

        let value = U32Struct { value: 100000 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_u64() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U64Struct {
            value: u64,
        }

        let value = U64Struct { value: 10000000000 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_i8() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct I8Struct {
            value: i8,
        }

        let value = I8Struct { value: -42 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_i16() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct I16Struct {
            value: i16,
        }

        let value = I16Struct { value: -1000 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_i32() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct I32Struct {
            value: i32,
        }

        let value = I32Struct { value: -100000 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_i64() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct I64Struct {
            value: i64,
        }

        let value = I64Struct {
            value: -10000000000,
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_bool() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct BoolStruct {
            value: bool,
        }

        let value = BoolStruct { value: true };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let value = BoolStruct { value: false };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_f32() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct F32Struct {
            value: f32,
        }

        let value = F32Struct { value: 3.15 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_f64() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct F64Struct {
            value: f64,
        }

        let value = F64Struct { value: 3.14159266 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_string() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct StringStruct {
            value: String,
        }

        let value = StringStruct {
            value: "hello world".to_string(),
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_vec() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct VecStruct {
            values: Vec<u32>,
        }

        let value = VecStruct {
            values: vec![1, 2, 3, 4, 5],
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_vec_u8() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct ByteVecStruct {
            bytes: Vec<u8>,
        }

        let value = ByteVecStruct {
            bytes: vec![1, 2, 3, 4, 5],
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_option_some() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct OptionStruct {
            value: Option<u32>,
        }

        let value = OptionStruct { value: Some(42) };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_option_none() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct OptionStruct {
            value: Option<u32>,
        }

        let value = OptionStruct { value: None };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_nested_struct() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct Inner {
            x: i32,
            y: i32,
        }

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct Outer {
            name: String,
            inner: Inner,
        }

        let value = Outer {
            name: "test".to_string(),
            inner: Inner { x: 10, y: 20 },
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_unit_enum() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        #[repr(C)]
        enum Color {
            Red,
            Green,
            Blue,
        }

        let facet_bytes = to_vec(&Color::Red).unwrap();
        let postcard_bytes = postcard_to_vec(&Color::Red).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let facet_bytes = to_vec(&Color::Green).unwrap();
        let postcard_bytes = postcard_to_vec(&Color::Green).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let facet_bytes = to_vec(&Color::Blue).unwrap();
        let postcard_bytes = postcard_to_vec(&Color::Blue).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_tuple_enum() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        #[repr(C)]
        enum Value {
            Int(i32),
            Text(String),
            Pair(i32, i32),
        }

        let facet_bytes = to_vec(&Value::Int(42)).unwrap();
        let postcard_bytes = postcard_to_vec(&Value::Int(42)).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let facet_bytes = to_vec(&Value::Text("hello".to_string())).unwrap();
        let postcard_bytes = postcard_to_vec(&Value::Text("hello".to_string())).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let facet_bytes = to_vec(&Value::Pair(10, 20)).unwrap();
        let postcard_bytes = postcard_to_vec(&Value::Pair(10, 20)).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_struct_enum() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        #[repr(C)]
        enum Message {
            Quit,
            Move { x: i32, y: i32 },
            Write { text: String },
        }

        let facet_bytes = to_vec(&Message::Quit).unwrap();
        let postcard_bytes = postcard_to_vec(&Message::Quit).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let facet_bytes = to_vec(&Message::Move { x: 10, y: 20 }).unwrap();
        let postcard_bytes = postcard_to_vec(&Message::Move { x: 10, y: 20 }).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let facet_bytes = to_vec(&Message::Write {
            text: "hello".to_string(),
        })
        .unwrap();
        let postcard_bytes = postcard_to_vec(&Message::Write {
            text: "hello".to_string(),
        })
        .unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_tuple_struct() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct Point(i32, i32);

        let value = Point(10, 20);
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_unit_struct() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct Unit;

        let value = Unit;
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_fixed_array() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct ArrayStruct {
            values: [u8; 4],
        }

        let value = ArrayStruct {
            values: [1, 2, 3, 4],
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_to_slice() {
        facet_testhelpers::setup();

        let value = SimpleStruct {
            a: 123,
            b: "hello".to_string(),
            c: true,
        };

        let mut buffer = [0u8; 64];
        let len = to_slice(&value, &mut buffer).unwrap();
        let facet_bytes = &buffer[..len];

        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes.as_slice());
    }

    #[test]
    fn test_to_slice_buffer_too_small() {
        facet_testhelpers::setup();

        let value = SimpleStruct {
            a: 123,
            b: "hello".to_string(),
            c: true,
        };

        let mut buffer = [0u8; 2]; // Too small
        let result = to_slice(&value, &mut buffer);
        assert!(matches!(result, Err(SerializeError::BufferTooSmall)));
    }

    #[test]
    fn test_char() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct CharStruct {
            value: char,
        }

        let value = CharStruct { value: 'A' };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        // Test multibyte char
        let value = CharStruct { value: '日' };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_u128() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U128Struct {
            value: u128,
        }

        let value = U128Struct {
            value: 340282366920938463463374607431768211455u128,
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_i128() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct I128Struct {
            value: i128,
        }

        let value = I128Struct {
            value: -170141183460469231731687303715884105728i128,
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_cow_str() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct CowStruct<'a> {
            value: Cow<'a, str>,
        }

        let value = CowStruct {
            value: Cow::Borrowed("hello"),
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let value = CowStruct {
            value: Cow::Owned("world".to_string()),
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }
}
