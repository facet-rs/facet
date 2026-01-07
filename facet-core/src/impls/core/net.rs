//! Facet implementation for `core::net` types

#![cfg(feature = "net")]

extern crate alloc;

use crate::{Def, Facet, PtrConst, Shape, ShapeBuilder, TryFromOutcome, VTableDirect, vtable_direct};

/// Generate a try_from function for a net type that converts from &str via FromStr
macro_rules! net_try_from {
    ($type:ty) => {{
        /// # Safety
        /// `dst` must be valid for writes, `src` must point to valid data of type described by `src_shape`
        unsafe fn try_from_str(
            dst: *mut $type,
            src_shape: &'static Shape,
            src: PtrConst,
        ) -> TryFromOutcome {
            // Check if source is &str (Copy type, use get - doesn't consume)
            if src_shape.id == <&str as Facet>::SHAPE.id {
                let str_ref: &str = unsafe { *src.get::<&str>() };
                match str_ref.parse::<$type>() {
                    Ok(val) => {
                        unsafe { dst.write(val) };
                        TryFromOutcome::Converted
                    }
                    Err(_) => TryFromOutcome::Failed(
                        alloc::format!(
                            "failed to parse '{}' as {}",
                            str_ref,
                            stringify!($type)
                        )
                        .into(),
                    ),
                }
            } else if src_shape.id == <alloc::string::String as Facet>::SHAPE.id {
                // Consume the String
                let string: alloc::string::String = unsafe { src.read::<alloc::string::String>() };
                match string.parse::<$type>() {
                    Ok(val) => {
                        unsafe { dst.write(val) };
                        TryFromOutcome::Converted
                    }
                    Err(_) => TryFromOutcome::Failed(
                        alloc::format!(
                            "failed to parse '{}' as {}",
                            string,
                            stringify!($type)
                        )
                        .into(),
                    ),
                }
            } else {
                TryFromOutcome::Unsupported
            }
        }
        try_from_str
    }};
}

macro_rules! impl_facet_for_net_type {
    ($type:ty, $name:literal) => {
        unsafe impl Facet<'_> for $type {
            const SHAPE: &'static Shape = &const {
                const VTABLE: VTableDirect = vtable_direct!($type =>
                    FromStr,
                    Display,
                    Debug,
                    Hash,
                    PartialEq,
                    PartialOrd,
                    Ord,
                    [try_from = net_try_from!($type)],
                );

                ShapeBuilder::for_sized::<$type>($name)
                    .def(Def::Scalar)
                    .vtable_direct(&VTABLE)
                    .eq()
                    .copy()
                    .send()
                    .sync()
                    .build()
            };
        }
    };
}

impl_facet_for_net_type!(core::net::SocketAddr, "SocketAddr");
impl_facet_for_net_type!(core::net::SocketAddrV4, "SocketAddrV4");
impl_facet_for_net_type!(core::net::SocketAddrV6, "SocketAddrV6");
impl_facet_for_net_type!(core::net::IpAddr, "IpAddr");
impl_facet_for_net_type!(core::net::Ipv4Addr, "Ipv4Addr");
impl_facet_for_net_type!(core::net::Ipv6Addr, "Ipv6Addr");
