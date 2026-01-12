use crate::Shape;

/// Common fields for function pointer types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct FunctionPointerDef {
    /// The calling abi of the function pointer
    pub abi: FunctionAbi,

    /// All parameter types, in declaration order
    pub parameters: &'static [&'static Shape],

    /// The return type
    pub return_type: &'static Shape,
}

/// The calling ABI of a function pointer
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[repr(C)]
pub enum FunctionAbi {
    /// C ABI
    C,

    /// Rust ABI
    #[default]
    Rust,

    /// An unknown ABI
    Unknown,
}
impl FunctionAbi {
    /// Returns the string in `extern "abi-string"` if not [`FunctionAbi::Unknown`].
    pub const fn as_abi_str(&self) -> Option<&str> {
        match self {
            FunctionAbi::C => Some("C"),
            FunctionAbi::Rust => Some("Rust"),
            FunctionAbi::Unknown => None,
        }
    }
}

impl FunctionPointerDef {
    /// Construct a `FunctionPointerDef` from its components.
    pub const fn new(
        abi: FunctionAbi,
        parameters: &'static [&'static Shape],
        return_type: &'static Shape,
    ) -> Self {
        Self {
            abi,
            parameters,
            return_type,
        }
    }
}
