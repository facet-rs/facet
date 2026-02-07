//! Shape-only visitor API for deterministic traversal of [`Shape`] trees.
//!
//! This module provides a visitor pattern for walking the static type structure
//! described by [`Shape`], emitting [`Path`] context at each node. It traverses
//! only the type metadata — no values are inspected.
//!
//! # Traversal order
//!
//! - **Depth-first, declaration order.** Struct fields are visited in the order
//!   they appear in the source. Enum variants are visited in declaration order,
//!   and within each variant, fields follow the same rule.
//! - `enter` is called **before** children; `leave` is called **after** children
//!   (only if `enter` returned [`VisitDecision::Recurse`]).
//!
//! # Traversal control
//!
//! [`VisitDecision`] returned from [`ShapeVisitor::enter`] controls descent:
//!
//! | Decision        | Effect                                          |
//! |-----------------|-------------------------------------------------|
//! | `Recurse`       | Visit children, then call `leave`.              |
//! | `SkipChildren`  | Skip descendants of this node; `leave` is still called. |
//! | `Stop`          | Terminate the entire walk immediately.           |
//!
//! # Cycle handling
//!
//! Recursive types (e.g. a tree node whose children are `Vec<Node>`) would cause
//! infinite descent. The walker tracks ancestor types by [`ConstTypeId`] and
//! **does not recurse** into a type that is already on the current path. When a
//! cycle is detected the node is still reported to `enter`/`leave`, but its
//! children are skipped automatically — the visitor does not need to handle this.

use alloc::vec::Vec;

use facet_core::{ConstTypeId, Def, Shape, Type, UserType};

use crate::{Path, PathStep};

/// Decision returned by [`ShapeVisitor::enter`] to control traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitDecision {
    /// Descend into this node's children, then call [`ShapeVisitor::leave`].
    Recurse,
    /// Skip this node's descendants. [`ShapeVisitor::leave`] is still called.
    SkipChildren,
    /// Stop the entire walk immediately. No further callbacks are made.
    Stop,
}

/// Outcome of [`walk_shape`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkStatus {
    /// The walk visited every reachable node.
    Completed,
    /// The walk was terminated early by [`VisitDecision::Stop`].
    Stopped,
}

/// Visitor trait for shape-only traversal.
///
/// Implement this trait to receive callbacks as [`walk_shape`] descends through
/// a [`Shape`] tree.
pub trait ShapeVisitor {
    /// Called when the walker enters a node, **before** visiting children.
    ///
    /// Return a [`VisitDecision`] to control whether children are visited.
    fn enter(&mut self, path: &Path, shape: &'static Shape) -> VisitDecision;

    /// Called when the walker leaves a node, **after** visiting children (or
    /// after skipping them if `enter` returned [`VisitDecision::SkipChildren`]).
    ///
    /// Not called if `enter` returned [`VisitDecision::Stop`].
    fn leave(&mut self, path: &Path, shape: &'static Shape) {
        let _ = (path, shape);
    }
}

/// Walk a [`Shape`] tree depth-first, calling `visitor` at each node.
///
/// See the [module docs](self) for traversal order and control semantics.
pub fn walk_shape(shape: &'static Shape, visitor: &mut impl ShapeVisitor) -> WalkStatus {
    let mut path = Path::new(shape);
    let mut ancestors: Vec<ConstTypeId> = Vec::new();
    if walk_recursive(shape, visitor, &mut path, &mut ancestors) {
        WalkStatus::Stopped
    } else {
        WalkStatus::Completed
    }
}

/// Returns `true` if the walk was stopped.
fn walk_recursive(
    shape: &'static Shape,
    visitor: &mut impl ShapeVisitor,
    path: &mut Path,
    ancestors: &mut Vec<ConstTypeId>,
) -> bool {
    // Cycle detection: if this type is already an ancestor, don't recurse
    let is_cycle = ancestors.contains(&shape.id);

    let decision = visitor.enter(path, shape);
    match decision {
        VisitDecision::Stop => return true,
        VisitDecision::SkipChildren => {
            visitor.leave(path, shape);
            return false;
        }
        VisitDecision::Recurse => {
            if is_cycle {
                // Cycle detected — treat as SkipChildren automatically
                visitor.leave(path, shape);
                return false;
            }
        }
    }

    // Push onto ancestor stack for cycle detection
    ancestors.push(shape.id);

    let stopped = walk_children(shape, visitor, path, ancestors);

    ancestors.pop();

    if !stopped {
        visitor.leave(path, shape);
    }

    stopped
}

/// Walk the children of a shape. Returns `true` if stopped.
///
/// A shape can describe its children through two orthogonal axes:
///
/// - `shape.ty` — structural type information (struct fields, enum variants)
/// - `shape.def` — semantic container information (list element, map key/value, etc.)
///
/// Some types use both (e.g. `Option<T>` is an enum in `ty` and has `Def::Option`),
/// so we must avoid double-visiting. The rule:
///
/// 1. If `ty` is `Struct` or `Enum`, walk children through `ty` (fields/variants).
/// 2. Otherwise, walk children through `def` (container element shapes).
/// 3. Walk `shape.inner` only if neither `ty` nor `def` provided child access.
fn walk_children(
    shape: &'static Shape,
    visitor: &mut impl ShapeVisitor,
    path: &mut Path,
    ancestors: &mut Vec<ConstTypeId>,
) -> bool {
    // Track whether we found children via ty or def, to avoid
    // double-visiting through `inner`.
    let mut has_structural_children = false;

    match shape.ty {
        Type::User(UserType::Struct(st)) => {
            has_structural_children = true;
            for (i, field) in st.fields.iter().enumerate() {
                path.push(PathStep::Field(i as u32));
                if walk_recursive(field.shape(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();
            }
        }
        Type::User(UserType::Enum(et)) => {
            has_structural_children = true;
            for (vi, variant) in et.variants.iter().enumerate() {
                path.push(PathStep::Variant(vi as u32));

                for (fi, field) in variant.data.fields.iter().enumerate() {
                    path.push(PathStep::Field(fi as u32));
                    if walk_recursive(field.shape(), visitor, path, ancestors) {
                        return true;
                    }
                    path.pop();
                }

                path.pop();
            }
        }
        _ => {}
    }

    // For types without structural children (Opaque, Primitive, Sequence, Pointer),
    // descend through the semantic definition.
    if !has_structural_children {
        match shape.def {
            Def::List(ld) => {
                has_structural_children = true;
                path.push(PathStep::Index(0));
                if walk_recursive(ld.t(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();
            }
            Def::Array(ad) => {
                has_structural_children = true;
                path.push(PathStep::Index(0));
                if walk_recursive(ad.t(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();
            }
            Def::Slice(sd) => {
                has_structural_children = true;
                path.push(PathStep::Index(0));
                if walk_recursive(sd.t(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();
            }
            Def::NdArray(nd) => {
                has_structural_children = true;
                path.push(PathStep::Index(0));
                if walk_recursive(nd.t(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();
            }
            Def::Set(sd) => {
                has_structural_children = true;
                path.push(PathStep::Index(0));
                if walk_recursive(sd.t(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();
            }
            Def::Map(md) => {
                has_structural_children = true;
                path.push(PathStep::MapKey(0));
                if walk_recursive(md.k(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();

                path.push(PathStep::MapValue(0));
                if walk_recursive(md.v(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();
            }
            Def::Option(od) => {
                has_structural_children = true;
                path.push(PathStep::OptionSome);
                if walk_recursive(od.t(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();
            }
            Def::Result(rd) => {
                has_structural_children = true;
                // Result<T, E> is Opaque in ty, so we walk both T and E here.
                // Use Variant(0) for Ok, Variant(1) for Err to match Result's
                // semantic structure.
                path.push(PathStep::Variant(0));
                path.push(PathStep::Field(0));
                if walk_recursive(rd.t(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();
                path.pop();

                path.push(PathStep::Variant(1));
                path.push(PathStep::Field(0));
                if walk_recursive(rd.e(), visitor, path, ancestors) {
                    return true;
                }
                path.pop();
                path.pop();
            }
            Def::Pointer(pd) => {
                if let Some(pointee) = pd.pointee() {
                    has_structural_children = true;
                    path.push(PathStep::Deref);
                    if walk_recursive(pointee, visitor, path, ancestors) {
                        return true;
                    }
                    path.pop();
                }
            }
            _ => {}
        }
    }

    // Transparent inner type — only if nothing above already provided children.
    // Types like Vec<T> set both Def::List and inner, and we don't want to
    // visit T twice.
    if !has_structural_children && let Some(inner) = shape.inner {
        path.push(PathStep::Inner);
        if walk_recursive(inner, visitor, path, ancestors) {
            return true;
        }
        path.pop();
    }

    false
}
