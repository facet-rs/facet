use bitflags::bitflags;

use crate::OpaqueConst;

use super::Shape;

/// Describes a smart pointer — including a vtable to query and alter its state,
/// and the inner shape (the `T` in `Option<T>`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct SmartPointerDef {
    /// vtable for interacting with the smart pointer
    pub vtable: &'static SmartPointerVTable,

    /// shape of the inner type of the smart pointer
    pub t: &'static Shape,

    /// Flags representing various characteristics of the smart pointer
    pub flags: SmartPointerFlags,

    /// An optional field to identify the kind of smart pointer
    pub known: Option<KnownSmartPointer>,
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
pub type TryUpgradeFn = for<'ptr> unsafe fn(opaque: OpaqueConst<'ptr>) -> Option<OpaqueConst<'ptr>>;

/// Downgrades a strong pointer to a weak one.
///
/// Only strong pointers can be downgraded (like [`std::sync::Arc`] or [`std::rc::Rc`]).
pub type DowngradeFn = for<'ptr> unsafe fn(opaque: OpaqueConst<'ptr>) -> OpaqueConst<'ptr>;

/// Tries to obtain a reference to the inner value of the smart pointer.
///
/// Weak pointers don't even have that function in their vtable.
pub type BorrowFn = for<'ptr> unsafe fn(opaque: OpaqueConst<'ptr>) -> Option<OpaqueConst<'ptr>>;

/// Creates a new smart pointer wrapping the given value
///
/// Weak pointers don't even have that function in their vtable.
pub type NewFn = for<'ptr> unsafe fn(t: OpaqueConst<'ptr>) -> OpaqueConst<'ptr>;

/// Type-erased result of locking a mutex-like smart pointer
pub struct LockResult<'ptr> {
    /// The data that was locked
    data: OpaqueConst<'ptr>,
    /// The guard that protects the data
    guard: OpaqueConst<'ptr>,
    /// The vtable for the guard
    guard_vtable: &'static GuardVTable,
}

impl<'ptr> LockResult<'ptr> {
    /// Returns a reference to the locked data
    pub fn data(&self) -> &OpaqueConst<'ptr> {
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
pub struct GuardVTable {
    /// Drops the guard in place
    pub drop_in_place: for<'ptr> unsafe fn(guard: OpaqueConst<'ptr>),
}

/// Acquires a lock on a mutex-like smart pointer
pub type LockFn = for<'ptr> unsafe fn(opaque: OpaqueConst<'ptr>) -> Result<LockResult<'ptr>, ()>;

/// Acquires a read lock on a reader-writer lock-like smart pointer
pub type ReadFn = for<'ptr> unsafe fn(opaque: OpaqueConst<'ptr>) -> Result<LockResult<'ptr>, ()>;

/// Acquires a write lock on a reader-writer lock-like smart pointer
pub type WriteFn = for<'ptr> unsafe fn(opaque: OpaqueConst<'ptr>) -> Result<LockResult<'ptr>, ()>;

/// Functions for interacting with a smart pointer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SmartPointerVTable {
    /// See [`TryUpgradeFn`]
    pub try_upgrade_fn: Option<TryUpgradeFn>,

    /// See [`DowngradeFn`]
    pub downgrade_fn: Option<DowngradeFn>,

    /// See [`BorrowFn`]
    pub borrow_fn: Option<BorrowFn>,

    /// See [`NewFn`]
    pub new_fn: Option<NewFn>,

    /// See [`LockFn`]
    pub lock_fn: Option<LockFn>,

    /// See [`ReadFn`]
    pub read_fn: Option<ReadFn>,

    /// See [`WriteFn`]
    pub write_fn: Option<WriteFn>,
}

/// Represents common standard library smart pointer kinds
#[non_exhaustive]
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
}
