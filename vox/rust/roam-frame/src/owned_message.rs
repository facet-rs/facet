// SAFETY: this module uses unsafe for a self-referential wrapper (yoke-like).
#![allow(unsafe_code)]

use std::mem::ManuallyDrop;

use crate::Frame;

/// A deserialized value co-located with its backing frame.
///
/// The value can borrow from the frame's payload bytes, enabling zero-copy
/// deserialization for `&'a [u8]`, `&'a str`, and `Cow<'a, _>`.
///
/// `T` must be covariant in any lifetime parameters; this is checked at runtime
/// using facet's variance tracking.
pub struct OwnedMessage<T: 'static> {
    value: ManuallyDrop<T>,
    frame: ManuallyDrop<Box<Frame>>,
}

impl<T: 'static> Drop for OwnedMessage<T> {
    fn drop(&mut self) {
        // Drop value first (it may borrow from frame), then frame.
        unsafe {
            ManuallyDrop::drop(&mut self.value);
            ManuallyDrop::drop(&mut self.frame);
        }
    }
}

impl<T: 'static + facet::Facet<'static>> OwnedMessage<T> {
    #[inline]
    pub fn try_new<E>(
        frame: Frame,
        builder: impl FnOnce(&'static [u8]) -> Result<T, E>,
    ) -> Result<Self, E> {
        let variance = (T::SHAPE.variance)(T::SHAPE);
        assert!(
            variance.can_shrink(),
            "OwnedMessage<T> requires T to be covariant (lifetime can shrink safely). Type {:?} has variance {:?}",
            T::SHAPE.id,
            variance
        );

        let frame = Box::new(frame);

        // Create a 'static slice pointing to the boxed frame's payload bytes.
        // This is sound because:
        // - Inline payload lives inside the boxed descriptor (stable address)
        // - Other payload variants live in stable heap storage
        // - We drop `value` before `frame`
        let payload: &'static [u8] = unsafe {
            let bytes = (*frame).payload_bytes();
            std::slice::from_raw_parts(bytes.as_ptr(), bytes.len())
        };

        let value = builder(payload)?;

        Ok(Self {
            value: ManuallyDrop::new(value),
            frame: ManuallyDrop::new(frame),
        })
    }

    #[inline]
    pub fn new(frame: Frame, builder: impl FnOnce(&'static [u8]) -> T) -> Self {
        Self::try_new(frame, |payload| {
            Ok::<_, std::convert::Infallible>(builder(payload))
        })
        .unwrap_or_else(|e: std::convert::Infallible| match e {})
    }
}

impl<T: 'static> OwnedMessage<T> {
    #[inline]
    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    #[inline]
    pub fn value(&self) -> &T {
        &self.value
    }

    #[inline]
    pub fn into_frame(mut self) -> Frame {
        unsafe {
            ManuallyDrop::drop(&mut self.value);
        }
        let frame = unsafe { ManuallyDrop::take(&mut self.frame) };
        std::mem::forget(self);
        *frame
    }
}

impl<T: 'static> std::ops::Deref for OwnedMessage<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: 'static> AsRef<T> for OwnedMessage<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T: 'static + std::fmt::Debug> std::fmt::Debug for OwnedMessage<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedMessage")
            .field("value", &*self.value)
            .finish_non_exhaustive()
    }
}
