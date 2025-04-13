use facet_core::{OpaqueConst, Shape, SmartPointerDef};

use super::{HeapVal, PokeSmartPointer, PokeValueUninit};

/// Allows initializing an uninitialized option
pub struct PokeSmartPointerUninit<'mem> {
    pub(crate) value: PokeValueUninit<'mem>,
    pub(crate) def: SmartPointerDef,
}

impl<'mem> PokeSmartPointerUninit<'mem> {
    /// Returns the shape for this smart pointer.
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.value.shape
    }

    /// Returns the smart pointer definition.
    #[inline(always)]
    pub fn def(&self) -> &SmartPointerDef {
        &self.def
    }

    /// Get a reference to the underlying PokeValue
    #[inline(always)]
    pub fn into_value(self) -> crate::PokeValueUninit<'mem> {
        self.value
    }

    // /// Creates a new smart pointer from an existing [`PeekValue`].
    // ///
    // /// Note: The `PeekValue` is moved out of (consumed) during this operation.
    // /// It must be deallocated by the caller on success.
    // ///
    // /// Returns `None` if the smart pointer cannot be created directly
    // /// (like for weak pointers).
    // pub fn from_peek_value(self, value: PeekValue<'mem>) -> Option<PokeSmartPointer<'mem>> {
    //     // Assert that the value's shape matches the expected inner type
    //     assert_eq!(
    //         value.shape(),
    //         self.def.t,
    //         "Inner value shape does not match expected smart pointer inner type"
    //     );

    //     let into_fn = self.def.vtable.new_into_fn?;

    //     let opaque = unsafe { into_fn(self.data, value.data()) };
    //     Some(PokeSmartPointer {
    //         value: crate::PokeValue {
    //             data: opaque,
    //             shape: self.shape,
    //         },
    //         def: self.def,
    //     })
    // }
}

impl<'mem> HeapVal<PokeSmartPointerUninit<'mem>> {
    /// Creates a new smart pointer around a given T
    ///
    /// Returns `None` if the smart pointer cannot be created directly
    /// (like for weak pointers).
    pub fn from_t<T>(self, value: T) -> Option<HeapVal<PokeSmartPointer<'mem>>> {
        let into_fn = self.def.vtable.new_into_fn?;

        let value_opaque = OpaqueConst::new(&raw const value);
        let opaque = unsafe { into_fn(self.value.data, value_opaque) };
        core::mem::forget(value);
        Some(self.map(|this| PokeSmartPointer {
            value: crate::PokeValue {
                data: opaque,
                shape: this.value.shape,
            },
            def: this.def,
        }))
    }
}
