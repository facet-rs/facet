use facet::{Facet, Shape};
use facet_path::{Path, walk_shape};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::channel;

/// Precomputed plan for an RPC type (args, response, or error).
///
/// Contains the shape and locations of all channels within the type structure.
/// Deserialization plans are cached transparently by facet via `TypePlanCore::from_shape`.
pub struct RpcPlan {
    /// The shape this plan was built for. Used for type-safe construction.
    pub shape: &'static Shape,

    /// Locations of all Rx/Tx channels in this type, in declaration order.
    pub channel_locations: &'static [ChannelLocation],
}

/// A precomputed location of a channel within a type structure.
pub struct ChannelLocation {
    /// Path from the root to this channel.
    pub path: Path,

    /// Whether this is an Rx or Tx channel.
    pub kind: ChannelKind,

    /// Initial credit for this channel, from the const generic `N`.
    pub initial_credit: u32,
}

/// The kind of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Rx,
    Tx,
}

impl RpcPlan {
    fn from_shape(shape: &'static Shape) -> Self {
        let mut visitor = ChannelDiscovery {
            locations: Vec::new(),
        };
        walk_shape(shape, &mut visitor);

        RpcPlan {
            shape,
            channel_locations: visitor.locations.leak(),
        }
    }

    /// Return a process-global cached plan for the given shape.
    pub fn for_shape(shape: &'static Shape) -> &'static Self {
        static CACHE: OnceLock<Mutex<HashMap<usize, &'static RpcPlan>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

        let key = shape as *const Shape as usize;

        let mut guard = cache
            .lock()
            .expect("rpc plan cache mutex should not be poisoned");
        if let Some(plan) = guard.get(&key) {
            return plan;
        }

        let plan = Box::leak(Box::new(Self::from_shape(shape)));
        guard.insert(key, plan);
        plan
    }

    /// Return a process-global cached plan for a concrete type.
    pub fn for_type<T: Facet<'static>>() -> &'static Self {
        Self::for_shape(T::SHAPE)
    }
}

/// Extract the initial credit `N` from a Tx/Rx shape's const params.
fn extract_initial_credit(shape: &'static Shape) -> u32 {
    shape
        .const_params
        .iter()
        .find(|cp| cp.name == "N")
        .map(|cp| cp.value as u32)
        .unwrap_or(16)
}

/// Visitor that discovers Rx/Tx channel locations in a type structure.
// r[impl rpc.channel.discovery]
// r[impl rpc.channel.no-collections]
struct ChannelDiscovery {
    locations: Vec<ChannelLocation>,
}

impl facet_path::ShapeVisitor for ChannelDiscovery {
    fn enter(&mut self, path: &Path, shape: &'static Shape) -> facet_path::VisitDecision {
        if channel::is_tx(shape) {
            self.locations.push(ChannelLocation {
                path: path.clone(),
                kind: ChannelKind::Tx,
                initial_credit: extract_initial_credit(shape),
            });
            return facet_path::VisitDecision::SkipChildren;
        }

        if channel::is_rx(shape) {
            self.locations.push(ChannelLocation {
                path: path.clone(),
                kind: ChannelKind::Rx,
                initial_credit: extract_initial_credit(shape),
            });
            return facet_path::VisitDecision::SkipChildren;
        }

        // Skip all collection subtrees — schema-driven discovery only
        if matches!(
            shape.def,
            facet::Def::List(_) | facet::Def::Array(_) | facet::Def::Map(_) | facet::Def::Set(_)
        ) {
            return facet_path::VisitDecision::SkipChildren;
        }

        facet_path::VisitDecision::Recurse
    }
}
