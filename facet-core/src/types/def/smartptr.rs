use bitflags::bitflags;

use crate::{GenericPtr, PtrConst, PtrMut, PtrUninit};

use super::Shape;

/// Describes a smart pointer — including a vtable to query and alter its state,
/// and the inner shape (the pointee type in the smart pointer).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SmartPointerDef<'shape> {
    /// vtable for interacting with the smart pointer
    pub vtable: &'shape SmartPointerVTable<'shape>,

    /// shape of the inner type of the smart pointer, if not opaque
    pub pointee: Option<fn() -> &'shape Shape<'shape>>,

    /// shape of the corresponding strong pointer, if this pointer is weak
    pub weak: Option<fn() -> &'shape Shape<'shape>>,

    /// shape of the corresponding weak pointer, if this pointer is strong
    pub strong: Option<fn() -> &'shape Shape<'shape>>,

    /// Flags representing various characteristics of the smart pointer
    pub flags: SmartPointerFlags,

    /// An optional field to identify the kind of smart pointer
    pub known: Option<KnownSmartPointer>,
}

impl<'shape> SmartPointerDef<'shape> {
    /// Creates a new `SmartPointerDefBuilder` with all fields set to `None`.
    #[must_use]
    pub const fn builder() -> SmartPointerDefBuilder<'shape> {
        SmartPointerDefBuilder {
            vtable: None,
            pointee: None,
            flags: None,
            known: None,
            weak: None,
            strong: None,
        }
    }

    /// Returns shape of the inner type of the smart pointer, if not opaque
    pub fn pointee(&self) -> Option<&'shape Shape<'shape>> {
        self.pointee.map(|v| v())
    }

    /// Returns shape of the corresponding strong pointer, if this pointer is weak
    pub fn weak(&self) -> Option<&'shape Shape<'shape>> {
        self.weak.map(|v| v())
    }

    /// Returns shape of the corresponding weak pointer, if this pointer is strong
    pub fn strong(&self) -> Option<&'shape Shape<'shape>> {
        self.strong.map(|v| v())
    }
}

/// Builder for creating a `SmartPointerDef`.
#[derive(Debug)]
pub struct SmartPointerDefBuilder<'shape> {
    vtable: Option<&'shape SmartPointerVTable<'shape>>,
    pointee: Option<fn() -> &'shape Shape<'shape>>,
    flags: Option<SmartPointerFlags>,
    known: Option<KnownSmartPointer>,
    weak: Option<fn() -> &'shape Shape<'shape>>,
    strong: Option<fn() -> &'shape Shape<'shape>>,
}

impl<'shape> SmartPointerDefBuilder<'shape> {
    /// Creates a new `SmartPointerDefBuilder` with all fields set to `None`.
    #[must_use]
    #[expect(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            pointee: None,
            flags: None,
            known: None,
            weak: None,
            strong: None,
        }
    }

    /// Sets the vtable for the smart pointer.
    #[must_use]
    pub const fn vtable(mut self, vtable: &'shape SmartPointerVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the shape of the inner type of the smart pointer.
    #[must_use]
    pub const fn pointee(mut self, pointee: fn() -> &'shape Shape<'shape>) -> Self {
        self.pointee = Some(pointee);
        self
    }

    /// Sets the flags for the smart pointer.
    #[must_use]
    pub const fn flags(mut self, flags: SmartPointerFlags) -> Self {
        self.flags = Some(flags);
        self
    }

    /// Sets the known smart pointer type.
    #[must_use]
    pub const fn known(mut self, known: KnownSmartPointer) -> Self {
        self.known = Some(known);
        self
    }

    /// Sets the shape of the corresponding weak pointer, if this pointer is strong.
    #[must_use]
    pub const fn weak(mut self, weak: fn() -> &'shape Shape<'shape>) -> Self {
        self.weak = Some(weak);
        self
    }

    /// Sets the shape of the corresponding strong pointer, if this pointer is weak
    #[must_use]
    pub const fn strong(mut self, strong: fn() -> &'shape Shape<'shape>) -> Self {
        self.strong = Some(strong);
        self
    }

    /// Builds a `SmartPointerDef` from the provided configuration.
    ///
    /// # Panics
    ///
    /// Panics if any required field (vtable, flags) is not set.
    #[must_use]
    pub const fn build(self) -> SmartPointerDef<'shape> {
        SmartPointerDef {
            vtable: self.vtable.unwrap(),
            pointee: self.pointee,
            weak: self.weak,
            strong: self.strong,
            flags: self.flags.unwrap(),
            known: self.known,
        }
    }
}

bitflags! {
    /// Flags to represent various characteristics of smart pointers
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct SmartPointerFlags: u8 {
        /// An empty set of flags
        const EMPTY = 0;

        /// Whether the smart pointer is weak (like [`std::sync::Weak`])
        const WEAK = 1 << 0;
        /// Whether the smart pointer is atomic (like [`std::sync::Arc`])
        const ATOMIC = 1 << 1;
        /// Whether the pointer is a lock (like [`std::sync::Mutex`])
        const LOCK = 1 << 2;
    }
}

/// Tries to upgrade the weak pointer to a strong one.
///
/// If the upgrade succeeds, initializes the smart pointer into the given `strong`, and returns a
/// copy of `strong`, which has been guaranteed to be initialized. If the upgrade fails, `None` is
/// returned and `strong` is not initialized.
///
/// `weak` is not moved out of.
///
/// # Safety
///
/// `weak` must be a valid weak smart pointer (like [`std::sync::Weak`] or [`std::rc::Weak`]).
///
/// `strong` must be allocated, and of the right layout for the corresponding smart pointer.
///
/// `strong` must not have been initialized yet.
pub type UpgradeIntoFn =
    for<'ptr> unsafe fn(weak: PtrMut<'ptr>, strong: PtrUninit<'ptr>) -> Option<PtrMut<'ptr>>;

/// Downgrades a strong pointer to a weak one.
///
/// Initializes the smart pointer into the given `weak`, and returns a copy of `weak`, which has
/// been guaranteed to be initialized.
///
/// Only strong pointers can be downgraded (like [`std::sync::Arc`] or [`std::rc::Rc`]).
///
/// # Safety
///
/// `strong` must be a valid strong smart pointer (like [`std::sync::Arc`] or [`std::rc::Rc`]).
///
/// `weak` must be allocated, and of the right layout for the corresponding weak pointer.
///
/// `weak` must not have been initialized yet.
pub type DowngradeIntoFn =
    for<'ptr> unsafe fn(strong: PtrMut<'ptr>, weak: PtrUninit<'ptr>) -> PtrMut<'ptr>;

/// Tries to obtain a reference to the inner value of the smart pointer.
///
/// This can only be used with strong pointers (like [`std::sync::Arc`] or [`std::rc::Rc`]).
///
/// # Safety
///
/// `this` must be a valid strong smart pointer (like [`std::sync::Arc`] or [`std::rc::Rc`]).
pub type BorrowFn = for<'ptr> unsafe fn(this: PtrConst<'ptr>) -> GenericPtr<'ptr>;

/// Creates a new smart pointer wrapping the given value.
///
/// Initializes the smart pointer into the given `this`, and returns a copy of `this`, which has
/// been guaranteed to be initialized.
///
/// This can only be used with strong pointers (like [`std::sync::Arc`] or [`std::rc::Rc`]).
///
/// # Safety
///
/// `this` must be allocated, and of the right layout for the corresponding smart pointer.
///
/// `this` must not have been initialized yet.
///
/// `ptr` must point to a value of type `T`.
///
/// `ptr` is moved out of (with [`core::ptr::read`]) — it should be deallocated afterwards (e.g.
/// with [`core::mem::forget`]) but NOT dropped).
pub type NewIntoFn = for<'ptr> unsafe fn(this: PtrUninit<'ptr>, ptr: PtrMut<'ptr>) -> PtrMut<'ptr>;

/// Type-erased result of locking a mutex-like smart pointer
pub struct LockResult<'ptr> {
    /// The data that was locked
    data: PtrMut<'ptr>,
    /// The guard that protects the data
    guard: PtrConst<'ptr>,
    /// The vtable for the guard
    guard_vtable: &'static LockGuardVTable,
}

impl<'ptr> LockResult<'ptr> {
    /// Returns a reference to the locked data
    #[must_use]
    pub fn data(&self) -> &PtrMut<'ptr> {
        &self.data
    }
}

impl Drop for LockResult<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.guard_vtable.drop_in_place)(self.guard);
        }
    }
}

/// Functions for manipulating a guard
pub struct LockGuardVTable {
    /// Drops the guard in place
    pub drop_in_place: for<'ptr> unsafe fn(guard: PtrConst<'ptr>),
}

/// Acquires a lock on a mutex-like smart pointer
pub type LockFn = for<'ptr> unsafe fn(opaque: PtrConst<'ptr>) -> Result<LockResult<'ptr>, ()>;

/// Acquires a read lock on a reader-writer lock-like smart pointer
pub type ReadFn = for<'ptr> unsafe fn(opaque: PtrConst<'ptr>) -> Result<LockResult<'ptr>, ()>;

/// Acquires a write lock on a reader-writer lock-like smart pointer
pub type WriteFn = for<'ptr> unsafe fn(opaque: PtrConst<'ptr>) -> Result<LockResult<'ptr>, ()>;

/// Creates a new slice builder for a smart pointer: ie. for `Arc<[u8]>`, it builds a
/// `Vec<u8>` internally, to which you can push, and then turn into an `Arc<[u8]>` at
/// the last stage.
///
/// This works for any `U` in `Arc<[U]>` because those have separate concrete implementations, and
/// thus, separate concrete vtables.
pub type SliceBuilderNewFn = fn() -> PtrMut<'static>;

/// Pushes a value into a slice builder.
///
/// # Safety
///
/// Item must point to a valid value of type `U` and must be initialized.
/// Function is infallible as it uses the global allocator.
pub type SliceBuilderPushFn = unsafe fn(builder: PtrMut, item: PtrMut);

/// Converts a slice builder into a smart pointer. This takes ownership of the builder
/// and frees the backing storage.
///
/// # Safety
///
/// The builder must be valid and must not be used after this function is called.
/// Function is infallible as it uses the global allocator.
pub type SliceBuilderConvertFn = unsafe fn(builder: PtrMut<'static>) -> PtrConst<'static>;

/// Frees a slice builder without converting it into a smart pointer
///
/// # Safety
///
/// The builder must be valid and must not be used after this function is called.
pub type SliceBuilderFreeFn = unsafe fn(builder: PtrMut<'static>);

/// Functions for creating and manipulating slice builders.
#[derive(Debug, Clone, Copy)]
pub struct SliceBuilderVTable {
    /// See [`SliceBuilderNewFn`]
    pub new_fn: SliceBuilderNewFn,
    /// See [`SliceBuilderPushFn`]
    pub push_fn: SliceBuilderPushFn,
    /// See [`SliceBuilderConvertFn`]
    pub convert_fn: SliceBuilderConvertFn,
    /// See [`SliceBuilderFreeFn`]
    pub free_fn: SliceBuilderFreeFn,
}

impl SliceBuilderVTable {
    /// Creates a new `SliceBuilderVTableBuilder` with all fields set to `None`.
    #[must_use]
    pub const fn builder() -> SliceBuilderVTableBuilder {
        SliceBuilderVTableBuilder {
            new_fn: None,
            push_fn: None,
            convert_fn: None,
            free_fn: None,
        }
    }
}

/// Builder for creating a `SliceBuilderVTable`.
#[derive(Debug)]
pub struct SliceBuilderVTableBuilder {
    new_fn: Option<SliceBuilderNewFn>,
    push_fn: Option<SliceBuilderPushFn>,
    convert_fn: Option<SliceBuilderConvertFn>,
    free_fn: Option<SliceBuilderFreeFn>,
}

impl SliceBuilderVTableBuilder {
    /// Creates a new `SliceBuilderVTableBuilder` with all fields set to `None`.
    #[must_use]
    #[expect(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            new_fn: None,
            push_fn: None,
            convert_fn: None,
            free_fn: None,
        }
    }

    /// Sets the `new` function for the slice builder.
    #[must_use]
    pub const fn new_fn(mut self, new_fn: SliceBuilderNewFn) -> Self {
        self.new_fn = Some(new_fn);
        self
    }

    /// Sets the `push` function for the slice builder.
    #[must_use]
    pub const fn push_fn(mut self, push_fn: SliceBuilderPushFn) -> Self {
        self.push_fn = Some(push_fn);
        self
    }

    /// Sets the `convert` function for the slice builder.
    #[must_use]
    pub const fn convert_fn(mut self, convert_fn: SliceBuilderConvertFn) -> Self {
        self.convert_fn = Some(convert_fn);
        self
    }

    /// Sets the `free` function for the slice builder.
    #[must_use]
    pub const fn free_fn(mut self, free_fn: SliceBuilderFreeFn) -> Self {
        self.free_fn = Some(free_fn);
        self
    }

    /// Builds a `SliceBuilderVTable` from the provided configuration.
    #[must_use]
    pub const fn build(self) -> SliceBuilderVTable {
        SliceBuilderVTable {
            new_fn: self.new_fn.expect("new_fn must be set"),
            push_fn: self.push_fn.expect("push_fn must be set"),
            convert_fn: self.convert_fn.expect("convert_fn must be set"),
            free_fn: self.free_fn.expect("free_fn must be set"),
        }
    }
}

/// Functions for interacting with a smart pointer
#[derive(Debug, Clone, Copy)]
pub struct SmartPointerVTable<'shape> {
    /// See [`UpgradeIntoFn`]
    pub upgrade_into_fn: Option<UpgradeIntoFn>,

    /// See [`DowngradeIntoFn`]
    pub downgrade_into_fn: Option<DowngradeIntoFn>,

    /// See [`BorrowFn`]
    pub borrow_fn: Option<BorrowFn>,

    /// See [`NewIntoFn`]
    pub new_into_fn: Option<NewIntoFn>,

    /// See [`LockFn`]
    pub lock_fn: Option<LockFn>,

    /// See [`ReadFn`]
    pub read_fn: Option<ReadFn>,

    /// See [`WriteFn`]
    pub write_fn: Option<WriteFn>,

    /// See [`SliceBuilderVTable`]
    pub slice_builder_vtable: Option<&'shape SliceBuilderVTable>,
}

impl<'shape> SmartPointerVTable<'shape> {
    /// Creates a new `SmartPointerVTableBuilder` with all fields set to `None`.
    #[must_use]
    pub const fn builder() -> SmartPointerVTableBuilder<'shape> {
        SmartPointerVTableBuilder {
            upgrade_into_fn: None,
            downgrade_into_fn: None,
            borrow_fn: None,
            new_into_fn: None,
            lock_fn: None,
            read_fn: None,
            write_fn: None,
            slice_builder_vtable: None,
        }
    }
}

/// Builder for creating a `SmartPointerVTable`.
#[derive(Debug)]
pub struct SmartPointerVTableBuilder<'shape> {
    upgrade_into_fn: Option<UpgradeIntoFn>,
    downgrade_into_fn: Option<DowngradeIntoFn>,
    borrow_fn: Option<BorrowFn>,
    new_into_fn: Option<NewIntoFn>,
    lock_fn: Option<LockFn>,
    read_fn: Option<ReadFn>,
    write_fn: Option<WriteFn>,
    slice_builder_vtable: Option<&'shape SliceBuilderVTable>,
}

impl<'shape> SmartPointerVTableBuilder<'shape> {
    /// Creates a new `SmartPointerVTableBuilder` with all fields set to `None`.
    #[must_use]
    #[expect(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            upgrade_into_fn: None,
            downgrade_into_fn: None,
            borrow_fn: None,
            new_into_fn: None,
            lock_fn: None,
            read_fn: None,
            write_fn: None,
            slice_builder_vtable: None,
        }
    }

    /// Sets the `try_upgrade` function.
    #[must_use]
    pub const fn upgrade_into_fn(mut self, upgrade_into_fn: UpgradeIntoFn) -> Self {
        self.upgrade_into_fn = Some(upgrade_into_fn);
        self
    }

    /// Sets the `downgrade` function.
    #[must_use]
    pub const fn downgrade_into_fn(mut self, downgrade_into_fn: DowngradeIntoFn) -> Self {
        self.downgrade_into_fn = Some(downgrade_into_fn);
        self
    }

    /// Sets the `borrow` function.
    #[must_use]
    pub const fn borrow_fn(mut self, borrow_fn: BorrowFn) -> Self {
        self.borrow_fn = Some(borrow_fn);
        self
    }

    /// Sets the `new_into` function.
    #[must_use]
    pub const fn new_into_fn(mut self, new_into_fn: NewIntoFn) -> Self {
        self.new_into_fn = Some(new_into_fn);
        self
    }

    /// Sets the `lock` function.
    #[must_use]
    pub const fn lock_fn(mut self, lock_fn: LockFn) -> Self {
        self.lock_fn = Some(lock_fn);
        self
    }

    /// Sets the `read` function.
    #[must_use]
    pub const fn read_fn(mut self, read_fn: ReadFn) -> Self {
        self.read_fn = Some(read_fn);
        self
    }

    /// Sets the `write` function.
    #[must_use]
    pub const fn write_fn(mut self, write_fn: WriteFn) -> Self {
        self.write_fn = Some(write_fn);
        self
    }

    /// Sets the `slice_builder_vtable` function.
    #[must_use]
    pub const fn slice_builder_vtable(
        mut self,
        slice_builder_vtable: &'shape SliceBuilderVTable,
    ) -> Self {
        self.slice_builder_vtable = Some(slice_builder_vtable);
        self
    }

    /// Builds a `SmartPointerVTable` from the provided configuration.
    #[must_use]
    pub const fn build(self) -> SmartPointerVTable<'shape> {
        SmartPointerVTable {
            upgrade_into_fn: self.upgrade_into_fn,
            downgrade_into_fn: self.downgrade_into_fn,
            borrow_fn: self.borrow_fn,
            new_into_fn: self.new_into_fn,
            lock_fn: self.lock_fn,
            read_fn: self.read_fn,
            write_fn: self.write_fn,
            slice_builder_vtable: self.slice_builder_vtable,
        }
    }
}

/// Represents common standard library smart pointer kinds
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KnownSmartPointer {
    /// [`Box<T>`](std::boxed::Box), heap-allocated values with single ownership
    Box,
    /// [`Rc<T>`](std::rc::Rc), reference-counted values with multiple ownership
    Rc,
    /// [`Weak<T>`](std::rc::Weak), a weak reference to an `Rc`-managed value
    RcWeak,
    /// [`Arc<T>`](std::sync::Arc), thread-safe reference-counted values with multiple ownership
    Arc,
    /// [`Weak<T>`](std::sync::Weak), a weak reference to an `Arc`-managed value
    ArcWeak,
    /// [`Cow<'a, T>`](std::borrow::Cow), a clone-on-write smart pointer
    Cow,
    /// [`Pin<P>`](std::pin::Pin), a type that pins values behind a pointer
    Pin,
    /// [`Cell<T>`](std::cell::Cell), a mutable memory location with interior mutability
    Cell,
    /// [`RefCell<T>`](std::cell::RefCell), a mutable memory location with dynamic borrowing rules
    RefCell,
    /// [`OnceCell<T>`](std::cell::OnceCell), a cell that can be written to only once
    OnceCell,
    /// [`Mutex<T>`](std::sync::Mutex), a mutual exclusion primitive
    Mutex,
    /// [`RwLock<T>`](std::sync::RwLock), a reader-writer lock
    RwLock,
    /// [`NonNull<T>`](core::ptr::NonNull), a wrapper around a raw pointer that is not null
    NonNull,
}
