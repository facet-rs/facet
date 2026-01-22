//! Structural sameness checking for Facet types.

use facet_core::Facet;
use facet_diff::{DiffOptions, DiffReport, diff_new_peek_with_options};
use facet_reflect::Peek;

/// Options for customizing structural comparison behavior.
///
/// Use the builder pattern to configure options:
///
/// ```
/// use facet_assert::SameOptions;
///
/// let options = SameOptions::new()
///     .float_tolerance(1e-6);
/// ```
#[derive(Debug, Clone, Default)]
pub struct SameOptions {
    /// Tolerance for floating-point comparisons.
    /// If set, two floats are considered equal if their absolute difference
    /// is less than or equal to this value.
    float_tolerance: Option<f64>,
}

impl SameOptions {
    /// Create a new `SameOptions` with default settings (exact comparison).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tolerance for floating-point comparisons.
    ///
    /// When set, two `f32` or `f64` values are considered equal if:
    /// `|left - right| <= tolerance`
    ///
    /// # Example
    ///
    /// ```
    /// use facet_assert::{assert_same_with, SameOptions};
    ///
    /// let a = 1.0000001_f64;
    /// let b = 1.0000002_f64;
    ///
    /// // This would fail with exact comparison:
    /// // assert_same!(a, b);
    ///
    /// // But passes with tolerance:
    /// assert_same_with!(a, b, SameOptions::new().float_tolerance(1e-6));
    /// ```
    pub const fn float_tolerance(mut self, tolerance: f64) -> Self {
        self.float_tolerance = Some(tolerance);
        self
    }
}

/// Result of checking if two values are structurally the same.
pub enum Sameness {
    /// The values are structurally the same.
    Same,
    /// The values differ - contains a formatted diff.
    Different(String),
    /// Encountered an opaque type that cannot be compared.
    Opaque {
        /// The type name of the opaque type.
        type_name: &'static str,
    },
}

/// Detailed comparison result that retains the computed diff.
pub enum SameReport<'mem, 'facet> {
    /// The values are structurally the same.
    Same,
    /// The values differ - includes a diff report that can be rendered in multiple formats.
    Different(Box<DiffReport<'mem, 'facet>>),
    /// Encountered an opaque type that cannot be compared.
    Opaque {
        /// The type name of the opaque type.
        type_name: &'static str,
    },
}

impl<'mem, 'facet> SameReport<'mem, 'facet> {
    /// Returns `true` if the two values matched.
    pub const fn is_same(&self) -> bool {
        matches!(self, Self::Same)
    }

    /// Convert this report into a [`Sameness`] summary, formatting diffs using the legacy display.
    pub fn into_sameness(self) -> Sameness {
        match self {
            SameReport::Same => Sameness::Same,
            SameReport::Different(report) => Sameness::Different(report.legacy_string()),
            SameReport::Opaque { type_name } => Sameness::Opaque { type_name },
        }
    }

    /// Get the diff report if the values were different.
    pub fn diff(&self) -> Option<&DiffReport<'mem, 'facet>> {
        match self {
            SameReport::Different(report) => Some(report.as_ref()),
            _ => None,
        }
    }
}

// =============================================================================
// Same-type comparison (the common case)
// =============================================================================

/// Check if two Facet values are structurally the same.
///
/// This does NOT require `PartialEq` - it walks the structure via reflection.
/// Both values must have the same type, which enables type inference to flow
/// between arguments.
///
/// # Example
///
/// ```
/// use facet_assert::check_same;
///
/// let x: Option<Option<i32>> = Some(None);
/// check_same(&x, &Some(None)); // Type of Some(None) inferred from x
/// ```
///
/// For comparing values of different types, use [`check_sameish`].
pub fn check_same<'f, T: Facet<'f>>(left: &T, right: &T) -> Sameness {
    check_same_report(left, right).into_sameness()
}

/// Check if two Facet values are structurally the same, returning a detailed report.
pub fn check_same_report<'f, 'mem, T: Facet<'f>>(
    left: &'mem T,
    right: &'mem T,
) -> SameReport<'mem, 'f> {
    check_same_with_report(left, right, SameOptions::default())
}

/// Check if two Facet values are structurally the same, with custom options.
///
/// # Example
///
/// ```
/// use facet_assert::{check_same_with, SameOptions, Sameness};
///
/// let a = 1.0000001_f64;
/// let b = 1.0000002_f64;
///
/// // With tolerance, these are considered the same
/// let options = SameOptions::new().float_tolerance(1e-6);
/// assert!(matches!(check_same_with(&a, &b, options), Sameness::Same));
/// ```
pub fn check_same_with<'f, T: Facet<'f>>(left: &T, right: &T, options: SameOptions) -> Sameness {
    check_same_with_report(left, right, options).into_sameness()
}

/// Detailed comparison with custom options.
pub fn check_same_with_report<'f, 'mem, T: Facet<'f>>(
    left: &'mem T,
    right: &'mem T,
    options: SameOptions,
) -> SameReport<'mem, 'f> {
    check_sameish_with_report(left, right, options)
}

// =============================================================================
// Cross-type comparison (for migration scenarios, etc.)
// =============================================================================

/// Check if two Facet values of potentially different types are structurally the same.
///
/// Unlike [`check_same`], this allows comparing values of different types.
/// Two values are "sameish" if they have the same structure and values,
/// even if they have different type names.
///
/// **Note:** Because the two arguments can have different types, the compiler
/// cannot infer types from one side to the other. If you get type inference
/// errors, either add type annotations or use [`check_same`] instead.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_assert::check_sameish;
///
/// #[derive(Facet)]
/// struct PersonV1 { name: String }
///
/// #[derive(Facet)]
/// struct PersonV2 { name: String }
///
/// let old = PersonV1 { name: "Alice".into() };
/// let new = PersonV2 { name: "Alice".into() };
/// check_sameish(&old, &new); // Different types, same structure
/// ```
pub fn check_sameish<'f, T: Facet<'f>, U: Facet<'f>>(left: &T, right: &U) -> Sameness {
    check_sameish_report(left, right).into_sameness()
}

/// Check if two Facet values of different types are structurally the same, returning a detailed report.
pub fn check_sameish_report<'f, 'mem, T: Facet<'f>, U: Facet<'f>>(
    left: &'mem T,
    right: &'mem U,
) -> SameReport<'mem, 'f> {
    check_sameish_with_report(left, right, SameOptions::default())
}

/// Check if two Facet values of different types are structurally the same, with custom options.
pub fn check_sameish_with<'f, T: Facet<'f>, U: Facet<'f>>(
    left: &T,
    right: &U,
    options: SameOptions,
) -> Sameness {
    check_sameish_with_report(left, right, options).into_sameness()
}

/// Detailed cross-type comparison with custom options.
pub fn check_sameish_with_report<'f, 'mem, T: Facet<'f>, U: Facet<'f>>(
    left: &'mem T,
    right: &'mem U,
    options: SameOptions,
) -> SameReport<'mem, 'f> {
    let left_peek = Peek::new(left);
    let right_peek = Peek::new(right);

    // Convert SameOptions to DiffOptions
    let mut diff_options = DiffOptions::new();
    if let Some(tol) = options.float_tolerance {
        diff_options = diff_options.with_float_tolerance(tol);
    }

    // Compute diff with options applied during computation
    let diff = diff_new_peek_with_options(left_peek, right_peek, &diff_options);

    if diff.is_equal() {
        SameReport::Same
    } else {
        let mut report = DiffReport::new(diff, left_peek, right_peek);
        if let Some(tol) = options.float_tolerance {
            report = report.with_float_tolerance(tol);
        }
        SameReport::Different(Box::new(report))
    }
}
