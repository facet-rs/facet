use super::Field;

/// Common fields for struct-like types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct Struct {
    /// the kind of struct (e.g. struct, tuple struct, tuple)
    pub kind: StructKind,

    /// all fields, in declaration order (not necessarily in memory order)
    pub fields: &'static [Field],
}

impl Struct {
    /// Returns a builder for StructDef
    pub const fn builder() -> StructDefBuilder {
        StructDefBuilder::new()
    }
}

/// Builder for StructDef
pub struct StructDefBuilder {
    kind: Option<StructKind>,
    fields: Option<&'static [Field]>,
}

impl StructDefBuilder {
    /// Creates a new StructDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            kind: None,
            fields: None,
        }
    }

    /// Sets the kind for the StructDef
    pub const fn kind(mut self, kind: StructKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Sets the fields for the StructDef
    pub const fn fields(mut self, fields: &'static [Field]) -> Self {
        self.fields = Some(fields);
        self
    }

    /// Builds the StructDef
    pub const fn build(self) -> Struct {
        Struct {
            kind: self.kind.unwrap(),
            fields: self.fields.unwrap(),
        }
    }
}

/// Describes the kind of struct (useful for deserializing)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum StructKind {
    /// struct UnitStruct;
    Unit,

    /// struct TupleStruct(T0, T1);
    TupleStruct,

    /// struct S { foo: T0, bar: T1 }
    Struct,

    /// (T0, T1)
    Tuple,
}
