use crate::Shape;

/// Common fields for function pointer types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct FunctionPointerDef {
    /// The calling abi of the function pointer
    pub abi: FunctionAbi,

    /// All parameter types, in declaration order
    pub parameters: &'static [fn() -> &'static Shape],

    /// The return type
    pub return_type: fn() -> &'static Shape,
}

/// The calling ABI of a function pointer
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[repr(C)]
#[non_exhaustive]
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
    pub fn as_abi_str(&self) -> Option<&str> {
        match self {
            FunctionAbi::C => Some("C"),
            FunctionAbi::Rust => Some("Rust"),
            FunctionAbi::Unknown => None,
        }
    }
}

impl FunctionPointerDef {
    /// Returns a builder for FunctionPointerDef
    pub const fn builder() -> FunctionPointerDefBuilder {
        FunctionPointerDefBuilder::new()
    }
}

/// Builder for FunctionPointerDef
pub struct FunctionPointerDefBuilder {
    abi: Option<FunctionAbi>,
    parameters: &'static [fn() -> &'static Shape],
    return_type: Option<fn() -> &'static Shape>,
}

impl FunctionPointerDefBuilder {
    /// Creates a new FunctionPointerDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            parameters: &[],
            abi: None,
            return_type: None,
        }
    }

    /// Sets the abi for the FunctionPointerDef
    pub const fn abi(mut self, abi: FunctionAbi) -> Self {
        self.abi = Some(abi);
        self
    }

    /// Sets the parameters for the FunctionPointerDef
    pub const fn parameter_types(mut self, parameters: &'static [fn() -> &'static Shape]) -> Self {
        self.parameters = parameters;
        self
    }

    /// Sets the return type for the FunctionPointerDef
    pub const fn return_type(mut self, ty: fn() -> &'static Shape) -> Self {
        self.return_type = Some(ty);
        self
    }

    /// Builds the FunctionPointerDef
    pub const fn build(self) -> FunctionPointerDef {
        FunctionPointerDef {
            parameters: self.parameters,
            return_type: self.return_type.unwrap(),
            abi: self.abi.unwrap(),
        }
    }
}
