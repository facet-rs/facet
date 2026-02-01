//! Result frame handling for `Result<T, E>`.
//!
//! # Variant Selection
//!
//! Result has two variants accessed via path segment:
//! - `[0]` = `Ok(T)`
//! - `[1]` = `Err(E)`
//!
//! # State Transitions
//!
//! ```text
//!     ┌─────────────────────────────────────────┐
//!     │                                         │
//!     │  ┌──────────────┐    ┌──────────────┐  │
//!     │  │   PENDING    │    │   COMPLETE   │  │
//!     │  │              │───►│              │  │
//!     │  │ • inner=     │    │ • INIT=true  │  │
//!     │  │   NOT_STARTED│    │ • selected=  │  │
//!     │  └──────────────┘    │   0 or 1     │  │
//!     │         │            └──────────────┘  │
//!     │         │                   ▲          │
//!     │         │ Set dst=[0|1]     │          │
//!     │         │ src=Stage         │ End      │
//!     │         ▼                   │          │
//!     │  ┌──────────────┐           │          │
//!     │  │   STAGING    │───────────┘          │
//!     │  │              │                      │
//!     │  │ • inner=     │                      │
//!     │  │   child_idx  │                      │
//!     │  └──────────────┘                      │
//!     │                                         │
//!     └─────────────────────────────────────────┘
//! ```

use std::alloc::alloc;

use crate::arena::Idx;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind, ParentLink};
use crate::ops::{Path, PathSegment, Source};
use crate::shape_desc::ShapeDesc;
use facet_core::{Def, PtrUninit, Type, UserType};

use super::Partial;

/// Helper to extract the first field index from a path.
fn first_field_idx(path: &Path) -> Option<u32> {
    match path.segments().first() {
        Some(PathSegment::Field(n)) => Some(*n),
        _ => None,
    }
}

impl<'facet> Partial<'facet> {
    /// Apply a Set operation for Result variant selection.
    ///
    /// Variant indices:
    /// - 0 = Ok(T)
    /// - 1 = Err(E)
    pub(crate) fn apply_result_variant_set(
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

        // Validate variant index: 0 = Ok, 1 = Err
        if variant_idx > 1 {
            return Err(
                self.error(ReflectErrorKind::ResultVariantOutOfBounds { index: variant_idx })
            );
        }

        // Get Result def
        let frame = self.arena.get(self.current);
        let Def::Result(_result_def) = frame.shape.def() else {
            return Err(self.error(ReflectErrorKind::NotAResult));
        };

        // Drop any existing value before switching variants
        self.result_drop_existing()?;

        // Re-get frame and result_def
        let frame = self.arena.get(self.current);
        let Def::Result(result_def) = frame.shape.def() else {
            return Err(self.error(ReflectErrorKind::NotAResult));
        };

        // Get the inner shape based on variant
        let inner_shape = if variant_idx == 0 {
            result_def.t // Ok type
        } else {
            result_def.e // Err type
        };

        self.result_set_variant(result_def, inner_shape, variant_idx, source)
    }

    /// Drop any existing Result value before switching variants.
    fn result_drop_existing(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get_mut(self.current);
        if frame.flags.contains(FrameFlags::INIT) {
            frame.uninit();
        } else if let FrameKind::Result(r) = &mut frame.kind {
            r.selected = None;
            r.inner = Idx::NOT_STARTED;
        }
        Ok(())
    }

    /// Set Result to Ok or Err variant.
    fn result_set_variant(
        &mut self,
        result_def: &facet_core::ResultDef,
        inner_shape: &'static facet_core::Shape,
        variant_idx: u32,
        source: &Source<'_>,
    ) -> Result<(), ReflectError> {
        match source {
            Source::Default => self.result_variant_default(result_def, inner_shape, variant_idx),
            Source::Imm(imm) => self.result_variant_imm(result_def, inner_shape, variant_idx, imm),
            Source::Stage(_capacity) => self.result_variant_stage(inner_shape, variant_idx),
        }
    }

    /// Initialize Result variant with default inner value.
    fn result_variant_default(
        &mut self,
        result_def: &facet_core::ResultDef,
        inner_shape: &'static facet_core::Shape,
        variant_idx: u32,
    ) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);

        // Initialize inner with default, then wrap
        // SAFETY: we'll write through the vtable
        let ok = unsafe { inner_shape.call_default_in_place(frame.data) };
        if ok.is_none() {
            return Err(self.error(ReflectErrorKind::NoDefault {
                shape: ShapeDesc::Static(inner_shape),
            }));
        }

        // Wrap in Ok or Err
        if variant_idx == 0 {
            let init_ok_fn = result_def.vtable.init_ok;
            unsafe {
                init_ok_fn(frame.data, frame.data.assume_init());
            }
        } else {
            let init_err_fn = result_def.vtable.init_err;
            unsafe {
                init_err_fn(frame.data, frame.data.assume_init());
            }
        }

        let frame = self.arena.get_mut(self.current);
        frame.flags |= FrameFlags::INIT;
        if let FrameKind::Result(r) = &mut frame.kind {
            r.selected = Some(variant_idx);
            r.inner = Idx::COMPLETE;
        }

        Ok(())
    }

    /// Initialize Result variant with immediate value.
    fn result_variant_imm(
        &mut self,
        result_def: &facet_core::ResultDef,
        inner_shape: &'static facet_core::Shape,
        variant_idx: u32,
        imm: &crate::ops::Imm<'_>,
    ) -> Result<(), ReflectError> {
        // Verify shape matches inner type
        if !inner_shape.is_shape(imm.shape()) {
            return Err(self.error(ReflectErrorKind::ShapeMismatch {
                expected: ShapeDesc::Static(inner_shape),
                actual: ShapeDesc::Static(imm.shape()),
            }));
        }

        let frame = self.arena.get(self.current);

        // Initialize as Ok or Err with the moved value
        if variant_idx == 0 {
            let init_ok_fn = result_def.vtable.init_ok;
            unsafe {
                init_ok_fn(frame.data, imm.ptr_mut());
            }
        } else {
            let init_err_fn = result_def.vtable.init_err;
            unsafe {
                init_err_fn(frame.data, imm.ptr_mut());
            }
        }

        let frame = self.arena.get_mut(self.current);
        frame.flags |= FrameFlags::INIT;
        if let FrameKind::Result(r) = &mut frame.kind {
            r.selected = Some(variant_idx);
            r.inner = Idx::COMPLETE;
        }

        Ok(())
    }

    /// Stage Result variant for incremental construction.
    fn result_variant_stage(
        &mut self,
        inner_shape: &'static facet_core::Shape,
        variant_idx: u32,
    ) -> Result<(), ReflectError> {
        // Allocate temporary space for the inner value
        let layout = inner_shape.layout.sized_layout().map_err(|_| {
            self.error(ReflectErrorKind::Unsized {
                shape: ShapeDesc::Static(inner_shape),
            })
        })?;

        let temp_ptr = if layout.size() == 0 {
            PtrUninit::new(std::ptr::NonNull::<u8>::dangling().as_ptr())
        } else {
            let ptr = unsafe { alloc(layout) };
            if ptr.is_null() {
                return Err(self.error(ReflectErrorKind::AllocFailed { layout }));
            }
            PtrUninit::new(ptr)
        };

        // Create appropriate frame based on inner shape
        // Check Def first because Option/Result have Def::Option/Result
        // but are also UserType::Enum at the ty level
        let mut inner_frame = match &inner_shape.def {
            Def::Option(_) => Frame::new_option(temp_ptr, inner_shape),
            Def::Result(_) => Frame::new_result(temp_ptr, inner_shape),
            _ => match inner_shape.ty {
                Type::User(UserType::Struct(ref s)) => {
                    Frame::new_struct(temp_ptr, inner_shape, s.fields.len())
                }
                Type::User(UserType::Enum(_)) => Frame::new_enum(temp_ptr, inner_shape),
                _ => Frame::new(temp_ptr, inner_shape),
            },
        };
        inner_frame.flags |= FrameFlags::OWNS_ALLOC;
        inner_frame.parent_link = ParentLink::ResultInner {
            parent: self.current,
            is_ok: variant_idx == 0,
        };

        let inner_idx = self.arena.alloc(inner_frame);

        // Record in Result frame
        let frame = self.arena.get_mut(self.current);
        if let FrameKind::Result(r) = &mut frame.kind {
            r.selected = Some(variant_idx);
            r.inner = inner_idx;
        }

        self.current = inner_idx;
        Ok(())
    }
}
