use crate::OpaqueConst;

/// Definition for scalar types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct ScalarDef {
    /// Affinity of the scalar — is spiritually more like a number, more like a string, something else?
    /// example: an IPv4 address is both. good luck.
    pub affinity: ScalarAffinity,
}

impl ScalarDef {
    /// Returns a builder for ScalarDef
    pub const fn builder() -> ScalarDefBuilder {
        ScalarDefBuilder::new()
    }
}

/// Builder for ScalarDef
#[derive(Default)]
pub struct ScalarDefBuilder {
    affinity: Option<ScalarAffinity>,
}

impl ScalarDefBuilder {
    /// Creates a new ScalarDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self { affinity: None }
    }

    /// Sets the affinity for the ScalarDef
    pub const fn affinity(mut self, affinity: ScalarAffinity) -> Self {
        self.affinity = Some(affinity);
        self
    }

    /// Builds the ScalarDef
    pub const fn build(self) -> ScalarDef {
        ScalarDef {
            affinity: self.affinity.unwrap(),
        }
    }
}

/// Scalar affinity: what a scalar spiritually is: a number, a string, a bool, something else
/// entirely?
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum ScalarAffinity {
    /// Number-like scalar affinity
    Number(NumberAffinity),
    /// String-like scalar affinity
    String(StringAffinity),
    /// Boolean scalar affinity
    Boolean(BoolAffinity),
    /// Empty scalar affinity
    Empty(EmptyAffinity),
    /// Socket address scalar affinity
    SocketAddr(SocketAddrAffinity),
    /// Ip Address scalar affinity
    IpAddr(IpAddrAffinity),
    /// Something you're not supposed to look inside of
    Opaque(OpaqueAffinity),
    /// Other scalar affinity
    Other(OtherAffinity),
    /// Character scalar affinity
    Char(CharAffinity),
}

impl ScalarAffinity {
    /// Returns a NumberAffinityBuilder
    pub const fn number() -> NumberAffinityBuilder {
        NumberAffinityBuilder::new()
    }

    /// Returns a StringAffinityBuilder
    pub const fn string() -> StringAffinityBuilder {
        StringAffinityBuilder::new()
    }

    /// Returns a BoolAffinityBuilder
    pub const fn boolean() -> BoolAffinityBuilder {
        BoolAffinityBuilder::new()
    }

    /// Returns an EmptyAffinityBuilder
    pub const fn empty() -> EmptyAffinityBuilder {
        EmptyAffinityBuilder::new()
    }

    /// Returns a SocketAddrAffinityBuilder
    pub const fn socket_addr() -> SocketAddrAffinityBuilder {
        SocketAddrAffinityBuilder::new()
    }

    /// Returns an IpAddrAffinityBuilder
    pub const fn ip_addr() -> IpAddrAffinityBuilder {
        IpAddrAffinityBuilder::new()
    }

    /// Returns an OpaqueAffinityBuilder
    pub const fn opaque() -> OpaqueAffinityBuilder {
        OpaqueAffinityBuilder::new()
    }

    /// Returns an OtherAffinityBuilder
    pub const fn other() -> OtherAffinityBuilder {
        OtherAffinityBuilder::new()
    }

    /// Returns a CharAffinityBuilder
    pub const fn char() -> CharAffinityBuilder {
        CharAffinityBuilder::new()
    }
}

//////////////////////////////////////////////////////////////////////////////////////////
// Affinities
//////////////////////////////////////////////////////////////////////////////////////////

/// Definition for number-like scalar affinities
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct NumberAffinity {
    /// Bit representation of numbers
    pub bits: NumberBits,

    /// Minimum representable value
    pub min: OpaqueConst<'static>,

    /// Maximum representable value
    pub max: OpaqueConst<'static>,

    /// Positive infinity representable value
    pub positive_infinity: Option<OpaqueConst<'static>>,

    /// Negative infinity representable value
    pub negative_infinity: Option<OpaqueConst<'static>>,

    /// Example NaN (Not a Number) value.
    /// Why sample? Because there are many NaN values, and we need to provide a representative one.
    pub nan_sample: Option<OpaqueConst<'static>>,

    /// Positive zero representation. If there's only one zero, only set this one.
    pub positive_zero: Option<OpaqueConst<'static>>,

    /// Negative zero representation
    pub negative_zero: Option<OpaqueConst<'static>>,
}

/// Represents whether a numeric type is signed or unsigned
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum Signedness {
    /// Signed numeric type
    Signed,
    /// Unsigned numeric type
    Unsigned,
}

/// Bit representation of numbers
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum NumberBits {
    /// Integer number limits with specified number of bits
    Integer {
        /// Number of bits in the integer representation
        bits: usize,
        /// Whether the integer is signed or unsigned
        sign: Signedness,
    },
    /// Floating-point number limits with specified sign, exponent and mantissa bits
    Float {
        /// Number of bits used for the sign (typically 1)
        sign_bits: usize,
        /// Number of bits used for the exponent
        exponent_bits: usize,
        /// Number of bits used for the mantissa (fraction part)
        mantissa_bits: usize,
    },
    /// Fixed-point number limits with specified integer and fractional bits
    Fixed {
        /// Number of bits used for the sign (typically 0 or 1)
        sign_bits: usize,
        /// Number of bits used for the integer part
        integer_bits: usize,
        /// Number of bits used for the fractional part
        fraction_bits: usize,
    },
}

impl NumberAffinity {
    /// Returns a builder for NumberAffinity
    pub const fn builder() -> NumberAffinityBuilder {
        NumberAffinityBuilder::new()
    }
}

/// Builder for NumberAffinity
#[repr(C)]
pub struct NumberAffinityBuilder {
    limits: Option<NumberBits>,
    min: Option<OpaqueConst<'static>>,
    max: Option<OpaqueConst<'static>>,
    positive_infinity: Option<OpaqueConst<'static>>,
    negative_infinity: Option<OpaqueConst<'static>>,
    nan_sample: Option<OpaqueConst<'static>>,
    positive_zero: Option<OpaqueConst<'static>>,
    negative_zero: Option<OpaqueConst<'static>>,
}

impl NumberAffinityBuilder {
    /// Creates a new NumberAffinityBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            limits: None,
            min: None,
            max: None,
            positive_infinity: None,
            negative_infinity: None,
            nan_sample: None,
            positive_zero: None,
            negative_zero: None,
        }
    }

    /// Sets the number limits as integer with specified bits and sign
    pub const fn integer(mut self, bits: usize, sign: Signedness) -> Self {
        self.limits = Some(NumberBits::Integer { bits, sign });
        self
    }

    /// Sets the number limits as signed integer with specified bits
    pub const fn signed_integer(self, bits: usize) -> Self {
        self.integer(bits, Signedness::Signed)
    }

    /// Sets the number limits as unsigned integer with specified bits
    pub const fn unsigned_integer(self, bits: usize) -> Self {
        self.integer(bits, Signedness::Unsigned)
    }

    /// Sets the number limits as float with specified bits
    pub const fn float(
        mut self,
        sign_bits: usize,
        exponent_bits: usize,
        mantissa_bits: usize,
    ) -> Self {
        self.limits = Some(NumberBits::Float {
            sign_bits,
            exponent_bits,
            mantissa_bits,
        });
        self
    }

    /// Sets the number limits as fixed-point with specified bits
    pub const fn fixed(
        mut self,
        sign_bits: usize,
        integer_bits: usize,
        fraction_bits: usize,
    ) -> Self {
        self.limits = Some(NumberBits::Fixed {
            sign_bits,
            integer_bits,
            fraction_bits,
        });
        self
    }

    /// Sets the min value for the NumberAffinity
    pub const fn min(mut self, min: OpaqueConst<'static>) -> Self {
        self.min = Some(min);
        self
    }

    /// Sets the max value for the NumberAffinity
    pub const fn max(mut self, max: OpaqueConst<'static>) -> Self {
        self.max = Some(max);
        self
    }

    /// Sets the positive infinity value for the NumberAffinity
    pub const fn positive_infinity(mut self, value: OpaqueConst<'static>) -> Self {
        self.positive_infinity = Some(value);
        self
    }

    /// Sets the negative infinity value for the NumberAffinity
    pub const fn negative_infinity(mut self, value: OpaqueConst<'static>) -> Self {
        self.negative_infinity = Some(value);
        self
    }

    /// Sets the NaN sample value for the NumberAffinity
    pub const fn nan_sample(mut self, value: OpaqueConst<'static>) -> Self {
        self.nan_sample = Some(value);
        self
    }

    /// Sets the positive zero value for the NumberAffinity
    pub const fn positive_zero(mut self, value: OpaqueConst<'static>) -> Self {
        self.positive_zero = Some(value);
        self
    }

    /// Sets the negative zero value for the NumberAffinity
    pub const fn negative_zero(mut self, value: OpaqueConst<'static>) -> Self {
        self.negative_zero = Some(value);
        self
    }

    /// Builds the ScalarAffinity
    pub const fn build(self) -> ScalarAffinity {
        ScalarAffinity::Number(NumberAffinity {
            bits: self.limits.unwrap(),
            min: self.min.unwrap(),
            max: self.max.unwrap(),
            positive_infinity: self.positive_infinity,
            negative_infinity: self.negative_infinity,
            nan_sample: self.nan_sample,
            positive_zero: self.positive_zero,
            negative_zero: self.negative_zero,
        })
    }
}

/// Definition for string-like scalar affinities
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct StringAffinity {
    /// Maximum inline length
    pub max_inline_length: Option<usize>,
}

impl StringAffinity {
    /// Returns a builder for StringAffinity
    pub const fn builder() -> StringAffinityBuilder {
        StringAffinityBuilder::new()
    }
}

/// Builder for StringAffinity
#[repr(C)]
pub struct StringAffinityBuilder {
    max_inline_length: Option<usize>,
}

impl StringAffinityBuilder {
    /// Creates a new StringAffinityBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            max_inline_length: None,
        }
    }

    /// Sets the max_inline_length for the StringAffinity
    pub const fn max_inline_length(mut self, max_inline_length: usize) -> Self {
        self.max_inline_length = Some(max_inline_length);
        self
    }

    /// Builds the ScalarAffinity
    pub const fn build(self) -> ScalarAffinity {
        ScalarAffinity::String(StringAffinity {
            max_inline_length: self.max_inline_length,
        })
    }
}

/// Definition for boolean scalar affinities
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct BoolAffinity {}

impl BoolAffinity {
    /// Returns a builder for BoolAffinity
    pub const fn builder() -> BoolAffinityBuilder {
        BoolAffinityBuilder::new()
    }
}

/// Builder for BoolAffinity
#[repr(C)]
pub struct BoolAffinityBuilder {}

impl BoolAffinityBuilder {
    /// Creates a new BoolAffinityBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {}
    }

    /// Builds the ScalarAffinity
    pub const fn build(self) -> ScalarAffinity {
        ScalarAffinity::Boolean(BoolAffinity {})
    }
}

/// Definition for empty scalar affinities
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct EmptyAffinity {}

impl EmptyAffinity {
    /// Returns a builder for EmptyAffinity
    pub const fn builder() -> EmptyAffinityBuilder {
        EmptyAffinityBuilder::new()
    }
}

/// Builder for EmptyAffinity
#[repr(C)]
pub struct EmptyAffinityBuilder {}

impl EmptyAffinityBuilder {
    /// Creates a new EmptyAffinityBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {}
    }

    /// Builds the ScalarAffinity
    pub const fn build(self) -> ScalarAffinity {
        ScalarAffinity::Empty(EmptyAffinity {})
    }
}

/// Definition for socket address scalar affinities
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct SocketAddrAffinity {}

impl SocketAddrAffinity {
    /// Returns a builder for SocketAddrAffinity
    pub const fn builder() -> SocketAddrAffinityBuilder {
        SocketAddrAffinityBuilder::new()
    }
}

/// Builder for SocketAddrAffinity
#[repr(C)]
pub struct SocketAddrAffinityBuilder {}

impl SocketAddrAffinityBuilder {
    /// Creates a new SocketAddrAffinityBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {}
    }

    /// Builds the ScalarAffinity
    pub const fn build(self) -> ScalarAffinity {
        ScalarAffinity::SocketAddr(SocketAddrAffinity {})
    }
}

/// Definition for IP address scalar affinities
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct IpAddrAffinity {}

impl IpAddrAffinity {
    /// Returns a builder for IpAddrAffinity
    pub const fn builder() -> IpAddrAffinityBuilder {
        IpAddrAffinityBuilder::new()
    }
}

/// Builder for IpAddrAffinity
#[repr(C)]
pub struct IpAddrAffinityBuilder {}

impl IpAddrAffinityBuilder {
    /// Creates a new IpAddrAffinityBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {}
    }

    /// Builds the ScalarAffinity
    pub const fn build(self) -> ScalarAffinity {
        ScalarAffinity::IpAddr(IpAddrAffinity {})
    }
}

/// Definition for opaque scalar affinities
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct OpaqueAffinity {}

impl OpaqueAffinity {
    /// Returns a builder for OpaqueAffinity
    pub const fn builder() -> OpaqueAffinityBuilder {
        OpaqueAffinityBuilder::new()
    }
}

/// Builder for OpaqueAffinity
#[repr(C)]
pub struct OpaqueAffinityBuilder {}

impl OpaqueAffinityBuilder {
    /// Creates a new OpaqueAffinityBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {}
    }

    /// Builds the ScalarAffinity
    pub const fn build(self) -> ScalarAffinity {
        ScalarAffinity::Opaque(OpaqueAffinity {})
    }
}

/// Definition for other scalar affinities
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct OtherAffinity {}

impl OtherAffinity {
    /// Returns a builder for OtherAffinity
    pub const fn builder() -> OtherAffinityBuilder {
        OtherAffinityBuilder::new()
    }
}

/// Builder for OtherAffinity
#[repr(C)]
pub struct OtherAffinityBuilder {}

impl OtherAffinityBuilder {
    /// Creates a new OtherAffinityBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {}
    }

    /// Builds the ScalarAffinity
    pub const fn build(self) -> ScalarAffinity {
        ScalarAffinity::Other(OtherAffinity {})
    }
}

/// Definition for character scalar affinities
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct CharAffinity {}

impl CharAffinity {
    /// Returns a builder for CharAffinity
    pub const fn builder() -> CharAffinityBuilder {
        CharAffinityBuilder::new()
    }
}

/// Builder for CharAffinity
#[repr(C)]
pub struct CharAffinityBuilder {}

impl CharAffinityBuilder {
    /// Creates a new CharAffinityBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {}
    }

    /// Builds the ScalarAffinity
    pub const fn build(self) -> ScalarAffinity {
        ScalarAffinity::Char(CharAffinity {})
    }
}
