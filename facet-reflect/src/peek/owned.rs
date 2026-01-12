use crate::trace;
use core::marker::PhantomData;
use facet_core::{PtrMut, Shape};

use super::Peek;

/// An Owned version of a Peek used for custom serialization
///
/// Should be held onto until the serialization of the type
/// is completed.
pub struct OwnedPeek<'mem> {
    pub(crate) data: PtrMut,
    pub(crate) shape: &'static Shape,
    pub(crate) _phantom: PhantomData<&'mem ()>,
}

impl<'mem, 'facet> OwnedPeek<'mem> {
    /// returns the shape of the peek
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// returns a borrowed version of the peek
    pub fn as_peek(&'mem self) -> Peek<'mem, 'facet> {
        unsafe { Peek::unchecked_new(self.data.as_const(), self.shape) }
    }
}

impl<'mem> Drop for OwnedPeek<'mem> {
    fn drop(&mut self) {
        trace!("Dropping owned peek of shape '{}'", self.shape);
        unsafe { self.shape.call_drop_in_place(self.data) };
        let _ = unsafe { self.shape.deallocate_mut(self.data) };
    }
}
