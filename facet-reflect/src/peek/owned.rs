use crate::trace;
use facet_core::{PtrMut, Shape};

use super::Peek;

/// An Owned version of a Peek used for custom serialization
///
/// Should be held onto until the serialization of the type
/// is completed.
pub struct OwnedPeek<'mem> {
    pub(crate) data: PtrMut<'mem>,
    pub(crate) shape: &'static Shape,
}

impl<'mem, 'facet> OwnedPeek<'mem> {
    /// returns the shape of the peek
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// returns a borrowed version of the peek
    pub fn as_peek(&self) -> Peek<'mem, 'facet> {
        unsafe { Peek::unchecked_new(self.data.as_const(), self.shape) }
    }
}

impl<'mem> Drop for OwnedPeek<'mem> {
    fn drop(&mut self) {
        trace!("Dropping owned peek of shape '{}'", self.shape);
        let _ = unsafe { self.shape.deallocate_mut(self.data) };
    }
}
