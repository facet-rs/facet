use bitflags::bitflags;

use crate::{Opaque, OpaqueConst, OpaqueUninit};

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

impl SmartPointerDef {
    /// Creates a new `SmartPointerDefBuilder` with all fields set to `None`.
    #[must_use]
    pub const fn builder() -> SmartPointerDefBuilder {
        SmartPointerDefBuilder {
            vtable: None,
            t: None,
            flags: None,
            known: None,
        }
    }
}

/// Builder for creating a `SmartPointerDef`.
#[derive(Debug)]
#[allow(clippy::new_without_default)]
pub struct SmartPointerDefBuilder {
    vtable: Option<&'static SmartPointerVTable>,
    t: Option<&'static Shape>,
    flags: Option<SmartPointerFlags>,
    known: Option<Option<KnownSmartPointer>>,
}

impl SmartPointerDefBuilder {
    /// Creates a new `SmartPointerDefBuilder` with all fields set to `None`.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            t: None,
            flags: None,
            known: None,
        }
    }

    /// Sets the vtable for the smart pointer.
    #[must_use]
    pub const fn vtable(mut self, vtable: &'static SmartPointerVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the shape of the inner type of the smart pointer.
    #[must_use]
    pub const fn t(mut self, t: &'static Shape) -> Self {
        self.t = Some(t);
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
    pub const fn known(mut self, known: Option<KnownSmartPointer>) -> Self {
        self.known = Some(known);
        self
    }

    /// Builds a `SmartPointerDef` from the provided configuration.
    ///
    /// # Panics
    ///
    /// Panics if any required field (vtable, t, flags) is not set.
    #[must_use]
    pub const fn build(self) -> SmartPointerDef {
        let vtable = match self.vtable {
            Some(vtable) => vtable,
            None => panic!("vtable must be set"),
        };

        let t = match self.t {
            Some(t) => t,
            None => panic!("t must be set"),
        };

        let flags = match self.flags {
            Some(flags) => flags,
            None => panic!("flags must be set"),
        };

        let known = match self.known {
            Some(known) => known,
            None => None,
        };

        SmartPointerDef {
            vtable,
            t,
            flags,
            known,
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
pub type TryUpgradeFn = for<'ptr> unsafe fn(opaque: Opaque<'ptr>) -> Option<Opaque<'ptr>>;

/// Downgrades a strong pointer to a weak one.
///
/// Only strong pointers can be downgraded (like [`std::sync::Arc`] or [`std::rc::Rc`]).
pub type DowngradeFn = for<'ptr> unsafe fn(opaque: Opaque<'ptr>) -> Opaque<'ptr>;

/// Tries to obtain a reference to the inner value of the smart pointer.
///
/// Weak pointers don't even have that function in their vtable.
pub type BorrowFn = for<'ptr> unsafe fn(opaque: OpaqueConst<'ptr>) -> OpaqueConst<'ptr>;

/// Creates a new smart pointer wrapping the given value. Writes the smart pointer
/// into the given `this`.
///
/// Weak pointers don't even have that function in their vtable.
///
/// # Safety
///
/// `this` must have the correct layout.
///
/// `ptr` must point to a value of type `T`.
///
/// After calling this, `ptr` has been moved out of, and must be
/// deallocated (but not dropped).
pub type NewIntoFn =
    for<'ptr> unsafe fn(this: OpaqueUninit<'ptr>, ptr: OpaqueConst<'ptr>) -> Opaque<'ptr>;

/// Type-erased result of locking a mutex-like smart pointer
pub struct LockResult<'ptr> {
    /// The data that was locked
    data: Opaque<'ptr>,
    /// The guard that protects the data
    guard: OpaqueConst<'ptr>,
    /// The vtable for the guard
    guard_vtable: &'static LockGuardVTable,
}

impl<'ptr> LockResult<'ptr> {
    /// Returns a reference to the locked data
    pub fn data(&self) -> &Opaque<'ptr> {
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

    /// See [`NewIntoFn`]
    pub new_into_fn: Option<NewIntoFn>,

    /// See [`LockFn`]
    pub lock_fn: Option<LockFn>,

    /// See [`ReadFn`]
    pub read_fn: Option<ReadFn>,

    /// See [`WriteFn`]
    pub write_fn: Option<WriteFn>,
}

impl SmartPointerVTable {
    /// Creates a new `SmartPointerVTableBuilder` with all fields set to `None`.
    #[must_use]
    pub const fn builder() -> SmartPointerVTableBuilder {
        SmartPointerVTableBuilder {
            try_upgrade_fn: None,
            downgrade_fn: None,
            borrow_fn: None,
            new_fn: None,
            lock_fn: None,
            read_fn: None,
            write_fn: None,
        }
    }
}

/// Builder for creating a `SmartPointerVTable`.
#[derive(Debug)]
pub struct SmartPointerVTableBuilder {
    try_upgrade_fn: Option<TryUpgradeFn>,
    downgrade_fn: Option<DowngradeFn>,
    borrow_fn: Option<BorrowFn>,
    new_fn: Option<NewIntoFn>,
    lock_fn: Option<LockFn>,
    read_fn: Option<ReadFn>,
    write_fn: Option<WriteFn>,
}

impl SmartPointerVTableBuilder {
    /// Creates a new `SmartPointerVTableBuilder` with all fields set to `None`.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            try_upgrade_fn: None,
            downgrade_fn: None,
            borrow_fn: None,
            new_fn: None,
            lock_fn: None,
            read_fn: None,
            write_fn: None,
        }
    }

    /// Sets the try_upgrade function.
    #[must_use]
    pub const fn try_upgrade_fn(mut self, try_upgrade_fn: TryUpgradeFn) -> Self {
        self.try_upgrade_fn = Some(try_upgrade_fn);
        self
    }

    /// Sets the downgrade function.
    #[must_use]
    pub const fn downgrade_fn(mut self, downgrade_fn: DowngradeFn) -> Self {
        self.downgrade_fn = Some(downgrade_fn);
        self
    }

    /// Sets the borrow function.
    #[must_use]
    pub const fn borrow_fn(mut self, borrow_fn: BorrowFn) -> Self {
        self.borrow_fn = Some(borrow_fn);
        self
    }

    /// Sets the new function.
    #[must_use]
    pub const fn new_into_fn(mut self, new_fn: NewIntoFn) -> Self {
        self.new_fn = Some(new_fn);
        self
    }

    /// Sets the lock function.
    #[must_use]
    pub const fn lock_fn(mut self, lock_fn: LockFn) -> Self {
        self.lock_fn = Some(lock_fn);
        self
    }

    /// Sets the read function.
    #[must_use]
    pub const fn read_fn(mut self, read_fn: ReadFn) -> Self {
        self.read_fn = Some(read_fn);
        self
    }

    /// Sets the write function.
    #[must_use]
    pub const fn write_fn(mut self, write_fn: WriteFn) -> Self {
        self.write_fn = Some(write_fn);
        self
    }

    /// Builds a `SmartPointerVTable` from the provided configuration.
    #[must_use]
    pub const fn build(self) -> SmartPointerVTable {
        SmartPointerVTable {
            try_upgrade_fn: self.try_upgrade_fn,
            downgrade_fn: self.downgrade_fn,
            borrow_fn: self.borrow_fn,
            new_into_fn: self.new_fn,
            lock_fn: self.lock_fn,
            read_fn: self.read_fn,
            write_fn: self.write_fn,
        }
    }
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
