#![cfg(feature = "semver")]

pub use semver::{Version, VersionReq};

use crate::{
    Def, Facet, OxPtrConst, OxPtrUninit, ParseError, PtrConst, Shape, ShapeBuilder, TryFromOutcome,
    Type, UserType, VTableIndirect,
};

unsafe fn try_from_semver_version(
    target: OxPtrUninit,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        // Handle &str (Copy type, use get)
        if src_shape.id == <&str as Facet>::SHAPE.id {
            let source_str: &str = src.get::<&str>();
            match Version::parse(source_str) {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("SemVer::Version parsing failed".into()),
            }
        }
        // Handle String (consume via read)
        else if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match Version::parse(&source_str) {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("SemVer::Version parsing failed".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_semver_version(s: &str, target: OxPtrUninit) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed =
            Version::parse(s).map_err(|_| ParseError::from_str("SemVer::Version parsing failed"));
        Some(match parsed {
            Ok(val) => {
                target.put(val);
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn display_semver_version(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let version = source.get::<Version>();
        Some(write!(f, "{version}"))
    }
}

unsafe fn partial_eq_semver_version(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<Version>();
        let b = b.get::<Version>();
        Some(a == b)
    }
}

const SEMVER_VERSION_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_semver_version),
    try_from: Some(try_from_semver_version),
    parse: Some(parse_semver_version),
    partial_eq: Some(partial_eq_semver_version),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for Version {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Version>("Version")
            .module_path("semver")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&SEMVER_VERSION_VTABLE)
            .build()
    };
}

unsafe fn try_from_semver_versionreq(
    target: OxPtrUninit,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    unsafe {
        // Handle &str (Copy type, use get)
        if src_shape.id == <&str as Facet>::SHAPE.id {
            let source_str: &str = src.get::<&str>();
            match VersionReq::parse(source_str) {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("SemVer::Version parsing failed".into()),
            }
        }
        // Handle String (consume via read)
        else if src_shape.id == <String as Facet>::SHAPE.id {
            let source_str = src.read::<String>();
            match VersionReq::parse(&source_str) {
                Ok(val) => {
                    target.put(val);
                    TryFromOutcome::Converted
                }
                Err(_) => TryFromOutcome::Failed("SemVer::Version parsing failed".into()),
            }
        } else {
            TryFromOutcome::Unsupported
        }
    }
}

unsafe fn parse_semver_versionreq(s: &str, target: OxPtrUninit) -> Option<Result<(), ParseError>> {
    unsafe {
        let parsed = VersionReq::parse(s)
            .map_err(|_| ParseError::from_str("SemVer::VersionReq parsing failed"));
        Some(match parsed {
            Ok(val) => {
                target.put(val);
                Ok(())
            }
            Err(e) => Err(e),
        })
    }
}

unsafe fn display_semver_versionreq(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let version_req = source.get::<VersionReq>();
        Some(write!(f, "{version_req}"))
    }
}

unsafe fn partial_eq_semver_versionreq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<VersionReq>();
        let b = b.get::<VersionReq>();
        Some(a == b)
    }
}

const SEMVER_VERSIONREQ_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_semver_versionreq),
    try_from: Some(try_from_semver_versionreq),
    parse: Some(parse_semver_versionreq),
    partial_eq: Some(partial_eq_semver_versionreq),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for VersionReq {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<VersionReq>("VersionReq")
            .module_path("semver")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&SEMVER_VERSIONREQ_VTABLE)
            .build()
    };
}
