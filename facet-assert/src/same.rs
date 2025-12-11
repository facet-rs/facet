//! Structural sameness checking for Facet types.

use facet_core::Facet;
use facet_diff::{Diff, DiffOptions, diff_new_peek_with_options};
use facet_diff_core::layout::{
    AnsiBackend, BuildOptions, ColorBackend, DiffFlavor, JsonFlavor, RenderOptions, RustFlavor,
    XmlFlavor, build_layout, render_to_string,
};
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
    pub fn is_same(&self) -> bool {
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

/// A reusable diff plus its original inputs, allowing rendering in different output styles.
pub struct DiffReport<'mem, 'facet> {
    diff: Diff<'mem, 'facet>,
    left: Peek<'mem, 'facet>,
    right: Peek<'mem, 'facet>,
}

impl<'mem, 'facet> DiffReport<'mem, 'facet> {
    /// Access the raw diff tree.
    pub fn diff(&self) -> &Diff<'mem, 'facet> {
        &self.diff
    }

    /// Peek into the left-hand value.
    pub fn left(&self) -> Peek<'mem, 'facet> {
        self.left
    }

    /// Peek into the right-hand value.
    pub fn right(&self) -> Peek<'mem, 'facet> {
        self.right
    }

    /// Format the diff using the legacy tree display (same output as before).
    pub fn legacy_string(&self) -> String {
        format!("{}", self.diff)
    }

    /// Render the diff with a custom flavor and render/build options.
    pub fn render_with_options<B: ColorBackend, F: DiffFlavor>(
        &self,
        flavor: &F,
        build_opts: &BuildOptions,
        render_opts: &RenderOptions<B>,
    ) -> String {
        let layout = build_layout(&self.diff, self.left, self.right, build_opts, flavor);
        render_to_string(&layout, render_opts, flavor)
    }

    /// Render using ANSI colors with the provided flavor.
    pub fn render_ansi_with<F: DiffFlavor>(&self, flavor: &F) -> String {
        let build_opts = BuildOptions::default();
        let render_opts = RenderOptions::<AnsiBackend>::default();
        self.render_with_options(flavor, &build_opts, &render_opts)
    }

    /// Render without colors using the provided flavor.
    pub fn render_plain_with<F: DiffFlavor>(&self, flavor: &F) -> String {
        let build_opts = BuildOptions::default();
        let render_opts = RenderOptions::plain();
        self.render_with_options(flavor, &build_opts, &render_opts)
    }

    /// Render using the Rust flavor with ANSI colors.
    pub fn render_ansi_rust(&self) -> String {
        self.render_ansi_with(&RustFlavor)
    }

    /// Render using the Rust flavor without colors.
    pub fn render_plain_rust(&self) -> String {
        self.render_plain_with(&RustFlavor)
    }

    /// Render using the JSON flavor with ANSI colors.
    pub fn render_ansi_json(&self) -> String {
        self.render_ansi_with(&JsonFlavor)
    }

    /// Render using the JSON flavor without colors.
    pub fn render_plain_json(&self) -> String {
        self.render_plain_with(&JsonFlavor)
    }

    /// Render using the XML flavor with ANSI colors.
    pub fn render_ansi_xml(&self) -> String {
        self.render_ansi_with(&XmlFlavor)
    }

    /// Render using the XML flavor without colors.
    pub fn render_plain_xml(&self) -> String {
        self.render_plain_with(&XmlFlavor)
    }
}

/// Check if two Facet values are structurally the same.
///
/// This does NOT require `PartialEq` - it walks the structure via reflection.
/// Two values are "same" if they have the same structure and values, even if
/// they have different type names.
///
/// Returns [`Sameness::Opaque`] if either value contains an opaque type.
pub fn check_same<'f, T: Facet<'f>, U: Facet<'f>>(left: &T, right: &U) -> Sameness {
    check_same_report(left, right).into_sameness()
}

/// Check if two Facet values are structurally the same, returning a detailed report.
pub fn check_same_report<'f, 'mem, T: Facet<'f>, U: Facet<'f>>(
    left: &'mem T,
    right: &'mem U,
) -> SameReport<'mem, 'f> {
    check_same_with_report(left, right, SameOptions::default())
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
    check_same_with_report(left, right, options).into_sameness()
}

/// Detailed comparison with custom options.
pub fn check_same_with_report<'f, 'mem, T: Facet<'f>, U: Facet<'f>>(
    left: &'mem T,
    right: &'mem U,
    options: SameOptions,
) -> SameReport<'mem, 'f> {
    let left_peek = Peek::new(left);
    let right_peek = Peek::new(right);

    // Convert SameOptions to DiffOptions
    let diff_options = if let Some(tol) = options.float_tolerance {
        DiffOptions::new().with_float_tolerance(tol)
    } else {
        DiffOptions::new()
    };

    // Compute diff with options applied during computation
    let diff = diff_new_peek_with_options(left_peek, right_peek, &diff_options);

    if diff.is_equal() {
        SameReport::Same
    } else {
        SameReport::Different(Box::new(DiffReport {
            diff,
            left: left_peek,
            right: right_peek,
        }))
    }
}
