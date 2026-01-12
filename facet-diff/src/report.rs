//! Diff report with multi-format rendering capabilities.
//!
//! This module provides [`DiffReport`], which holds a computed diff along with
//! references to the original values, enabling rendering in multiple output formats
//! (Rust, JSON, XML) with or without ANSI colors.

use crate::Diff;
use facet_diff_core::layout::{
    AnsiBackend, BuildOptions, ColorBackend, DiffFlavor, JsonFlavor, RenderOptions, RustFlavor,
    XmlFlavor, build_layout, render_to_string,
};
use facet_reflect::Peek;

/// A reusable diff plus its original inputs, allowing rendering in different output styles.
///
/// `DiffReport` holds a computed [`Diff`] along with [`Peek`] references to the original
/// left and right values. This allows rendering the same diff in multiple formats without
/// recomputing the diff tree.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_diff::{DiffReport, diff_new_peek};
/// use facet_reflect::Peek;
///
/// #[derive(Facet)]
/// struct Point { x: i32, y: i32 }
///
/// let old = Point { x: 10, y: 20 };
/// let new = Point { x: 10, y: 30 };
///
/// let left = Peek::new(&old);
/// let right = Peek::new(&new);
/// let diff = diff_new_peek(left, right);
///
/// let report = DiffReport::new(diff, left, right);
///
/// // Render in different formats
/// println!("Rust format:\n{}", report.render_plain_rust());
/// println!("JSON format:\n{}", report.render_plain_json());
/// println!("XML format:\n{}", report.render_plain_xml());
/// ```
pub struct DiffReport<'mem, 'facet> {
    diff: Diff<'mem, 'facet>,
    left: Peek<'mem, 'facet>,
    right: Peek<'mem, 'facet>,
    /// Float tolerance used during comparison, stored to compute display precision.
    float_tolerance: Option<f64>,
}

impl<'mem, 'facet> DiffReport<'mem, 'facet> {
    /// Create a new diff report from a computed diff and the original values.
    pub const fn new(
        diff: Diff<'mem, 'facet>,
        left: Peek<'mem, 'facet>,
        right: Peek<'mem, 'facet>,
    ) -> Self {
        Self {
            diff,
            left,
            right,
            float_tolerance: None,
        }
    }

    /// Create a new diff report with float tolerance information.
    ///
    /// The tolerance is used to determine appropriate decimal precision when rendering
    /// floating-point values in the diff output.
    pub const fn with_float_tolerance(mut self, tolerance: f64) -> Self {
        self.float_tolerance = Some(tolerance);
        self
    }

    /// Access the raw diff tree.
    pub const fn diff(&self) -> &Diff<'mem, 'facet> {
        &self.diff
    }

    /// Peek into the left-hand value.
    pub const fn left(&self) -> Peek<'mem, 'facet> {
        self.left
    }

    /// Peek into the right-hand value.
    pub const fn right(&self) -> Peek<'mem, 'facet> {
        self.right
    }

    /// Format the diff using the legacy tree display (same output as `Display` impl).
    pub fn legacy_string(&self) -> String {
        format!("{}", self.diff)
    }

    /// Compute float precision from tolerance.
    ///
    /// If tolerance is 0.002, we need ~3 decimal places to see differences at that scale.
    /// Formula: ceil(-log10(tolerance))
    fn float_precision_from_tolerance(&self) -> Option<usize> {
        self.float_tolerance.map(|tol| {
            if tol <= 0.0 {
                6 // fallback to reasonable precision
            } else {
                (-tol.log10()).ceil() as usize
            }
        })
    }

    /// Build options with float precision derived from tolerance.
    fn build_opts_with_precision(&self) -> BuildOptions {
        BuildOptions {
            float_precision: self.float_precision_from_tolerance(),
            ..Default::default()
        }
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
        let build_opts = self.build_opts_with_precision();
        let render_opts = RenderOptions::<AnsiBackend>::default();
        self.render_with_options(flavor, &build_opts, &render_opts)
    }

    /// Render without colors using the provided flavor.
    pub fn render_plain_with<F: DiffFlavor>(&self, flavor: &F) -> String {
        let build_opts = self.build_opts_with_precision();
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
