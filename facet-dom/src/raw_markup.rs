//! Raw markup capture type.

use std::ops::Deref;

use facet_core::{OxPtrConst, OxPtrMut, ParseError, PtrConst, TryFromOutcome, VTableIndirect};

/// A string containing raw markup captured verbatim from the source.
///
/// When deserializing, if the parser supports raw capture, the entire
/// element (including tags) is captured as-is instead of being parsed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct RawMarkup(pub String);

unsafe fn display_raw_markup(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let rm = unsafe { source.get::<RawMarkup>() };
    Some(write!(f, "{}", rm.0))
}

unsafe fn try_from_raw_markup(
    target: OxPtrMut,
    src_shape: &'static facet_core::Shape,
    src: PtrConst,
) -> TryFromOutcome {
    // Handle &str
    if src_shape.id == <&str as facet_core::Facet>::SHAPE.id {
        let s: &str = unsafe { src.get::<&str>() };
        unsafe { *target.as_mut::<RawMarkup>() = RawMarkup(s.to_owned()) };
        TryFromOutcome::Converted
    }
    // Handle String
    else if src_shape.id == <String as facet_core::Facet>::SHAPE.id {
        let s = unsafe { src.read::<String>() };
        unsafe { *target.as_mut::<RawMarkup>() = RawMarkup(s) };
        TryFromOutcome::Converted
    } else {
        TryFromOutcome::Unsupported
    }
}

unsafe fn parse_raw_markup(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe { *target.as_mut::<RawMarkup>() = RawMarkup(s.to_owned()) };
    Some(Ok(()))
}

const RAW_MARKUP_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_raw_markup),
    try_from: Some(try_from_raw_markup),
    parse: Some(parse_raw_markup),
    ..VTableIndirect::EMPTY
};

impl RawMarkup {
    /// Create new RawMarkup from a string.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the raw markup as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl Deref for RawMarkup {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<String> for RawMarkup {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RawMarkup {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl std::fmt::Display for RawMarkup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// Facet impl - scalar with vtable for string conversion
unsafe impl facet_core::Facet<'_> for RawMarkup {
    const SHAPE: &'static facet_core::Shape = &const {
        facet_core::ShapeBuilder::for_sized::<RawMarkup>("RawMarkup")
            .def(facet_core::Def::Scalar)
            .vtable_indirect(&RAW_MARKUP_VTABLE)
            .inner(<String as facet_core::Facet>::SHAPE)
            .build()
    };
}

/// Check if a shape is the RawMarkup type.
pub fn is_raw_markup(shape: &facet_core::Shape) -> bool {
    // Just check the type name - module path is set by macro
    shape.type_identifier == "RawMarkup"
}
