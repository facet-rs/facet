//! Wire-level vocabulary shared by every codec backend.
//!
//! These types are the layout-agnostic IR primitives that both the Rust JIT
//! and the Swift codec FFI use to talk about postcard wire format. They have
//! no facet dependency, no Cranelift dependency, and no awareness of the
//! local in-memory layout — they only describe what bytes go on the wire.
//!
//! Backend-specific lowerings (e.g. `vox_postcard::ir::lower` for facet
//! shapes) convert their own type representations into these primitives.

use vox_jit_cal::DescriptorHandle;

/// Handle to a calibrated opaque-type descriptor (e.g. `Vec<T>`, `String`,
/// or a Swift `Array<T>`).
///
/// The concrete descriptor type is owned by the calibration subsystem of
/// each backend. The IR stores only the handle; the interpreter or codegen
/// resolves it at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpaqueDescriptorId(pub u32);

impl From<DescriptorHandle> for OpaqueDescriptorId {
    fn from(h: DescriptorHandle) -> Self {
        OpaqueDescriptorId(h.0)
    }
}

impl From<OpaqueDescriptorId> for DescriptorHandle {
    fn from(id: OpaqueDescriptorId) -> Self {
        DescriptorHandle(id.0)
    }
}

/// Wire-level primitive that can be encoded/decoded without any local
/// shape information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WirePrimitive {
    Unit,
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    USize,
    I8,
    I16,
    I32,
    I64,
    I128,
    ISize,
    F32,
    F64,
    /// varint-length-prefixed UTF-8 string
    String,
    /// varint-length-prefixed byte buffer
    Bytes,
    /// u32le-length-prefixed opaque payload
    Payload,
    /// char encoded as a length-1 UTF-8 string
    Char,
}

/// Width (in bytes) of an enum discriminant on the wire / in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagWidth {
    U8,
    U16,
    U32,
    U64,
}

impl TagWidth {
    pub fn byte_size(self) -> usize {
        match self {
            TagWidth::U8 => 1,
            TagWidth::U16 => 2,
            TagWidth::U32 => 4,
            TagWidth::U64 => 8,
        }
    }
}
