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
//! **Note**: This is the only API for partial value construction. The previous `TypedPartial`
//! wrapper has been removed in favor of using `Partial` directly.
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
//! partial = partial.set_field("name", "Alice")?;
//! partial = partial.set_field("age", 30u32)?;
//!
//! // Work with nested structures
//! partial = partial.begin_field("address")?;
//! partial = partial.set_field("street", "123 Main St")?;
//! partial = partial.set_field("city", "Springfield")?;
//! partial = partial.end()?;
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
//! partial = partial.begin_list_item()?;
//! partial = partial.set("first")?;
//! partial = partial.end()?;
//!
//! partial = partial.begin_list_item()?;
//! partial = partial.set("second")?;
//! partial = partial.end()?;
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
//! partial = partial.begin_key()?;
//! partial = partial.set("score")?;
//! partial = partial.end()?;
//! partial = partial.begin_value()?;
//! partial = partial.set(100i32)?;
//! partial = partial.end()?;
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

use alloc::{collections::BTreeMap, vec::Vec};

mod iset;
pub mod typeplan;

mod partial_api;

use crate::{KeyPath, ReflectErrorKind, TrackerKind, trace};

use core::marker::PhantomData;

mod heap_value;
pub use heap_value::*;

use facet_core::{
    Def, EnumType, Field, PtrMut, PtrUninit, Shape, SliceBuilderVTable, Type, UserType, Variant,
};
use iset::ISet;
use typeplan::{FieldDefault, FieldInitPlan, FillRule};

/// State of a partial value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartialState {
    /// Partial is active and can be modified
    Active,

    /// Partial has been successfully built and cannot be reused
    Built,
}

/// Mode of operation for frame management.
///
/// In `Strict` mode, frames must be fully initialized before being popped.
/// In `Deferred` mode, frames can be stored when popped and restored on re-entry,
/// with final validation happening in `finish_deferred()`.
enum FrameMode {
    /// Strict mode: frames must be fully initialized before popping.
    Strict {
        /// Stack of frames for nested initialization.
        stack: Vec<Frame>,
    },

    /// Deferred mode: frames are stored when popped, can be re-entered.
    Deferred {
        /// Stack of frames for nested initialization.
        stack: Vec<Frame>,

        /// The frame depth when deferred mode was started.
        /// Path calculations are relative to this depth.
        start_depth: usize,

        /// Current path as we navigate (e.g., ["inner", "x"]).
        // TODO: Intern key paths to avoid repeated allocations. The Resolution
        // already knows all possible paths, so we could use indices into that.
        current_path: KeyPath,

        /// Frames saved when popped, keyed by their path.
        /// When we re-enter a path, we restore the stored frame.
        // TODO: Consider using path indices instead of cloned KeyPaths as keys.
        stored_frames: BTreeMap<KeyPath, Frame>,
    },
}

impl FrameMode {
    /// Get a reference to the frame stack.
    const fn stack(&self) -> &Vec<Frame> {
        match self {
            FrameMode::Strict { stack } | FrameMode::Deferred { stack, .. } => stack,
        }
    }

    /// Get a mutable reference to the frame stack.
    const fn stack_mut(&mut self) -> &mut Vec<Frame> {
        match self {
            FrameMode::Strict { stack } | FrameMode::Deferred { stack, .. } => stack,
        }
    }

    /// Check if we're in deferred mode.
    const fn is_deferred(&self) -> bool {
        matches!(self, FrameMode::Deferred { .. })
    }

    /// Get the start depth if in deferred mode.
    const fn start_depth(&self) -> Option<usize> {
        match self {
            FrameMode::Deferred { start_depth, .. } => Some(*start_depth),
            FrameMode::Strict { .. } => None,
        }
    }

    /// Get the current path if in deferred mode.
    const fn current_path(&self) -> Option<&KeyPath> {
        match self {
            FrameMode::Deferred { current_path, .. } => Some(current_path),
            FrameMode::Strict { .. } => None,
        }
    }
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
pub struct Partial<'facet, const BORROW: bool = true> {
    /// Frame management mode (strict or deferred) and associated state.
    mode: FrameMode,

    /// current state of the Partial
    state: PartialState,

    /// Precomputed deserialization plan for the root type.
    /// Built once at allocation time, navigated in parallel with value construction.
    /// Each Frame holds a pointer into this tree.
    root_plan: alloc::boxed::Box<typeplan::TypePlan>,

    invariant: PhantomData<fn(&'facet ()) -> &'facet ()>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum MapInsertState {
    /// Not currently inserting
    Idle,

    /// Pushing key - memory allocated, waiting for initialization
    PushingKey {
        /// Temporary storage for the key being built
        key_ptr: PtrUninit,
        /// Whether the key has been fully initialized
        key_initialized: bool,
        /// Whether the key's TrackedBuffer frame is still on the stack.
        /// When true, the frame handles cleanup. When false (after end()),
        /// the Map tracker owns the buffer and must clean it up.
        key_frame_on_stack: bool,
    },

    /// Pushing value after key is done
    PushingValue {
        /// Temporary storage for the key that was built (always initialized)
        key_ptr: PtrUninit,
        /// Temporary storage for the value being built
        value_ptr: Option<PtrUninit>,
        /// Whether the value has been fully initialized
        value_initialized: bool,
        /// Whether the value's TrackedBuffer frame is still on the stack.
        /// When true, the frame handles cleanup. When false (after end()),
        /// the Map tracker owns the buffer and must clean it up.
        value_frame_on_stack: bool,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum FrameOwnership {
    /// This frame owns the allocation and should deallocate it on drop
    Owned,

    /// This frame points to a field/element within a parent's allocation.
    /// The parent's `iset[field_idx]` was CLEARED when this frame was created.
    /// On drop: deinit if initialized, but do NOT deallocate.
    /// On successful end(): parent's `iset[field_idx]` will be SET.
    Field { field_idx: usize },

    /// Temporary buffer tracked by parent's MapInsertState.
    /// Used by begin_key(), begin_value() for map insertions.
    /// Safe to drop on deinit - parent's cleanup respects is_init propagation.
    TrackedBuffer,

    /// Pointer into existing collection entry (Value object, Option inner, etc.)
    /// Used by begin_object_entry() on existing key, begin_some() re-entry.
    /// NOT safe to drop on deinit - parent collection has no per-entry tracking
    /// and would try to drop the freed value again (double-free).
    BorrowedInPlace,

    /// Pointer to externally-owned memory (e.g., caller's stack via MaybeUninit).
    /// Used by `from_raw()` for stack-friendly deserialization.
    /// On drop: deinit if initialized (drop partially constructed values), but do NOT deallocate.
    /// The caller owns the memory and is responsible for its lifetime.
    External,

    /// Points directly into a Vec's reserved buffer space.
    /// Used by `begin_list_item()` for direct-fill optimization.
    /// On successful end(): parent calls `set_len(len + 1)` instead of `push`.
    /// On drop/failure: no dealloc (memory belongs to Vec), no `set_len` (element not complete).
    ListSlot,
}

impl FrameOwnership {
    /// Returns true if this frame is responsible for deallocating its memory.
    ///
    /// Both `Owned` and `TrackedBuffer` frames allocated their memory and need
    /// to deallocate it. `Field`, `BorrowedInPlace`, and `External` frames borrow from
    /// parent, existing structures, or caller-provided memory.
    const fn needs_dealloc(&self) -> bool {
        matches!(self, FrameOwnership::Owned | FrameOwnership::TrackedBuffer)
    }
}

/// Immutable pairing of a shape with its actual allocation size.
///
/// This ensures that the shape and allocated size are always in sync and cannot
/// drift apart, preventing the class of bugs where a frame's shape doesn't match
/// what was actually allocated (see issue #1568).
pub(crate) struct AllocatedShape {
    shape: &'static Shape,
    allocated_size: usize,
}

impl AllocatedShape {
    pub(crate) const fn new(shape: &'static Shape, allocated_size: usize) -> Self {
        Self {
            shape,
            allocated_size,
        }
    }

    pub(crate) const fn shape(&self) -> &'static Shape {
        self.shape
    }

    pub(crate) const fn allocated_size(&self) -> usize {
        self.allocated_size
    }
}

/// Points somewhere in a partially-initialized value. If we're initializing
/// `a.b.c`, then the first frame would point to the beginning of `a`, the
/// second to the beginning of the `b` field of `a`, etc.
///
/// A frame can point to a complex data structure, like a struct or an enum:
/// it keeps track of whether a variant was selected, which fields are initialized,
/// etc. and is able to drop & deinitialize
#[must_use]
pub(crate) struct Frame {
    /// Address of the value being initialized
    pub(crate) data: PtrUninit,

    /// Shape of the value being initialized, paired with the actual allocation size
    pub(crate) allocated: AllocatedShape,

    /// Whether this frame's data is fully initialized
    pub(crate) is_init: bool,

    /// Tracks building mode and partial initialization state
    pub(crate) tracker: Tracker,

    /// Whether this frame owns the allocation or is just a field pointer
    pub(crate) ownership: FrameOwnership,

    /// Whether this frame is for a custom deserialization pipeline
    pub(crate) using_custom_deserialization: bool,

    /// Container-level proxy definition (from `#[facet(proxy = ...)]` on the shape).
    /// Used during custom deserialization to convert from proxy type to target type.
    pub(crate) shape_level_proxy: Option<&'static facet_core::ProxyDef>,

    /// NodeId into the precomputed TypePlan for this frame's type.
    /// This is navigated in parallel with the value - when we begin_nth_field,
    /// the new frame gets the NodeId for that field's child plan.
    /// The TypePlan arena is owned by Partial and lives as long as the Partial.
    /// Always present - TypePlan is built for what we actually deserialize into
    /// (including proxies).
    pub(crate) type_plan: typeplan::NodeId,
}

#[derive(Debug)]
pub(crate) enum Tracker {
    /// Simple scalar value - no partial initialization tracking needed.
    /// Whether it's initialized is tracked by `Frame::is_init`.
    Scalar,

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
        /// if we're pushing another frame, this is set to the index of the struct field
        current_child: Option<usize>,
    },

    /// Smart pointer being initialized.
    /// Whether it's initialized is tracked by `Frame::is_init`.
    SmartPointer,

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
        /// Index of the variant in the enum's variants array
        variant_idx: usize,
        /// tracks enum fields (for the given variant)
        data: ISet,
        /// If we're pushing another frame, this is set to the field index
        current_child: Option<usize>,
    },

    /// Partially initialized list (Vec, etc.)
    /// Whether it's initialized is tracked by `Frame::is_init`.
    List {
        /// If we're pushing another frame for an element, this is the element index
        current_child: Option<usize>,
    },

    /// Partially initialized map (HashMap, BTreeMap, etc.)
    /// Whether it's initialized is tracked by `Frame::is_init`.
    Map {
        /// State of the current insertion operation
        insert_state: MapInsertState,
    },

    /// Partially initialized set (HashSet, BTreeSet, etc.)
    /// Whether it's initialized is tracked by `Frame::is_init`.
    Set {
        /// If we're pushing another frame for an element
        current_child: bool,
    },

    /// Option being initialized with Some(inner_value)
    Option {
        /// Whether we're currently building the inner value
        building_inner: bool,
    },

    /// Result being initialized with Ok or Err
    Result {
        /// Whether we're building Ok (true) or Err (false)
        is_ok: bool,
        /// Whether we're currently building the inner value
        building_inner: bool,
    },

    /// Dynamic value (e.g., facet_value::Value) being initialized
    DynamicValue {
        /// What kind of dynamic value we're building
        state: DynamicValueState,
    },
}

/// State for building a dynamic value
#[derive(Debug)]
#[allow(dead_code)] // Some variants are for future use (object support)
pub(crate) enum DynamicValueState {
    /// Not yet initialized - will be set to scalar, array, or object
    Uninit,
    /// Initialized as a scalar (null, bool, number, string, bytes)
    Scalar,
    /// Initialized as an array, currently building an element
    Array { building_element: bool },
    /// Initialized as an object
    Object {
        insert_state: DynamicObjectInsertState,
    },
}

/// State for inserting into a dynamic object
#[derive(Debug)]
#[allow(dead_code)] // For future use (object support)
pub(crate) enum DynamicObjectInsertState {
    /// Idle - ready for a new key-value pair
    Idle,
    /// Currently building the value for a key
    BuildingValue {
        /// The key for the current entry
        key: alloc::string::String,
    },
}

impl Tracker {
    const fn kind(&self) -> TrackerKind {
        match self {
            Tracker::Scalar => TrackerKind::Scalar,
            Tracker::Array { .. } => TrackerKind::Array,
            Tracker::Struct { .. } => TrackerKind::Struct,
            Tracker::SmartPointer => TrackerKind::SmartPointer,
            Tracker::SmartPointerSlice { .. } => TrackerKind::SmartPointerSlice,
            Tracker::Enum { .. } => TrackerKind::Enum,
            Tracker::List { .. } => TrackerKind::List,
            Tracker::Map { .. } => TrackerKind::Map,
            Tracker::Set { .. } => TrackerKind::Set,
            Tracker::Option { .. } => TrackerKind::Option,
            Tracker::Result { .. } => TrackerKind::Result,
            Tracker::DynamicValue { .. } => TrackerKind::DynamicValue,
        }
    }

    /// Set the current_child index for trackers that support it
    const fn set_current_child(&mut self, idx: usize) {
        match self {
            Tracker::Struct { current_child, .. }
            | Tracker::Enum { current_child, .. }
            | Tracker::Array { current_child, .. } => {
                *current_child = Some(idx);
            }
            _ => {}
        }
    }

    /// Clear the current_child index for trackers that support it
    const fn clear_current_child(&mut self) {
        match self {
            Tracker::Struct { current_child, .. }
            | Tracker::Enum { current_child, .. }
            | Tracker::Array { current_child, .. } => {
                *current_child = None;
            }
            _ => {}
        }
    }
}

impl Frame {
    fn new(
        data: PtrUninit,
        allocated: AllocatedShape,
        ownership: FrameOwnership,
        type_plan: typeplan::NodeId,
    ) -> Self {
        // For empty structs (structs with 0 fields), start as initialized since there's nothing to initialize
        // This includes empty tuples () which are zero-sized types with no fields to initialize
        let is_init = matches!(
            allocated.shape().ty,
            Type::User(UserType::Struct(struct_type)) if struct_type.fields.is_empty()
        );

        Self {
            data,
            allocated,
            is_init,
            tracker: Tracker::Scalar,
            ownership,
            using_custom_deserialization: false,
            shape_level_proxy: None,
            type_plan,
        }
    }

    /// Deinitialize any initialized field: calls `drop_in_place` but does not free any
    /// memory even if the frame owns that memory.
    ///
    /// After this call, `is_init` will be false and `tracker` will be [Tracker::Scalar].
    fn deinit(&mut self) {
        // For BorrowedInPlace frames, we must NOT drop. These point into existing
        // collection entries (Value objects, Option inners) where the parent has no
        // per-entry tracking. Dropping here would cause double-free when parent drops.
        //
        // For TrackedBuffer frames, we CAN drop. These are temporary buffers where
        // the parent's MapInsertState tracks initialization via is_init propagation.
        if matches!(self.ownership, FrameOwnership::BorrowedInPlace) {
            self.is_init = false;
            self.tracker = Tracker::Scalar;
            return;
        }

        // Field frames are responsible for their value during cleanup.
        // The ownership model ensures no double-free:
        // - begin_field: parent's iset[idx] is cleared (parent relinquishes responsibility)
        // - end: parent's iset[idx] is set (parent reclaims responsibility), frame is popped
        // So if Field frame is still on stack during cleanup, parent's iset[idx] is false,
        // meaning the parent won't drop this field - the Field frame must do it.

        match &self.tracker {
            Tracker::Scalar => {
                // Simple scalar - drop if initialized
                if self.is_init {
                    unsafe {
                        self.allocated
                            .shape()
                            .call_drop_in_place(self.data.assume_init())
                    };
                }
            }
            Tracker::Array { iset, .. } => {
                // Drop initialized array elements
                if let Type::Sequence(facet_core::SequenceType::Array(array_def)) =
                    self.allocated.shape().ty
                {
                    let element_layout = array_def.t.layout.sized_layout().ok();
                    if let Some(layout) = element_layout {
                        for idx in 0..array_def.n {
                            if iset.get(idx) {
                                let offset = layout.size() * idx;
                                let element_ptr = unsafe { self.data.field_init(offset) };
                                unsafe { array_def.t.call_drop_in_place(element_ptr) };
                            }
                        }
                    }
                }
            }
            Tracker::Struct { iset, .. } => {
                // Drop initialized struct fields
                if let Type::User(UserType::Struct(struct_type)) = self.allocated.shape().ty {
                    if iset.all_set(struct_type.fields.len()) {
                        unsafe {
                            self.allocated
                                .shape()
                                .call_drop_in_place(self.data.assume_init())
                        };
                    } else {
                        for (idx, field) in struct_type.fields.iter().enumerate() {
                            if iset.get(idx) {
                                // This field was initialized, drop it
                                let field_ptr = unsafe { self.data.field_init(field.offset) };
                                unsafe { field.shape().call_drop_in_place(field_ptr) };
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
                        let field_ptr = unsafe { self.data.field_init(field.offset) };
                        unsafe { field.shape().call_drop_in_place(field_ptr) };
                    }
                }
            }
            Tracker::SmartPointer => {
                // Drop the initialized Box
                if self.is_init {
                    unsafe {
                        self.allocated
                            .shape()
                            .call_drop_in_place(self.data.assume_init())
                    };
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
            Tracker::List { .. } => {
                // Drop the initialized List
                if self.is_init {
                    unsafe {
                        self.allocated
                            .shape()
                            .call_drop_in_place(self.data.assume_init())
                    };
                }
            }
            Tracker::Map { insert_state } => {
                // Drop the initialized Map
                if self.is_init {
                    unsafe {
                        self.allocated
                            .shape()
                            .call_drop_in_place(self.data.assume_init())
                    };
                }

                // Clean up key/value buffers based on whether their TrackedBuffer frames
                // are still on the stack. If a frame is on the stack, it handles cleanup.
                // If a frame was already popped (via end()), we own the buffer and must clean it.
                match insert_state {
                    MapInsertState::PushingKey {
                        key_ptr,
                        key_initialized,
                        key_frame_on_stack,
                    } => {
                        // Only clean up if the frame was already popped.
                        // If key_frame_on_stack is true, the TrackedBuffer frame above us
                        // will handle dropping and deallocating the key buffer.
                        if !*key_frame_on_stack
                            && let Def::Map(map_def) = self.allocated.shape().def
                        {
                            // Drop the key if it was initialized
                            if *key_initialized {
                                unsafe { map_def.k().call_drop_in_place(key_ptr.assume_init()) };
                            }
                            // Deallocate the key buffer
                            if let Ok(key_layout) = map_def.k().layout.sized_layout()
                                && key_layout.size() > 0
                            {
                                unsafe {
                                    alloc::alloc::dealloc(key_ptr.as_mut_byte_ptr(), key_layout)
                                };
                            }
                        }
                    }
                    MapInsertState::PushingValue {
                        key_ptr,
                        value_ptr,
                        value_initialized,
                        value_frame_on_stack,
                    } => {
                        if let Def::Map(map_def) = self.allocated.shape().def {
                            // Key was already popped (that's how we got to PushingValue state),
                            // so we always own the key buffer and must clean it up.
                            unsafe { map_def.k().call_drop_in_place(key_ptr.assume_init()) };
                            if let Ok(key_layout) = map_def.k().layout.sized_layout()
                                && key_layout.size() > 0
                            {
                                unsafe {
                                    alloc::alloc::dealloc(key_ptr.as_mut_byte_ptr(), key_layout)
                                };
                            }

                            // Only clean up value if the frame was already popped.
                            // If value_frame_on_stack is true, the TrackedBuffer frame above us
                            // will handle dropping and deallocating the value buffer.
                            if !*value_frame_on_stack && let Some(value_ptr) = value_ptr {
                                // Drop the value if it was initialized
                                if *value_initialized {
                                    unsafe {
                                        map_def.v().call_drop_in_place(value_ptr.assume_init())
                                    };
                                }
                                // Deallocate the value buffer
                                if let Ok(value_layout) = map_def.v().layout.sized_layout()
                                    && value_layout.size() > 0
                                {
                                    unsafe {
                                        alloc::alloc::dealloc(
                                            value_ptr.as_mut_byte_ptr(),
                                            value_layout,
                                        )
                                    };
                                }
                            }
                        }
                    }
                    MapInsertState::Idle => {}
                }
            }
            Tracker::Set { .. } => {
                // Drop the initialized Set
                if self.is_init {
                    unsafe {
                        self.allocated
                            .shape()
                            .call_drop_in_place(self.data.assume_init())
                    };
                }
            }
            Tracker::Option { building_inner } => {
                // If we're building the inner value, it will be handled by the Option vtable
                // No special cleanup needed here as the Option will either be properly
                // initialized or remain uninitialized
                if !building_inner {
                    // Option is fully initialized, drop it normally
                    unsafe {
                        self.allocated
                            .shape()
                            .call_drop_in_place(self.data.assume_init())
                    };
                }
            }
            Tracker::Result { building_inner, .. } => {
                // If we're building the inner value, it will be handled by the Result vtable
                // No special cleanup needed here as the Result will either be properly
                // initialized or remain uninitialized
                if !building_inner {
                    // Result is fully initialized, drop it normally
                    unsafe {
                        self.allocated
                            .shape()
                            .call_drop_in_place(self.data.assume_init())
                    };
                }
            }
            Tracker::DynamicValue { .. } => {
                // Drop if initialized
                if self.is_init {
                    let result = unsafe {
                        self.allocated
                            .shape()
                            .call_drop_in_place(self.data.assume_init())
                    };
                    if result.is_none() {
                        // This would be a bug - DynamicValue should always have drop_in_place
                        panic!(
                            "DynamicValue type {} has no drop_in_place implementation",
                            self.allocated.shape()
                        );
                    }
                }
            }
        }

        self.is_init = false;
        self.tracker = Tracker::Scalar;
    }

    /// Deinitialize any initialized value for REPLACEMENT purposes.
    ///
    /// Unlike `deinit()` which is used during error cleanup, this method is used when
    /// we're about to overwrite a value with a new one (e.g., in `set_shape`).
    ///
    /// The difference is important for Field frames with simple trackers:
    /// - During cleanup: parent struct will drop all initialized fields, so Field frames skip dropping
    /// - During replacement: we're about to overwrite, so we MUST drop the old value
    ///
    /// For BorrowedInPlace frames: same logic applies - we must drop when replacing.
    fn deinit_for_replace(&mut self) {
        // For BorrowedInPlace frames, deinit() skips dropping (parent owns on cleanup).
        // But when REPLACING a value, we must drop the old value first.
        if matches!(self.ownership, FrameOwnership::BorrowedInPlace) && self.is_init {
            unsafe {
                self.allocated
                    .shape()
                    .call_drop_in_place(self.data.assume_init());
            }

            // CRITICAL: For DynamicValue (e.g., facet_value::Value), the parent Object's
            // HashMap entry still points to this location. If we just drop and leave garbage,
            // the parent will try to drop that garbage when it's cleaned up, causing
            // use-after-free. We must reinitialize to a safe default (Null) so the parent
            // can safely drop it later.
            if let Def::DynamicValue(dyn_def) = &self.allocated.shape().def {
                unsafe {
                    (dyn_def.vtable.set_null)(self.data);
                }
                // Keep is_init = true since we just initialized it to Null
                self.tracker = Tracker::DynamicValue {
                    state: DynamicValueState::Scalar,
                };
                return;
            }

            self.is_init = false;
            self.tracker = Tracker::Scalar;
            return;
        }

        // Field frames handle their own cleanup in deinit() - no special handling needed here.

        // All other cases: use normal deinit
        self.deinit();
    }

    /// This must be called after (fully) initializing a value.
    ///
    /// This sets `is_init` to `true` to indicate the value is initialized.
    /// Composite types (structs, enums, etc.) might be handled differently.
    ///
    /// # Safety
    ///
    /// This should only be called when `self.data` has been actually initialized.
    const unsafe fn mark_as_init(&mut self) {
        self.is_init = true;
    }

    /// Deallocate the memory associated with this frame, if it owns it.
    ///
    /// The memory has to be deinitialized first, see [Frame::deinit]
    fn dealloc(self) {
        // Only deallocate if this frame owns its memory
        if !self.ownership.needs_dealloc() {
            return;
        }

        // If we need to deallocate, the frame must be deinitialized first
        if self.is_init {
            unreachable!("a frame has to be deinitialized before being deallocated")
        }

        // Deallocate using the actual allocated size (not derived from shape)
        if self.allocated.allocated_size() > 0 {
            // Use the shape for alignment, but the stored size for the actual allocation
            if let Ok(layout) = self.allocated.shape().layout.sized_layout() {
                let actual_layout = core::alloc::Layout::from_size_align(
                    self.allocated.allocated_size(),
                    layout.align(),
                )
                .expect("allocated_size must be valid");
                unsafe { alloc::alloc::dealloc(self.data.as_mut_byte_ptr(), actual_layout) };
            }
        }
    }

    /// Fill in defaults for any unset fields that have default values.
    ///
    /// This handles:
    /// - Container-level defaults (when no fields set and struct has Default impl)
    /// - Fields with `#[facet(default = ...)]` - uses the explicit default function
    /// - Fields with `#[facet(default)]` - uses the type's Default impl
    /// - `Option<T>` fields - default to None
    ///
    /// Returns Ok(()) if successful, or an error if a field has `#[facet(default)]`
    /// but no default implementation is available.
    fn fill_defaults(&mut self) -> Result<(), ReflectErrorKind> {
        // First, check if we need to upgrade from Scalar to Struct tracker
        // This happens when no fields were visited at all in deferred mode
        if !self.is_init
            && matches!(self.tracker, Tracker::Scalar)
            && let Type::User(UserType::Struct(struct_type)) = self.allocated.shape().ty
        {
            // If no fields were visited and the container has a default, use it
            // SAFETY: We're about to initialize the entire struct with its default value
            let data_mut = unsafe { self.data.assume_init() };
            if unsafe { self.allocated.shape().call_default_in_place(data_mut) }.is_some() {
                self.is_init = true;
                return Ok(());
            }
            // Otherwise initialize the struct tracker with empty iset
            self.tracker = Tracker::Struct {
                iset: ISet::new(struct_type.fields.len()),
                current_child: None,
            };
        }

        match &mut self.tracker {
            Tracker::Struct { iset, .. } => {
                if let Type::User(UserType::Struct(struct_type)) = self.allocated.shape().ty {
                    // Fast path: if ALL fields are set, nothing to do
                    if iset.all_set(struct_type.fields.len()) {
                        return Ok(());
                    }

                    // Check if NO fields have been set and the container has a default
                    let no_fields_set = (0..struct_type.fields.len()).all(|i| !iset.get(i));
                    if no_fields_set {
                        // SAFETY: We're about to initialize the entire struct with its default value
                        let data_mut = unsafe { self.data.assume_init() };
                        if unsafe { self.allocated.shape().call_default_in_place(data_mut) }
                            .is_some()
                        {
                            self.tracker = Tracker::Scalar;
                            self.is_init = true;
                            return Ok(());
                        }
                    }

                    // Check if the container has #[facet(default)] attribute
                    let container_has_default = self.allocated.shape().has_default_attr();

                    // Fill defaults for individual fields
                    for (idx, field) in struct_type.fields.iter().enumerate() {
                        // Skip already-initialized fields
                        if iset.get(idx) {
                            continue;
                        }

                        // Calculate field pointer
                        let field_ptr = unsafe { self.data.field_uninit(field.offset) };

                        // Try to initialize with default
                        if unsafe {
                            Self::try_init_field_default(field, field_ptr, container_has_default)
                        } {
                            // Mark field as initialized
                            iset.set(idx);
                        } else if field.has_default() {
                            // Field has #[facet(default)] but we couldn't find a default function.
                            // This happens with opaque types that don't have default_in_place.
                            return Err(ReflectErrorKind::DefaultAttrButNoDefaultImpl {
                                shape: field.shape(),
                            });
                        }
                    }
                }
            }
            Tracker::Enum { variant, data, .. } => {
                // Fast path: if ALL fields are set, nothing to do
                let num_fields = variant.data.fields.len();
                if num_fields == 0 || data.all_set(num_fields) {
                    return Ok(());
                }

                // Check if the container has #[facet(default)] attribute
                let container_has_default = self.allocated.shape().has_default_attr();

                // Handle enum variant fields
                for (idx, field) in variant.data.fields.iter().enumerate() {
                    // Skip already-initialized fields
                    if data.get(idx) {
                        continue;
                    }

                    // Calculate field pointer within the variant data
                    let field_ptr = unsafe { self.data.field_uninit(field.offset) };

                    // Try to initialize with default
                    if unsafe {
                        Self::try_init_field_default(field, field_ptr, container_has_default)
                    } {
                        // Mark field as initialized
                        data.set(idx);
                    } else if field.has_default() {
                        // Field has #[facet(default)] but we couldn't find a default function.
                        return Err(ReflectErrorKind::DefaultAttrButNoDefaultImpl {
                            shape: field.shape(),
                        });
                    }
                }
            }
            // Other tracker types don't have fields with defaults
            _ => {}
        }
        Ok(())
    }

    /// Initialize a field with its default value if one is available.
    ///
    /// Priority:
    /// 1. Explicit field-level default_fn (from `#[facet(default = ...)]`)
    /// 2. Type-level default_in_place (from Default impl, including `Option<T>`)
    ///    but only if the field has the DEFAULT flag
    /// 3. Container-level default: if the container has `#[facet(default)]` and
    ///    the field's type implements Default, use that
    /// 4. Special cases: `Option<T>` (defaults to None), () (unit type)
    ///
    /// Returns true if a default was applied, false otherwise.
    ///
    /// # Safety
    ///
    /// `field_ptr` must point to uninitialized memory of the appropriate type.
    unsafe fn try_init_field_default(
        field: &Field,
        field_ptr: PtrUninit,
        container_has_default: bool,
    ) -> bool {
        use facet_core::DefaultSource;

        // First check for explicit field-level default
        if let Some(default_source) = field.default {
            match default_source {
                DefaultSource::Custom(default_fn) => {
                    // Custom default function - it expects PtrUninit
                    unsafe { default_fn(field_ptr) };
                    return true;
                }
                DefaultSource::FromTrait => {
                    // Use the type's Default trait - needs PtrMut
                    let field_ptr_mut = unsafe { field_ptr.assume_init() };
                    if unsafe { field.shape().call_default_in_place(field_ptr_mut) }.is_some() {
                        return true;
                    }
                }
            }
        }

        // If container has #[facet(default)] and the field's type implements Default,
        // use the type's Default impl. This allows `#[facet(default)]` on a struct to
        // mean "use Default for any missing fields whose types implement Default".
        if container_has_default {
            let field_ptr_mut = unsafe { field_ptr.assume_init() };
            if unsafe { field.shape().call_default_in_place(field_ptr_mut) }.is_some() {
                return true;
            }
        }

        // Special case: Option<T> always defaults to None, even without explicit #[facet(default)]
        // This is because Option is fundamentally "optional" - if not set, it should be None
        if matches!(field.shape().def, Def::Option(_)) {
            let field_ptr_mut = unsafe { field_ptr.assume_init() };
            if unsafe { field.shape().call_default_in_place(field_ptr_mut) }.is_some() {
                return true;
            }
        }

        // Special case: () unit type always defaults to ()
        if field.shape().is_type::<()>() {
            let field_ptr_mut = unsafe { field_ptr.assume_init() };
            if unsafe { field.shape().call_default_in_place(field_ptr_mut) }.is_some() {
                return true;
            }
        }

        // Special case: Collection types (Vec, HashMap, HashSet, etc.) default to empty
        // These types have obvious "zero values" and it's almost always what you want
        // when deserializing data where the collection is simply absent.
        if matches!(field.shape().def, Def::List(_) | Def::Map(_) | Def::Set(_)) {
            let field_ptr_mut = unsafe { field_ptr.assume_init() };
            if unsafe { field.shape().call_default_in_place(field_ptr_mut) }.is_some() {
                return true;
            }
        }

        false
    }

    /// Returns an error if the value is not fully initialized
    fn require_full_initialization(&self) -> Result<(), ReflectErrorKind> {
        match &self.tracker {
            Tracker::Scalar => {
                if self.is_init {
                    Ok(())
                } else {
                    Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    })
                }
            }
            Tracker::Array { iset, .. } => {
                match self.allocated.shape().ty {
                    Type::Sequence(facet_core::SequenceType::Array(array_def)) => {
                        // Check if all array elements are initialized
                        if (0..array_def.n).all(|idx| iset.get(idx)) {
                            Ok(())
                        } else {
                            Err(ReflectErrorKind::UninitializedValue {
                                shape: self.allocated.shape(),
                            })
                        }
                    }
                    _ => Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    }),
                }
            }
            Tracker::Struct { iset, .. } => {
                match self.allocated.shape().ty {
                    Type::User(UserType::Struct(struct_type)) => {
                        if iset.all_set(struct_type.fields.len()) {
                            Ok(())
                        } else {
                            // Find index of the first bit not set
                            let first_missing_idx =
                                (0..struct_type.fields.len()).find(|&idx| !iset.get(idx));
                            if let Some(missing_idx) = first_missing_idx {
                                let field_name = struct_type.fields[missing_idx].name;
                                Err(ReflectErrorKind::UninitializedField {
                                    shape: self.allocated.shape(),
                                    field_name,
                                })
                            } else {
                                // fallback, something went wrong
                                Err(ReflectErrorKind::UninitializedValue {
                                    shape: self.allocated.shape(),
                                })
                            }
                        }
                    }
                    _ => Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    }),
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
                        Err(ReflectErrorKind::UninitializedEnumField {
                            shape: self.allocated.shape(),
                            field_name,
                            variant_name: variant.name,
                        })
                    } else {
                        Err(ReflectErrorKind::UninitializedValue {
                            shape: self.allocated.shape(),
                        })
                    }
                }
            }
            Tracker::SmartPointer => {
                if self.is_init {
                    Ok(())
                } else {
                    Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    })
                }
            }
            Tracker::SmartPointerSlice { building_item, .. } => {
                if *building_item {
                    Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    })
                } else {
                    Ok(())
                }
            }
            Tracker::List { current_child } => {
                if self.is_init && current_child.is_none() {
                    Ok(())
                } else {
                    Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    })
                }
            }
            Tracker::Map { insert_state } => {
                if self.is_init && matches!(insert_state, MapInsertState::Idle) {
                    Ok(())
                } else {
                    Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    })
                }
            }
            Tracker::Set { current_child } => {
                if self.is_init && !current_child {
                    Ok(())
                } else {
                    Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    })
                }
            }
            Tracker::Option { building_inner } => {
                if *building_inner {
                    Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    })
                } else {
                    Ok(())
                }
            }
            Tracker::Result { building_inner, .. } => {
                if *building_inner {
                    Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    })
                } else {
                    Ok(())
                }
            }
            Tracker::DynamicValue { state } => {
                if matches!(state, DynamicValueState::Uninit) {
                    Err(ReflectErrorKind::UninitializedValue {
                        shape: self.allocated.shape(),
                    })
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Fill defaults and check required fields in a single pass using precomputed plans.
    ///
    /// This replaces the separate `fill_defaults` + `require_full_initialization` calls
    /// with a single iteration over the precomputed `FieldInitPlan` list.
    ///
    /// # Arguments
    /// * `plans` - Precomputed field initialization plans from TypePlan
    /// * `num_fields` - Total number of fields (from StructPlan/VariantPlanMeta)
    ///
    /// # Returns
    /// `Ok(())` if all required fields are set (or filled with defaults), or an error
    /// describing the first missing required field.
    #[allow(unsafe_code)]
    fn fill_and_require_fields(
        &mut self,
        plans: &[FieldInitPlan],
        num_fields: usize,
    ) -> Result<(), ReflectErrorKind> {
        // Get the iset based on tracker type
        let iset = match &mut self.tracker {
            Tracker::Struct { iset, .. } => iset,
            Tracker::Enum { data, .. } => data,
            // Other tracker types don't use field_init_plans
            _ => return Ok(()),
        };

        // Fast path for defaults: if all fields are already set, no defaults needed.
        // But validators still need to run.
        let all_fields_set = iset.all_set(num_fields);

        for plan in plans {
            if !all_fields_set && !iset.get(plan.index) {
                // Field not set - handle according to fill rule
                match &plan.fill_rule {
                    FillRule::Defaultable(default) => {
                        // Calculate field pointer
                        let field_ptr = unsafe { self.data.field_uninit(plan.offset) };

                        // Call the appropriate default function
                        let success = match default {
                            FieldDefault::Custom(default_fn) => {
                                // SAFETY: default_fn writes to uninitialized memory
                                unsafe { default_fn(field_ptr) };
                                true
                            }
                            FieldDefault::FromTrait(shape) => {
                                // SAFETY: call_default_in_place writes to the pointer
                                let field_ptr_mut = PtrMut::new(field_ptr.as_mut_byte_ptr());
                                unsafe { shape.call_default_in_place(field_ptr_mut) }.is_some()
                            }
                        };

                        if success {
                            iset.set(plan.index);
                        } else {
                            // Default function not available - this shouldn't happen
                            // if TypePlan was built correctly, but handle gracefully
                            return Err(ReflectErrorKind::UninitializedField {
                                shape: self.allocated.shape(),
                                field_name: plan.name,
                            });
                        }
                    }
                    FillRule::Required => {
                        // Field is required but not set - error
                        return Err(ReflectErrorKind::UninitializedField {
                            shape: self.allocated.shape(),
                            field_name: plan.name,
                        });
                    }
                }
            }

            // Run validators on the (now initialized) field
            if !plan.validators.is_empty() {
                let field_ptr = unsafe { self.data.field_init(plan.offset) };
                for validator in &plan.validators {
                    validator.run(field_ptr.into(), plan.name, self.allocated.shape())?;
                }
            }
        }

        Ok(())
    }

    /// Get the [EnumType] of the frame's shape, if it is an enum type
    pub(crate) const fn get_enum_type(&self) -> Result<EnumType, ReflectErrorKind> {
        match self.allocated.shape().ty {
            Type::User(UserType::Enum(e)) => Ok(e),
            _ => Err(ReflectErrorKind::WasNotA {
                expected: "enum",
                actual: self.allocated.shape(),
            }),
        }
    }

    pub(crate) fn get_field(&self) -> Option<&Field> {
        match self.allocated.shape().ty {
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

// Convenience methods on Partial for accessing FrameMode internals.
// These help minimize changes to the rest of the codebase during the refactor.
impl<'facet, const BORROW: bool> Partial<'facet, BORROW> {
    /// Get a reference to the frame stack.
    #[inline]
    pub(crate) const fn frames(&self) -> &Vec<Frame> {
        self.mode.stack()
    }

    /// Get a mutable reference to the frame stack.
    #[inline]
    pub(crate) const fn frames_mut(&mut self) -> &mut Vec<Frame> {
        self.mode.stack_mut()
    }

    /// Check if we're in deferred mode.
    #[inline]
    pub const fn is_deferred(&self) -> bool {
        self.mode.is_deferred()
    }

    /// Get the start depth if in deferred mode.
    #[inline]
    pub(crate) const fn start_depth(&self) -> Option<usize> {
        self.mode.start_depth()
    }

    /// Get the current path if in deferred mode.
    #[inline]
    pub(crate) const fn current_path(&self) -> Option<&KeyPath> {
        self.mode.current_path()
    }
}

impl<'facet, const BORROW: bool> Drop for Partial<'facet, BORROW> {
    fn drop(&mut self) {
        trace!("🧹 Partial is being dropped");

        // With the ownership transfer model:
        // - When we enter a field, parent's iset[idx] is cleared
        // - Parent won't try to drop fields with iset[idx] = false
        // - No double-free possible by construction

        // 1. Clean up stored frames from deferred state
        if let FrameMode::Deferred { stored_frames, .. } = &mut self.mode {
            // Stored frames have ownership of their data (parent's iset was cleared).
            // IMPORTANT: Process in deepest-first order so children are dropped before parents.
            // Child frames have data pointers into parent memory, so parents must stay valid
            // until all their children are cleaned up.
            let mut stored_frames = core::mem::take(stored_frames);
            let mut paths: Vec<_> = stored_frames.keys().cloned().collect();
            paths.sort_by_key(|p| core::cmp::Reverse(p.len()));
            for path in paths {
                if let Some(mut frame) = stored_frames.remove(&path) {
                    frame.deinit();
                    frame.dealloc();
                }
            }
        }

        // 2. Pop and deinit stack frames
        loop {
            let stack = self.mode.stack_mut();
            if stack.is_empty() {
                break;
            }

            let mut frame = stack.pop().unwrap();
            frame.deinit();
            frame.dealloc();
        }
    }
}
