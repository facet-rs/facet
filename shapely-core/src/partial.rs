use crate::{FieldError, Innards, ShapeDesc, Shapely, Slot, trace};
use std::{alloc, ptr::NonNull};

/// Origin of the partial — did we allocate it? Or is it borrowed?
pub enum Origin<'s> {
    /// It was allocated via `alloc::alloc` and needs to be deallocated on drop,
    /// moving out, etc.
    HeapAllocated,

    /// It was generously lent to us by some outside code, and we are NOT
    /// to free it (although we should still uninitialize any fields that we initialized).
    Borrowed {
        parent: Option<&'s Partial<'s>>,
        init_mark: InitMark<'s>,
    },
}

/// A partially-initialized shape.
///
/// This type keeps track of the initialized state of every field and only allows getting out the
/// concrete type or the boxed concrete type or moving out of this partial into a pointer if all the
/// fields have been initialized.
pub struct Partial<'s> {
    /// Address of the value we're building in memory.
    /// If the type is a ZST, then the addr will be dangling.
    pub(crate) addr: NonNull<u8>,

    /// Where `addr` came from (ie. are we responsible for freeing it?)
    pub(crate) origin: Origin<'s>,

    /// Keeps track of which fields are initialized
    pub(crate) init_set: InitSet64,

    /// The shape we're building, asserted when building, but
    /// also when getting fields slots, etc.
    pub(crate) shape: ShapeDesc,
}

/// We can build a tree of partials when deserializing, so `Partial<'s>` has to be covariant over 's.
fn _assert_partial_covariant<'long: 'short, 'short>(partial: Partial<'long>) -> Partial<'short> {
    partial
}

impl Drop for Partial<'_> {
    // This drop function is only really called when a partial is dropped without being fully
    // built out. Otherwise, it's forgotten because the value has been moved elsewhere.
    //
    // As a result, its only job is to drop any fields that may have been initialized. And finally
    // to free the memory for the partial itself if we own it.
    fn drop(&mut self) {
        match self.shape.get().innards {
            crate::Innards::Struct { fields } => {
                fields
                    .iter()
                    .enumerate()
                    .filter_map(|(i, field)| {
                        if self.init_set.is_set(i) {
                            Some((field, field.shape.get().drop_in_place?))
                        } else {
                            None
                        }
                    })
                    .for_each(|(field, drop_fn)| {
                        unsafe {
                            // SAFETY: field_addr is valid, aligned, and initialized.
                            //
                            // If the struct is a ZST, then `self.addr` is dangling.
                            // That also means that all the fields are ZSTs, which means
                            // the actual address we pass to the drop fn does not matter,
                            // but we do want the side effects.
                            //
                            // If the struct is not a ZST, then `self.addr` is a valid address.
                            // The fields can still be ZST and that's not a special case, really.
                            drop_fn(self.addr.byte_add(field.offset).as_ptr());
                        }
                    })
            }
            crate::Innards::Scalar(_) => {
                if self.init_set.is_set(0) {
                    // Drop the scalar value if it has a drop function
                    if let Some(drop_fn) = self.shape.get().drop_in_place {
                        // SAFETY: self.addr is always valid for Scalar types,
                        // even for ZSTs where it might be dangling.
                        unsafe {
                            drop_fn(self.addr.as_ptr());
                        }
                    }
                }
            }
            _ => {}
        }

        self.deallocate()
    }
}

impl Partial<'_> {
    /// Allocates a partial on the heap for the given shape descriptor.
    pub fn alloc(shape: ShapeDesc) -> Self {
        let sh = shape.get();
        let layout = sh.layout;
        let addr = if layout.size() == 0 {
            // ZSTs need a well-aligned address
            sh.dangling()
        } else {
            let addr = unsafe { alloc::alloc(layout) };
            if addr.is_null() {
                alloc::handle_alloc_error(layout);
            }
            // SAFETY: We just allocated this memory and checked that it's not null,
            // so it's safe to create a NonNull from it.
            unsafe { NonNull::new_unchecked(addr) }
        };

        Self {
            origin: Origin::HeapAllocated,
            addr,
            init_set: Default::default(),
            shape,
        }
    }

    /// Borrows a `MaybeUninit<Self>` and returns a `Partial`.
    ///
    /// Before calling assume_init, make sure to call Partial.build_in_place().
    pub fn borrow<T: Shapely>(uninit: &mut std::mem::MaybeUninit<T>) -> Self {
        Self {
            origin: Origin::Borrowed {
                parent: None,
                init_mark: InitMark::Ignored,
            },
            addr: NonNull::new(uninit.as_mut_ptr() as _).unwrap(),
            init_set: Default::default(),
            shape: T::shape_desc(),
        }
    }

    /// Checks if all fields in the struct or scalar value have been initialized.
    /// Panics if any field is not initialized, providing details about the uninitialized field.
    pub(crate) fn assert_all_fields_initialized(&self) {
        match self.shape.get().innards {
            crate::Innards::Struct { fields } => {
                for (i, field) in fields.iter().enumerate() {
                    if !self.init_set.is_set(i) {
                        panic!(
                            "Field '{}' was not initialized. Complete schema:\n{:?}",
                            field.name,
                            self.shape.get()
                        );
                    }
                }
            }
            crate::Innards::Scalar(_) => {
                if !self.init_set.is_set(0) {
                    panic!(
                        "Scalar value was not initialized. Complete schema:\n{:?}",
                        self.shape.get()
                    );
                }
            }
            _ => {}
        }
    }

    /// Returns a slot for assigning this whole shape as a scalar
    pub fn scalar_slot(&mut self) -> Option<Slot<'_>> {
        match self.shape.get().innards {
            crate::Innards::Scalar(_) => {
                let slot = Slot::for_ptr(
                    self.addr,
                    self.shape,
                    InitMark::Struct {
                        index: 0,
                        set: &mut self.init_set,
                    },
                );
                Some(slot)
            }
            crate::Innards::Transparent(inner_shape) => {
                let slot = Slot::for_ptr(
                    self.addr,
                    inner_shape,
                    InitMark::Struct {
                        index: 0,
                        set: &mut self.init_set,
                    },
                );
                Some(slot)
            }
            _ => panic!(
                "Expected scalar innards, found {:?}",
                self.shape.get().innards
            ),
        }
    }

    /// Returns a slot for initializing a field in the shape.
    pub fn slot_by_name<'s>(&'s mut self, name: &str) -> Result<Slot<'s>, FieldError> {
        let shape = self.shape.get();
        match shape.innards {
            Innards::Struct { fields } => {
                let (index, field) = fields
                    .iter()
                    .enumerate()
                    .find(|(_, f)| f.name == name)
                    .ok_or(FieldError::NoSuchStaticField)?;
                let field_addr = unsafe {
                    // SAFETY: self.addr is a valid pointer to the start of the struct,
                    // and field.offset is the correct offset for this field within the struct.
                    // The resulting pointer is properly aligned and within the bounds of the allocated memory.
                    self.addr.byte_add(field.offset)
                };
                Ok(Slot::for_ptr(
                    field_addr,
                    field.shape,
                    self.init_set.field(index),
                ))
            }
            Innards::HashMap { value_shape } => {
                Ok(Slot::for_hash_map(self.addr, name.to_string(), value_shape))
            }
            Innards::Transparent(_) => return Err(FieldError::NoStaticFields),
            Innards::Scalar(_) => return Err(FieldError::NoStaticFields),
            Innards::Array(_) => return Err(FieldError::NoStaticFields),
            Innards::Enum { .. } => {
                // Enum variants aren't supported yet for slot_by_name
                Err(FieldError::NotAStruct)
            }
        }
    }

    /// Returns a slot for initializing a field in the shape by index.
    pub fn slot_by_index(&mut self, index: usize) -> Result<Slot<'_>, FieldError> {
        let sh = self.shape.get();
        let field = sh.field_by_index(index)?;
        let field_addr = unsafe {
            // SAFETY: self.addr is a valid pointer to the start of the struct,
            // and field.offset is the correct offset for this field within the struct.
            // The resulting pointer is properly aligned and within the bounds of the allocated memory.
            self.addr.byte_add(field.offset)
        };
        let slot = Slot::for_ptr(field_addr, field.shape, self.init_set.field(index));
        Ok(slot)
    }

    fn assert_matching_shape<T: Shapely>(&self) {
        if self.shape != T::shape_desc() {
            let partial_shape = self.shape.get();
            let target_shape = T::shape();

            panic!(
                "This is a partial \x1b[1;34m{}\x1b[0m, you can't build a \x1b[1;32m{}\x1b[0m out of it",
                partial_shape, target_shape,
            );
        }
    }

    fn deallocate(&mut self) {
        // ZSTs don't need to be deallocated
        if self.shape.get().layout.size() != 0 {
            unsafe { alloc::dealloc(self.addr.as_ptr(), self.shape.get().layout) }
        }
    }

    /// Asserts that every field has been initialized and forgets the Partial.
    ///
    /// This method is only used when the origin is borrowed.
    /// If this method is not called, all fields will be freed when the Partial is dropped.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - The origin is not borrowed (i.e., it's heap allocated).
    /// - Any field is not initialized.
    pub fn build_in_place(mut self) {
        // ensure all fields are initialized
        self.assert_all_fields_initialized();

        match &mut self.origin {
            Origin::Borrowed { init_mark, .. } => {
                // Mark the borrowed field as initialized
                init_mark.set();
            }
            Origin::HeapAllocated => {
                panic!("Cannot build in place for heap allocated Partial");
            }
        }

        // prevent field drops when the Partial is dropped
        std::mem::forget(self);
    }

    /// Build that partial into the completed shape.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - Not all the fields have been initialized.
    /// - The generic type parameter T does not match the shape that this partial is building.
    pub fn build<T: Shapely>(mut self) -> T {
        self.assert_all_fields_initialized();
        self.assert_matching_shape::<T>();

        // SAFETY: We've verified that all fields are initialized and that the shape matches T.
        // For zero-sized types, all pointer values are valid.
        // See https://doc.rust-lang.org/stable/std/ptr/index.html#safety for more details.
        let result = unsafe {
            let ptr = self.addr.as_ptr() as *const T;
            std::ptr::read(ptr)
        };
        trace!("Built \x1b[1;33m{}\x1b[0m successfully", T::shape());
        self.deallocate();
        std::mem::forget(self);
        result
    }
    /// Build that partial into a boxed completed shape.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - Not all the fields have been initialized.
    /// - The generic type parameter T does not match the shape that this partial is building.
    ///
    /// # Safety
    ///
    /// This function uses unsafe code to create a Box from a raw pointer.
    /// It's safe because we've verified the initialization and shape matching,
    /// and we forget `self` to prevent double-freeing.
    pub fn build_boxed<T: Shapely>(self) -> Box<T> {
        self.assert_all_fields_initialized();
        self.assert_matching_shape::<T>();

        let boxed = unsafe { Box::from_raw(self.addr.as_ptr() as *mut T) };
        std::mem::forget(self);
        boxed
    }

    /// Moves the contents of this `Partial` into a target memory location.
    ///
    /// This function is useful when you need to place the fully initialized value
    /// into a specific memory address, such as when working with FFI or custom allocators.
    ///
    /// # Safety
    ///
    /// The target pointer must be valid and properly aligned,
    /// and must be large enough to hold the value.
    /// The caller is responsible for ensuring that the target memory is properly deallocated
    /// when it's no longer needed.
    pub unsafe fn move_into(mut self, target: NonNull<u8>) {
        self.assert_all_fields_initialized();
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.addr.as_ptr(),
                target.as_ptr(),
                // note: copy_nonoverlapping takes a count,
                // since we're dealing with `*mut u8`, it's a byte count.
                // if we were dealing with `*mut ()`, we'd have a nasty surprise.
                self.shape.get().layout.size(),
            );
        }
        self.deallocate();
        std::mem::forget(self);
    }

    /// Returns the shape we're currently building.
    pub fn shape(&self) -> ShapeDesc {
        self.shape
    }

    /// Returns the address of the value we're building in memory.
    pub fn addr(&self) -> NonNull<u8> {
        self.addr
    }

    // TODO: Add support for enum variant instantiation:
    // - Implement a variant_slot method on Partial to select a variant
    // - Add support for filling variant fields based on the variant kind
    // - Update the build method to properly construct enum instances
}

/// A bit array to keep track of which fields were initialized, up to 64 fields
#[derive(Clone, Copy, Default)]
pub struct InitSet64(u64);

impl InitSet64 {
    /// Sets the bit at the given index.
    pub fn set(&mut self, index: usize) {
        if index >= 64 {
            panic!("InitSet64 can only track up to 64 fields. Index {index} is out of bounds.");
        }
        self.0 |= 1 << index;
    }

    /// Unsets the bit at the given index.
    pub fn unset(&mut self, index: usize) {
        if index >= 64 {
            panic!("InitSet64 can only track up to 64 fields. Index {index} is out of bounds.");
        }
        self.0 &= !(1 << index);
    }

    /// Checks if the bit at the given index is set.
    pub fn is_set(&self, index: usize) -> bool {
        if index >= 64 {
            panic!("InitSet64 can only track up to 64 fields. Index {index} is out of bounds.");
        }
        (self.0 & (1 << index)) != 0
    }

    /// Checks if all bits up to the given count are set.
    pub fn all_set(&self, count: usize) -> bool {
        if count > 64 {
            panic!("InitSet64 can only track up to 64 fields. Count {count} is out of bounds.");
        }
        let mask = (1 << count) - 1;
        self.0 & mask == mask
    }

    /// Gets an [InitMark] to track the initialization state of a single field
    pub fn field(&mut self, index: usize) -> InitMark {
        InitMark::Struct { index, set: self }
    }
}

/// `InitMark` is used to track the initialization state of a single field within an `InitSet64`.
/// It is part of a system used to progressively initialize structs, where each field's
/// initialization status is represented by a bit in a 64-bit set.
pub enum InitMark<'s> {
    /// Represents a field in a struct that needs to be tracked for initialization.
    Struct {
        /// The index of the field in the struct (0-63).
        index: usize,
        /// A reference to the `InitSet64` that tracks all fields' initialization states.
        set: &'s mut InitSet64,
    },
    /// Represents a field or value that doesn't need initialization tracking.
    Ignored,
}

impl InitMark<'_> {
    /// Marks the field as initialized by setting its corresponding bit in the `InitSet64`.
    pub fn set(&mut self) {
        if let Self::Struct { index, set } = self {
            set.set(*index);
        }
    }

    /// Marks the field as uninitialized by clearing its corresponding bit in the `InitSet64`.
    pub fn unset(&mut self) {
        if let Self::Struct { index, set } = self {
            set.0 &= !(1 << *index);
        }
    }

    /// Checks if the field is marked as initialized.
    ///
    /// Returns `true` if the field is initialized, `false` otherwise.
    /// Always returns `true` for `Ignored` fields.
    pub fn get(&self) -> bool {
        match self {
            Self::Struct { index, set } => set.is_set(*index),
            Self::Ignored => true,
        }
    }
}
