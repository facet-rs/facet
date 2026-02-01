//! Option frame handling for `Option<T>`.
//!
//! # Variant Selection
//!
//! Option has two variants accessed via path segment:
//! - `[0]` = None - completes immediately
//! - `[1]` = `Some(T)` - may require staging for inner value
//!
//! # State Transitions
//!
//! ```text
//!                    Set dst=[0] src=Default
//!                    (None variant)
//!                           │
//!                           ▼
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
//!     │         │ Set dst=[1]       │          │
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
    /// Apply a Set operation for Option variant selection.
    ///
    /// Variant indices:
    /// - 0 = None
    /// - 1 = Some(T)
    pub(crate) fn apply_option_variant_set(
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

        // Validate variant index: 0 = None, 1 = Some
        if variant_idx > 1 {
            return Err(
                self.error(ReflectErrorKind::OptionVariantOutOfBounds { index: variant_idx })
            );
        }

        // Get Option def
        let frame = self.arena.get(self.current);
        let Def::Option(_option_def) = frame.shape.def() else {
            return Err(self.error(ReflectErrorKind::NotAnOption));
        };

        // Drop any existing value before switching variants
        self.option_drop_existing()?;

        // Re-get frame and option_def
        let frame = self.arena.get(self.current);
        let Def::Option(option_def) = frame.shape.def() else {
            return Err(self.error(ReflectErrorKind::NotAnOption));
        };
        let inner_shape = option_def.t;

        match variant_idx {
            0 => self.option_set_none(option_def),
            1 => self.option_set_some(option_def, inner_shape, source),
            _ => unreachable!(),
        }
    }

    /// Drop any existing Option value before switching variants.
    fn option_drop_existing(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get_mut(self.current);
        if frame.flags.contains(FrameFlags::INIT) {
            frame.uninit();
        } else if let FrameKind::Option(o) = &mut frame.kind {
            // If we had previously selected Some and the inner was complete,
            // we need to drop it. But since Option doesn't expose variant fields
            // like enums do, we rely on the vtable. For now, if the Option frame
            // was never fully initialized (INIT flag), the inner value was never
            // written via the vtable, so nothing to drop.
            o.selected = None;
            o.inner = Idx::NOT_STARTED;
        }
        Ok(())
    }

    /// Set Option to None variant.
    fn option_set_none(&mut self, option_def: &facet_core::OptionDef) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let init_none_fn = option_def.vtable.init_none;

        // SAFETY: frame.data points to uninitialized Option memory
        unsafe {
            init_none_fn(frame.data);
        }

        // Mark as initialized
        let frame = self.arena.get_mut(self.current);
        frame.flags |= FrameFlags::INIT;
        if let FrameKind::Option(o) = &mut frame.kind {
            o.selected = Some(0);
            o.inner = Idx::COMPLETE;
        }

        Ok(())
    }

    /// Set Option to Some variant.
    fn option_set_some(
        &mut self,
        option_def: &facet_core::OptionDef,
        inner_shape: &'static facet_core::Shape,
        source: &Source<'_>,
    ) -> Result<(), ReflectError> {
        match source {
            Source::Default => self.option_some_default(option_def, inner_shape),
            Source::Imm(imm) => self.option_some_imm(option_def, inner_shape, imm),
            Source::Stage(_capacity) => self.option_some_stage(inner_shape),
        }
    }

    /// Initialize Some with default inner value.
    fn option_some_default(
        &mut self,
        option_def: &facet_core::OptionDef,
        inner_shape: &'static facet_core::Shape,
    ) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);

        // Initialize inner with default, then wrap in Some
        // SAFETY: we'll write through the vtable
        let ok = unsafe { inner_shape.call_default_in_place(frame.data) };
        if ok.is_none() {
            return Err(self.error(ReflectErrorKind::NoDefault {
                shape: ShapeDesc::Static(inner_shape),
            }));
        }

        // The default was written to frame.data, now wrap it in Some
        let init_some_fn = option_def.vtable.init_some;
        // SAFETY: inner value is at frame.data, init_some moves it
        unsafe {
            init_some_fn(frame.data, frame.data.assume_init());
        }

        let frame = self.arena.get_mut(self.current);
        frame.flags |= FrameFlags::INIT;
        if let FrameKind::Option(o) = &mut frame.kind {
            o.selected = Some(1);
            o.inner = Idx::COMPLETE;
        }

        Ok(())
    }

    /// Initialize Some with immediate value.
    fn option_some_imm(
        &mut self,
        option_def: &facet_core::OptionDef,
        inner_shape: &'static facet_core::Shape,
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

        // Initialize as Some with the moved value
        let init_some_fn = option_def.vtable.init_some;
        // SAFETY: imm.ptr_mut() points to initialized value of inner type
        unsafe {
            init_some_fn(frame.data, imm.ptr_mut());
        }

        let frame = self.arena.get_mut(self.current);
        frame.flags |= FrameFlags::INIT;
        if let FrameKind::Option(o) = &mut frame.kind {
            o.selected = Some(1);
            o.inner = Idx::COMPLETE;
        }

        Ok(())
    }

    /// Stage Some variant for incremental construction.
    fn option_some_stage(
        &mut self,
        inner_shape: &'static facet_core::Shape,
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
        inner_frame.parent_link = ParentLink::OptionInner {
            parent: self.current,
        };

        let inner_idx = self.arena.alloc(inner_frame);

        // Record in Option frame
        let frame = self.arena.get_mut(self.current);
        if let FrameKind::Option(o) = &mut frame.kind {
            o.selected = Some(1);
            o.inner = inner_idx;
        }

        self.current = inner_idx;
        Ok(())
    }
}
