#![cfg(feature = "url")]

use alloc::string::String;

use url::Url;

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, Shape, ShapeBuilder, Type, UserType,
    VTableIndirect,
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

unsafe fn try_from_url(source: OxPtrConst, target: OxPtrMut) -> Option<Result<(), String>> {
    unsafe {
        if source.shape.is_type::<String>() {
            let source_str = source.ptr().read::<String>();
            let parsed = Url::parse(&source_str).map_err(|error| {
                let message = match error {
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
                };
                message.to_string()
            });
            Some(match parsed {
                Ok(val) => {
                    *target.as_mut::<Url>() = val;
                    Ok(())
                }
                Err(e) => Err(e),
            })
        } else {
            Some(Err(format!(
                "unsupported source shape for Url, expected String, got {}",
                source.shape.type_identifier
            )))
        }
    }
}

unsafe fn parse_url(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = Url::parse(s).map_err(|error| {
            let message = match error {
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
            };
            ParseError::from_str(message)
        });
        Some(match parsed {
            Ok(val) => {
                *target.as_mut::<Url>() = val;
                Ok(())
            }
            Err(e) => Err(e),
        })
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
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&URL_VTABLE)
            .inner(<String as Facet>::SHAPE)
            .build()
    };
}
