//! Partial value construction for dynamic reflection
//!
//! This module provides APIs for incrementally building values through reflection,
//! particularly useful when deserializing data from external formats like JSON or YAML.
//!
//! # Overview
//!
//! The `Partial` type (formerly known as `Wip` - Work In Progress) allows you to:
//! - Allocate memory for a value based on its `Shape`
//! - Initialize fields incrementally in a type-safe manner
//! - Handle complex nested structures including structs, enums, collections, and smart pointers
//! - Build the final value once all required fields are initialized
//!
//! # Basic Usage
//!
//! ```no_run
//! # use facet_reflect::Partial;
//! # use facet_core::{Shape, Facet};
//! # fn example<T: Facet<'static>>() -> Result<(), Box<dyn std::error::Error>> {
//! // Allocate memory for a struct
//! let mut partial = Partial::alloc::<T>()?;
//!
//! // Set simple fields
//! partial.set_field("name", "Alice")?;
//! partial.set_field("age", 30u32)?;
//!
//! // Work with nested structures
//! partial.begin_field("address")?;
//! partial.set_field("street", "123 Main St")?;
//! partial.set_field("city", "Springfield")?;
//! partial.end()?;
//!
//! // Build the final value
//! let value = partial.build()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Chaining Style
//!
//! The API supports method chaining for cleaner code:
//!
//! ```no_run
//! # use facet_reflect::Partial;
//! # use facet_core::{Shape, Facet};
//! # fn example<T: Facet<'static>>() -> Result<(), Box<dyn std::error::Error>> {
//! let value = Partial::alloc::<T>()?
//!     .set_field("name", "Bob")?
//!     .begin_field("scores")?
//!         .set(vec![95, 87, 92])?
//!     .end()?
//!     .build()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Working with Collections
//!
//! ```no_run
//! # use facet_reflect::Partial;
//! # use facet_core::{Shape, Facet};
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut partial = Partial::alloc::<Vec<String>>()?;
//!
//! // Add items to a list
//! partial.begin_list_item()?;
//! partial.set("first")?;
//! partial.end()?;
//!
//! partial.begin_list_item()?;
//! partial.set("second")?;
//! partial.end()?;
//!
//! let vec = partial.build()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Working with Maps
//!
//! ```no_run
//! # use facet_reflect::Partial;
//! # use facet_core::{Shape, Facet};
//! # use std::collections::HashMap;
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut partial = Partial::alloc::<HashMap<String, i32>>()?;
//!
//! // Insert key-value pairs
//! partial.begin_key()?;
//! partial.set("score")?;
//! partial.end()?;
//! partial.begin_value()?;
//! partial.set(100i32)?;
//! partial.end()?;
//!
//! let map = partial.build()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Safety and Memory Management
//!
//! The `Partial` type ensures memory safety by:
//! - Tracking initialization state of all fields
//! - Preventing use-after-build through state tracking
//! - Properly handling drop semantics for partially initialized values
//! - Supporting both owned and borrowed values through lifetime parameters

use alloc::vec::Vec;

#[cfg(test)]
mod tests;

mod iset;

mod partial_api;

mod typed;
pub use typed::*;

use crate::{ReflectError, TrackerKind, trace};

use core::{marker::PhantomData, ptr::NonNull};

mod heap_value;
pub use heap_value::*;

use facet_core::{
    Def, EnumType, Field, PtrMut, PtrUninit, Shape, SliceBuilderVTable, Type, UserType, Variant,
};
use iset::ISet;

/// State of a partial value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartialState {
    /// Partial is active and can be modified
    Active,

    /// Partial has been successfully built and cannot be reused
    Built,

    /// Building failed and Partial is poisoned
    BuildFailed,
}

/// A type-erased, heap-allocated, partially-initialized value.
///
/// [Partial] keeps track of the state of initialiation of the underlying
/// value: if we're building `struct S { a: u32, b: String }`, we may
/// have initialized `a`, or `b`, or both, or neither.
///
/// [Partial] allows navigating down nested structs and initializing them
/// progressively: [Partial::begin_field] pushes a frame onto the stack,
/// which then has to be initialized, and popped off with [Partial::end].
///
/// If [Partial::end] is called but the current frame isn't fully initialized,
/// an error is returned: in other words, if you navigate down to a field,
/// you have to fully initialize it one go. You can't go back up and back down
/// to it again.
///
/// You might be interested in [TypedPartial] as well, which carries a generic
/// type parameter to make [TypedPartial::build] type-safe. However, when
/// implementing deserializers for example, if you want to avoid monomorphization,
/// you might want to work with [Partial] directly.
pub struct Partial<'facet> {
    /// stack of frames to keep track of deeply nested initialization
    frames: Vec<Frame>,

    /// current state of the Partial
    state: PartialState,

    invariant: PhantomData<fn(&'facet ()) -> &'facet ()>,
}

#[derive(Clone, Copy, Debug)]
enum MapInsertState {
    /// Not currently inserting
    Idle,

    /// Pushing key
    PushingKey {
        /// Temporary storage for the key being built
        key_ptr: Option<PtrUninit<'static>>,
    },

    /// Pushing value after key is done
    PushingValue {
        /// Temporary storage for the key that was built
        key_ptr: PtrUninit<'static>,
        /// Temporary storage for the value being built
        value_ptr: Option<PtrUninit<'static>>,
    },
}

#[derive(Debug)]
enum FrameOwnership {
    /// This frame owns the allocation and should deallocate it on drop
    Owned,

    /// This frame is a field pointer into a parent allocation
    Field,

    /// This frame's allocation is managed elsewhere (e.g., in MapInsertState)
    ManagedElsewhere,
}

/// Points somewhere in a partially-initialized value. If we're initializing
/// `a.b.c`, then the first frame would point to the beginning of `a`, the
/// second to the beginning of the `b` field of `a`, etc.
///
/// A frame can point to a complex data structure, like a struct or an enum:
/// it keeps track of whether a variant was selected, which fields are initialized,
/// etc. and is able to drop & deinitialize
#[must_use]
struct Frame {
    /// Address of the value being initialized
    data: PtrUninit<'static>,

    /// Shape of the value being initialized
    shape: &'static Shape,

    /// Tracks initialized fields
    tracker: Tracker,

    /// Whether this frame owns the allocation or is just a field pointer
    ownership: FrameOwnership,

    /// Whether this frame is for a custom deserialization pipeline
    using_custom_deserialization: bool,
}

#[derive(Debug)]
enum Tracker {
    /// Wholly uninitialized
    Uninit,

    /// Wholly initialized
    Init,

    /// Partially initialized array
    Array {
        /// Track which array elements are initialized (up to 63 elements)
        iset: ISet,

        /// If we're pushing another frame, this is set to the array index
        current_child: Option<usize>,
    },

    /// Partially initialized struct/tuple-struct etc.
    Struct {
        /// fields need to be individually tracked — we only
        /// support up to 63 fields.
        iset: ISet,

        /// if we're pushing another frame, this is set to the
        /// index of the struct field
        current_child: Option<usize>,
    },

    /// Smart pointer being initialized
    SmartPointer {
        /// Whether the inner value has been initialized
        is_initialized: bool,
    },

    /// We're initializing an `Arc<[T]>`, `Box<[T]>`, `Rc<[T]>`, etc.
    ///
    /// We're using the slice builder API to construct the slice
    SmartPointerSlice {
        /// The slice builder vtable
        vtable: &'static SliceBuilderVTable,

        /// Whether we're currently building an item to push
        building_item: bool,
    },

    /// Partially initialized enum (but we picked a variant,
    /// so it's not Uninit)
    Enum {
        /// Variant chosen for the enum
        variant: &'static Variant,

        /// tracks enum fields (for the given variant)
        data: ISet,

        /// If we're pushing another frame, this is set to the field index
        current_child: Option<usize>,
    },

    /// Partially initialized list (Vec, etc.)
    List {
        /// The list has been initialized with capacity
        is_initialized: bool,
        /// If we're pushing another frame for an element
        current_child: bool,
    },

    /// Partially initialized map (HashMap, BTreeMap, etc.)
    Map {
        /// The map has been initialized with capacity
        is_initialized: bool,
        /// State of the current insertion operation
        insert_state: MapInsertState,
    },

    /// Option being initialized with Some(inner_value)
    Option {
        /// Whether we're currently building the inner value
        building_inner: bool,
    },
}

impl Tracker {
    fn kind(&self) -> TrackerKind {
        match self {
            Tracker::Uninit => TrackerKind::Uninit,
            Tracker::Init => TrackerKind::Init,
            Tracker::Array { .. } => TrackerKind::Array,
            Tracker::Struct { .. } => TrackerKind::Struct,
            Tracker::SmartPointer { .. } => TrackerKind::SmartPointer,
            Tracker::SmartPointerSlice { .. } => TrackerKind::SmartPointerSlice,
            Tracker::Enum { .. } => TrackerKind::Enum,
            Tracker::List { .. } => TrackerKind::List,
            Tracker::Map { .. } => TrackerKind::Map,
            Tracker::Option { .. } => TrackerKind::Option,
        }
    }
}

impl Frame {
    fn new(data: PtrUninit<'static>, shape: &'static Shape, ownership: FrameOwnership) -> Self {
        // For empty structs (structs with 0 fields), start as Init since there's nothing to initialize
        // This includes empty tuples () which are zero-sized types with no fields to initialize
        let tracker = match shape.ty {
            Type::User(UserType::Struct(struct_type)) if struct_type.fields.is_empty() => {
                Tracker::Init
            }
            _ => Tracker::Uninit,
        };

        Self {
            data,
            shape,
            tracker,
            ownership,
            using_custom_deserialization: false,
        }
    }

    /// Deinitialize any initialized field: calls `drop_in_place` but does not free any
    /// memory even if the frame owns that memory.
    ///
    /// After this call, the [Tracker] should be back to [Tracker::Uninit]
    fn deinit(&mut self) {
        match &self.tracker {
            Tracker::Uninit => {
                // Nothing was initialized, nothing to drop
            }
            Tracker::Init => {
                // Fully initialized, drop it
                if let Some(drop_fn) = self.shape.vtable.drop_in_place {
                    unsafe { drop_fn(self.data.assume_init()) };
                }
            }
            Tracker::Array { iset, .. } => {
                // Drop initialized array elements
                if let Type::Sequence(facet_core::SequenceType::Array(array_def)) = self.shape.ty {
                    let element_layout = array_def.t.layout.sized_layout().ok();
                    if let Some(layout) = element_layout {
                        for idx in 0..array_def.n {
                            if iset.get(idx) {
                                let offset = layout.size() * idx;
                                let element_ptr = unsafe { self.data.field_init_at(offset) };
                                if let Some(drop_fn) = array_def.t.vtable.drop_in_place {
                                    unsafe { drop_fn(element_ptr) };
                                }
                            }
                        }
                    }
                }
            }
            Tracker::Struct { iset, .. } => {
                // Drop initialized struct fields
                if let Type::User(UserType::Struct(struct_type)) = self.shape.ty {
                    if iset.all_set() && self.shape.vtable.drop_in_place.is_some() {
                        unsafe {
                            (self.shape.vtable.drop_in_place.unwrap())(self.data.assume_init())
                        };
                    } else {
                        for (idx, field) in struct_type.fields.iter().enumerate() {
                            if iset.get(idx) {
                                // This field was initialized, drop it
                                let field_ptr = unsafe { self.data.field_init_at(field.offset) };
                                if let Some(drop_fn) = field.shape().vtable.drop_in_place {
                                    unsafe { drop_fn(field_ptr) };
                                }
                            }
                        }
                    }
                }
            }
            Tracker::Enum { variant, data, .. } => {
                // Drop initialized enum variant fields
                for (idx, field) in variant.data.fields.iter().enumerate() {
                    if data.get(idx) {
                        // This field was initialized, drop it
                        let field_ptr = unsafe { self.data.as_mut_byte_ptr().add(field.offset) };
                        if let Some(drop_fn) = field.shape().vtable.drop_in_place {
                            unsafe { drop_fn(PtrMut::new(NonNull::new_unchecked(field_ptr))) };
                        }
                    }
                }
            }
            Tracker::SmartPointer { is_initialized } => {
                // Drop the initialized Box
                if *is_initialized {
                    if let Some(drop_fn) = self.shape.vtable.drop_in_place {
                        unsafe { drop_fn(self.data.assume_init()) };
                    }
                }
                // Note: we don't deallocate the inner value here because
                // the Box's drop will handle that
            }
            Tracker::SmartPointerSlice { vtable, .. } => {
                // Free the slice builder
                let builder_ptr = unsafe { self.data.assume_init() };
                unsafe {
                    (vtable.free_fn)(builder_ptr);
                }
            }
            Tracker::List { is_initialized, .. } => {
                // Drop the initialized List
                if *is_initialized {
                    if let Some(drop_fn) = self.shape.vtable.drop_in_place {
                        unsafe { drop_fn(self.data.assume_init()) };
                    }
                }
            }
            Tracker::Map {
                is_initialized,
                insert_state,
            } => {
                // Drop the initialized Map
                if *is_initialized {
                    if let Some(drop_fn) = self.shape.vtable.drop_in_place {
                        unsafe { drop_fn(self.data.assume_init()) };
                    }
                }

                // Clean up any in-progress insertion state
                match insert_state {
                    MapInsertState::PushingKey { key_ptr } => {
                        if let Some(key_ptr) = key_ptr {
                            // Deallocate the key buffer
                            if let Def::Map(map_def) = self.shape.def {
                                if let Ok(key_shape) = map_def.k().layout.sized_layout() {
                                    if key_shape.size() > 0 {
                                        unsafe {
                                            alloc::alloc::dealloc(
                                                key_ptr.as_mut_byte_ptr(),
                                                key_shape,
                                            )
                                        };
                                    }
                                }
                            }
                        }
                    }
                    MapInsertState::PushingValue { key_ptr, value_ptr } => {
                        // Drop and deallocate both key and value buffers
                        if let Def::Map(map_def) = self.shape.def {
                            // Drop and deallocate the key
                            if let Some(drop_fn) = map_def.k().vtable.drop_in_place {
                                unsafe { drop_fn(key_ptr.assume_init()) };
                            }
                            if let Ok(key_shape) = map_def.k().layout.sized_layout() {
                                if key_shape.size() > 0 {
                                    unsafe {
                                        alloc::alloc::dealloc(key_ptr.as_mut_byte_ptr(), key_shape)
                                    };
                                }
                            }

                            // Drop and deallocate the value if it exists
                            if let Some(value_ptr) = value_ptr {
                                if let Ok(value_shape) = map_def.v().layout.sized_layout() {
                                    if value_shape.size() > 0 {
                                        unsafe {
                                            alloc::alloc::dealloc(
                                                value_ptr.as_mut_byte_ptr(),
                                                value_shape,
                                            )
                                        };
                                    }
                                }
                            }
                        }
                    }
                    MapInsertState::Idle => {}
                }
            }
            Tracker::Option { building_inner } => {
                // If we're building the inner value, it will be handled by the Option vtable
                // No special cleanup needed here as the Option will either be properly
                // initialized or remain uninitialized
                if !building_inner {
                    // Option is fully initialized, drop it normally
                    if let Some(drop_fn) = self.shape.vtable.drop_in_place {
                        unsafe { drop_fn(self.data.assume_init()) };
                    }
                }
            }
        }

        self.tracker = Tracker::Uninit;
    }

    /// This must be called after (fully) initializing a value.
    ///
    /// This will most often result in a transition to [Tracker::Init] although
    /// composite types (structs, enums, etc.) might be handled differently
    ///
    /// # Safety
    ///
    /// This should only be called when `self.data` has been actually initialized.
    unsafe fn mark_as_init(&mut self) {
        self.tracker = Tracker::Init;
    }

    /// Deallocate the memory associated with this frame, if it owns it.
    ///
    /// The memory has to be deinitialized first, see [Frame::deinit]
    fn dealloc(self) {
        if !matches!(self.tracker, Tracker::Uninit) {
            unreachable!("a frame has to be deinitialized before being deallocated")
        }

        // Now, deallocate temporary String allocation if necessary
        if let FrameOwnership::Owned = self.ownership {
            if let Ok(layout) = self.shape.layout.sized_layout() {
                if layout.size() > 0 {
                    unsafe { alloc::alloc::dealloc(self.data.as_mut_byte_ptr(), layout) };
                }
            }
            // no need to update `self.ownership` since `self` drops at the end of this
        }
    }

    /// Returns an error if the value is not fully initialized
    fn require_full_initialization(&self) -> Result<(), ReflectError> {
        match self.tracker {
            Tracker::Uninit => Err(ReflectError::UninitializedValue { shape: self.shape }),
            Tracker::Init => Ok(()),
            Tracker::Array { iset, .. } => {
                match self.shape.ty {
                    Type::Sequence(facet_core::SequenceType::Array(array_def)) => {
                        // Check if all array elements are initialized
                        if (0..array_def.n).all(|idx| iset.get(idx)) {
                            Ok(())
                        } else {
                            Err(ReflectError::UninitializedValue { shape: self.shape })
                        }
                    }
                    _ => Err(ReflectError::UninitializedValue { shape: self.shape }),
                }
            }
            Tracker::Struct { iset, .. } => {
                if iset.all_set() {
                    Ok(())
                } else {
                    // Attempt to find the first uninitialized field, if possible
                    match self.shape.ty {
                        Type::User(UserType::Struct(struct_type)) => {
                            // Find index of the first bit not set
                            let first_missing_idx =
                                (0..struct_type.fields.len()).find(|&idx| !iset.get(idx));
                            if let Some(missing_idx) = first_missing_idx {
                                let field_name = struct_type.fields[missing_idx].name;
                                Err(ReflectError::UninitializedField {
                                    shape: self.shape,
                                    field_name,
                                })
                            } else {
                                // fallback, something went wrong
                                Err(ReflectError::UninitializedValue { shape: self.shape })
                            }
                        }
                        _ => Err(ReflectError::UninitializedValue { shape: self.shape }),
                    }
                }
            }
            Tracker::Enum { variant, data, .. } => {
                // Check if all fields of the variant are initialized
                let num_fields = variant.data.fields.len();
                if num_fields == 0 {
                    // Unit variant, always initialized
                    Ok(())
                } else if (0..num_fields).all(|idx| data.get(idx)) {
                    Ok(())
                } else {
                    // Find the first uninitialized field
                    let first_missing_idx = (0..num_fields).find(|&idx| !data.get(idx));
                    if let Some(missing_idx) = first_missing_idx {
                        let field_name = variant.data.fields[missing_idx].name;
                        Err(ReflectError::UninitializedEnumField {
                            shape: self.shape,
                            field_name,
                            variant_name: variant.name,
                        })
                    } else {
                        Err(ReflectError::UninitializedValue { shape: self.shape })
                    }
                }
            }
            Tracker::SmartPointer { is_initialized } => {
                if is_initialized {
                    Ok(())
                } else {
                    Err(ReflectError::UninitializedValue { shape: self.shape })
                }
            }
            Tracker::SmartPointerSlice { building_item, .. } => {
                if building_item {
                    Err(ReflectError::UninitializedValue { shape: self.shape })
                } else {
                    Ok(())
                }
            }
            Tracker::List { is_initialized, .. } => {
                if is_initialized {
                    Ok(())
                } else {
                    Err(ReflectError::UninitializedValue { shape: self.shape })
                }
            }
            Tracker::Map {
                is_initialized,
                insert_state,
            } => {
                if is_initialized && matches!(insert_state, MapInsertState::Idle) {
                    Ok(())
                } else {
                    Err(ReflectError::UninitializedValue { shape: self.shape })
                }
            }
            Tracker::Option { building_inner } => {
                if building_inner {
                    Err(ReflectError::UninitializedValue { shape: self.shape })
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Get the [EnumType] of the frame's shape, if it is an enum type
    pub(crate) fn get_enum_type(&self) -> Result<EnumType, ReflectError> {
        match self.shape.ty {
            Type::User(UserType::Enum(e)) => Ok(e),
            _ => Err(ReflectError::WasNotA {
                expected: "enum",
                actual: self.shape,
            }),
        }
    }

    pub(crate) fn get_field(&self) -> Option<&Field> {
        match self.shape.ty {
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => {
                    // Try to get currently active field index
                    if let Tracker::Struct {
                        current_child: Some(idx),
                        ..
                    } = &self.tracker
                    {
                        struct_type.fields.get(*idx)
                    } else {
                        None
                    }
                }
                UserType::Enum(_enum_type) => {
                    if let Tracker::Enum {
                        variant,
                        current_child: Some(idx),
                        ..
                    } = &self.tracker
                    {
                        variant.data.fields.get(*idx)
                    } else {
                        None
                    }
                }
                _ => None,
            },
            _ => None,
        }
    }
}

impl<'facet> Drop for Partial<'facet> {
    fn drop(&mut self) {
        trace!("🧹 Partial is being dropped");

        // We need to properly drop all initialized fields
        while let Some(mut frame) = self.frames.pop() {
            frame.deinit();

            // Only deallocate if this frame owns the allocation
            if let FrameOwnership::Owned = frame.ownership {
                if let Ok(layout) = frame.shape.layout.sized_layout() {
                    if layout.size() > 0 {
                        unsafe { alloc::alloc::dealloc(frame.data.as_mut_byte_ptr(), layout) };
                    }
                }
            }
        }
    }
}
