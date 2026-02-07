use std::sync::Arc;

use facet::Facet;
use facet_core::Shape;
use facet_path::{Path, walk_shape};
use facet_reflect::TypePlanCore;

use crate::{Rx, Tx};

/// Precomputed plan for an RPC type (args, response, or error).
///
/// Contains both the deserialization plan and the locations of all channels
/// within the type structure. Computed once per monomorphized type via `OnceLock`.
pub struct RpcPlan {
    /// Deserialization plan for this type.
    pub type_plan: Arc<TypePlanCore>,
    /// Locations of all Rx/Tx channels in this type, in declaration order.
    pub channel_locations: Vec<ChannelLocation>,
}

/// A precomputed location of a channel within a type structure.
pub struct ChannelLocation {
    /// Path from the root to this channel.
    pub path: Path,
    /// Whether this is an Rx or Tx channel.
    pub kind: ChannelKind,
}

/// The kind of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Rx,
    Tx,
}

impl RpcPlan {
    /// Build an RpcPlan for the given shape.
    ///
    /// # Safety
    ///
    /// `shape` must come from a `Facet` implementation.
    #[allow(unsafe_code)]
    pub unsafe fn from_shape(shape: &'static Shape) -> Self {
        // Build deserialization plan
        // SAFETY: caller guarantees shape comes from a Facet implementation
        let type_plan =
            unsafe { TypePlanCore::from_shape(shape) }.expect("TypePlanCore::from_shape failed");

        // Walk the type structure to discover channel locations
        let mut visitor = ChannelDiscovery {
            locations: Vec::new(),
        };
        walk_shape(shape, &mut visitor);

        RpcPlan {
            type_plan,
            channel_locations: visitor.locations,
        }
    }

    /// Build an RpcPlan for a concrete type.
    #[allow(unsafe_code)]
    pub fn for_type<T: Facet<'static>>() -> Self {
        // SAFETY: T::SHAPE comes from a Facet implementation
        unsafe { Self::from_shape(T::SHAPE) }
    }
}

/// Visitor that discovers Rx/Tx channel locations in a type structure.
struct ChannelDiscovery {
    locations: Vec<ChannelLocation>,
}

impl facet_path::ShapeVisitor for ChannelDiscovery {
    fn enter(&mut self, path: &Path, shape: &'static Shape) -> facet_path::VisitDecision {
        // Check if this is an Rx type
        if shape.decl_id == Rx::<()>::SHAPE.decl_id {
            self.locations.push(ChannelLocation {
                path: path.clone(),
                kind: ChannelKind::Rx,
            });
            return facet_path::VisitDecision::SkipChildren;
        }

        // Check if this is a Tx type
        if shape.decl_id == Tx::<()>::SHAPE.decl_id {
            self.locations.push(ChannelLocation {
                path: path.clone(),
                kind: ChannelKind::Tx,
            });
            return facet_path::VisitDecision::SkipChildren;
        }

        // Skip all collection subtrees — schema-driven discovery only
        // r[call.request.channels.schema-driven]
        if matches!(
            shape.def,
            facet::Def::List(_) | facet::Def::Array(_) | facet::Def::Map(_) | facet::Def::Set(_)
        ) {
            return facet_path::VisitDecision::SkipChildren;
        }

        facet_path::VisitDecision::Recurse
    }
}
