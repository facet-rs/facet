use crate::{PtrConst, PtrMut};

/// Create a new iterator that iterates over the provided value
///
/// # Safety
///
/// The `value` parameter must point to aligned, initialized memory of the correct type.
pub type IterInitWithValueFn = for<'value> unsafe fn(value: PtrConst<'value>) -> PtrMut<'value>;

/// Advance the iterator, returning the next value from the iterator
///
/// # Safety
///
/// The `iter` parameter must point to aligned, initialized memory of the correct type.
pub type IterNextFn<T> =
    for<'iter> unsafe fn(iter: PtrMut<'iter>) -> Option<<T as IterItem>::Item<'iter>>;

/// Advance the iterator in reverse, returning the next value from the end
/// of the iterator.
///
/// # Safety
///
/// The `iter` parameter must point to aligned, initialized memory of the correct type.
pub type IterNextBackFn<T> =
    for<'iter> unsafe fn(iter: PtrMut<'iter>) -> Option<<T as IterItem>::Item<'iter>>;

/// Return the lower and upper bounds of the iterator, if known.
///
/// # Safety
///
/// The `iter` parameter must point to aligned, initialized memory of the correct type.
pub type IterSizeHintFn =
    for<'iter> unsafe fn(iter: PtrMut<'iter>) -> Option<(usize, Option<usize>)>;

/// Deallocate the iterator
///
/// # Safety
///
/// The `iter` parameter must point to aligned, initialized memory of the correct type.
pub type IterDeallocFn = for<'iter> unsafe fn(iter: PtrMut<'iter>);

/// VTable for an iterator
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
#[non_exhaustive]
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
    /// Returns a builder for [`IterVTable`]
    pub const fn builder() -> IterVTableBuilder<T> {
        IterVTableBuilder::new()
    }
}

/// Builds an [`IterVTable`]
pub struct IterVTableBuilder<T: IterItem> {
    init_with_value: Option<IterInitWithValueFn>,
    next: Option<IterNextFn<T>>,
    next_back: Option<IterNextBackFn<T>>,
    size_hint: Option<IterSizeHintFn>,
    dealloc: Option<IterDeallocFn>,
}

impl<T: IterItem> IterVTableBuilder<T> {
    /// Creates a new [`IterVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            init_with_value: None,
            next: None,
            next_back: None,
            size_hint: None,
            dealloc: None,
        }
    }

    /// Sets the `init_with_value` function
    pub const fn init_with_value(mut self, f: IterInitWithValueFn) -> Self {
        self.init_with_value = Some(f);
        self
    }

    /// Sets the `next` function
    pub const fn next(mut self, f: IterNextFn<T>) -> Self {
        self.next = Some(f);
        self
    }

    /// Sets the `next_back` function
    pub const fn next_back(mut self, f: IterNextBackFn<T>) -> Self {
        self.next_back = Some(f);
        self
    }

    /// Sets the `dealloc` function
    pub const fn dealloc(mut self, f: IterDeallocFn) -> Self {
        self.dealloc = Some(f);
        self
    }

    /// Builds the [`IterVTable`] from the current state of the builder.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields are `None`.
    pub const fn build(self) -> IterVTable<T> {
        assert!(self.init_with_value.is_some());
        assert!(self.next_back.is_some());
        assert!(self.size_hint.is_some());
        IterVTable {
            init_with_value: self.init_with_value,
            next: self.next.unwrap(),
            next_back: self.next_back,
            size_hint: self.size_hint,
            dealloc: self.dealloc.unwrap(),
        }
    }
}

/// A kind of item that an [`IterVTable`] returns
///
/// This trait is needed as a utility, so the functions within [`IterVTable`]
/// can apply the appropriate lifetime to their result types. In other words,
/// this trait acts like a higher-kinded type that takes a lifetime.
pub trait IterItem {
    /// The output type of the iterator, bound by the lifetime `'a`
    type Item<'a>;
}

impl IterItem for PtrConst<'_> {
    type Item<'a> = PtrConst<'a>;
}

impl<T, U> IterItem for (T, U)
where
    T: IterItem,
    U: IterItem,
{
    type Item<'a> = (T::Item<'a>, U::Item<'a>);
}
