use crate::{PtrConst, PtrMut};

/// Create a new iterator that iterates over the provided value
///
/// # Safety
///
/// The `value` parameter must point to aligned, initialized memory of the correct type.
pub type IterInitWithValueFn = unsafe fn(value: PtrConst) -> PtrMut;

/// Advance the iterator, returning the next value from the iterator
///
/// # Safety
///
/// The `iter` parameter must point to aligned, initialized memory of the correct type.
pub type IterNextFn<T> = unsafe fn(iter: PtrMut) -> Option<<T as IterItem>::Item>;

/// Advance the iterator in reverse, returning the next value from the end
/// of the iterator.
///
/// # Safety
///
/// The `iter` parameter must point to aligned, initialized memory of the correct type.
pub type IterNextBackFn<T> = unsafe fn(iter: PtrMut) -> Option<<T as IterItem>::Item>;

/// Return the lower and upper bounds of the iterator, if known.
///
/// # Safety
///
/// The `iter` parameter must point to aligned, initialized memory of the correct type.
pub type IterSizeHintFn = unsafe fn(iter: PtrMut) -> Option<(usize, Option<usize>)>;

/// Deallocate the iterator
///
/// # Safety
///
/// The `iter` parameter must point to aligned, initialized memory of the correct type.
pub type IterDeallocFn = unsafe fn(iter: PtrMut);

/// VTable for an iterator
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct IterVTable<T: IterItem> {
    /// cf. [`IterInitWithValueFn`]
    pub init_with_value: Option<IterInitWithValueFn>,

    /// cf. [`IterNextFn`]
    pub next: IterNextFn<T>,

    /// cf. [`IterNextBackFn`]
    pub next_back: Option<IterNextBackFn<T>>,

    /// cf. [`IterSizeHintFn`]
    pub size_hint: Option<IterSizeHintFn>,

    /// cf. [`IterDeallocFn`]
    pub dealloc: IterDeallocFn,
}

impl<T: IterItem> IterVTable<T> {
    /// Const ctor; required: `next`, `dealloc`. Others default to `None`.
    pub const fn new(next: IterNextFn<T>, dealloc: IterDeallocFn) -> Self {
        Self {
            init_with_value: None,
            next,
            next_back: None,
            size_hint: None,
            dealloc,
        }
    }
}

/// A kind of item that an [`IterVTable`] returns
///
/// This trait is needed as a utility, so the functions within [`IterVTable`]
/// can apply the appropriate type to their result types.
pub trait IterItem {
    /// The output type of the iterator
    type Item;
}

impl IterItem for PtrConst {
    type Item = PtrConst;
}

impl<T, U> IterItem for (T, U)
where
    T: IterItem,
    U: IterItem,
{
    type Item = (T::Item, U::Item);
}
