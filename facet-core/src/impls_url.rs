use alloc::borrow::ToOwned;
use alloc::string::String;

use url::Url;

use crate::{
    Def, Facet, ParseError, PtrConst, PtrMut, PtrUninit, Shape, TryBorrowInnerError,
    TryIntoInnerError, Type, UserType, value_vtable,
};

unsafe impl Facet<'_> for Url {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable({
                // Custom parse impl with detailed errors
                unsafe fn parse<'target>(
                    s: &str,
                    target: PtrUninit<'target>,
                ) -> Result<PtrMut<'target>, ParseError> {
                    let url = Url::parse(s).map_err(|error| {
                        let message = match error {
                            url::ParseError::EmptyHost => "empty host",
                            url::ParseError::IdnaError => "invalid international domain name",
                            url::ParseError::InvalidPort => "invalid port number",
                            url::ParseError::InvalidIpv4Address => "invalid IPv4 address",
                            url::ParseError::InvalidIpv6Address => "invalid IPv6 address",
                            url::ParseError::InvalidDomainCharacter => "invalid domain character",
                            url::ParseError::RelativeUrlWithoutBase => {
                                "relative URL without a base"
                            }
                            url::ParseError::RelativeUrlWithCannotBeABaseBase => {
                                "relative URL with a cannot-be-a-base base"
                            }
                            url::ParseError::SetHostOnCannotBeABaseUrl => {
                                "a cannot-be-a-base URL doesn’t have a host to set"
                            }
                            url::ParseError::Overflow => "URLs more than 4 GB are not supported",
                            _ => "failed to parse URL",
                        };
                        ParseError::Generic(message)
                    })?;
                    Ok(unsafe { target.put(url) })
                }

                unsafe fn try_into_inner<'dst>(
                    src_ptr: PtrMut<'_>,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
                    let url = unsafe { src_ptr.get::<Url>() };
                    Ok(unsafe { dst.put(url.as_str().to_owned()) })
                }

                unsafe fn try_borrow_inner(
                    src_ptr: PtrConst<'_>,
                ) -> Result<PtrConst<'_>, TryBorrowInnerError> {
                    let url = unsafe { src_ptr.get::<Url>() };
                    Ok(PtrConst::new(url.as_str().into()))
                }

                let mut vtable =
                    value_vtable!(Url, |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier));
                {
                    vtable.parse = Some(parse);
                    vtable.try_into_inner = Some(try_into_inner);
                    vtable.try_borrow_inner = Some(try_borrow_inner);
                }
                vtable
            })
            .type_identifier("Url")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .inner(<String as Facet>::SHAPE)
            .build()
    };
}
