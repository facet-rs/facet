//! Zero-copy message wrapper using self-referential structs.
//!
//! This module provides [`OwnedMessage`], a wrapper that co-locates a
//! deserialized value with its backing [`Frame`]. The deserialized value can
//! borrow from the frame's payload bytes, enabling zero-copy deserialization.
//!
//! # How it works
//!
//! Rust doesn't support self-referential structs directly. We work around this by:
//! 1. Boxing the frame for a stable memory address

// Allow unsafe for self-referential struct implementation
#![allow(unsafe_code)]
//! 2. Creating a `'static` slice pointing to the frame's payload (the "lie")
//! 3. Ensuring the value is dropped before the frame
//!
//! This is safe because `T` must be **covariant** in its lifetime parameter,
//! which we verify at runtime using facet's variance tracking.
//!
//! # Example
//!
//! ```ignore
//! use rapace_core::{Frame, OwnedMessage};
//! use std::borrow::Cow;
//!
//! #[derive(facet::Facet)]
//! struct Request<'a> {
//!     name: Cow<'a, str>,
//!     data: &'a [u8],
//! }
//!
//! let owned: OwnedMessage<Request<'static>> = OwnedMessage::try_new(frame, |payload| {
//!     facet_postcard::from_slice(payload)
//! })?;
//!
//! println!("Name: {}", owned.name);
//! ```

use std::mem::ManuallyDrop;

use crate::Frame;

/// A deserialized value co-located with its backing Frame.
///
/// The value can borrow from the frame's payload bytes. Both are dropped
/// together, with the value dropped first to maintain safety.
///
/// This enables zero-copy deserialization: instead of copying strings and
/// byte arrays out of the frame, they can reference the original payload.
///
/// # Type Parameter
///
/// `T` should be the `'static` version of a borrowing type (e.g., `Request<'static>`
/// where `Request<'a>` contains `Cow<'a, str>`). The actual data borrows from
/// the frame, but we erase the lifetime for ergonomics.
///
/// # Safety
///
/// `T` must be covariant in any lifetime parameters. This is checked at runtime
/// via facet's variance tracking. Types containing `&T`, `Cow<T>`, `Box<T>` are
/// covariant. Types containing `&mut T` or `fn(&T)` are NOT covariant.
pub struct OwnedMessage<T: 'static> {
    // SAFETY: value MUST be dropped before frame!
    // We use ManuallyDrop to control drop order in our Drop impl.
    value: ManuallyDrop<T>,
    frame: ManuallyDrop<Box<Frame>>,
}

impl<T: 'static> Drop for OwnedMessage<T> {
    fn drop(&mut self) {
        // SAFETY: Drop value first (it borrows from frame), then frame.
        // This is the critical safety invariant.
        unsafe {
            ManuallyDrop::drop(&mut self.value);
            ManuallyDrop::drop(&mut self.frame);
        }
    }
}

impl<T: 'static + facet::Facet<'static>> OwnedMessage<T> {
    /// Create a new OwnedMessage by deserializing from a frame.
    ///
    /// The `builder` function receives a `'static` slice pointing to the frame's
    /// payload. This lifetime is a "lie" - the data actually lives as long as
    /// the returned `OwnedMessage`.
    ///
    /// # Panics
    ///
    /// Panics if `T` is not covariant in its lifetime parameters.
    #[inline]
    pub fn try_new<E>(
        frame: Frame,
        builder: impl FnOnce(&'static [u8]) -> Result<T, E>,
    ) -> Result<Self, E> {
        // Runtime check for covariance using facet's variance tracking.
        // Covariance means: if T<'long> works, T<'short> also works (lifetime can shrink).
        // This is required because we create a fake 'static slice that actually has
        // a shorter lifetime tied to the OwnedMessage.
        let variance = (T::SHAPE.variance)(T::SHAPE);
        assert!(
            variance.can_shrink(),
            "OwnedMessage<T> requires T to be covariant (lifetime can shrink safely).\n\
             \n\
             Covariant types contain only shared/read-only borrows:\n\
             - &'a T, Cow<'a, T>, Box<T>, Vec<T> where T is covariant\n\
             \n\
             Non-covariant types contain mutable or contravariant positions:\n\
             - &'a mut T, fn(&'a T), Cell<&'a T>\n\
             \n\
             Type {:?} has variance {:?}",
            T::SHAPE.id,
            variance
        );

        // Box the frame for stable address
        let frame = Box::new(frame);

        // Create a 'static slice pointing to the frame's payload.
        // SAFETY: The frame is boxed (stable address) and we ensure
        // the value is dropped before the frame in our Drop impl.
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

    /// Create a new OwnedMessage (infallible version).
    ///
    /// # Panics
    ///
    /// Panics if `T` is not covariant in its lifetime parameters.
    #[inline]
    pub fn new(frame: Frame, builder: impl FnOnce(&'static [u8]) -> T) -> Self {
        Self::try_new(frame, |payload| {
            Ok::<_, std::convert::Infallible>(builder(payload))
        })
        .unwrap_or_else(|e: std::convert::Infallible| match e {})
    }
}

impl<T: 'static> OwnedMessage<T> {
    /// Returns a reference to the underlying frame.
    #[inline]
    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// Returns a reference to the deserialized value.
    #[inline]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Consumes self and returns the underlying frame.
    ///
    /// The deserialized value is dropped before the frame is returned.
    #[inline]
    pub fn into_frame(mut self) -> Frame {
        // Drop value first, then extract frame
        unsafe {
            ManuallyDrop::drop(&mut self.value);
        }
        let frame = unsafe { ManuallyDrop::take(&mut self.frame) };
        // Don't run our Drop impl (we already handled it)
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

// Tests require facet types with proper variance, which need the full
// facet-format-postcard integration. See rapace/tests/ for integration tests.
