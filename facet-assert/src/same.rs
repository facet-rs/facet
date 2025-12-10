//! Structural sameness checking for Facet types.

use facet_core::Facet;
use facet_diff::{DiffOptions, diff_new_peek_with_options};
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
    pub fn float_tolerance(mut self, tolerance: f64) -> Self {
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

/// Check if two Facet values are structurally the same.
///
/// This does NOT require `PartialEq` - it walks the structure via reflection.
/// Two values are "same" if they have the same structure and values, even if
/// they have different type names.
///
/// Returns [`Sameness::Opaque`] if either value contains an opaque type.
pub fn check_same<'f, T: Facet<'f>, U: Facet<'f>>(left: &T, right: &U) -> Sameness {
    check_same_with(left, right, SameOptions::default())
}

/// Check if two Facet values are structurally the same, with custom options.
///
/// Like [`check_same`], but allows configuring comparison behavior via [`SameOptions`].
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
pub fn check_same_with<'f, T: Facet<'f>, U: Facet<'f>>(
    left: &T,
    right: &U,
    options: SameOptions,
) -> Sameness {
    // Convert SameOptions to DiffOptions
    let diff_options = DiffOptions::new();
    let diff_options = if let Some(tol) = options.float_tolerance {
        diff_options.with_float_tolerance(tol)
    } else {
        diff_options
    };

    // Compute diff with options applied during computation
    let diff = diff_new_peek_with_options(Peek::new(left), Peek::new(right), &diff_options);

    if diff.is_equal() {
        Sameness::Same
    } else {
        // Use facet-diff's native Display formatting
        Sameness::Different(format!("{}", diff))
    }
}
