#![allow(unsafe_code)]

use std::mem::ManuallyDrop;
use std::sync::Arc;

/// A decoded value `T` that may borrow from its own backing storage.
///
/// Transports decode into storage they own (heap buffer, VarSlot, mmap).
/// `SelfRef` keeps that storage alive so `T` can safely borrow from it.
///
/// Uses `ManuallyDrop` + custom `Drop` to guarantee drop order: value is
/// dropped before backing, so borrowed references in `T` remain valid
/// through `T`'s drop.
// r[impl zerocopy.recv.selfref]
pub struct SelfRef<T: 'static> {
    /// The decoded value, potentially borrowing from `backing`.
    value: ManuallyDrop<T>,

    /// Backing storage keeping decoded bytes alive.
    backing: ManuallyDrop<Backing>,
}

/// Backing storage for a [`SelfRef`].
pub trait SharedBacking: Send + Sync + 'static {
    /// Access backing bytes.
    fn as_bytes(&self) -> &[u8];
}

// r[impl zerocopy.backing]
pub enum Backing {
    // r[impl zerocopy.backing.boxed]
    /// Heap-allocated buffer (TCP read, BipBuffer copy-out for small messages).
    Boxed(Box<[u8]>),
    /// Shared backing that can be provided by transports (for example SHM slots).
    Shared(Arc<dyn SharedBacking>),
}

impl Backing {
    /// Wrap a transport-provided shared backing.
    pub fn shared(shared: Arc<dyn SharedBacking>) -> Self {
        Self::Shared(shared)
    }

    /// Access the backing bytes.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Backing::Boxed(b) => b,
            Backing::Shared(s) => s.as_bytes(),
        }
    }
}

impl<T: 'static> Drop for SelfRef<T> {
    fn drop(&mut self) {
        // Drop value first (it may borrow from backing), then backing.
        unsafe {
            ManuallyDrop::drop(&mut self.value);
            ManuallyDrop::drop(&mut self.backing);
        }
    }
}

impl<T: 'static> SelfRef<T> {
    /// Construct a `SelfRef` from backing storage and a builder.
    ///
    /// The builder receives a `&'static [u8]` view of the backing bytes —
    /// sound because the backing is heap-allocated (stable address) and
    /// dropped after the value.
    pub fn try_new<E>(
        backing: Backing,
        builder: impl FnOnce(&'static [u8]) -> Result<T, E>,
    ) -> Result<Self, E> {
        // Create a 'static slice from the backing bytes.
        // Sound because:
        // - Backing is heap-allocated (stable address)
        // - We drop value before backing (custom Drop impl)
        let bytes: &'static [u8] = unsafe {
            let b = backing.as_bytes();
            std::slice::from_raw_parts(b.as_ptr(), b.len())
        };

        let value = builder(bytes)?;

        Ok(Self {
            value: ManuallyDrop::new(value),
            backing: ManuallyDrop::new(backing),
        })
    }

    /// Infallible variant of [`try_new`](Self::try_new).
    pub fn new(backing: Backing, builder: impl FnOnce(&'static [u8]) -> T) -> Self {
        Self::try_new(backing, |bytes| {
            Ok::<_, std::convert::Infallible>(builder(bytes))
        })
        .unwrap_or_else(|e: std::convert::Infallible| match e {})
    }
    /// Wrap an owned value that does NOT borrow from backing.
    ///
    /// No variance check — the value is fully owned. The backing is kept
    /// alive but the value doesn't reference it. Useful for in-memory
    /// transports (MemoryLink) where no deserialization occurs.
    pub fn owning(backing: Backing, value: T) -> Self {
        Self {
            value: ManuallyDrop::new(value),
            backing: ManuallyDrop::new(backing),
        }
    }

    /// Transform the contained value, keeping the same backing storage.
    ///
    /// Useful for projecting through wrapper types:
    /// `SelfRef<Frame<T>>` → `SelfRef<T>` by extracting the inner item.
    ///
    /// The closure receives the old value by move and returns the new value.
    /// Any references the new value holds into the backing storage (inherited
    /// from fields of `T`) remain valid — the backing is preserved.
    /// Like [`try_map`](Self::try_map), but the closure also receives a `&'static [u8]`
    /// view of the backing bytes, so the new value `U` can borrow from them.
    pub fn try_repack<U: 'static, E>(
        mut self,
        f: impl FnOnce(T, &'static [u8]) -> Result<U, E>,
    ) -> Result<SelfRef<U>, E> {
        let value = unsafe { ManuallyDrop::take(&mut self.value) };
        let backing = unsafe { ManuallyDrop::take(&mut self.backing) };
        core::mem::forget(self);

        let bytes: &'static [u8] = unsafe {
            let b = backing.as_bytes();
            std::slice::from_raw_parts(b.as_ptr(), b.len())
        };

        match f(value, bytes) {
            Ok(u) => Ok(SelfRef {
                value: ManuallyDrop::new(u),
                backing: ManuallyDrop::new(backing),
            }),
            Err(e) => Err(e),
        }
    }

    pub fn try_map<U: 'static, E>(
        mut self,
        f: impl FnOnce(T) -> Result<U, E>,
    ) -> Result<SelfRef<U>, E> {
        let value = unsafe { ManuallyDrop::take(&mut self.value) };
        let backing = unsafe { ManuallyDrop::take(&mut self.backing) };
        core::mem::forget(self);

        match f(value) {
            Ok(u) => Ok(SelfRef {
                value: ManuallyDrop::new(u),
                backing: ManuallyDrop::new(backing),
            }),
            Err(e) => Err(e),
        }
    }

    pub fn map<U: 'static>(mut self, f: impl FnOnce(T) -> U) -> SelfRef<U> {
        // SAFETY: we take both fields via ManuallyDrop::take, then forget
        // self to prevent its Drop impl from double-dropping them.
        let value = unsafe { ManuallyDrop::take(&mut self.value) };
        let backing = unsafe { ManuallyDrop::take(&mut self.backing) };
        core::mem::forget(self);

        SelfRef {
            value: ManuallyDrop::new(f(value)),
            backing: ManuallyDrop::new(backing),
        }
    }
}

impl<T: 'static> core::ops::Deref for SelfRef<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

// No `into_inner()` — T may borrow from backing. Use Deref instead.
// No `DerefMut` — mutating T could invalidate borrowed references.

/// Pattern-match on a field of a `SelfRef<T>`, projecting to `SelfRef<VariantInner>`
/// in each arm body.
///
/// Uses `Deref` to peek at the discriminant without consuming, then `.map()` to
/// project into the taken arm. The `unreachable!()` in each map closure is genuinely
/// unreachable because the `matches!` guard ensures only the correct variant reaches it.
///
/// Variants not listed are silently consumed and dropped.
///
/// # Example
///
/// ```ignore
/// selfref_match!(msg, payload {
///     MessagePayload::RequestMessage(r) => { /* r: SelfRef<RequestMessage> */ }
///     MessagePayload::ChannelMessage(c) => { /* c: SelfRef<ChannelMessage> */ }
/// })
/// ```
#[macro_export]
macro_rules! selfref_match {
    (
        $selfref:expr, $field:ident {
            $( $first:ident $(:: $rest:ident)* ($binding:tt) => $body:block )*
        }
    ) => {{
        let __sref = $selfref;
        $(
            if ::core::matches!(&__sref.$field, $first$(::$rest)*(_)) {
                #[allow(unused_variables)]
                let $binding = __sref.map(|__v| match __v.$field {
                    $first$(::$rest)*(__inner) => __inner,
                    _ => unreachable!(),
                });
                $body
            } else
        )*
        {
            // Unlisted variant — consume and drop.
            let _ = __sref;
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestSharedBacking {
        bytes: Vec<u8>,
        dropped: Arc<AtomicBool>,
    }

    impl SharedBacking for TestSharedBacking {
        fn as_bytes(&self) -> &[u8] {
            &self.bytes
        }
    }

    impl Drop for TestSharedBacking {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Release);
        }
    }

    struct DropOrderValue {
        backing_dropped: Arc<AtomicBool>,
        value_dropped_before_backing: Arc<AtomicBool>,
    }

    impl Drop for DropOrderValue {
        fn drop(&mut self) {
            let backing_is_dropped = self.backing_dropped.load(Ordering::Acquire);
            self.value_dropped_before_backing
                .store(!backing_is_dropped, Ordering::Release);
        }
    }

    #[test]
    fn try_new_builds_borrowing_value_from_backing() {
        let backing = Backing::Boxed(Box::from([1_u8, 2, 3, 4]));
        let sref = SelfRef::try_new(backing, |bytes| Ok::<_, ()>(&bytes[1..3]))
            .expect("try_new should succeed");
        assert_eq!(&**sref, &[2_u8, 3]);
    }

    #[test]
    fn try_new_propagates_builder_error() {
        let backing = Backing::Boxed(Box::from([9_u8, 8, 7]));
        let err = match SelfRef::<u32>::try_new(backing, |_| Err::<u32, _>("boom")) {
            Ok(_) => panic!("try_new should return builder error"),
            Err(err) => err,
        };
        assert_eq!(err, "boom");
    }

    #[test]
    fn try_map_and_try_repack_preserve_backing_and_transform_value() {
        let backing = Backing::Boxed(Box::from(*b"hello"));
        let sref = SelfRef::new(backing, |bytes| bytes);
        let len_ref = sref
            .try_map(|bytes| Ok::<_, ()>(bytes.len()))
            .expect("try_map should succeed");
        assert_eq!(*len_ref, 5);

        let backing = Backing::Boxed(Box::from(*b"abcdef"));
        let sref = SelfRef::new(backing, |_| 10_u32);
        let repacked = sref
            .try_repack(|value, bytes| Ok::<_, ()>((value + 1, bytes[0], bytes[5])))
            .expect("try_repack should succeed");
        assert_eq!(*repacked, (11_u32, b'a', b'f'));
    }

    #[test]
    fn try_map_and_try_repack_propagate_errors() {
        let backing = Backing::Boxed(Box::from([1_u8, 2, 3]));
        let sref = SelfRef::new(backing, |_| 7_u8);
        let err = match sref.try_map::<u8, _>(|_| Err::<u8, _>("nope")) {
            Ok(_) => panic!("try_map error should propagate"),
            Err(err) => err,
        };
        assert_eq!(err, "nope");

        let backing = Backing::Boxed(Box::from([4_u8, 5, 6]));
        let sref = SelfRef::new(backing, |_| 9_u8);
        let err = match sref.try_repack::<u8, _>(|_, _| Err::<u8, _>("bad")) {
            Ok(_) => panic!("try_repack error should propagate"),
            Err(err) => err,
        };
        assert_eq!(err, "bad");
    }

    #[test]
    fn drop_order_drops_value_before_backing() {
        let backing_dropped = Arc::new(AtomicBool::new(false));
        let value_dropped_before_backing = Arc::new(AtomicBool::new(false));

        let shared = Arc::new(TestSharedBacking {
            bytes: vec![1, 2, 3],
            dropped: Arc::clone(&backing_dropped),
        });

        let value = DropOrderValue {
            backing_dropped: Arc::clone(&backing_dropped),
            value_dropped_before_backing: Arc::clone(&value_dropped_before_backing),
        };

        let sref = SelfRef::owning(Backing::shared(shared), value);
        drop(sref);

        assert!(
            value_dropped_before_backing.load(Ordering::Acquire),
            "value should drop before backing"
        );
        assert!(
            backing_dropped.load(Ordering::Acquire),
            "backing should eventually be dropped"
        );
    }
}
