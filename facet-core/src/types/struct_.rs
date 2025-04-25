use super::Field;

/// Common fields for struct-like types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct StructDef {
    /// the kind of struct (e.g. struct, tuple struct, tuple)
    pub kind: StructKind,

    /// all fields, in declaration order (not necessarily in memory order)
    pub fields: &'static [Field],
}

impl StructDef {
    /// Returns a builder for StructDef
    pub const fn builder() -> StructBuilder {
        StructBuilder::new()
    }
}

/// Builder for StructDef
pub struct StructBuilder {
    kind: Option<StructKind>,
    fields: &'static [Field],
}

impl StructBuilder {
    /// Creates a new StructDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            kind: None,
            fields: &[],
        }
    }
    /// Sets the kind to Unit and returns self
    pub const fn unit(mut self) -> Self {
        self.kind = Some(StructKind::Unit);
        self
    }

    /// Sets the kind to Tuple and returns self
    pub const fn tuple(mut self) -> Self {
        self.kind = Some(StructKind::Tuple);
        self
    }

    /// Sets the kind to Struct and returns self
    pub const fn struct_(mut self) -> Self {
        self.kind = Some(StructKind::Struct);
        self
    }

    /// Sets the kind for the StructDef
    pub const fn kind(mut self, kind: StructKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Sets the fields for the StructDef
    pub const fn fields(mut self, fields: &'static [Field]) -> Self {
        self.fields = fields;
        self
    }

    /// Builds the StructDef
    pub const fn build(self) -> StructDef {
        StructDef {
            kind: self.kind.unwrap(),
            fields: self.fields,
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
