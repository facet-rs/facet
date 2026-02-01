//! Enum frame handling for general enums.
//!
//! # Variant Selection
//!
//! Enums are selected via path segment `[variant_idx]` where the index
//! corresponds to the variant's position in the enum definition.
//!
//! # State Transitions
//!
//! ```text
//!     ┌─────────────────────────────────────────┐
//!     │                                         │
//!     │  ┌──────────────┐    ┌──────────────┐  │
//!     │  │   PENDING    │    │   COMPLETE   │  │
//!     │  │              │───►│              │  │
//!     │  │ • selected=  │    │ • INIT=true  │  │
//!     │  │   None       │    │ • selected=  │  │
//!     │  └──────────────┘    │   (idx,      │  │
//!     │         │            │    COMPLETE) │  │
//!     │         │            └──────────────┘  │
//!     │         │                   ▲          │
//!     │         │ Set dst=[n]       │          │
//!     │         │ src=Stage         │ End      │
//!     │         ▼                   │          │
//!     │  ┌──────────────┐           │          │
//!     │  │   STAGING    │───────────┘          │
//!     │  │              │                      │
//!     │  │ • selected=  │                      │
//!     │  │   (idx,      │                      │
//!     │  │    child_idx)│                      │
//!     │  └──────────────┘                      │
//!     │                                         │
//!     └─────────────────────────────────────────┘
//! ```
//!
//! # Variant Types
//!
//! - **Unit variants**: No fields, complete immediately after discriminant write
//! - **Tuple variants**: Single unnamed field, can use `Imm` source directly
//! - **Struct variants**: Named fields, require `Stage` for field-by-field construction

use crate::arena::Idx;
use crate::enum_helpers::{drop_variant_fields, write_discriminant};
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind, ParentLink};
use crate::ops::{Path, PathSegment, Source};
use crate::shape_desc::ShapeDesc;
use facet_core::{PtrUninit, Type, UserType};

use super::Partial;

/// Helper to extract the first field index from a path.
fn first_field_idx(path: &Path) -> Option<u32> {
    match path.segments().first() {
        Some(PathSegment::Field(n)) => Some(*n),
        _ => None,
    }
}

impl<'facet> Partial<'facet> {
    /// Apply a Set operation for enum variant selection.
    ///
    /// This handles general enums (not `Option` or `Result` which have
    /// specialized vtables). The path segment `[n]` selects variant index `n`.
    pub(crate) fn apply_enum_variant_set(
        &mut self,
        path: &Path,
        source: &Source<'_>,
    ) -> Result<(), ReflectError> {
        let segments = path.segments();
        if segments.len() != 1 {
            return Err(self.error(ReflectErrorKind::MultiLevelPathNotSupported {
                depth: segments.len(),
            }));
        }
        let variant_idx = first_field_idx(path).expect("path must have field index");

        // Get enum type and variant
        let frame = self.arena.get(self.current);
        let Type::User(UserType::Enum(ref enum_type)) = *frame.shape.ty() else {
            return Err(self.error(ReflectErrorKind::NotAnEnum));
        };
        let new_variant = self.get_enum_variant(enum_type, variant_idx)?;

        // Drop any existing value before switching variants
        self.enum_drop_existing(enum_type)?;

        // Re-get frame after potential drop/uninit
        let frame = self.arena.get(self.current);

        // Write the discriminant
        // SAFETY: frame.data points to valid enum memory
        unsafe {
            write_discriminant(frame.data, enum_type, new_variant)
                .map_err(|kind| self.error(kind))?;
        }

        match source {
            Source::Default => self.enum_variant_default(new_variant, variant_idx),
            Source::Imm(imm) => self.enum_variant_imm(new_variant, variant_idx, imm),
            Source::Stage(_capacity) => self.enum_variant_stage(new_variant, variant_idx),
        }
    }

    /// Drop any existing enum value before switching variants.
    ///
    /// If INIT is set, the whole enum was initialized (e.g., via `Imm` at `[]`),
    /// so we use `uninit()` which calls `drop_in_place` on the whole shape.
    ///
    /// If INIT is not set but `selected` has a complete variant, we drop just
    /// that variant's fields (the variant was set via `apply_enum_variant_set`).
    fn enum_drop_existing(&mut self, enum_type: &facet_core::EnumType) -> Result<(), ReflectError> {
        let frame = self.arena.get_mut(self.current);
        if frame.flags.contains(FrameFlags::INIT) {
            frame.uninit();
        } else if let FrameKind::Enum(e) = &mut frame.kind {
            if let Some((old_variant_idx, status)) = e.selected
                && status.is_complete()
            {
                let old_variant = &enum_type.variants[old_variant_idx as usize];
                // SAFETY: the variant was marked complete, so its fields are initialized
                unsafe {
                    drop_variant_fields(frame.data.assume_init().as_const(), old_variant);
                }
                // TODO: handle partially initialized variants (status is a valid frame idx)
            }
            // Clear selected so uninit() won't try to drop again if we error later
            e.selected = None;
        }
        Ok(())
    }

    /// Set enum to a unit variant with Default source.
    ///
    /// For unit variants, just writing the discriminant is enough.
    /// For struct variants with Default, we'd need to default-initialize fields.
    /// For now, only support unit variants with Default.
    fn enum_variant_default(
        &mut self,
        new_variant: &'static facet_core::Variant,
        variant_idx: u32,
    ) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);

        if !new_variant.data.fields.is_empty() {
            return Err(self.error(ReflectErrorKind::NoDefault { shape: frame.shape }));
        }

        // Mark variant as complete
        let frame = self.arena.get_mut(self.current);
        let Some(e) = frame.kind.as_enum_mut() else {
            return Err(self.error(ReflectErrorKind::NotAnEnum));
        };
        e.selected = Some((variant_idx, Idx::COMPLETE));

        Ok(())
    }

    /// Set enum to a tuple variant with immediate value.
    ///
    /// For tuple variants with a single field, copy the field value directly.
    /// The `Imm` shape should match the tuple field's shape.
    fn enum_variant_imm(
        &mut self,
        new_variant: &'static facet_core::Variant,
        variant_idx: u32,
        imm: &crate::ops::Imm<'_>,
    ) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);

        // Only single-field tuple variants can use Imm directly
        if new_variant.data.fields.len() != 1 {
            return Err(self.error(ReflectErrorKind::ShapeMismatch {
                expected: frame.shape,
                actual: ShapeDesc::Static(imm.shape()),
            }));
        }

        let field = &new_variant.data.fields[0];
        if !field.shape().is_shape(imm.shape()) {
            return Err(self.error(ReflectErrorKind::ShapeMismatch {
                expected: ShapeDesc::Static(field.shape()),
                actual: ShapeDesc::Static(imm.shape()),
            }));
        }

        // Copy the value into the field
        let field_ptr = unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(field.offset)) };
        unsafe {
            field_ptr.copy_from(imm.ptr(), imm.shape()).unwrap();
        }

        // Mark variant as complete
        let frame = self.arena.get_mut(self.current);
        let Some(e) = frame.kind.as_enum_mut() else {
            return Err(self.error(ReflectErrorKind::NotAnEnum));
        };
        e.selected = Some((variant_idx, Idx::COMPLETE));

        Ok(())
    }

    /// Stage enum variant for incremental field construction.
    ///
    /// Creates a child frame for the variant's fields. The variant data
    /// lives in the same memory as the enum (no separate allocation needed).
    fn enum_variant_stage(
        &mut self,
        new_variant: &'static facet_core::Variant,
        variant_idx: u32,
    ) -> Result<(), ReflectError> {
        // Push a frame for the variant's fields
        let frame = self.arena.get(self.current);
        let mut new_frame = Frame::new_variant(frame.data, frame.shape, new_variant);
        new_frame.parent_link = ParentLink::EnumVariant {
            parent: self.current,
            variant_idx,
        };

        // Store in arena and make it current
        let new_idx = self.arena.alloc(new_frame);

        // Record the frame in enum's selected variant
        let frame = self.arena.get_mut(self.current);
        let Some(e) = frame.kind.as_enum_mut() else {
            return Err(self.error(ReflectErrorKind::NotAnEnum));
        };
        e.selected = Some((variant_idx, new_idx));

        self.current = new_idx;
        Ok(())
    }
}
