use super::Partial;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameKind};
use crate::ops::Source;
use facet_core::{Def, PtrUninit, Shape, Type, UserType};

impl<'facet> Partial<'facet> {
    /// Apply a Push operation to add an element to the current list or set.
    pub(crate) fn apply_push(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        // Check if this is a list or set and delegate to the appropriate module
        let frame = self.arena.get(self.current);
        match &frame.kind {
            FrameKind::List(_) => self.list_append(source),
            FrameKind::Set(_) => self.set_append(source),
            _ => Err(self.error(ReflectErrorKind::NotAList)),
        }
    }

    /// Create a frame appropriate for the given shape.
    pub(crate) fn create_frame_for_shape(ptr: PtrUninit, shape: &'static Shape) -> Frame {
        match shape.def {
            Def::List(list_def) => Frame::new_list(ptr, shape, list_def),
            Def::Map(map_def) => Frame::new_map(ptr, shape, map_def),
            Def::Set(set_def) => Frame::new_set(ptr, shape, set_def),
            Def::Option(_) => Frame::new_option(ptr, shape),
            Def::Result(_) => Frame::new_result(ptr, shape),
            _ => match shape.ty {
                Type::User(UserType::Struct(ref s)) => {
                    Frame::new_struct(ptr, shape, s.fields.len())
                }
                Type::User(UserType::Enum(_)) => Frame::new_enum(ptr, shape),
                _ => Frame::new(ptr, shape),
            },
        }
    }
}
