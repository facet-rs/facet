# XML Serializer: Remove Intermediate Allocations

## Goal

Eliminate `String` allocations when serializing attributes. Currently `value_to_string` creates a `String`, which gets passed to `attribute()`, which stores it in `pending_attributes`, which later gets written out. That's wasteful.

## Plan

### 1. Add `EscapingWriter`

Wraps a `&mut dyn Write` and escapes XML special characters as bytes pass through.

```rust
struct EscapingWriter<'a> {
    inner: &'a mut dyn Write,
    escape_quotes: bool,  // true for attributes, false for text
}

impl<'a> EscapingWriter<'a> {
    fn text(inner: &'a mut dyn Write) -> Self {
        Self { inner, escape_quotes: false }
    }
    
    fn attribute(inner: &'a mut dyn Write) -> Self {
        Self { inner, escape_quotes: true }
    }
}

impl Write for EscapingWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for &b in buf {
            match b {
                b'&' => self.inner.write_all(b"&amp;")?,
                b'<' => self.inner.write_all(b"&lt;")?,
                b'>' => self.inner.write_all(b"&gt;")?,
                b'"' if self.escape_quotes => self.inner.write_all(b"&quot;")?,
                _ => self.inner.write_all(&[b])?,
            }
        }
        Ok(buf.len())
    }
    
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
```

### 2. Add `write_scalar_value`

Writes a scalar `Peek` value directly to a `&mut dyn Write`. No escaping - caller wraps with `EscapingWriter` if needed.

```rust
fn write_scalar_value(
    out: &mut dyn Write,
    value: Peek<'_, '_>,
    float_formatter: Option<FloatFormatter>,
) -> bool {
    // Handle Option<T> - unwrap if Some, return false if None
    if let Def::Option(_) = &value.shape().def {
        if let Ok(opt) = value.into_option() {
            return match opt.value() {
                Some(inner) => write_scalar_value(out, inner, float_formatter),
                None => false,
            };
        }
    }

    let Some(scalar_type) = value.scalar_type() else {
        // Try Display for Def::Scalar types (SmolStr, etc.)
        if matches!(value.shape().def, Def::Scalar) && value.shape().vtable.has_display() {
            let _ = write!(out, "{}", value);
            return true;
        }
        return false;
    };

    match scalar_type {
        ScalarType::Unit => {
            let _ = out.write_all(b"null");
        }
        ScalarType::Bool => {
            let b = value.get::<bool>().unwrap();
            let _ = out.write_all(if *b { b"true" } else { b"false" });
        }
        ScalarType::Char => {
            let c = value.get::<char>().unwrap();
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            let _ = out.write_all(s.as_bytes());
        }
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            let s = value.as_str().unwrap();
            let _ = out.write_all(s.as_bytes());
        }
        ScalarType::F32 => {
            let v = value.get::<f32>().unwrap();
            if let Some(fmt) = float_formatter {
                let _ = fmt(*v as f64, out);
            } else {
                let _ = write!(out, "{}", v);
            }
        }
        ScalarType::F64 => {
            let v = value.get::<f64>().unwrap();
            if let Some(fmt) = float_formatter {
                let _ = fmt(*v, out);
            } else {
                let _ = write!(out, "{}", v);
            }
        }
        ScalarType::U8 => { let _ = write!(out, "{}", value.get::<u8>().unwrap()); }
        ScalarType::U16 => { let _ = write!(out, "{}", value.get::<u16>().unwrap()); }
        ScalarType::U32 => { let _ = write!(out, "{}", value.get::<u32>().unwrap()); }
        ScalarType::U64 => { let _ = write!(out, "{}", value.get::<u64>().unwrap()); }
        ScalarType::U128 => { let _ = write!(out, "{}", value.get::<u128>().unwrap()); }
        ScalarType::USize => { let _ = write!(out, "{}", value.get::<usize>().unwrap()); }
        ScalarType::I8 => { let _ = write!(out, "{}", value.get::<i8>().unwrap()); }
        ScalarType::I16 => { let _ = write!(out, "{}", value.get::<i16>().unwrap()); }
        ScalarType::I32 => { let _ = write!(out, "{}", value.get::<i32>().unwrap()); }
        ScalarType::I64 => { let _ = write!(out, "{}", value.get::<i64>().unwrap()); }
        ScalarType::I128 => { let _ = write!(out, "{}", value.get::<i128>().unwrap()); }
        ScalarType::ISize => { let _ = write!(out, "{}", value.get::<isize>().unwrap()); }
        #[cfg(feature = "net")]
        ScalarType::IpAddr => { let _ = write!(out, "{}", value.get::<core::net::IpAddr>().unwrap()); }
        #[cfg(feature = "net")]
        ScalarType::Ipv4Addr => { let _ = write!(out, "{}", value.get::<core::net::Ipv4Addr>().unwrap()); }
        #[cfg(feature = "net")]
        ScalarType::Ipv6Addr => { let _ = write!(out, "{}", value.get::<core::net::Ipv6Addr>().unwrap()); }
        #[cfg(feature = "net")]
        ScalarType::SocketAddr => { let _ = write!(out, "{}", value.get::<core::net::SocketAddr>().unwrap()); }
        _ => return false,
    }
    true
}
```

### 3. Change `write_attribute` to take `Peek`

```rust
fn write_attribute(&mut self, name: &str, value: Peek<'_, '_>, namespace: Option<&str>) {
    self.out.push(b' ');
    
    if let Some(ns_uri) = namespace {
        let prefix = self.get_or_create_prefix(ns_uri);
        self.out.extend_from_slice(b"xmlns:");
        self.out.extend_from_slice(prefix.as_bytes());
        self.out.extend_from_slice(b"=\"");
        self.out.extend_from_slice(ns_uri.as_bytes());
        self.out.extend_from_slice(b"\" ");
        self.out.extend_from_slice(prefix.as_bytes());
        self.out.push(b':');
    }
    
    self.out.extend_from_slice(name.as_bytes());
    self.out.extend_from_slice(b"=\"");
    
    write_scalar_value(
        &mut EscapingWriter::attribute(&mut self.out),
        value,
        self.options.float_formatter,
    );
    
    self.out.push(b'"');
}
```

### 4. Simplify `DomSerializer::attribute` impl

```rust
fn attribute(
    &mut self,
    name: &str,
    value: Peek<'_, '_>,
    namespace: Option<&str>,
) -> Result<(), Self::Error> {
    if !self.collecting_attributes {
        return Err(XmlSerializeError {
            msg: Cow::Borrowed("attribute() called after children_start()"),
        });
    }
    
    let ns = namespace.or(self.pending_namespace.as_deref());
    self.write_attribute(name, value, ns);
    Ok(())
}
```

### 5. Remove `pending_attributes` field

No longer needed - we write directly to `self.out`.

### 6. Delete `value_to_string` from facet-dom

It's no longer used.

### 7. Delete `format_scalar` from `WriteScalar` trait

It returns `Option<String>` which defeats the purpose.

### 8. Delete `ScalarBuffer`

No longer needed.

### 9. Write tests

```rust
#[test]
fn attribute_before_children_start_works() {
    let mut ser = XmlSerializer::new();
    ser.element_start("foo", None).unwrap();
    
    let value = 42i32;
    let peek = Peek::new(&value);
    ser.attribute("x", peek, None).unwrap();
    
    ser.children_start().unwrap();
    ser.children_end().unwrap();
    ser.element_end("foo").unwrap();
    
    let xml = String::from_utf8(ser.finish()).unwrap();
    assert_eq!(xml, r#"<foo x="42"></foo>"#);
}

#[test]
fn attribute_after_children_start_errors() {
    let mut ser = XmlSerializer::new();
    ser.element_start("foo", None).unwrap();
    ser.children_start().unwrap();
    
    let value = 42i32;
    let peek = Peek::new(&value);
    let result = ser.attribute("x", peek, None);
    
    assert!(result.is_err());
}
```

## Cleanup

After all the above:

- Remove `EscapeMode` enum (replaced by `EscapingWriter`)
- Remove old `write_escaped` function
- Remove `WriteAdapter` 
- Remove `write_int`, `write_display`, `write_float` helpers (inlined into `write_scalar_value`)
