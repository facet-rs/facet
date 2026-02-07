//! Error types for path-based value access.

use facet_core::Shape;

use crate::PathStep;

/// Error returned when navigating a value using a [`Path`](crate::Path).
///
/// Each variant captures enough context for a caller to produce
/// a meaningful diagnostic without re-walking the path.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PathAccessError {
    /// The root shape of the path doesn't match the value being navigated.
    RootShapeMismatch {
        /// The shape recorded in the path.
        expected: &'static Shape,
        /// The shape of the value we tried to navigate.
        actual: &'static Shape,
    },

    /// The step kind doesn't apply to the current shape.
    ///
    /// For example, `PathStep::Field` on a scalar, or `PathStep::Index` on a struct.
    WrongStepKind {
        /// The step that didn't apply.
        step: PathStep,
        /// Index of this step in the path (0-based).
        step_index: usize,
        /// The shape at the point where the step was attempted.
        shape: &'static Shape,
    },

    /// A field or list index is out of bounds.
    IndexOutOfBounds {
        /// The step that contained the out-of-bounds index.
        step: PathStep,
        /// Index of this step in the path (0-based).
        step_index: usize,
        /// The shape of the container.
        shape: &'static Shape,
        /// The index that was requested.
        index: usize,
        /// The number of available items (fields, elements, variants, etc.).
        bound: usize,
    },

    /// The path says we should be at variant X, but the live value is variant Y.
    VariantMismatch {
        /// Index of this step in the path (0-based).
        step_index: usize,
        /// The enum shape.
        shape: &'static Shape,
        /// The variant index the path expected.
        expected_variant: usize,
        /// The variant index found at runtime.
        actual_variant: usize,
    },

    /// A `Deref`, `Inner`, or `Proxy` step has no target.
    ///
    /// This can happen when a smart pointer has no `borrow_fn`,
    /// or the shape has no `inner`, or no proxy definition.
    MissingTarget {
        /// The step that required a target.
        step: PathStep,
        /// Index of this step in the path (0-based).
        step_index: usize,
        /// The shape that was expected to provide a target.
        shape: &'static Shape,
    },

    /// An `OptionSome` step was encountered but the value is `None`.
    OptionIsNone {
        /// Index of this step in the path (0-based).
        step_index: usize,
        /// The option shape.
        shape: &'static Shape,
    },
}

impl core::fmt::Display for PathAccessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PathAccessError::RootShapeMismatch { expected, actual } => {
                write!(
                    f,
                    "root shape mismatch: path expects {expected}, value is {actual}"
                )
            }
            PathAccessError::WrongStepKind {
                step,
                step_index,
                shape,
            } => {
                write!(
                    f,
                    "step {step_index} ({step:?}) does not apply to shape {shape}"
                )
            }
            PathAccessError::IndexOutOfBounds {
                step,
                step_index,
                shape,
                index,
                bound,
            } => {
                write!(
                    f,
                    "step {step_index} ({step:?}): index {index} out of bounds for {shape} (has {bound})"
                )
            }
            PathAccessError::VariantMismatch {
                step_index,
                shape,
                expected_variant,
                actual_variant,
            } => {
                write!(
                    f,
                    "step {step_index}: variant mismatch on {shape}: path expects variant {expected_variant}, value has variant {actual_variant}"
                )
            }
            PathAccessError::MissingTarget {
                step,
                step_index,
                shape,
            } => {
                write!(
                    f,
                    "step {step_index} ({step:?}): no target available for {shape}"
                )
            }
            PathAccessError::OptionIsNone { step_index, shape } => {
                write!(
                    f,
                    "step {step_index}: option {shape} is None, cannot navigate into Some"
                )
            }
        }
    }
}

impl core::error::Error for PathAccessError {}
