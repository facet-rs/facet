#![cfg(feature = "url")]

use alloc::string::String;

use url::Url;

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, PtrConst, Shape, ShapeBuilder, TryFromOutcome,
    Type, UserType, VTableIndirect,
};

unsafe fn display_url(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let url = source.get::<Url>();
        Some(write!(f, "{}", url.as_str()))
    }
}

const fn url_parse_error_message(error: url::ParseError) -> &'static str {
    match error {
        url::ParseError::EmptyHost => "empty host",
        url::ParseError::IdnaError => "invalid international domain name",
        url::ParseError::InvalidPort => "invalid port number",
        url::ParseError::InvalidIpv4Address => "invalid IPv4 address",
        url::ParseError::InvalidIpv6Address => "invalid IPv6 address",
        url::ParseError::InvalidDomainCharacter => "invalid domain character",
        url::ParseError::RelativeUrlWithoutBase => "relative URL without a base",
        url::ParseError::RelativeUrlWithCannotBeABaseBase => {
            "relative URL with a cannot-be-a-base base"
        }
        url::ParseError::SetHostOnCannotBeABaseUrl => {
            "a cannot-be-a-base URL doesn't have a host to set"
        }
        url::ParseError::Overflow => "URLs more than 4 GB are not supported",
        _ => "failed to parse URL",
    }
}

unsafe fn try_from_url(
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        // Handle &str (Copy type, use get)
        if src_shape.id == <&str as Facet>::SHAPE.id {
            let source_str: &str = src.get::<&str>();
            match Url::parse(source_str) {
                Ok(val) => {
                    *target.as_mut::<Url>() = val;
                    TryFromOutcome::Converted
                }
                Err(e) => TryFromOutcome::Failed(url_parse_error_message(e).into()),
            }
        }
        // Handle String (consume via read)
        else if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match Url::parse(&source_str) {
                Ok(val) => {
                    *target.as_mut::<Url>() = val;
                    TryFromOutcome::Converted
                }
                Err(e) => TryFromOutcome::Failed(url_parse_error_message(e).into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_url(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        match Url::parse(s) {
            Ok(val) => {
                *target.as_mut::<Url>() = val;
                Some(Ok(()))
            }
            Err(e) => Some(Err(ParseError::from_str(url_parse_error_message(e)))),
        }
    }
}

const URL_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_url),
    try_from: Some(try_from_url),
    parse: Some(parse_url),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for Url {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Url>("Url")
            .module_path("url")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&URL_VTABLE)
            .inner(<String as Facet>::SHAPE)
            .build()
    };
}
