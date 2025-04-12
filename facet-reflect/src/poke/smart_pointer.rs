use facet_core::{
    LockResult, Opaque, Shape, SmartPointerDef, SmartPointerFlags, SmartPointerVTable,
};

use super::PokeValue;

/// Allows mutating an initialized smart pointer
pub struct PokeSmartPointer<'mem> {
    pub(crate) value: PokeValue<'mem>,

    pub(crate) def: SmartPointerDef,
}

impl<'mem> PokeSmartPointer<'mem> {
    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.value.shape()
    }

    /// Def getter
    #[inline(always)]
    pub fn def(&self) -> &SmartPointerDef {
        &self.def
    }

    /// Returns the data
    #[inline(always)]
    fn data(&self) -> Opaque<'mem> {
        self.value.data
    }

    /// Returns the smart pointer vtable
    #[inline(always)]
    fn vtable(&self) -> &'static SmartPointerVTable {
        self.def.vtable
    }

    /// Returns whether this smart pointer is weak (like [`std::sync::Weak`]).
    pub fn is_weak(&self) -> bool {
        self.def.flags.contains(SmartPointerFlags::WEAK)
    }

    /// Returns whether this smart pointer is atomic (like [`std::sync::Arc`]).
    pub fn is_atomic(&self) -> bool {
        self.def.flags.contains(SmartPointerFlags::ATOMIC)
    }

    /// Returns whether this pointer is a lock (like [`std::sync::Mutex`]).
    pub fn is_lock(&self) -> bool {
        self.def.flags.contains(SmartPointerFlags::LOCK)
    }

    /// Gets the known smart pointer type, if available.
    pub fn known_type(&self) -> Option<facet_core::KnownSmartPointer> {
        self.def.known
    }

    /// Returns the shape of the inner type of the smart pointer.
    pub fn inner_type(&self) -> &'static Shape {
        self.def.t
    }

    // /// Attempts to borrow the inner value if the smart pointer supports it.
    // pub fn try_borrow(&self) -> Option<PeekValue<'_>> {
    //     let borrow_fn = self.def.vtable.borrow_fn?;
    //     let opaque = unsafe { borrow_fn(self.data().as_const()) };
    //     Some(unsafe { PeekValue::unchecked_new(opaque, self.def.t) })
    // }

    /// Attempts to upgrade this pointer if it's a weak reference.
    pub fn try_upgrade(&self) -> Option<Self> {
        let upgrade_fn = self.def.vtable.try_upgrade_fn?;
        let (data, shape) = unsafe { upgrade_fn(self.data())? };
        Some(Self {
            value: PokeValue { data, shape },
            def: self.def.clone(),
        })
    }

    /// Attempts to lock this pointer if it's a mutex-like smart pointer.
    pub fn try_lock(&self) -> Option<Result<PokeSmartPointerWriteGuard<'_>, ()>> {
        let lock_fn = self.def.vtable.lock_fn?;
        Some(unsafe {
            lock_fn(self.data().as_const())
                .map(|result| PokeSmartPointerWriteGuard::from_lock_result(result, self.def.t))
        })
    }

    /// Attempts to acquire a read lock on this pointer if it's a reader-writer lock.
    pub fn try_read(&self) -> Option<Result<PokeSmartPointerReadGuard<'_>, ()>> {
        let read_fn = self.def.vtable.read_fn?;
        Some(unsafe {
            read_fn(self.data().as_const())
                .map(|result| PokeSmartPointerReadGuard::from_lock_result(result, self.def.t))
        })
    }

    /// Attempts to acquire a write lock on this pointer if it's a reader-writer lock.
    pub fn try_write(&self) -> Option<Result<PokeSmartPointerWriteGuard<'_>, ()>> {
        let write_fn = self.def.vtable.write_fn?;
        Some(unsafe {
            write_fn(self.data().as_const())
                .map(|result| PokeSmartPointerWriteGuard::from_lock_result(result, self.def.t))
        })
    }

    /// Get a reference to the underlying PokeValue
    #[inline(always)]
    pub fn into_value(self) -> crate::PokeValue<'mem> {
        self.value
    }

    // /// Moves `U` out of this `PokeSmartPointer`.
    // ///
    // /// Note that `U` should be something like `Arc<T>`, `Rc<T>`, etc.
    // pub fn build_in_place<U: Facet>(self) -> U {
    //     // Ensure the shape matches the expected type
    //     self.shape.assert_type::<U>();
    //     unsafe { self.data.read::<U>() }
    // }
}

/// Represents a write guard for a lock
pub struct PokeSmartPointerWriteGuard<'mem> {
    #[allow(dead_code)]
    lr: LockResult<'mem>,
    shape: &'static Shape,
}

impl<'mem> PokeSmartPointerWriteGuard<'mem> {
    /// Creates a new write guard from a lock result
    pub(crate) unsafe fn from_lock_result(lr: LockResult<'mem>, shape: &'static Shape) -> Self {
        Self { lr, shape }
    }

    /// Returns the shape for this guard
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }
}

/// Represents a read guard for a lock
pub struct PokeSmartPointerReadGuard<'mem> {
    #[allow(dead_code)]
    lr: LockResult<'mem>,
    shape: &'static Shape,
}

impl<'mem> PokeSmartPointerReadGuard<'mem> {
    /// Creates a new read guard from a lock result
    pub(crate) unsafe fn from_lock_result(lr: LockResult<'mem>, shape: &'static Shape) -> Self {
        Self { lr, shape }
    }

    /// Returns the shape for this guard
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }
}
