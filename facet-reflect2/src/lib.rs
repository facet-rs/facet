//! # facet-reflect2
//!
//! Partial value construction for facet - v2 API with tree-based tracking.
//!
//! This crate provides a clean reimplementation of facet-reflect with:
//! - Tree-based frame tracking (not BTreeMap<Path, Frame>)
//! - Arena allocation with free list for frame reuse
//! - Simplified 4-operation API: Set, End, Push, Insert
//! - Proper deferred mode support for formats like TOML
//!
//! See the design docs at the crate root:
//! - README.md: Core operation design
//! - MAPPING.md: API v1 to v2 operation mapping
//! - TRACKING.md: Initialization tracking concerns
//! - TREE.md: Tree-based frame structure

mod arena;
mod frame;
mod partial;

pub use partial::Partial;
