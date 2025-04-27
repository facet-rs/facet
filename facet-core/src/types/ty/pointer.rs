use crate::FunctionPointerDef;

use super::Shape;

/// Describes all pointer types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
pub enum PointerType {
    /// Describees bound const and mut references (`&`/`&mut`)
    Reference(ValuePointerType),
    /// Describes raw pointers
    ///
    /// Dereferencing invalid raw pointers may lead to undefined behavior
    Raw(ValuePointerType),
    /// Describes function pointers
    Function(FunctionPointerDef),
}

/// Describes the raw/reference pointer
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
pub struct ValuePointerType {
    /// Is the pointer mutable or not.
    pub mutable: bool,
    /// Describes whether the pointer is wider or not
    ///
    /// Note: if the pointer is wide, then the `target` shape will have `ShapeLayout::Unsized`, and
    /// the vtables of the target shape will expect the pointer to _this_ pointer, rather than the
    /// resulting address of unsized data. This is because wide pointer's metadata information is
    /// an undefined implementation detail, at this current moment.
    ///
    /// See: <https://github.com/rust-lang/rust/issues/81513>
    pub wide: bool,
    /// Shape of the pointer's pointee
    ///
    /// This needs to be indirect (behind a function), in order to allow recursive types without
    /// overflowing the const-eval system.
    pub target: fn() -> &'static Shape,
}
