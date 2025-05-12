use super::{Field, Repr};

/// Common fields for struct-like types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct StructType {
    /// Representation of the struct's data
    pub repr: Repr,

    /// the kind of struct (e.g. struct, tuple struct, tuple)
    pub kind: StructKind,

    /// all fields, in declaration order (not necessarily in memory order)
    pub fields: &'static [Field],
}

impl StructType {
    /// Returns a builder for StructType
    pub const fn builder() -> StructBuilder {
        StructBuilder::new()
    }
}

/// Builder for StructType
pub struct StructBuilder {
    repr: Option<Repr>,
    kind: Option<StructKind>,
    fields: &'static [Field],
}

impl StructBuilder {
    /// Creates a new StructBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            repr: None,
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

    /// Sets the repr for the StructType
    pub const fn repr(mut self, repr: Repr) -> Self {
        self.repr = Some(repr);
        self
    }

    /// Sets the kind for the StructType
    pub const fn kind(mut self, kind: StructKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Sets the fields for the StructType
    pub const fn fields(mut self, fields: &'static [Field]) -> Self {
        self.fields = fields;
        self
    }

    /// Builds the StructType
    pub const fn build(self) -> StructType {
        StructType {
            repr: self.repr.unwrap(),
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

    // TODO: remove this
    /// (T0, T1)
    Tuple,
}
