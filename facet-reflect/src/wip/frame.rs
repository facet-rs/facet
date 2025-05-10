use crate::trace;
use core::fmt;

use facet_core::{Def, PointerType, PrimitiveType, PtrUninit, SequenceType, Shape, Type, UserType};
#[allow(unused_imports)]
use owo_colors::OwoColorize;

use crate::{ISet, Peek, ValueId};

use super::{FrameFlags, IState};

pub(crate) fn ty_kind(ty: &Type) -> &'static str {
    use PointerType::*;
    use PrimitiveType::*;
    use SequenceType::*;
    use Type::*;
    use UserType::*;

    match ty {
        Primitive(Boolean) => "primitive(boolean)",
        Primitive(Textual(_)) => "primitive(textual)",
        Primitive(Numeric(_)) => "primitive(numeric)",
        Primitive(Never) => "primitive(never)",
        Sequence(Tuple(_)) => "sequence(tuple)",
        Sequence(Array(_)) => "sequence(array)",
        Sequence(Slice(_)) => "sequence(slice)",
        User(Struct(_)) => "user(struct)",
        User(Enum(_)) => "user(enum)",
        User(Union(_)) => "user(union)",
        User(_) => "user(other)",
        Pointer(Reference(_)) => "pointer(reference)",
        Pointer(Raw(_)) => "pointer(raw)",
        Pointer(Function(_)) => "pointer(function)",
        _ => "other",
    }
}

fn def_kind(def: &Def) -> &'static str {
    match def {
        Def::Scalar(_) => "scalar",
        Def::Map(_) => "map",
        Def::List(_) => "list",
        Def::Option(_) => "option",
        Def::SmartPointer(_) => "smart_ptr",
        _ => "other",
    }
}

/// Represents a frame in the initialization stack
pub(crate) struct Frame {
    /// The value we're initializing
    pub(crate) data: PtrUninit<'static>,

    /// The shape of the value
    pub(crate) shape: &'static Shape,

    /// If set, when we're initialized, we must mark the
    /// parent's indexth field as initialized.
    pub(crate) field_index_in_parent: Option<usize>,

    /// Tracking which of our fields are initialized
    /// TODO: I'm not sure we should track "ourselves" as initialized — we always have the
    /// parent to look out for, right now we're tracking children in two states, which isn't ideal
    pub(crate) istate: IState,
}

impl Frame {
    /// Given a ValueId and an IState, recompose a Frame suitable for tracking
    pub(crate) fn recompose(id: ValueId, istate: IState) -> Self {
        Frame {
            data: PtrUninit::new(id.ptr as *mut u8),
            shape: id.shape,
            field_index_in_parent: None,
            istate,
        }
    }

    /// Deallocates the memory used by this frame if it was heap-allocated.
    pub(crate) fn dealloc_if_needed(&mut self) {
        if self.istate.flags.contains(FrameFlags::ALLOCATED) {
            trace!(
                "[{}] {:p} => deallocating {}",
                self.istate.depth,
                self.data.as_mut_byte_ptr().magenta(),
                self.shape.green(),
            );
            match self.shape.layout {
                facet_core::ShapeLayout::Sized(layout) => {
                    if layout.size() != 0 {
                        unsafe {
                            alloc::alloc::dealloc(self.data.as_mut_byte_ptr(), layout);
                        }
                    }
                }
                facet_core::ShapeLayout::Unsized => unimplemented!(),
            }
            self.istate.flags.remove(FrameFlags::ALLOCATED);
        } else {
            trace!(
                "[{}] {:p} => NOT deallocating {} (not ALLOCATED)",
                self.istate.depth,
                self.data.as_mut_byte_ptr().magenta(),
                self.shape.green(),
            );
        }
    }
}

struct DisplayToDebug<T>(T);

impl<T> fmt::Debug for DisplayToDebug<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("shape", &DisplayToDebug(&self.shape))
            .field("def_kind", &def_kind(&self.shape.def))
            .field("ty_kind", &ty_kind(&self.shape.ty))
            .field("index", &self.field_index_in_parent)
            .field("mode", &self.istate.mode)
            .field("id", &self.id())
            .finish()
    }
}

impl Frame {
    /// Returns the value ID for a frame
    pub(crate) fn id(&self) -> ValueId {
        ValueId::new(self.shape, self.data.as_byte_ptr())
    }

    /// Returns true if the frame is fully initialized
    pub(crate) fn is_fully_initialized(&self) -> bool {
        is_fully_initialized(self.shape, &self.istate)
    }

    // Safety: only call if is fully initialized
    pub(crate) unsafe fn drop_and_dealloc_if_needed(mut self) {
        trace!(
            "[Frame::drop] Dropping frame for shape {} at {:p}",
            self.shape.blue(),
            self.data.as_byte_ptr()
        );
        if let Some(drop_in_place) = self.shape.vtable.drop_in_place {
            unsafe {
                trace!(
                    "[Frame::drop] Invoking drop_in_place for shape {} at {:p}",
                    self.shape.green(),
                    self.data.as_byte_ptr()
                );
                drop_in_place(self.data.assume_init());
            }
        } else {
            trace!(
                "[Frame::drop] No drop_in_place function for shape {}",
                self.shape.blue(),
            );
        }
        self.dealloc_if_needed();
    }

    /// Marks the frame as fully initialized
    pub(crate) unsafe fn mark_fully_initialized(&mut self) {
        trace!(
            "[{}] Marking frame as fully initialized: shape={}, type={:?}, def={:?}",
            self.istate.depth,
            self.shape.blue(),
            self.shape.ty,
            self.shape.def
        );

        // Special case for arrays - need to set list_index to the array length
        if let Def::Array(array_def) = self.shape.def {
            trace!(
                "[{}] Marking array as fully initialized with {} elements",
                self.istate.depth, array_def.n
            );
            self.istate.list_index = Some(array_def.n);
        }

        match self.shape.ty {
            Type::User(UserType::Struct(sd)) => {
                trace!(
                    "[{}] Setting all {} struct fields as initialized",
                    self.istate.depth,
                    sd.fields.len()
                );
                self.istate.fields = ISet::all(sd.fields);
            }
            Type::User(UserType::Enum(_)) => {
                if let Some(variant) = &self.istate.variant {
                    trace!(
                        "[{}] Setting all {} fields of variant '{}' as initialized",
                        self.istate.depth,
                        variant.data.fields.len(),
                        variant.name
                    );
                    self.istate.fields = ISet::all(variant.data.fields);
                } else {
                    trace!(
                        "[{}] Trying to mark enum as initialized without variant",
                        self.istate.depth
                    );

                    // now let's find which variant was set with a Peek
                    let peek = unsafe {
                        Peek::unchecked_new(self.data.assume_init().as_const(), self.shape)
                    };
                    let enum_peek = peek.into_enum().unwrap();
                    let variant = enum_peek.active_variant().unwrap();
                    self.istate.variant = Some(*variant);
                    if variant.data.fields.is_empty() {
                        // for unit variants, we mark "fields zero" as initialized
                        self.istate.fields.set(0);
                    } else {
                        self.istate.fields = ISet::all(variant.data.fields);
                    }
                }
            }
            _ => {
                trace!(
                    "[{}] Setting scalar field (0) as initialized",
                    self.istate.depth
                );
                self.istate.fields.set(0);
            }
        }
    }
}

/// Returns true if the frame is fully initialized
pub(crate) fn is_fully_initialized(shape: &'static Shape, istate: &IState) -> bool {
    match shape.ty {
        Type::User(UserType::Struct(sd)) => istate.fields.are_all_set(sd.fields.len()),
        Type::User(UserType::Enum(_)) => match istate.variant.as_ref() {
            None => false,
            Some(v) => istate.fields.are_all_set(v.data.fields.len()),
        },
        _ => istate.fields.are_all_set(1),
    }
}
