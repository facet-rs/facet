use crate::Serializer;

use alloc::vec::Vec;

struct DebugSerializer<W> {
    writer: W,
    need_comma: Vec<bool>,
}

#[derive(Debug)]
enum DebugError {
    Fmt(core::fmt::Error),
}

impl core::fmt::Display for DebugError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}

impl core::error::Error for DebugError {}

impl From<core::fmt::Error> for DebugError {
    fn from(err: core::fmt::Error) -> Self {
        DebugError::Fmt(err)
    }
}

impl<W> Serializer for DebugSerializer<W>
where
    W: core::fmt::Write,
{
    type Error = DebugError;

    fn serialize_u8(&mut self, value: u8) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_u16(&mut self, value: u16) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_u32(&mut self, value: u32) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_u64(&mut self, value: u64) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_u128(&mut self, value: u128) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_usize(&mut self, value: usize) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_i8(&mut self, value: i8) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_i16(&mut self, value: i16) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_i32(&mut self, value: i32) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_i64(&mut self, value: i64) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_i128(&mut self, value: i128) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_isize(&mut self, value: isize) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_f32(&mut self, value: f32) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_f64(&mut self, value: f64) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_bool(&mut self, value: bool) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{}", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_char(&mut self, value: char) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "\"{}\"", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_str(&mut self, value: &str) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "\"{}\"", value)?;
        self.set_comma();
        Ok(())
    }

    fn serialize_bytes(&mut self, value: &[u8]) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "[")?;

        self.need_comma.push(false);
        for byte in value.iter() {
            self.write_comma()?;
            write!(self.writer, "{}", byte)?;
            self.set_comma();
        }
        self.need_comma.pop();

        write!(self.writer, "]")?;
        self.set_comma();
        Ok(())
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "null")?;
        self.set_comma();
        Ok(())
    }

    fn serialize_unit(&mut self) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "null")?;
        self.set_comma();
        Ok(())
    }

    fn serialize_unit_variant(
        &mut self,
        _variant_index: usize,
        variant_name: &'static str,
    ) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "\"{}\"", variant_name)?;
        self.set_comma();
        Ok(())
    }

    fn start_object(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{{")?;
        self.need_comma.push(false);
        Ok(())
    }

    fn end_object(&mut self) -> Result<(), Self::Error> {
        self.need_comma.pop();
        write!(self.writer, "}}")?;
        self.set_comma();
        Ok(())
    }

    fn start_array(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "[")?;
        self.need_comma.push(false);
        Ok(())
    }

    fn end_array(&mut self) -> Result<(), Self::Error> {
        self.need_comma.pop();
        write!(self.writer, "]")?;
        self.set_comma();
        Ok(())
    }

    fn start_map(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "{{")?;
        self.need_comma.push(false);
        Ok(())
    }

    fn end_map(&mut self) -> Result<(), Self::Error> {
        self.need_comma.pop();
        write!(self.writer, "}}")?;
        self.set_comma();
        Ok(())
    }

    fn serialize_field_name(&mut self, name: &'static str) -> Result<(), Self::Error> {
        self.write_comma()?;
        write!(self.writer, "\"{}\":", name)?;
        if let Some(need_comma) = self.need_comma.last_mut() {
            *need_comma = false;
        }
        Ok(())
    }
}

impl<W> DebugSerializer<W>
where
    W: core::fmt::Write,
{
    fn write_comma(&mut self) -> Result<(), DebugError> {
        if let Some(&true) = self.need_comma.last() {
            write!(self.writer, ", ")?;
        }
        Ok(())
    }

    fn set_comma(&mut self) {
        if let Some(need_comma) = self.need_comma.last_mut() {
            *need_comma = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use facet::Facet;
    use facet_reflect::Peek;

    use crate::serialize_iterative;

    use super::DebugSerializer;

    #[derive(Facet)]
    struct FooBarBaz {
        foo: u32,
        bar: String,
        baz: bool,
    }

    #[test]
    fn test_serialize_debug() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let val = FooBarBaz {
            foo: 42,
            bar: "Hello".to_string(),
            baz: true,
        };
        let peek = Peek::new(&val);

        let mut s = String::new();
        let mut serializer = DebugSerializer {
            writer: &mut s,
            need_comma: vec![false],
        };
        serialize_iterative(peek, &mut serializer)?;
        #[cfg(not(miri))]
        insta::assert_snapshot!(s);

        Ok(())
    }

    #[derive(Facet)]
    struct FooBarBazContainer {
        foo_bar_baz: FooBarBaz,
        other_things: Vec<SomeEnum>,
        maybe_value: Option<u32>,
        definitely_none: Option<String>,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SomeEnum {
        Variant1,
        Variant2(u32, String),
        Variant3 { field1: u32, field2: String },
    }

    #[test]
    fn test_serialize_debug_container() {
        facet_testhelpers::setup();

        let val = FooBarBazContainer {
            foo_bar_baz: FooBarBaz {
                foo: 42,
                bar: "Hello".to_string(),
                baz: true,
            },
            other_things: vec![
                SomeEnum::Variant1,
                SomeEnum::Variant2(123, "TupleVariant".to_string()),
                SomeEnum::Variant3 {
                    field1: 456,
                    field2: "StructVariant".to_string(),
                },
            ],
            maybe_value: Some(42),
            definitely_none: None,
        };
        let peek = Peek::new(&val);

        let mut s = String::new();
        let mut serializer = DebugSerializer {
            writer: &mut s,
            need_comma: vec![false],
        };
        serialize_iterative(peek, &mut serializer).unwrap();
        #[cfg(not(miri))]
        insta::assert_snapshot!(s);
    }

    #[derive(Facet)]
    #[facet(transparent)]
    #[repr(transparent)]
    struct Wrapper(String);

    #[test]
    fn test_serialize_transparent() {
        facet_testhelpers::setup();

        let val = Wrapper("TransparentValue".to_string());
        let peek = Peek::new(&val);

        let mut s = String::new();
        let mut serializer = DebugSerializer {
            writer: &mut s,
            need_comma: vec![false],
        };
        serialize_iterative(peek, &mut serializer).unwrap();
        #[cfg(not(miri))]
        insta::assert_snapshot!(s);
    }
}
