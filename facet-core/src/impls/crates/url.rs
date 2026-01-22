#![cfg(feature = "url")]

use alloc::string::String;

use url::{Host, Url};

use crate::{
    Def, Facet, OxPtrConst, OxPtrUninit, ParseError, PtrConst, Shape, ShapeBuilder, TryFromOutcome,
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
    target: OxPtrUninit,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        // Handle &str (Copy type, use get)
        if src_shape.id == <&str as Facet>::SHAPE.id {
            let source_str: &str = src.get::<&str>();
            match Url::parse(source_str) {
                Ok(val) => {
                    target.put(val);
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
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(e) => TryFromOutcome::Failed(url_parse_error_message(e).into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_url(s: &str, target: OxPtrUninit) -> Option<Result<(), ParseError>> {
    unsafe {
        match Url::parse(s) {
            Ok(val) => {
                target.put(val);
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

unsafe fn display_host(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let host = source.get::<Host<String>>();
        Some(write!(f, "{}", host))
    }
}

const fn host_parse_error_message(error: url::ParseError) -> &'static str {
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
        _ => "failed to parse host",
    }
}

unsafe fn try_from_host(
    target: OxPtrUninit,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        // Handle &str (Copy type, use get)
        if src_shape.id == <&str as Facet>::SHAPE.id {
            let source_str: &str = src.get::<&str>();
            match Host::parse(source_str) {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(e) => TryFromOutcome::Failed(host_parse_error_message(e).into()),
            }
        }
        // Handle String (consume via read)
        else if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match Host::parse(&source_str) {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(e) => TryFromOutcome::Failed(host_parse_error_message(e).into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_host(s: &str, target: OxPtrUninit) -> Option<Result<(), ParseError>> {
    unsafe {
        match Host::parse(s) {
            Ok(val) => {
                target.put(val);
                Some(Ok(()))
            }
            Err(e) => Some(Err(ParseError::from_str(host_parse_error_message(e)))),
        }
    }
}

const HOST_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_host),
    try_from: Some(try_from_host),
    parse: Some(parse_host),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for Host<String> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Host<String>>("Host")
            .module_path("url")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&HOST_VTABLE)
            .inner(<String as Facet>::SHAPE)
            .build()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_shape() {
        let shape = <Host<String> as Facet>::SHAPE;
        assert_eq!(shape.type_identifier, "Host");
        assert_eq!(shape.module_path, Some("url"));
    }

    #[test]
    fn test_host_parse_domain() {
        let host = Host::parse("example.com").unwrap();
        assert_eq!(host.to_string(), "example.com");
    }

    #[test]
    fn test_host_parse_ipv4() {
        let host = Host::parse("127.0.0.1").unwrap();
        assert_eq!(host.to_string(), "127.0.0.1");
    }

    #[test]
    fn test_host_parse_ipv6() {
        let host = Host::parse("[::1]").unwrap();
        assert_eq!(host.to_string(), "[::1]");
    }

    #[test]
    fn test_host_parse_invalid() {
        let result = Host::<String>::parse("");
        assert!(result.is_err());
    }
}
