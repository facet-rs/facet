//! Path navigation for flattened struct deserialization.
//!
//! This module provides `PathNavigator`, which manages the complexity of navigating
//! through nested flattened structures during deserialization.

use facet_core::Def;
use facet_reflect::{FieldPath, Partial, Span, VariantSelection};
use facet_solver::PathSegment;

use crate::{DeserializeError, SpanGuard};

/// Tracks an open path segment during flatten deserialization.
#[derive(Debug, Clone)]
struct OpenSegment {
    /// The field name of this segment.
    name: &'static str,
    /// Whether this segment is wrapped in an Option (and we entered Some).
    is_option: bool,
}

/// Navigates through nested flattened structures by managing open/close of path segments.
///
/// This abstraction handles the complexity of:
/// - Finding common prefixes between current position and target path
/// - Closing segments that are no longer needed
/// - Opening new segments, handling Options along the way
/// - Selecting enum variants when required by the resolution
pub(crate) struct PathNavigator<'input, const BORROW: bool> {
    /// The work-in-progress partial. Stored as Option to allow taking ownership temporarily.
    wip: Option<Partial<'input, BORROW>>,
    /// Currently open path segments.
    open_segments: Vec<OpenSegment>,
    /// Last span for error reporting.
    last_span: Span,
}

impl<'input, const BORROW: bool> PathNavigator<'input, BORROW> {
    /// Create a new navigator starting at the root of the given partial.
    pub fn new(wip: Partial<'input, BORROW>, last_span: Span) -> Self {
        Self {
            wip: Some(wip),
            open_segments: Vec::new(),
            last_span,
        }
    }

    /// Update the span used for error reporting (call after consuming parser events).
    pub fn set_span(&mut self, span: Span) {
        self.last_span = span;
    }

    /// Get a reference to the wip for inspection.
    pub fn wip(&self) -> &Partial<'input, BORROW> {
        self.wip.as_ref().expect("wip taken but not returned")
    }

    /// Take the wip for operations that consume it, like deserialize_into.
    pub fn take_wip(&mut self) -> Partial<'input, BORROW> {
        self.wip.take().expect("wip taken but not returned")
    }

    /// Return the wip after operations that consumed it.
    pub fn return_wip(&mut self, wip: Partial<'input, BORROW>) {
        assert!(self.wip.is_none(), "wip returned but was not taken");
        self.wip = Some(wip);
    }

    /// Consume the navigator and return the wip.
    pub fn into_wip(mut self) -> Partial<'input, BORROW> {
        self.wip.take().expect("wip taken but not returned")
    }

    /// Navigate to a target path, closing/opening segments as needed.
    ///
    /// After this call, `wip` is positioned at the final field in the path,
    /// ready for deserialization. The final segment is NOT added to `open_segments`
    /// since it will typically be closed immediately after deserialization.
    ///
    /// Returns navigation metadata needed by the caller.
    pub fn navigate_to(
        &mut self,
        target: &FieldPath,
        variant_selections: &[VariantSelection],
    ) -> Result<NavigateResult, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Extract field names from the path (excluding trailing Variant)
        let target_fields: Vec<&'static str> = target
            .segments()
            .iter()
            .filter_map(|s| match s {
                PathSegment::Field(name) => Some(*name),
                PathSegment::Variant(_, _) => None,
            })
            .collect();

        // Check if this path ends with a Variant segment
        let trailing_variant = match target.segments().last() {
            Some(PathSegment::Variant(_, name)) => Some(*name),
            _ => None,
        };

        // Find common prefix with currently open segments
        let common_len = self
            .open_segments
            .iter()
            .zip(target_fields.iter())
            .take_while(|(seg, field_name)| seg.name == **field_name)
            .count();

        // Close segments that are no longer needed (in reverse order)
        self.close_to(common_len)?;

        // Split into intermediate segments and final segment
        let segments_to_open = &target_fields[common_len..];
        let (intermediate, final_segment) = match segments_to_open {
            [] => (&[][..], None),
            [.., last] => (&segments_to_open[..segments_to_open.len() - 1], Some(*last)),
        };

        // Open intermediate segments (these are flatten containers)
        for &segment in intermediate {
            self.open_segment(segment, variant_selections)?;
        }

        // Open the final segment but don't add it to open_segments
        // (caller will close it after deserializing)
        let final_is_option = if let Some(segment) = final_segment {
            let mut wip = self.take_wip();
            wip = wip.begin_field(segment)?;
            let is_option = matches!(wip.shape().def, Def::Option(_));
            if is_option {
                wip = wip.begin_some()?;
            }
            self.return_wip(wip);
            is_option
        } else {
            false
        };

        Ok(NavigateResult {
            _common_len: common_len,
            final_segment,
            final_is_option,
            trailing_variant,
        })
    }

    /// Open a single segment, handling Options and variant selection.
    fn open_segment(
        &mut self,
        name: &'static str,
        variant_selections: &[VariantSelection],
    ) -> Result<(), DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        let mut wip = self.take_wip();
        wip = wip.begin_field(name)?;
        let is_option = matches!(wip.shape().def, Def::Option(_));
        if is_option {
            wip = wip.begin_some()?;
        }

        // Check if we need to select a variant at this point
        let current_path: Vec<&str> = self
            .open_segments
            .iter()
            .map(|seg| seg.name)
            .chain(core::iter::once(name))
            .collect();

        for vs in variant_selections {
            let vs_fields: Vec<&str> = vs
                .path
                .segments()
                .iter()
                .filter_map(|s| match s {
                    PathSegment::Field(f) => Some(*f),
                    PathSegment::Variant(_, _) => None,
                })
                .collect();

            if current_path == vs_fields {
                wip = wip.select_variant_named(vs.variant_name)?;
                break;
            }
        }

        self.return_wip(wip);
        self.open_segments.push(OpenSegment { name, is_option });
        Ok(())
    }

    /// Close the final segment after deserialization.
    ///
    /// Call this after deserializing a value to close the segment that was
    /// opened by `navigate_to` but not added to `open_segments`.
    pub fn close_final(&mut self, is_option: bool) -> Result<(), DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        let mut wip = self.take_wip();
        if is_option {
            wip = wip.end()?;
        }
        wip = wip.end()?;
        self.return_wip(wip);
        Ok(())
    }

    /// Close segments back to a given depth.
    pub fn close_to(&mut self, target_len: usize) -> Result<(), DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        while self.open_segments.len() > target_len {
            let seg = self.open_segments.pop().unwrap();
            let mut wip = self.take_wip();
            if seg.is_option {
                wip = wip.end()?;
            }
            wip = wip.end()?;
            self.return_wip(wip);
        }
        Ok(())
    }

    /// Close all open segments back to the root.
    pub fn close_all(&mut self) -> Result<(), DeserializeError> {
        self.close_to(0)
    }

    /// Add the final segment to open_segments (for catch-all maps that stay open).
    pub fn keep_final_open(&mut self, nav_result: &NavigateResult) {
        if let Some(final_seg) = nav_result.final_segment {
            self.open_segments.push(OpenSegment {
                name: final_seg,
                is_option: nav_result.final_is_option,
            });
        }
    }
}

/// Result of navigating to a path.
pub(crate) struct NavigateResult {
    /// How many segments were already open (common prefix length).
    pub _common_len: usize,
    /// The final segment name, if any.
    pub final_segment: Option<&'static str>,
    /// Whether the final segment was an Option (and we entered Some).
    pub final_is_option: bool,
    /// If the path ends with a Variant segment, the variant name.
    pub trailing_variant: Option<&'static str>,
}
