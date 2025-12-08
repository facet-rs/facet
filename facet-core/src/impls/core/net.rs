//! Facet implementation for `core::net` types

#![cfg(feature = "net")]

use crate::{Def, Facet, Shape, ShapeBuilder, VTableDirect, vtable_direct};

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
