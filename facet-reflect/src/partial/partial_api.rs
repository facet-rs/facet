// This module contains the public-facing API for `Partial`

use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use core::{marker::PhantomData, mem::ManuallyDrop, ptr::NonNull};

use crate::{
    Guard, HeapValue, Partial, Peek, ReflectError, TypedPartial,
    partial::{Frame, FrameOwnership, MapInsertState, PartialState, Tracker, iset::ISet},
    trace,
};
use facet_core::{
    ArrayType, Characteristic, Def, EnumRepr, EnumType, Facet, Field, FieldAttribute, KnownPointer,
    PtrConst, PtrMut, PtrUninit, SequenceType, Shape, StructType, Type, UserType, Variant,
};

////////////////////////////////////////////////////////////////////////////////////////////////////
// Allocation, constructors etc.
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Allocates a new [TypedPartial] instance on the heap, with the given shape and type
    pub fn alloc<T>() -> Result<TypedPartial<'facet, T>, ReflectError>
    where
        T: Facet<'facet> + ?Sized,
    {
        Ok(TypedPartial {
            inner: Self::alloc_shape(T::SHAPE)?,
            phantom: PhantomData,
        })
    }

    /// Allocates a new [Partial] instance on the heap, with the given shape.
    pub fn alloc_shape(shape: &'static Shape) -> Result<Self, ReflectError> {
        crate::trace!(
            "alloc_shape({:?}), with layout {:?}",
            shape,
            shape.layout.sized_layout()
        );

        let data = shape.allocate().map_err(|_| ReflectError::Unsized {
            shape,
            operation: "alloc_shape",
        })?;

        // Preallocate a couple of frames. The cost of allocating 4 frames is
        // basically identical to allocating 1 frame, so for every type that
        // has at least 1 level of nesting, this saves at least one guaranteed reallocation.
        let mut frames = Vec::with_capacity(4);
        frames.push(Frame::new(data, shape, FrameOwnership::Owned));

        Ok(Self {
            frames,
            state: PartialState::Active,
            invariant: PhantomData,
        })
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Misc.
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Returns the current frame count (depth of nesting)
    ///
    /// The initial frame count is 1 — `begin_field` would push a new frame,
    /// bringing it to 2, then `end` would bring it back to `1`.
    ///
    /// This is an implementation detail of `Partial`, kinda, but deserializers
    /// might use this for debug assertions, to make sure the state is what
    /// they think it is.
    #[inline]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Returns the shape of the current frame.
    #[inline]
    pub fn shape(&self) -> &'static Shape {
        self.frames
            .last()
            .expect("Partial always has at least one frame")
            .shape
    }

    /// Pops the current frame off the stack, indicating we're done initializing the current field
    pub fn end(&mut self) -> Result<&mut Self, ReflectError> {
        crate::trace!("end() called");
        self.require_active()?;

        // Special handling for SmartPointerSlice - convert builder to Arc
        if self.frames.len() == 1 {
            if let Tracker::SmartPointerSlice {
                vtable,
                building_item,
            } = &self.frames[0].tracker
            {
                if *building_item {
                    return Err(ReflectError::OperationFailed {
                        shape: self.frames[0].shape,
                        operation: "still building an item, finish it first",
                    });
                }

                // Convert the builder to Arc<[T]>
                let builder_ptr = unsafe { self.frames[0].data.assume_init() };
                let arc_ptr = unsafe { (vtable.convert_fn)(builder_ptr) };

                // Update the frame to store the Arc
                self.frames[0].data = PtrUninit::new(unsafe {
                    NonNull::new_unchecked(arc_ptr.as_byte_ptr() as *mut u8)
                });
                self.frames[0].tracker = Tracker::Init;
                // The builder memory has been consumed by convert_fn, so we no longer own it
                self.frames[0].ownership = FrameOwnership::ManagedElsewhere;

                return Ok(self);
            }
        }

        if self.frames.len() <= 1 {
            // Never pop the last/root frame.
            return Err(ReflectError::InvariantViolation {
                invariant: "Partial::end() called with only one frame on the stack",
            });
        }

        // Require that the top frame is fully initialized before popping.
        {
            let frame = self.frames.last().unwrap();
            trace!(
                "end(): Checking full initialization for frame with shape {} and tracker {:?}",
                frame.shape,
                frame.tracker.kind()
            );
            frame.require_full_initialization()?
        }

        // Pop the frame and save its data pointer for SmartPointer handling
        let mut popped_frame = self.frames.pop().unwrap();

        // check if this needs deserialization from a different shape
        if popped_frame.using_custom_deserialization {
            if let Some(deserialize_with) = self
                .parent_field()
                .and_then(|field| field.vtable.deserialize_with)
            {
                let parent_frame = self.frames.last_mut().unwrap();

                trace!(
                    "Detected custom conversion needed from {} to {}",
                    popped_frame.shape, parent_frame.shape
                );

                unsafe {
                    let res = {
                        let inner_value_ptr = popped_frame.data.assume_init().as_const();
                        (deserialize_with)(inner_value_ptr, parent_frame.data)
                    };
                    let popped_frame_shape = popped_frame.shape;

                    // we need to do this before any error handling to avoid leaks
                    popped_frame.deinit();
                    popped_frame.dealloc();
                    let rptr = res.map_err(|message| ReflectError::CustomDeserializationError {
                        message,
                        src_shape: popped_frame_shape,
                        dst_shape: parent_frame.shape,
                    })?;
                    if rptr.as_uninit() != parent_frame.data {
                        return Err(ReflectError::CustomDeserializationError {
                            message: "deserialize_with did not return the expected pointer".into(),
                            src_shape: popped_frame_shape,
                            dst_shape: parent_frame.shape,
                        });
                    }
                    parent_frame.mark_as_init();
                }
                return Ok(self);
            }
        }

        // Update parent frame's tracking when popping from a child
        let parent_frame = self.frames.last_mut().unwrap();

        trace!(
            "end(): Popped {} (tracker {:?}), Parent {} (tracker {:?})",
            popped_frame.shape,
            popped_frame.tracker.kind(),
            parent_frame.shape,
            parent_frame.tracker.kind()
        );

        // Check if we need to do a conversion - this happens when:
        // 1. The parent frame has an inner type that matches the popped frame's shape
        // 2. The parent frame has try_from
        // 3. The parent frame is not yet initialized
        let needs_conversion = matches!(parent_frame.tracker, Tracker::Uninit)
            && parent_frame.shape.inner.is_some()
            && parent_frame.shape.inner.unwrap() == popped_frame.shape
            && parent_frame.shape.vtable.try_from.is_some();

        if needs_conversion {
            trace!(
                "Detected implicit conversion needed from {} to {}",
                popped_frame.shape, parent_frame.shape
            );
            // Perform the conversion
            if let Some(try_from_fn) = parent_frame.shape.vtable.try_from {
                let inner_ptr = unsafe { popped_frame.data.assume_init().as_const() };
                let inner_shape = popped_frame.shape;

                trace!("Converting from {} to {}", inner_shape, parent_frame.shape);
                let result = unsafe { try_from_fn(inner_ptr, inner_shape, parent_frame.data) };

                if let Err(e) = result {
                    trace!("Conversion failed: {e:?}");

                    // Deallocate the inner value's memory since conversion failed
                    if let FrameOwnership::Owned = popped_frame.ownership {
                        if let Ok(layout) = popped_frame.shape.layout.sized_layout() {
                            if layout.size() > 0 {
                                trace!(
                                    "Deallocating conversion frame memory after failure: size={}, align={}",
                                    layout.size(),
                                    layout.align()
                                );
                                unsafe {
                                    alloc::alloc::dealloc(
                                        popped_frame.data.as_mut_byte_ptr(),
                                        layout,
                                    );
                                }
                            }
                        }
                    }

                    return Err(ReflectError::TryFromError {
                        src_shape: inner_shape,
                        dst_shape: parent_frame.shape,
                        inner: e,
                    });
                }

                trace!("Conversion succeeded, marking parent as initialized");
                parent_frame.tracker = Tracker::Init;

                // Deallocate the inner value's memory since try_from consumed it
                if let FrameOwnership::Owned = popped_frame.ownership {
                    if let Ok(layout) = popped_frame.shape.layout.sized_layout() {
                        if layout.size() > 0 {
                            trace!(
                                "Deallocating conversion frame memory: size={}, align={}",
                                layout.size(),
                                layout.align()
                            );
                            unsafe {
                                alloc::alloc::dealloc(popped_frame.data.as_mut_byte_ptr(), layout);
                            }
                        }
                    }
                }

                return Ok(self);
            }
        }

        match &mut parent_frame.tracker {
            Tracker::Struct {
                iset,
                current_child,
            } => {
                if let Some(idx) = *current_child {
                    iset.set(idx);
                    *current_child = None;
                }
            }
            Tracker::Array {
                iset,
                current_child,
            } => {
                if let Some(idx) = *current_child {
                    iset.set(idx);
                    *current_child = None;
                }
            }
            Tracker::SmartPointer { is_initialized } => {
                // We just popped the inner value frame, so now we need to create the smart pointer
                if let Def::Pointer(smart_ptr_def) = parent_frame.shape.def {
                    let Some(new_into_fn) = smart_ptr_def.vtable.new_into_fn else {
                        return Err(ReflectError::OperationFailed {
                            shape: parent_frame.shape,
                            operation: "SmartPointer missing new_into_fn",
                        });
                    };

                    // The child frame contained the inner value
                    let inner_ptr = PtrMut::new(unsafe {
                        NonNull::new_unchecked(popped_frame.data.as_mut_byte_ptr())
                    });

                    // Use new_into_fn to create the Box
                    unsafe {
                        new_into_fn(parent_frame.data, inner_ptr);
                    }

                    // We just moved out of it
                    popped_frame.tracker = Tracker::Uninit;

                    // Deallocate the inner value's memory since new_into_fn moved it
                    popped_frame.dealloc();

                    *is_initialized = true;
                }
            }
            Tracker::Enum {
                data,
                current_child,
                ..
            } => {
                if let Some(idx) = *current_child {
                    data.set(idx);
                    *current_child = None;
                }
            }
            Tracker::List {
                is_initialized: true,
                current_child,
            } => {
                if *current_child {
                    // We just popped an element frame, now push it to the list
                    if let Def::List(list_def) = parent_frame.shape.def {
                        let Some(push_fn) = list_def.vtable.push else {
                            return Err(ReflectError::OperationFailed {
                                shape: parent_frame.shape,
                                operation: "List missing push function",
                            });
                        };

                        // The child frame contained the element value
                        let element_ptr = PtrMut::new(unsafe {
                            NonNull::new_unchecked(popped_frame.data.as_mut_byte_ptr())
                        });

                        // Use push to add element to the list
                        unsafe {
                            push_fn(
                                PtrMut::new(NonNull::new_unchecked(
                                    parent_frame.data.as_mut_byte_ptr(),
                                )),
                                element_ptr,
                            );
                        }

                        // Push moved out of popped_frame
                        popped_frame.tracker = Tracker::Uninit;
                        popped_frame.dealloc();

                        *current_child = false;
                    }
                }
            }
            Tracker::Map {
                is_initialized: true,
                insert_state,
            } => {
                match insert_state {
                    MapInsertState::PushingKey { key_ptr } => {
                        // We just popped the key frame
                        if let Some(key_ptr) = key_ptr {
                            // Transition to PushingValue state
                            *insert_state = MapInsertState::PushingValue {
                                key_ptr: *key_ptr,
                                value_ptr: None,
                            };
                        }
                    }
                    MapInsertState::PushingValue { key_ptr, value_ptr } => {
                        // We just popped the value frame, now insert the pair
                        if let (Some(value_ptr), Def::Map(map_def)) =
                            (value_ptr, parent_frame.shape.def)
                        {
                            let insert_fn = map_def.vtable.insert_fn;

                            // Use insert to add key-value pair to the map
                            unsafe {
                                insert_fn(
                                    PtrMut::new(NonNull::new_unchecked(
                                        parent_frame.data.as_mut_byte_ptr(),
                                    )),
                                    PtrMut::new(NonNull::new_unchecked(key_ptr.as_mut_byte_ptr())),
                                    PtrMut::new(NonNull::new_unchecked(
                                        value_ptr.as_mut_byte_ptr(),
                                    )),
                                );
                            }

                            // Note: We don't deallocate the key and value memory here.
                            // The insert function has semantically moved the values into the map,
                            // but we still need to deallocate the temporary buffers.
                            // However, since we don't have frames for them anymore (they were popped),
                            // we need to handle deallocation here.
                            if let Ok(key_shape) = map_def.k().layout.sized_layout() {
                                if key_shape.size() > 0 {
                                    unsafe {
                                        alloc::alloc::dealloc(key_ptr.as_mut_byte_ptr(), key_shape);
                                    }
                                }
                            }
                            if let Ok(value_shape) = map_def.v().layout.sized_layout() {
                                if value_shape.size() > 0 {
                                    unsafe {
                                        alloc::alloc::dealloc(
                                            value_ptr.as_mut_byte_ptr(),
                                            value_shape,
                                        );
                                    }
                                }
                            }

                            // Reset to idle state
                            *insert_state = MapInsertState::Idle;
                        }
                    }
                    MapInsertState::Idle => {
                        // Nothing to do
                    }
                }
            }
            Tracker::Option { building_inner } => {
                // We just popped the inner value frame for an Option's Some variant
                if *building_inner {
                    if let Def::Option(option_def) = parent_frame.shape.def {
                        // Use the Option vtable to initialize Some(inner_value)
                        let init_some_fn = option_def.vtable.init_some_fn;

                        // The popped frame contains the inner value
                        let inner_value_ptr = unsafe { popped_frame.data.assume_init().as_const() };

                        // Initialize the Option as Some(inner_value)
                        unsafe {
                            init_some_fn(parent_frame.data, inner_value_ptr);
                        }

                        // Deallocate the inner value's memory since init_some_fn moved it
                        if let FrameOwnership::Owned = popped_frame.ownership {
                            if let Ok(layout) = popped_frame.shape.layout.sized_layout() {
                                if layout.size() > 0 {
                                    unsafe {
                                        alloc::alloc::dealloc(
                                            popped_frame.data.as_mut_byte_ptr(),
                                            layout,
                                        );
                                    }
                                }
                            }
                        }

                        // Mark that we're no longer building the inner value
                        *building_inner = false;
                    } else {
                        return Err(ReflectError::OperationFailed {
                            shape: parent_frame.shape,
                            operation: "Option frame without Option definition",
                        });
                    }
                }
            }
            Tracker::Uninit | Tracker::Init => {
                // the main case here is: the popped frame was a `String` and the
                // parent frame is an `Arc<str>`, `Box<str>` etc.
                match &parent_frame.shape.def {
                    Def::Pointer(smart_ptr_def) => {
                        let pointee =
                            smart_ptr_def
                                .pointee()
                                .ok_or(ReflectError::InvariantViolation {
                                    invariant: "pointer type doesn't have a pointee",
                                })?;

                        if !pointee.is_shape(str::SHAPE) {
                            return Err(ReflectError::InvariantViolation {
                                invariant: "only T=str is supported when building SmartPointer<T> and T is unsized",
                            });
                        }

                        if !popped_frame.shape.is_shape(String::SHAPE) {
                            return Err(ReflectError::InvariantViolation {
                                invariant: "the popped frame should be String when building a SmartPointer<T>",
                            });
                        }

                        popped_frame.require_full_initialization()?;

                        // if the just-popped frame was a SmartPointerStr, we have some conversion to do:
                        // Special-case: SmartPointer<str> (Box<str>, Arc<str>, Rc<str>) via SmartPointerStr tracker
                        // Here, popped_frame actually contains a value for String that should be moved into the smart pointer.
                        // We convert the String into Box<str>, Arc<str>, or Rc<str> as appropriate and write it to the parent frame.
                        use alloc::{rc::Rc, string::String, sync::Arc};
                        let parent_shape = parent_frame.shape;

                        let Some(known) = smart_ptr_def.known else {
                            return Err(ReflectError::OperationFailed {
                                shape: parent_shape,
                                operation: "SmartPointerStr for unknown smart pointer kind",
                            });
                        };

                        parent_frame.deinit();

                        // Interpret the memory as a String, then convert and write.
                        let string_ptr = popped_frame.data.as_mut_byte_ptr() as *mut String;
                        let string_value = unsafe { core::ptr::read(string_ptr) };

                        match known {
                            KnownPointer::Box => {
                                let boxed: Box<str> = string_value.into_boxed_str();
                                unsafe {
                                    core::ptr::write(
                                        parent_frame.data.as_mut_byte_ptr() as *mut Box<str>,
                                        boxed,
                                    );
                                }
                            }
                            KnownPointer::Arc => {
                                let arc: Arc<str> = Arc::from(string_value.into_boxed_str());
                                unsafe {
                                    core::ptr::write(
                                        parent_frame.data.as_mut_byte_ptr() as *mut Arc<str>,
                                        arc,
                                    );
                                }
                            }
                            KnownPointer::Rc => {
                                let rc: Rc<str> = Rc::from(string_value.into_boxed_str());
                                unsafe {
                                    core::ptr::write(
                                        parent_frame.data.as_mut_byte_ptr() as *mut Rc<str>,
                                        rc,
                                    );
                                }
                            }
                            _ => {
                                return Err(ReflectError::OperationFailed {
                                    shape: parent_shape,
                                    operation: "Don't know how to build this pointer type",
                                });
                            }
                        }

                        parent_frame.tracker = Tracker::Init;

                        popped_frame.tracker = Tracker::Uninit;
                        popped_frame.dealloc();
                    }
                    _ => {
                        unreachable!(
                            "we popped a frame and parent was Init or Uninit, but it wasn't a smart pointer and... there's no way this should happen normally"
                        )
                    }
                }
            }
            Tracker::SmartPointerSlice {
                vtable,
                building_item,
            } => {
                if *building_item {
                    // We just popped an element frame, now push it to the slice builder
                    let element_ptr = PtrMut::new(unsafe {
                        NonNull::new_unchecked(popped_frame.data.as_mut_byte_ptr())
                    });

                    // Use the slice builder's push_fn to add the element
                    crate::trace!("Pushing element to slice builder");
                    unsafe {
                        let parent_ptr = parent_frame.data.assume_init();
                        (vtable.push_fn)(parent_ptr, element_ptr);
                    }

                    popped_frame.tracker = Tracker::Uninit;
                    popped_frame.dealloc();

                    if let Tracker::SmartPointerSlice {
                        building_item: bi, ..
                    } = &mut parent_frame.tracker
                    {
                        *bi = false;
                    }
                }
            }
            _ => {}
        }

        Ok(self)
    }

    /// Returns a human-readable path representing the current traversal in the builder,
    /// e.g., `RootStruct.fieldName[index].subfield`.
    pub fn path(&self) -> String {
        let mut out = String::new();

        let mut path_components = Vec::new();
        // The stack of enum/struct/sequence names currently in context.
        // Start from root and build upwards.
        for (i, frame) in self.frames.iter().enumerate() {
            match frame.shape.ty {
                Type::User(user_type) => match user_type {
                    UserType::Struct(struct_type) => {
                        // Try to get currently active field index
                        let mut field_str = None;
                        if let Tracker::Struct {
                            current_child: Some(idx),
                            ..
                        } = &frame.tracker
                        {
                            if let Some(field) = struct_type.fields.get(*idx) {
                                field_str = Some(field.name);
                            }
                        }
                        if i == 0 {
                            // Use Display for the root struct shape
                            path_components.push(format!("{}", frame.shape));
                        }
                        if let Some(field_name) = field_str {
                            path_components.push(format!(".{field_name}"));
                        }
                    }
                    UserType::Enum(_enum_type) => {
                        // Try to get currently active variant and field
                        if let Tracker::Enum {
                            variant,
                            current_child,
                            ..
                        } = &frame.tracker
                        {
                            if i == 0 {
                                // Use Display for the root enum shape
                                path_components.push(format!("{}", frame.shape));
                            }
                            path_components.push(format!("::{}", variant.name));
                            if let Some(idx) = *current_child {
                                if let Some(field) = variant.data.fields.get(idx) {
                                    path_components.push(format!(".{}", field.name));
                                }
                            }
                        } else if i == 0 {
                            // just the enum display
                            path_components.push(format!("{}", frame.shape));
                        }
                    }
                    UserType::Union(_union_type) => {
                        path_components.push(format!("{}", frame.shape));
                    }
                    UserType::Opaque => {
                        path_components.push("<opaque>".to_string());
                    }
                },
                Type::Sequence(seq_type) => match seq_type {
                    facet_core::SequenceType::Array(_array_def) => {
                        // Try to show current element index
                        if let Tracker::Array {
                            current_child: Some(idx),
                            ..
                        } = &frame.tracker
                        {
                            path_components.push(format!("[{idx}]"));
                        }
                    }
                    // You can add more for Slice, Vec, etc., if applicable
                    _ => {
                        // just indicate "[]" for sequence
                        path_components.push("[]".to_string());
                    }
                },
                Type::Pointer(_) => {
                    // Indicate deref
                    path_components.push("*".to_string());
                }
                _ => {
                    // No structural path
                }
            }
        }
        // Merge the path_components into a single string
        for component in path_components {
            out.push_str(&component);
        }
        out
    }

    /// Get the field for the parent frame
    pub fn parent_field(&self) -> Option<&Field> {
        self.frames.iter().rev().nth(1).and_then(|f| f.get_field())
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Build
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Builds the value
    pub fn build(&mut self) -> Result<HeapValue<'facet>, ReflectError> {
        self.require_active()?;
        if self.frames.len() != 1 {
            self.state = PartialState::BuildFailed;
            return Err(ReflectError::InvariantViolation {
                invariant: "Partial::build() expects a single frame — call end() until that's the case",
            });
        }

        let frame = self.frames.pop().unwrap();

        // Check initialization before proceeding
        if let Err(e) = frame.require_full_initialization() {
            // Put the frame back so Drop can handle cleanup properly
            self.frames.push(frame);
            self.state = PartialState::BuildFailed;
            return Err(e);
        }

        // Check invariants if present
        if let Some(invariants_fn) = frame.shape.vtable.invariants {
            // Safety: The value is fully initialized at this point (we just checked with require_full_initialization)
            let value_ptr = unsafe { frame.data.assume_init().as_const() };
            let invariants_ok = unsafe { invariants_fn(value_ptr) };

            if !invariants_ok {
                // Put the frame back so Drop can handle cleanup properly
                self.frames.push(frame);
                self.state = PartialState::BuildFailed;
                return Err(ReflectError::InvariantViolation {
                    invariant: "Type invariants check failed",
                });
            }
        }

        // Mark as built to prevent reuse
        self.state = PartialState::Built;

        match frame
            .shape
            .layout
            .sized_layout()
            .map_err(|_layout_err| ReflectError::Unsized {
                shape: frame.shape,
                operation: "build (final check for sized layout)",
            }) {
            Ok(layout) => Ok(HeapValue {
                guard: Some(Guard {
                    ptr: unsafe { NonNull::new_unchecked(frame.data.as_mut_byte_ptr()) },
                    layout,
                }),
                shape: frame.shape,
                phantom: PhantomData,
            }),
            Err(e) => {
                // Put the frame back for proper cleanup
                self.frames.push(frame);
                self.state = PartialState::BuildFailed;
                Err(e)
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// `Set` and set helpers
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Sets a value wholesale into the current frame.
    ///
    /// If the current frame was already initialized, the previous value is
    /// dropped. If it was partially initialized, the fields that were initialized
    /// are dropped, etc.
    pub fn set<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.require_active()?;
        struct DropVal<U> {
            ptr: *mut U,
        }
        impl<U> Drop for DropVal<U> {
            #[inline]
            fn drop(&mut self) {
                unsafe { core::ptr::drop_in_place(self.ptr) };
            }
        }

        let mut value = ManuallyDrop::new(value);
        let drop = DropVal {
            ptr: (&mut value) as *mut ManuallyDrop<U> as *mut U,
        };

        let ptr_const = PtrConst::new(unsafe { NonNull::new_unchecked(drop.ptr) });
        unsafe {
            // Safety: We are calling set_shape with a valid shape and a valid pointer
            self.set_shape(ptr_const, U::SHAPE)?
        };
        core::mem::forget(drop);

        Ok(self)
    }

    /// Sets a value into the current frame by [PtrConst] / [Shape].
    ///
    /// # Safety
    ///
    /// The caller must ensure that `src_value` points to a valid instance of a value
    /// whose memory layout and type matches `src_shape`, and that this value can be
    /// safely copied (bitwise) into the destination specified by the Partial's current frame.
    ///
    /// After a successful call, the ownership of the value at `src_value` is effectively moved
    /// into the Partial (i.e., the destination), and the original value should not be used
    /// or dropped by the caller; you should use `core::mem::forget` on the passed value.
    ///
    /// If an error is returned, the destination remains unmodified and safe for future operations.
    #[inline]
    pub unsafe fn set_shape(
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'static Shape,
    ) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let fr = self.frames.last_mut().unwrap();
        crate::trace!("set_shape({src_shape:?})");

        if !fr.shape.is_shape(src_shape) {
            return Err(ReflectError::WrongShape {
                expected: fr.shape,
                actual: src_shape,
            });
        }

        fr.deinit();

        // SAFETY: `fr.shape` and `src_shape` are the same, so they have the same size,
        // and the preconditions for this function are that `src_value` is fully intialized.
        unsafe {
            // unwrap safety: the only failure condition for copy_from is that shape is unsized,
            // which is not possible for `Partial`
            fr.data.copy_from(src_value, fr.shape).unwrap();
        }

        // SAFETY: if we reached this point, `fr.data` is correctly initialized
        unsafe {
            fr.mark_as_init();
        }

        Ok(self)
    }

    /// Sets the current frame using a function that initializes the value
    ///
    /// # Safety
    ///
    /// If `f` returns Ok(), it is assumed that it initialized the passed pointer fully and with a
    /// value of the right type.
    ///
    /// If `f` returns Err(), it is assumed that it did NOT initialize the passed pointer and that
    /// there is no need to drop it in place.
    pub unsafe fn set_from_function<F>(&mut self, f: F) -> Result<&mut Self, ReflectError>
    where
        F: FnOnce(PtrUninit<'_>) -> Result<(), ReflectError>,
    {
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        frame.deinit();
        f(frame.data)?;

        // safety: `f()` returned Ok, so `frame.data` must be initialized
        unsafe {
            frame.mark_as_init();
        }

        Ok(self)
    }

    /// Sets the current frame to its default value using `default_in_place` from the
    /// vtable.
    ///
    /// Note: if you have `struct S { field: F }`, and `F` does not implement `Default`
    /// but `S` does, this doesn't magically uses S's `Default` implementation to get a value
    /// for `field`.
    ///
    /// If the current frame's shape does not implement `Default`, then this returns an error.
    #[inline]
    pub fn set_default(&mut self) -> Result<&mut Self, ReflectError> {
        let frame = self.frames.last().unwrap();

        let Some(default_fn) = frame.shape.vtable.default_in_place else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "type does not implement Default",
            });
        };

        // SAFETY: `default_fn` fully initializes the passed pointer. we took it
        // from the vtable of `frame.shape`.
        unsafe {
            self.set_from_function(move |ptr| {
                default_fn(ptr);
                Ok(())
            })
        }
    }

    /// Copy a value from a Peek into the current frame.
    ///
    /// # Invariants
    ///
    /// `peek` must be a thin pointer, otherwise this panics.
    ///
    /// # Safety
    ///
    /// If this succeeds, the value `Peek` points to has been moved out of, and
    /// as such, should not be dropped (but should be deallocated).
    pub unsafe fn set_from_peek(&mut self, peek: &Peek<'_, '_>) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        // Get the source value's pointer and shape
        let src_ptr = peek.data();
        let src_shape = peek.shape();

        // SAFETY: `Peek` guarantees that src_ptr is initialized and of type src_shape
        unsafe { self.set_shape(src_ptr, src_shape) }
    }

    /// Parses a string value into the current frame using the type's ParseFn from the vtable.
    ///
    /// If the current frame was previously initialized, its contents are dropped in place.
    pub fn parse_from_str(&mut self, s: &str) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let frame = self.frames.last_mut().unwrap();

        // Check if the type has a parse function
        let Some(parse_fn) = frame.shape.vtable.parse else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "Type does not support parsing from string",
            });
        };

        // Note: deinit leaves us in `Tracker::Uninit` state which is valid even if we error out.
        frame.deinit();

        // Parse the string value using the type's parse function
        let result = unsafe { parse_fn(s, frame.data) };
        if let Err(_pe) = result {
            // TODO: can we propagate the ParseError somehow?
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "Failed to parse string value",
            });
        }

        // SAFETY: `parse_fn` returned `Ok`, so `frame.data` is fully initialized now.
        unsafe {
            frame.mark_as_init();
        }
        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Enum variant selection
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Get the currently selected variant for an enum
    pub fn selected_variant(&self) -> Option<Variant> {
        let frame = self.frames.last()?;

        match &frame.tracker {
            Tracker::Enum { variant, .. } => Some(**variant),
            _ => None,
        }
    }

    /// Find a variant by name in the current enum
    pub fn find_variant(&self, variant_name: &str) -> Option<(usize, &'static Variant)> {
        let frame = self.frames.last()?;

        if let Type::User(UserType::Enum(enum_def)) = frame.shape.ty {
            enum_def
                .variants
                .iter()
                .enumerate()
                .find(|(_, v)| v.name == variant_name)
        } else {
            None
        }
    }

    /// Assuming the current frame is an enum, this selects a variant by index
    /// (0-based, in declaration order).
    ///
    /// For example:
    ///
    /// ```rust,no_run
    /// enum E { A, B, C }
    /// ```
    ///
    /// Calling `select_nth_variant(2)` would select variant `C`.
    ///
    /// This will return an error if the current frame is anything other than fully-uninitialized.
    /// In other words, it's not possible to "switch to a different variant" once you've selected one.
    ///
    /// This does _not_ push a frame on the stack.
    pub fn select_nth_variant(&mut self, index: usize) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let frame = self.frames.last().unwrap();
        let enum_type = frame.get_enum_type()?;

        if index >= enum_type.variants.len() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "variant index out of bounds",
            });
        }
        let variant = &enum_type.variants[index];

        self.select_variant_internal(&enum_type, variant)?;
        Ok(self)
    }

    /// Pushes a variant for enum initialization by name
    ///
    /// See [Self::select_nth_variant] for more notes.
    pub fn select_variant_named(&mut self, variant_name: &str) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let frame = self.frames.last_mut().unwrap();
        let enum_type = frame.get_enum_type()?;

        let Some(variant) = enum_type.variants.iter().find(|v| v.name == variant_name) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "No variant found with the given name",
            });
        };

        self.select_variant_internal(&enum_type, variant)?;
        Ok(self)
    }

    /// Selects a given enum variant by discriminant. If none of the variants
    /// of the frame's enum have that discriminant, this returns an error.
    ///
    /// See [Self::select_nth_variant] for more notes.
    pub fn select_variant(&mut self, discriminant: i64) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        // Check all invariants early before making any changes
        let frame = self.frames.last().unwrap();

        // Check that we're dealing with an enum
        let enum_type = match frame.shape.ty {
            Type::User(UserType::Enum(e)) => e,
            _ => {
                return Err(ReflectError::WasNotA {
                    expected: "enum",
                    actual: frame.shape,
                });
            }
        };

        // Find the variant with the matching discriminant
        let Some(variant) = enum_type
            .variants
            .iter()
            .find(|v| v.discriminant == Some(discriminant))
        else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "No variant found with the given discriminant",
            });
        };

        // Update the frame tracker to select the variant
        self.select_variant_internal(&enum_type, variant)?;

        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Field selection
////////////////////////////////////////////////////////////////////////////////////////////////////
impl Partial<'_> {
    /// Find the index of a field by name in the current struct
    ///
    /// If the current frame isn't a struct or an enum (with a selected variant)
    /// then this returns `None` for sure.
    pub fn field_index(&self, field_name: &str) -> Option<usize> {
        let frame = self.frames.last()?;

        match frame.shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                struct_def.fields.iter().position(|f| f.name == field_name)
            }
            Type::User(UserType::Enum(_)) => {
                // If we're in an enum variant, check its fields
                if let Tracker::Enum { variant, .. } = &frame.tracker {
                    variant
                        .data
                        .fields
                        .iter()
                        .position(|f| f.name == field_name)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Check if a struct field at the given index has been set
    pub fn is_field_set(&self, index: usize) -> Result<bool, ReflectError> {
        let frame = self.frames.last().ok_or(ReflectError::NoActiveFrame)?;

        match &frame.tracker {
            Tracker::Uninit => Ok(false),
            Tracker::Init => Ok(true),
            Tracker::Struct { iset, .. } => Ok(iset.get(index)),
            Tracker::Enum { data, variant, .. } => {
                // Check if the field is already marked as set
                if data.get(index) {
                    return Ok(true);
                }

                // For enum variant fields that are empty structs, they are always initialized
                if let Some(field) = variant.data.fields.get(index) {
                    if let Type::User(UserType::Struct(field_struct)) = field.shape().ty {
                        if field_struct.fields.is_empty() {
                            return Ok(true);
                        }
                    }
                }

                Ok(false)
            }
            Tracker::Option { building_inner } => {
                // For Options, index 0 represents the inner value
                if index == 0 {
                    Ok(!building_inner)
                } else {
                    Err(ReflectError::InvalidOperation {
                        operation: "is_field_set",
                        reason: "Option only has one field (index 0)",
                    })
                }
            }
            _ => Err(ReflectError::InvalidOperation {
                operation: "is_field_set",
                reason: "Current frame is not a struct, enum variant, or option",
            }),
        }
    }

    /// Selects a field (by name) of a struct or enum data.
    ///
    /// For enums, the variant needs to be selected first, see [Self::select_nth_variant]
    /// and friends.
    pub fn begin_field(&mut self, field_name: &str) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let frame = self.frames.last().unwrap();
        let fields = self.get_fields()?;
        let Some(idx) = fields.iter().position(|f| f.name == field_name) else {
            return Err(ReflectError::FieldError {
                shape: frame.shape,
                field_error: facet_core::FieldError::NoSuchField,
            });
        };
        self.begin_nth_field(idx)
    }

    /// Begins the nth field of a struct, enum variant, or array, by index.
    ///
    /// On success, this pushes a new frame which must be ended with a call to [Partial::end]
    pub fn begin_nth_field(&mut self, idx: usize) -> Result<&mut Self, ReflectError> {
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        let next_frame = match frame.shape.ty {
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => {
                    Self::begin_nth_struct_field(frame, struct_type, idx)?
                }
                UserType::Enum(_) => {
                    // Check if we have a variant selected
                    match &frame.tracker {
                        Tracker::Enum { variant, .. } => {
                            Self::begin_nth_enum_field(frame, variant, idx)?
                        }
                        _ => {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "must call select_variant before selecting enum fields",
                            });
                        }
                    }
                }
                UserType::Union(_) => {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "cannot select a field from a union",
                    });
                }
                UserType::Opaque => {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "cannot select a field from an opaque type",
                    });
                }
            },
            Type::Sequence(sequence_type) => match sequence_type {
                SequenceType::Array(array_type) => {
                    Self::begin_nth_array_element(frame, array_type, idx)?
                }
                SequenceType::Slice(_) => {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "cannot select a field from slices yet",
                    });
                }
            },
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "cannot select a field from this type",
                });
            }
        };

        self.frames.push(next_frame);
        Ok(self)
    }

    /// Sets the given field to its default value, preferring:
    ///
    ///   * A `default = some_fn()` function
    ///   * The field's `Default` implementation if any
    ///
    /// But without going all the way up to the parent struct's `Default` impl.
    ///
    /// Errors out if idx is out of bound, if the field has no default method or Default impl.
    pub fn set_nth_field_to_default(&mut self, idx: usize) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let frame = self.frames.last().unwrap();
        let fields = self.get_fields()?;

        if idx >= fields.len() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "field index out of bounds",
            });
        }

        let field = fields[idx];

        // Check for field-level default function first, then type-level default
        if let Some(field_default_fn) = field.vtable.default_fn {
            self.begin_nth_field(idx)?;
            // the field default fn should be well-behaved
            unsafe {
                self.set_from_function(|ptr| {
                    field_default_fn(ptr);
                    Ok(())
                })?;
            }
            self.end()
        } else if field.shape().is(Characteristic::Default) {
            self.begin_nth_field(idx)?;
            self.set_default()?;
            self.end()
        } else {
            return Err(ReflectError::DefaultAttrButNoDefaultImpl {
                shape: field.shape(),
            });
        }
    }

    /// Given a `Partial` for the same shape, and assuming that partial has the nth
    /// field initialized, move the value from `src` to `self`, marking it as deinitialized
    /// in `src`.
    pub fn steal_nth_field(
        &mut self,
        src: &mut Partial,
        field_index: usize,
    ) -> Result<&mut Self, ReflectError> {
        let dst_shape = self.shape();
        let src_shape = src.shape();
        if dst_shape != src_shape {
            return Err(ReflectError::HeistCancelledDifferentShapes {
                src_shape,
                dst_shape,
            });
        }

        // FIXME: what about enums? we don't check that the right variant is
        // selected here.
        if !src.is_field_set(field_index)? {
            return Err(ReflectError::InvariantViolation {
                invariant: "stolen field must be initialized",
            });
        }

        let maybe_fields = match src_shape.ty {
            Type::Primitive(_primitive_type) => None,
            Type::Sequence(_sequence_type) => None,
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => Some(struct_type.fields),
                UserType::Enum(_enum_type) => match self.selected_variant() {
                    Some(variant) => Some(variant.data.fields),
                    None => {
                        return Err(ReflectError::InvariantViolation {
                            invariant: "enum field thief must have variant selected",
                        });
                    }
                },
                UserType::Union(_union_type) => None,
                UserType::Opaque => None,
            },
            Type::Pointer(_pointer_type) => None,
        };

        let Some(fields) = maybe_fields else {
            return Err(ReflectError::OperationFailed {
                shape: src_shape,
                operation: "fetching field list for steal_nth_field",
            });
        };

        if field_index >= fields.len() {
            return Err(ReflectError::OperationFailed {
                shape: src_shape,
                operation: "field index out of bounds",
            });
        }
        let field = fields[field_index];

        let src_frame = src.frames.last_mut().unwrap();

        self.begin_nth_field(field_index)?;
        unsafe {
            self.set_from_function(|dst_field_ptr| {
                let src_field_ptr = src_frame.data.field_init_at(field.offset).as_const();
                dst_field_ptr
                    .copy_from(src_field_ptr, field.shape())
                    .unwrap();
                Ok(())
            })?;
        }
        self.end()?;

        // now mark field as uninitialized in `src`
        match &mut src_frame.tracker {
            Tracker::Uninit => {
                unreachable!("we just stole a field from src, it couldn't have been fully uninit")
            }
            Tracker::Init => {
                // all struct fields were init so we don't even have a struct tracker,
                // let's make one!
                let mut iset = ISet::new(fields.len());
                iset.set_all();
                iset.unset(field_index);
                src_frame.tracker = Tracker::Struct {
                    iset,
                    current_child: None,
                }
            }
            Tracker::Array { .. } => unreachable!("can't steal fields from arrays"),
            Tracker::Struct { iset, .. } => {
                iset.unset(field_index);
            }
            Tracker::SmartPointer { .. } => {
                unreachable!("can't steal fields from smart pointers")
            }
            Tracker::SmartPointerSlice { .. } => {
                unreachable!("can't steal fields from smart pointer slices")
            }
            Tracker::Enum { data, .. } => {
                data.unset(field_index);
            }
            Tracker::List { .. } => {
                unreachable!("can't steal fields from lists")
            }
            Tracker::Map { .. } => {
                unreachable!("can't steal fields from maps")
            }
            Tracker::Option { .. } => {
                unreachable!("can't steal fields from options")
            }
        }

        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Smart pointers
////////////////////////////////////////////////////////////////////////////////////////////////////
impl Partial<'_> {
    /// Pushes a frame to initialize the inner value of a smart pointer (`Box<T>`, `Arc<T>`, etc.)
    pub fn begin_smart_ptr(&mut self) -> Result<&mut Self, ReflectError> {
        crate::trace!("begin_smart_ptr()");
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        // Check that we have a SmartPointer
        match &frame.shape.def {
            Def::Pointer(smart_ptr_def) if smart_ptr_def.constructible_from_pointee() => {
                // Get the pointee shape
                let pointee_shape = match smart_ptr_def.pointee() {
                    Some(shape) => shape,
                    None => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "Smart pointer must have a pointee shape",
                        });
                    }
                };

                if pointee_shape.layout.sized_layout().is_ok() {
                    // pointee is sized, we can allocate it — for `Arc<T>` we'll be allocating a `T` and
                    // holding onto it. We'll build a new Arc with it when ending the smart pointer frame.

                    if matches!(frame.tracker, Tracker::Uninit) {
                        frame.tracker = Tracker::SmartPointer {
                            is_initialized: false,
                        };
                    }

                    let inner_layout = match pointee_shape.layout.sized_layout() {
                        Ok(layout) => layout,
                        Err(_) => {
                            return Err(ReflectError::Unsized {
                                shape: pointee_shape,
                                operation: "begin_smart_ptr, calculating inner value layout",
                            });
                        }
                    };
                    let inner_ptr: *mut u8 = unsafe { alloc::alloc::alloc(inner_layout) };
                    let Some(inner_ptr) = NonNull::new(inner_ptr) else {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "failed to allocate memory for smart pointer inner value",
                        });
                    };

                    // Push a new frame for the inner value
                    self.frames.push(Frame::new(
                        PtrUninit::new(inner_ptr),
                        pointee_shape,
                        FrameOwnership::Owned,
                    ));
                } else {
                    // pointee is unsized, we only support a handful of cases there
                    if pointee_shape == str::SHAPE {
                        crate::trace!("Pointee is str");

                        // Allocate space for a String
                        let string_layout = String::SHAPE
                            .layout
                            .sized_layout()
                            .expect("String must have a sized layout");
                        let string_ptr: *mut u8 = unsafe { alloc::alloc::alloc(string_layout) };
                        let Some(string_ptr) = NonNull::new(string_ptr) else {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "failed to allocate memory for string",
                            });
                        };
                        let mut frame = Frame::new(
                            PtrUninit::new(string_ptr),
                            String::SHAPE,
                            FrameOwnership::Owned,
                        );
                        frame.tracker = Tracker::Uninit;
                        self.frames.push(frame);
                    } else if let Type::Sequence(SequenceType::Slice(_st)) = pointee_shape.ty {
                        crate::trace!("Pointee is [{}]", _st.t);

                        // Get the slice builder vtable
                        let slice_builder_vtable = smart_ptr_def
                            .vtable
                            .slice_builder_vtable
                            .ok_or(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "smart pointer does not support slice building",
                            })?;

                        // Create a new builder
                        let builder_ptr = (slice_builder_vtable.new_fn)();

                        // Deallocate the original Arc allocation before replacing with slice builder
                        if let FrameOwnership::Owned = frame.ownership {
                            if let Ok(layout) = frame.shape.layout.sized_layout() {
                                if layout.size() > 0 {
                                    unsafe {
                                        alloc::alloc::dealloc(frame.data.as_mut_byte_ptr(), layout)
                                    };
                                }
                            }
                        }

                        // Update the current frame to use the slice builder
                        frame.data = builder_ptr.as_uninit();
                        frame.tracker = Tracker::SmartPointerSlice {
                            vtable: slice_builder_vtable,
                            building_item: false,
                        };
                        // The slice builder memory is managed by the vtable, not by us
                        frame.ownership = FrameOwnership::ManagedElsewhere;
                    } else {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "push_smart_ptr can only be called on pointers to supported pointee types",
                        });
                    }
                }

                Ok(self)
            }
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "push_smart_ptr can only be called on compatible types",
            }),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Lists
////////////////////////////////////////////////////////////////////////////////////////////////////
impl Partial<'_> {
    /// Initializes a list (Vec, etc.) if it hasn't been initialized before.
    /// This is a prerequisite to `begin_push_item`/`set`/`end` or the shorthand
    /// `push`.
    ///
    /// `begin_list` does not clear the list if it was previously initialized.
    /// `begin_list` does not push a new frame to the stack, and thus does not
    /// require `end` to be called afterwards.
    pub fn begin_list(&mut self) -> Result<&mut Self, ReflectError> {
        crate::trace!("begin_list()");
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        match &frame.tracker {
            Tracker::Uninit => {
                // that's good, let's initialize it
            }
            Tracker::Init => {
                // initialized (perhaps from a previous round?) but should be a list tracker, let's fix that:
                frame.tracker = Tracker::List {
                    is_initialized: true,
                    current_child: false,
                };
                return Ok(self);
            }
            Tracker::List { is_initialized, .. } => {
                if *is_initialized {
                    // already initialized, nothing to do
                    return Ok(self);
                }
            }
            Tracker::SmartPointerSlice { .. } => {
                // begin_list is kinda superfluous when we're in a SmartPointerSlice state
                return Ok(self);
            }
            _ => {
                return Err(ReflectError::UnexpectedTracker {
                    message: "begin_list called but tracker isn't something list-like",
                    current_tracker: frame.tracker.kind(),
                });
            }
        };

        // Check that we have a List
        let list_def = match &frame.shape.def {
            Def::List(list_def) => list_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "begin_list can only be called on List types",
                });
            }
        };

        // Check that we have init_in_place_with_capacity function
        let init_fn = match list_def.vtable.init_in_place_with_capacity {
            Some(f) => f,
            None => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "list type does not support initialization with capacity",
                });
            }
        };

        // Initialize the list with default capacity (0)
        unsafe {
            init_fn(frame.data, 0);
        }

        // Update tracker to List state
        frame.tracker = Tracker::List {
            is_initialized: true,
            current_child: false,
        };

        Ok(self)
    }

    /// Pushes an element to the list
    /// The element should be set using `set()` or similar methods, then `pop()` to complete
    pub fn begin_list_item(&mut self) -> Result<&mut Self, ReflectError> {
        crate::trace!("begin_list_item()");
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        // Check if we're building a smart pointer slice
        if let Tracker::SmartPointerSlice {
            building_item,
            vtable: _,
        } = &frame.tracker
        {
            if *building_item {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "already building an item, call end() first",
                });
            }

            // Get the element type from the smart pointer's pointee
            let element_shape = match &frame.shape.def {
                Def::Pointer(smart_ptr_def) => match smart_ptr_def.pointee() {
                    Some(pointee_shape) => match &pointee_shape.ty {
                        Type::Sequence(SequenceType::Slice(slice_type)) => slice_type.t,
                        _ => {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "smart pointer pointee is not a slice",
                            });
                        }
                    },
                    None => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "smart pointer has no pointee",
                        });
                    }
                },
                _ => {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "expected smart pointer definition",
                    });
                }
            };

            // Allocate space for the element
            crate::trace!("Pointee is a slice of {element_shape}");
            let element_layout = match element_shape.layout.sized_layout() {
                Ok(layout) => layout,
                Err(_) => {
                    return Err(ReflectError::OperationFailed {
                        shape: element_shape,
                        operation: "cannot allocate unsized element",
                    });
                }
            };

            let element_ptr: *mut u8 = unsafe { alloc::alloc::alloc(element_layout) };
            let Some(element_ptr) = NonNull::new(element_ptr) else {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "failed to allocate memory for list element",
                });
            };

            // Create and push the element frame
            crate::trace!("Pushing element frame, which we just allocated");
            let element_frame = Frame::new(
                PtrUninit::new(element_ptr),
                element_shape,
                FrameOwnership::Owned,
            );
            self.frames.push(element_frame);

            // Mark that we're building an item
            // We need to update the tracker after pushing the frame
            let parent_idx = self.frames.len() - 2;
            if let Tracker::SmartPointerSlice { building_item, .. } =
                &mut self.frames[parent_idx].tracker
            {
                crate::trace!("Marking element frame as building item");
                *building_item = true;
            }

            return Ok(self);
        }

        // Check that we have a List that's been initialized
        let list_def = match &frame.shape.def {
            Def::List(list_def) => list_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "push can only be called on List types",
                });
            }
        };

        // Verify the tracker is in List state and initialized
        match &mut frame.tracker {
            Tracker::List {
                is_initialized: true,
                current_child,
            } => {
                if *current_child {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "already pushing an element, call pop() first",
                    });
                }
                *current_child = true;
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "must call begin_list() before push()",
                });
            }
        }

        // Get the element shape
        let element_shape = list_def.t();

        // Allocate space for the new element
        let element_layout = match element_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return Err(ReflectError::Unsized {
                    shape: element_shape,
                    operation: "begin_list_item: calculating element layout",
                });
            }
        };
        let element_ptr: *mut u8 = unsafe { alloc::alloc::alloc(element_layout) };

        let Some(element_ptr) = NonNull::new(element_ptr) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "failed to allocate memory for list element",
            });
        };

        // Push a new frame for the element
        self.frames.push(Frame::new(
            PtrUninit::new(element_ptr),
            element_shape,
            FrameOwnership::Owned,
        ));

        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Maps
////////////////////////////////////////////////////////////////////////////////////////////////////
impl Partial<'_> {
    /// Begins a map initialization operation
    ///
    /// This initializes the map with default capacity and allows inserting key-value pairs
    /// It does _not_ push a new frame onto the stack.
    pub fn begin_map(&mut self) -> Result<&mut Self, ReflectError> {
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        // Check that we have a Map
        let map_def = match &frame.shape.def {
            Def::Map(map_def) => map_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "begin_map can only be called on Map types",
                });
            }
        };

        let init_fn = map_def.vtable.init_in_place_with_capacity_fn;

        // Initialize the map with default capacity (0)
        unsafe {
            init_fn(frame.data, 0);
        }

        // Update tracker to Map state
        frame.tracker = Tracker::Map {
            is_initialized: true,
            insert_state: MapInsertState::Idle,
        };

        Ok(self)
    }

    /// Pushes a frame for the map key. After that, `set()` should be called
    /// (or the key should be initialized somehow) and `end()` should be called
    /// to pop the frame.
    pub fn begin_key(&mut self) -> Result<&mut Self, ReflectError> {
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        // Check that we have a Map and set up for key insertion
        let map_def = match (&frame.shape.def, &mut frame.tracker) {
            (
                Def::Map(map_def),
                Tracker::Map {
                    is_initialized: true,
                    insert_state,
                },
            ) => {
                match insert_state {
                    MapInsertState::Idle => {
                        // Start a new insert automatically
                        *insert_state = MapInsertState::PushingKey { key_ptr: None };
                    }
                    MapInsertState::PushingKey { key_ptr } => {
                        if key_ptr.is_some() {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "already pushing a key, call end() first",
                            });
                        }
                    }
                    _ => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "must complete current operation before begin_key()",
                        });
                    }
                }
                map_def
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "must call begin_map() before begin_key()",
                });
            }
        };

        // Get the key shape
        let key_shape = map_def.k();

        // Allocate space for the key
        let key_layout = match key_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return Err(ReflectError::Unsized {
                    shape: key_shape,
                    operation: "begin_key allocating key",
                });
            }
        };
        let key_ptr_raw: *mut u8 = unsafe { alloc::alloc::alloc(key_layout) };

        let Some(key_ptr_raw) = NonNull::new(key_ptr_raw) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "failed to allocate memory for map key",
            });
        };

        // Store the key pointer in the insert state
        match &mut frame.tracker {
            Tracker::Map {
                insert_state: MapInsertState::PushingKey { key_ptr: kp },
                ..
            } => {
                *kp = Some(PtrUninit::new(key_ptr_raw));
            }
            _ => unreachable!(),
        }

        // Push a new frame for the key
        self.frames.push(Frame::new(
            PtrUninit::new(key_ptr_raw),
            key_shape,
            FrameOwnership::ManagedElsewhere, // Ownership tracked in MapInsertState
        ));

        Ok(self)
    }

    /// Pushes a frame for the map value
    /// Must be called after the key has been set and popped
    pub fn begin_value(&mut self) -> Result<&mut Self, ReflectError> {
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        // Check that we have a Map in PushingValue state
        let map_def = match (&frame.shape.def, &mut frame.tracker) {
            (
                Def::Map(map_def),
                Tracker::Map {
                    insert_state: MapInsertState::PushingValue { value_ptr, .. },
                    ..
                },
            ) => {
                if value_ptr.is_some() {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "already pushing a value, call pop() first",
                    });
                }
                map_def
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "must complete key before push_value()",
                });
            }
        };

        // Get the value shape
        let value_shape = map_def.v();

        // Allocate space for the value
        let value_layout = match value_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return Err(ReflectError::Unsized {
                    shape: value_shape,
                    operation: "begin_value allocating value",
                });
            }
        };
        let value_ptr_raw: *mut u8 = unsafe { alloc::alloc::alloc(value_layout) };

        let Some(value_ptr_raw) = NonNull::new(value_ptr_raw) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "failed to allocate memory for map value",
            });
        };

        // Store the value pointer in the insert state
        match &mut frame.tracker {
            Tracker::Map {
                insert_state: MapInsertState::PushingValue { value_ptr: vp, .. },
                ..
            } => {
                *vp = Some(PtrUninit::new(value_ptr_raw));
            }
            _ => unreachable!(),
        }

        // Push a new frame for the value
        self.frames.push(Frame::new(
            PtrUninit::new(value_ptr_raw),
            value_shape,
            FrameOwnership::ManagedElsewhere, // Ownership tracked in MapInsertState
        ));

        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Option / inner
////////////////////////////////////////////////////////////////////////////////////////////////////
impl Partial<'_> {
    /// Begin building the Some variant of an Option
    pub fn begin_some(&mut self) -> Result<&mut Self, ReflectError> {
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        // Verify we're working with an Option
        let option_def = match frame.shape.def {
            Def::Option(def) => def,
            _ => {
                return Err(ReflectError::WasNotA {
                    expected: "Option",
                    actual: frame.shape,
                });
            }
        };

        // Initialize the tracker for Option building
        if matches!(frame.tracker, Tracker::Uninit) {
            frame.tracker = Tracker::Option {
                building_inner: true,
            };
        }

        // Get the inner type shape
        let inner_shape = option_def.t;

        // Allocate memory for the inner value
        let inner_layout =
            inner_shape
                .layout
                .sized_layout()
                .map_err(|_| ReflectError::Unsized {
                    shape: inner_shape,
                    operation: "begin_some, allocating Option inner value",
                })?;

        let inner_data = if inner_layout.size() == 0 {
            // For ZST, use a non-null but unallocated pointer
            PtrUninit::new(NonNull::<u8>::dangling())
        } else {
            // Allocate memory for the inner value
            let ptr = unsafe { alloc::alloc::alloc(inner_layout) };
            let Some(ptr) = NonNull::new(ptr) else {
                alloc::alloc::handle_alloc_error(inner_layout);
            };
            PtrUninit::new(ptr)
        };

        // Create a new frame for the inner value
        let inner_frame = Frame::new(inner_data, inner_shape, FrameOwnership::Owned);
        self.frames.push(inner_frame);

        Ok(self)
    }

    /// Begin building the inner value of a wrapper type
    pub fn begin_inner(&mut self) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        // Get the inner shape and check for try_from
        let (inner_shape, has_try_from, parent_shape) = {
            let frame = self.frames.last().unwrap();
            if let Some(inner_shape) = frame.shape.inner {
                let has_try_from = frame.shape.vtable.try_from.is_some();
                (Some(inner_shape), has_try_from, frame.shape)
            } else {
                (None, false, frame.shape)
            }
        };

        if let Some(inner_shape) = inner_shape {
            if has_try_from {
                // Create a conversion frame with the inner shape

                // For conversion frames, we leave the parent tracker unchanged
                // This allows automatic conversion detection to work properly

                // Allocate memory for the inner value (conversion source)
                let inner_layout =
                    inner_shape
                        .layout
                        .sized_layout()
                        .map_err(|_| ReflectError::Unsized {
                            shape: inner_shape,
                            operation: "begin_inner, getting inner layout",
                        })?;

                let inner_data = if inner_layout.size() == 0 {
                    // For ZST, use a non-null but unallocated pointer
                    PtrUninit::new(NonNull::<u8>::dangling())
                } else {
                    // Allocate memory for the inner value
                    let ptr = unsafe { alloc::alloc::alloc(inner_layout) };
                    let Some(ptr) = NonNull::new(ptr) else {
                        alloc::alloc::handle_alloc_error(inner_layout);
                    };
                    PtrUninit::new(ptr)
                };

                // For conversion frames, we create a frame directly with the inner shape
                // This allows setting values of the inner type which will be converted
                // The automatic conversion detection in end() will handle the conversion
                trace!(
                    "begin_inner: Creating frame for inner type {inner_shape} (parent is {parent_shape})"
                );
                self.frames
                    .push(Frame::new(inner_data, inner_shape, FrameOwnership::Owned));

                Ok(self)
            } else {
                // For wrapper types without try_from, navigate to the first field
                // This is a common pattern for newtype wrappers
                trace!("begin_inner: No try_from for {parent_shape}, using field navigation");
                self.begin_nth_field(0)
            }
        } else {
            Err(ReflectError::OperationFailed {
                shape: parent_shape,
                operation: "type does not have an inner value",
            })
        }
    }

    /// Begin bulding the source shape for custom deserialization, calling end() for this frame will
    /// call the deserialize_with function provided by the field and set the field using the result.
    pub fn begin_custom_deserialization(&mut self) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let current_frame = self.frames.last().unwrap();
        let target_shape = current_frame.shape;
        if let Some(field) = self.parent_field() {
            if field.vtable.deserialize_with.is_some() {
                // TODO: can we assume that this is set if the vtable element is set?
                // TODO: can we get the shape some other way?
                let Some(FieldAttribute::DeserializeFrom(source_shape)) = field
                    .attributes
                    .iter()
                    .find(|&p| matches!(p, FieldAttribute::DeserializeFrom(_)))
                else {
                    panic!("expected field attribute to be present with deserialize_with");
                };
                let source_data = source_shape.allocate().map_err(|_| ReflectError::Unsized {
                    shape: target_shape,
                    operation: "Not a Sized type",
                })?;

                trace!(
                    "begin_custom_deserialization: Creating frame for deserialization type {source_shape}"
                );
                let mut new_frame = Frame::new(source_data, source_shape, FrameOwnership::Owned);
                new_frame.using_custom_deserialization = true;
                self.frames.push(new_frame);

                Ok(self)
            } else {
                Err(ReflectError::OperationFailed {
                    shape: target_shape,
                    operation: "field does not have a deserialize_with function",
                })
            }
        } else {
            Err(ReflectError::OperationFailed {
                shape: target_shape,
                operation: "not currently processing a field",
            })
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Shorthands
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Convenience shortcut: sets the field at index `idx` directly to value, popping after.
    ///
    /// Works on structs, enums (after selecting a variant) and arrays.
    pub fn set_nth_field<U>(&mut self, idx: usize, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_nth_field(idx)?.set(value)?.end()
    }

    /// Convenience shortcut: sets the named field to value, popping after.
    pub fn set_field<U>(&mut self, field_name: &str, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_field(field_name)?.set(value)?.end()
    }

    /// Convenience shortcut: sets the key for a map key-value insertion, then pops after.
    pub fn set_key<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_key()?.set(value)?.end()
    }

    /// Convenience shortcut: sets the value for a map key-value insertion, then pops after.
    pub fn set_value<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_value()?.set(value)?.end()
    }

    /// Shorthand for: begin_list_item(), set(), end()
    pub fn push<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_list_item()?.set(value)?.end()
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Internal methods
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Preconditions:
    ///
    /// - `require_active()` check was made
    /// - frame.shape.ty is an Enum
    /// - `discriminant` is a valid discriminant
    ///
    /// Panics if current tracker is anything other than `Uninit`
    /// (switching variants is not supported for now).
    fn select_variant_internal(
        &mut self,
        enum_type: &EnumType,
        variant: &'static Variant,
    ) -> Result<(), ReflectError> {
        // Check all invariants early before making any changes
        let frame = self.frames.last().unwrap();

        // Check enum representation early
        match enum_type.enum_repr {
            EnumRepr::RustNPO => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "RustNPO enums are not supported for incremental building",
                });
            }
            EnumRepr::U8
            | EnumRepr::U16
            | EnumRepr::U32
            | EnumRepr::U64
            | EnumRepr::I8
            | EnumRepr::I16
            | EnumRepr::I32
            | EnumRepr::I64
            | EnumRepr::USize
            | EnumRepr::ISize => {
                // These are supported, continue
            }
        }

        let Some(discriminant) = variant.discriminant else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "trying to select an enum variant without a discriminant",
            });
        };

        // All checks passed, now we can safely make changes
        let fr = self.frames.last_mut().unwrap();

        // Write the discriminant to memory
        unsafe {
            match enum_type.enum_repr {
                EnumRepr::U8 => {
                    let ptr = fr.data.as_mut_byte_ptr();
                    *ptr = discriminant as u8;
                }
                EnumRepr::U16 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut u16;
                    *ptr = discriminant as u16;
                }
                EnumRepr::U32 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut u32;
                    *ptr = discriminant as u32;
                }
                EnumRepr::U64 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut u64;
                    *ptr = discriminant as u64;
                }
                EnumRepr::I8 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i8;
                    *ptr = discriminant as i8;
                }
                EnumRepr::I16 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i16;
                    *ptr = discriminant as i16;
                }
                EnumRepr::I32 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i32;
                    *ptr = discriminant as i32;
                }
                EnumRepr::I64 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i64;
                    *ptr = discriminant;
                }
                EnumRepr::USize => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut usize;
                    *ptr = discriminant as usize;
                }
                EnumRepr::ISize => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut isize;
                    *ptr = discriminant as isize;
                }
                _ => unreachable!("Already checked enum representation above"),
            }
        }

        // Update tracker to track the variant
        fr.tracker = Tracker::Enum {
            variant,
            data: ISet::new(variant.data.fields.len()),
            current_child: None,
        };

        Ok(())
    }

    /// Used by `begin_field` etc. to get a list of fields to look through, errors out
    /// if we're not pointing to a struct or an enum with an already-selected variant
    fn get_fields(&self) -> Result<&'static [Field], ReflectError> {
        let frame = self.frames.last().unwrap();
        match frame.shape.ty {
            Type::Primitive(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a primitive type",
            }),
            Type::Sequence(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a sequence type",
            }),
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => Ok(struct_type.fields),
                UserType::Enum(_) => {
                    let Tracker::Enum { variant, .. } = &frame.tracker else {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "must select variant before selecting enum fields",
                        });
                    };
                    Ok(variant.data.fields)
                }
                UserType::Union(_) => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "cannot select a field from a union type",
                }),
                UserType::Opaque => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "opaque types cannot be reflected upon",
                }),
            },
            Type::Pointer(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a pointer type",
            }),
        }
    }

    /// Selects the nth field of a struct by index
    fn begin_nth_struct_field(
        frame: &mut Frame,
        struct_type: StructType,
        idx: usize,
    ) -> Result<Frame, ReflectError> {
        if idx >= struct_type.fields.len() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "field index out of bounds",
            });
        }
        let field = &struct_type.fields[idx];

        if !matches!(frame.tracker, Tracker::Struct { .. }) {
            frame.tracker = Tracker::Struct {
                iset: ISet::new(struct_type.fields.len()),
                current_child: None,
            }
        }

        let was_field_init = match &mut frame.tracker {
            Tracker::Struct {
                iset,
                current_child,
            } => {
                *current_child = Some(idx);
                iset.get(idx)
            }
            _ => unreachable!(),
        };

        // Push a new frame for this field onto the frames stack.
        let field_ptr = unsafe { frame.data.field_uninit_at(field.offset) };
        let field_shape = field.shape();

        let mut next_frame = Frame::new(field_ptr, field_shape, FrameOwnership::Field);
        if was_field_init {
            unsafe {
                // the struct field tracker said so!
                next_frame.mark_as_init();
            }
        }

        Ok(next_frame)
    }

    /// Selects the nth element of an array by index
    fn begin_nth_array_element(
        frame: &mut Frame,
        array_type: ArrayType,
        idx: usize,
    ) -> Result<Frame, ReflectError> {
        if idx >= array_type.n {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "array index out of bounds",
            });
        }

        if array_type.n > 63 {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "arrays larger than 63 elements are not yet supported",
            });
        }

        // Ensure frame is in Array state
        match &frame.tracker {
            Tracker::Uninit => {
                // this is fine
                frame.tracker = Tracker::Array {
                    iset: ISet::default(),
                    current_child: None,
                };
            }
            Tracker::Array { .. } => {
                // fine too
            }
            _other => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "unexpected tracker state: expected Uninit or Array",
                });
            }
        }

        match &mut frame.tracker {
            Tracker::Array {
                iset,
                current_child,
            } => {
                *current_child = Some(idx);
                let was_field_init = iset.get(idx);

                // Calculate the offset for this array element
                let Ok(element_layout) = array_type.t.layout.sized_layout() else {
                    return Err(ReflectError::Unsized {
                        shape: array_type.t,
                        operation: "begin_nth_element, calculating array element offset",
                    });
                };
                let offset = element_layout.size() * idx;
                let element_data = unsafe { frame.data.field_uninit_at(offset) };

                let mut next_frame = Frame::new(element_data, array_type.t, FrameOwnership::Field);
                if was_field_init {
                    // safety: `iset` said it was initialized already
                    unsafe {
                        next_frame.mark_as_init();
                    }
                }
                Ok(next_frame)
            }
            _ => unreachable!(),
        }
    }

    /// Selects the nth field of an enum variant by index
    fn begin_nth_enum_field(
        frame: &mut Frame,
        variant: &'static Variant,
        idx: usize,
    ) -> Result<Frame, ReflectError> {
        if idx >= variant.data.fields.len() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "enum field index out of bounds",
            });
        }

        let field = &variant.data.fields[idx];

        // Update tracker
        let was_field_init = match &mut frame.tracker {
            Tracker::Enum {
                data,
                current_child,
                ..
            } => {
                *current_child = Some(idx);
                data.get(idx)
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "selecting a field on an enum requires selecting a variant first",
                });
            }
        };

        // SAFETY: the field offset comes from an unsafe impl of the Facet trait, we trust it.
        // also, we checked that the variant was selected.
        let field_ptr = unsafe { frame.data.field_uninit_at(field.offset) };
        let field_shape = field.shape();

        let mut next_frame = Frame::new(field_ptr, field_shape, FrameOwnership::Field);
        if was_field_init {
            // SAFETY: `ISet` told us the field was initialized
            unsafe {
                next_frame.mark_as_init();
            }
        }

        Ok(next_frame)
    }

    /// Require that the partial is active
    #[inline]
    pub(crate) fn require_active(&self) -> Result<(), ReflectError> {
        if self.state == PartialState::Active {
            Ok(())
        } else {
            Err(ReflectError::InvariantViolation {
                invariant: "Cannot use Partial after it has been built or poisoned",
            })
        }
    }
}
