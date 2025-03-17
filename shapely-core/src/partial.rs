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
            crate::Innards::Enum {
                variants: _,
                repr: _,
            } => {
                // Check if a variant has been selected (bit 0)
                if !self.init_set.is_set(0) {
                    panic!(
                        "No enum variant was selected. Complete schema:\n{:?}",
                        self.shape.get()
                    );
                }

                // Get the selected variant
                if let Some(variant_index) = self.selected_variant_index() {
                    let shape = self.shape.get();
                    if let crate::Innards::Enum { variants, repr: _ } = &shape.innards {
                        let variant = &variants[variant_index];

                        // Check if all fields of the selected variant are initialized
                        match &variant.kind {
                            crate::VariantKind::Unit => {
                                // Unit variants don't have fields, so they're initialized if the variant is selected
                            }
                            crate::VariantKind::Tuple { fields }
                            | crate::VariantKind::Struct { fields } => {
                                // Check each field
                                for (field_index, field) in fields.iter().enumerate() {
                                    // Field init bits start at index 1 (index 0 is for variant selection)
                                    let init_bit = field_index + 1;
                                    if !self.init_set.is_set(init_bit) {
                                        panic!(
                                            "Field '{}' of variant '{}' was not initialized. Complete schema:\n{:?}",
                                            field.name,
                                            variant.name,
                                            self.shape.get()
                                        );
                                    }
                                }
                            }
                        }
                    }
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
            Innards::Transparent(_) => Err(FieldError::NoStaticFields),
            Innards::Scalar(_) => Err(FieldError::NoStaticFields),
            Innards::Array(_) => Err(FieldError::NoStaticFields),
            Innards::Enum {
                variants: _,
                repr: _,
            } => {
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

    /// Builds a value of type `T` from the partial representation.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - Not all the fields have been initialized.
    /// - The generic type parameter T does not match the shape that this partial is building.
    pub fn build<T: Shapely>(mut self) -> T {
        self.assert_all_fields_initialized();
        self.assert_matching_shape::<T>();

        let shape = self.shape.get();

        // Special handling for enums to ensure the correct variant is built
        if let crate::Innards::Enum { variants, repr } = &shape.innards {
            if !self.init_set.is_set(0) {
                panic!("Enum variant not selected");
            }

            // Check if explicit enum representation is used
            if let crate::EnumRepr::Default = repr {
                panic!(
                    "Enum must have an explicit representation (e.g. #[repr(u8)]). Default representation is not supported."
                );
            }

            if let Some(variant_idx) = self.selected_variant_index() {
                // Create a properly initialized result with the correct variant
                let mut result_mem = std::mem::MaybeUninit::<T>::uninit();

                unsafe {
                    // Zero out memory first for safety
                    std::ptr::write_bytes(
                        result_mem.as_mut_ptr() as *mut u8,
                        0,
                        std::mem::size_of::<T>(),
                    );

                    // Get the variant info
                    let variant = &variants[variant_idx];

                    // Set discriminant value - this is the key part for fixing the enum issue
                    let discriminant_value = match &variant.discriminant {
                        Some(disc) => *disc,
                        None => variant_idx as i64,
                    };

                    // Write the discriminant value based on the representation
                    match repr {
                        crate::EnumRepr::U8 => {
                            let tag_ptr = result_mem.as_mut_ptr() as *mut u8;
                            *tag_ptr = discriminant_value as u8;
                        }
                        crate::EnumRepr::U16 => {
                            let tag_ptr = result_mem.as_mut_ptr() as *mut u16;
                            *tag_ptr = discriminant_value as u16;
                        }
                        crate::EnumRepr::U32 => {
                            let tag_ptr = result_mem.as_mut_ptr() as *mut u32;
                            *tag_ptr = discriminant_value as u32;
                        }
                        crate::EnumRepr::U64 => {
                            let tag_ptr = result_mem.as_mut_ptr() as *mut u64;
                            *tag_ptr = discriminant_value as u64;
                        }
                        crate::EnumRepr::USize => {
                            let tag_ptr = result_mem.as_mut_ptr() as *mut usize;
                            *tag_ptr = discriminant_value as usize;
                        }
                        crate::EnumRepr::I8 => {
                            let tag_ptr = result_mem.as_mut_ptr() as *mut i8;
                            *tag_ptr = discriminant_value as i8;
                        }
                        crate::EnumRepr::I16 => {
                            let tag_ptr = result_mem.as_mut_ptr() as *mut i16;
                            *tag_ptr = discriminant_value as i16;
                        }
                        crate::EnumRepr::I32 => {
                            let tag_ptr = result_mem.as_mut_ptr() as *mut i32;
                            *tag_ptr = discriminant_value as i32;
                        }
                        crate::EnumRepr::I64 => {
                            let tag_ptr = result_mem.as_mut_ptr() as *mut i64;
                            *tag_ptr = discriminant_value;
                        }
                        crate::EnumRepr::ISize => {
                            let tag_ptr = result_mem.as_mut_ptr() as *mut isize;
                            *tag_ptr = discriminant_value as isize;
                        }
                        crate::EnumRepr::Default => {
                            // Use a heuristic based on the number of variants
                            if variants.len() <= 256 {
                                // Can fit in a u8
                                let tag_ptr = result_mem.as_mut_ptr() as *mut u8;
                                *tag_ptr = discriminant_value as u8;
                            } else if variants.len() <= 65536 {
                                // Can fit in a u16
                                let tag_ptr = result_mem.as_mut_ptr() as *mut u16;
                                *tag_ptr = discriminant_value as u16;
                            } else {
                                // Default to u32
                                let tag_ptr = result_mem.as_mut_ptr() as *mut u32;
                                *tag_ptr = discriminant_value as u32;
                            }
                        }
                    }

                    // For non-unit variants, copy the initialized fields
                    match &variant.kind {
                        crate::VariantKind::Tuple { fields } => {
                            // Copy the fields from our partial to the result
                            for field in fields.iter() {
                                let src_ptr = (self.addr.as_ptr() as *const u8).add(field.offset);
                                let dst_ptr =
                                    (result_mem.as_mut_ptr() as *mut u8).add(field.offset);
                                // Access the layout from the shape field
                                let size = field.shape.get().layout.size();
                                std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, size);
                            }
                        }
                        crate::VariantKind::Struct { fields } => {
                            // Copy the fields from our partial to the result
                            for field in fields.iter() {
                                let src_ptr = (self.addr.as_ptr() as *const u8).add(field.offset);
                                let dst_ptr =
                                    (result_mem.as_mut_ptr() as *mut u8).add(field.offset);
                                // Access the layout from the shape field
                                let size = field.shape.get().layout.size();
                                std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, size);
                            }
                        }
                        crate::VariantKind::Unit => {
                            // Nothing to copy for unit variants, just the discriminant is enough
                        }
                    }

                    // Return the completed enum
                    let result = result_mem.assume_init();
                    trace!("Built \x1b[1;33m{}\x1b[0m successfully", T::shape());
                    self.deallocate();
                    std::mem::forget(self);
                    return result;
                }
            }
        }

        // For non-enum types, use the original implementation
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

    /// Sets the variant of an enum by name.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The shape doesn't represent an enum.
    /// - No variant with the given name exists.
    pub fn set_variant_by_name(&mut self, variant_name: &str) -> Result<(), crate::FieldError> {
        let shape = self.shape.get();

        if let crate::Innards::Enum { variants, repr: _ } = &shape.innards {
            let variant_index = variants
                .iter()
                .enumerate()
                .find(|(_, v)| v.name == variant_name)
                .map(|(i, _)| i)
                .ok_or(crate::FieldError::NoSuchStaticField)?;

            self.set_variant_by_index(variant_index)
        } else {
            Err(crate::FieldError::NotAStruct) // Using NotAStruct as a stand-in for "not an enum"
        }
    }

    /// Sets the variant of an enum by index.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The shape doesn't represent an enum.
    /// - The index is out of bounds.
    pub fn set_variant_by_index(&mut self, variant_index: usize) -> Result<(), crate::FieldError> {
        let shape = self.shape.get();

        if let crate::Innards::Enum { variants, repr } = &shape.innards {
            if variant_index >= variants.len() {
                return Err(crate::FieldError::IndexOutOfBounds);
            }

            // Get the current variant info
            let variant = &variants[variant_index];

            // Prepare memory for the enum
            unsafe {
                // Zero out the memory first to ensure clean state
                std::ptr::write_bytes(self.addr.as_ptr(), 0, shape.layout.size());

                // Set up the discriminant (tag)
                // For enums in Rust, the first bytes contain the discriminant
                // By default, we should use the smallest type that can represent all variants
                let discriminant_value = match &variant.discriminant {
                    // If we have an explicit discriminant, use it
                    Some(discriminant) => *discriminant,
                    // Otherwise, use the variant index directly
                    None => variant_index as i64,
                };

                // Write the discriminant value based on the representation
                match repr {
                    crate::EnumRepr::U8 => {
                        let tag_ptr = self.addr.as_ptr();
                        *tag_ptr = discriminant_value as u8;
                    }
                    crate::EnumRepr::U16 => {
                        let tag_ptr = self.addr.as_ptr() as *mut u16;
                        *tag_ptr = discriminant_value as u16;
                    }
                    crate::EnumRepr::U32 => {
                        let tag_ptr = self.addr.as_ptr() as *mut u32;
                        *tag_ptr = discriminant_value as u32;
                    }
                    crate::EnumRepr::U64 => {
                        let tag_ptr = self.addr.as_ptr() as *mut u64;
                        *tag_ptr = discriminant_value as u64;
                    }
                    crate::EnumRepr::USize => {
                        let tag_ptr = self.addr.as_ptr() as *mut usize;
                        *tag_ptr = discriminant_value as usize;
                    }
                    crate::EnumRepr::I8 => {
                        let tag_ptr = self.addr.as_ptr() as *mut i8;
                        *tag_ptr = discriminant_value as i8;
                    }
                    crate::EnumRepr::I16 => {
                        let tag_ptr = self.addr.as_ptr() as *mut i16;
                        *tag_ptr = discriminant_value as i16;
                    }
                    crate::EnumRepr::I32 => {
                        let tag_ptr = self.addr.as_ptr() as *mut i32;
                        *tag_ptr = discriminant_value as i32;
                    }
                    crate::EnumRepr::I64 => {
                        let tag_ptr = self.addr.as_ptr() as *mut i64;
                        *tag_ptr = discriminant_value;
                    }
                    crate::EnumRepr::ISize => {
                        let tag_ptr = self.addr.as_ptr() as *mut isize;
                        *tag_ptr = discriminant_value as isize;
                    }
                    crate::EnumRepr::Default => {
                        // Use a heuristic based on the number of variants
                        if variants.len() <= 256 {
                            // Can fit in a u8
                            let tag_ptr = self.addr.as_ptr();
                            *tag_ptr = discriminant_value as u8;
                        } else if variants.len() <= 65536 {
                            // Can fit in a u16
                            let tag_ptr = self.addr.as_ptr() as *mut u16;
                            *tag_ptr = discriminant_value as u16;
                        } else {
                            // Default to u32
                            let tag_ptr = self.addr.as_ptr() as *mut u32;
                            *tag_ptr = discriminant_value as u32;
                        }
                    }
                }
            }

            // Mark the variant as selected (bit 0)
            self.init_set.set(0);

            // Reset all field initialization bits (starting from bit 1)
            // InitSet64 can hold 64 bits, so we'll clear bits 1-63
            for i in 1..64 {
                self.init_set.unset(i);
            }

            Ok(())
        } else {
            Err(crate::FieldError::NotAStruct) // Using NotAStruct as a stand-in for "not an enum"
        }
    }

    /// Returns the currently selected variant index, if any.
    pub fn selected_variant_index(&self) -> Option<usize> {
        if !self.init_set.is_set(0) {
            return None;
        }

        let shape = self.shape.get();

        // We need to read the discriminant and map it back to the variant index
        if let crate::Innards::Enum { variants, repr } = &shape.innards {
            unsafe {
                // Attempt to read the tag based on the representation
                let discriminant_value = match repr {
                    crate::EnumRepr::U8 => {
                        let tag_ptr = self.addr.as_ptr() as *const u8;
                        *tag_ptr as i64
                    }
                    crate::EnumRepr::U16 => {
                        let tag_ptr = self.addr.as_ptr() as *const u16;
                        *tag_ptr as i64
                    }
                    crate::EnumRepr::U32 => {
                        let tag_ptr = self.addr.as_ptr() as *const u32;
                        *tag_ptr as i64
                    }
                    crate::EnumRepr::U64 => {
                        let tag_ptr = self.addr.as_ptr() as *const u64;
                        *tag_ptr as i64
                    }
                    crate::EnumRepr::USize => {
                        let tag_ptr = self.addr.as_ptr() as *const usize;
                        *tag_ptr as i64
                    }
                    crate::EnumRepr::I8 => {
                        let tag_ptr = self.addr.as_ptr() as *const i8;
                        *tag_ptr as i64
                    }
                    crate::EnumRepr::I16 => {
                        let tag_ptr = self.addr.as_ptr() as *const i16;
                        *tag_ptr as i64
                    }
                    crate::EnumRepr::I32 => {
                        let tag_ptr = self.addr.as_ptr() as *const i32;
                        *tag_ptr as i64
                    }
                    crate::EnumRepr::I64 => {
                        let tag_ptr = self.addr.as_ptr() as *const i64;
                        *tag_ptr
                    }
                    crate::EnumRepr::ISize => {
                        let tag_ptr = self.addr.as_ptr() as *const isize;
                        *tag_ptr as i64
                    }
                    crate::EnumRepr::Default => {
                        // Use a heuristic based on the number of variants
                        if variants.len() <= 256 {
                            // Likely a u8 discriminant
                            let tag_ptr = self.addr.as_ptr() as *const u8;
                            *tag_ptr as i64
                        } else if variants.len() <= 65536 {
                            // Likely a u16 discriminant
                            let tag_ptr = self.addr.as_ptr() as *const u16;
                            *tag_ptr as i64
                        } else {
                            // Default to u32
                            let tag_ptr = self.addr.as_ptr() as *const u32;
                            *tag_ptr as i64
                        }
                    }
                };

                // Find the variant with this discriminant or index
                // Try matching by discriminant first
                for (idx, variant) in variants.iter().enumerate() {
                    if let Some(disc) = variant.discriminant {
                        if disc == discriminant_value {
                            return Some(idx);
                        }
                    } else if idx as i64 == discriminant_value {
                        // Fallback to index-based match
                        return Some(idx);
                    }
                }

                // If we couldn't find a match, but we know a variant is selected,
                // assume it's the variant at the discriminant index if in bounds
                if (discriminant_value as usize) < variants.len() {
                    return Some(discriminant_value as usize);
                }
            }
        }

        None
    }

    /// Get a slot for a field in the currently selected variant.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The shape doesn't represent an enum.
    /// - No variant has been selected yet.
    /// - The field name doesn't exist in the selected variant.
    /// - The selected variant is a unit variant (which has no fields).
    pub fn variant_field_by_name<'s>(
        &'s mut self,
        name: &str,
    ) -> Result<Slot<'s>, crate::FieldError> {
        let variant_index = self
            .selected_variant_index()
            .ok_or(crate::FieldError::NotAStruct)?; // Using NotAStruct as a stand-in for "no variant selected"

        let shape = self.shape.get();
        if let crate::Innards::Enum { variants, repr: _ } = &shape.innards {
            let variant = &variants[variant_index];

            // Find the field in the variant
            match &variant.kind {
                crate::VariantKind::Unit => {
                    // Unit variants have no fields
                    Err(crate::FieldError::NoSuchStaticField)
                }
                crate::VariantKind::Tuple { fields } => {
                    // For tuple variants, find the field by name
                    let (field_index, field) = fields
                        .iter()
                        .enumerate()
                        .find(|(_, f)| f.name == name)
                        .ok_or(crate::FieldError::NoSuchStaticField)?;

                    // The field's initialization bit is offset by 1 (since bit 0 is used for variant selection)
                    let init_bit = field_index + 1;

                    // Get the field's address
                    let field_addr = unsafe {
                        // The actual offset may depend on the variant's layout, but we use the field index for now
                        // This is technically incorrect, as it assumes a simple layout where offsets are contiguous
                        self.addr.byte_add(field.offset)
                    };

                    Ok(Slot::for_ptr(
                        field_addr,
                        field.shape,
                        self.init_set.field(init_bit),
                    ))
                }
                crate::VariantKind::Struct { fields } => {
                    // For struct variants, find the field by name
                    let (field_index, field) = fields
                        .iter()
                        .enumerate()
                        .find(|(_, f)| f.name == name)
                        .ok_or(crate::FieldError::NoSuchStaticField)?;

                    // The field's initialization bit is offset by 1 (since bit 0 is used for variant selection)
                    let init_bit = field_index + 1;

                    // Get the field's address
                    let field_addr = unsafe {
                        // The actual offset may depend on the variant's layout, but we use the field index for now
                        // This is technically incorrect, as it assumes a simple layout where offsets are contiguous
                        self.addr.byte_add(field.offset)
                    };

                    Ok(Slot::for_ptr(
                        field_addr,
                        field.shape,
                        self.init_set.field(init_bit),
                    ))
                }
            }
        } else {
            Err(crate::FieldError::NotAStruct)
        }
    }
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
