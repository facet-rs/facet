use crate::{ReflectError, ValueId};
use crate::{debug, trace};
#[cfg(feature = "log")]
use alloc::string::ToString;
#[cfg(feature = "log")]
use owo_colors::OwoColorize;

mod drop;
mod pop;

mod frame;
pub(crate) use frame::*;

use alloc::format;
use bitflags::bitflags;
use core::marker::PhantomData;
use facet_core::{
    Def, DefaultInPlaceFn, Facet, FieldError, PtrConst, PtrUninit, ScalarAffinity, SequenceType,
    Shape, Type, UserType, Variant,
};
use flat_map::FlatMap;

use alloc::string::String;

mod iset;
pub use iset::*;

mod put_f64;
mod put_shape;

mod enum_;
mod flat_map;

mod heap_value;
pub use heap_value::*;

/// Initialization state
pub(crate) struct IState<'shape> {
    /// Variant chosen — for everything except enums, this stays None
    variant: Option<Variant<'shape>>,

    /// Fields that were initialized. For scalars, we only track 0
    fields: ISet,

    /// The depth of the frame in the stack
    depth: usize,

    /// The special mode of this frame (if any)
    mode: FrameMode,

    /// If true, must be freed when dropped
    flags: FrameFlags,

    /// The current index for list elements
    list_index: Option<usize>,

    /// The current key for map elements
    #[allow(dead_code)]
    map_key: Option<String>,
}

bitflags! {
    /// Flags that can be applied to frames
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FrameFlags: u64 {
        /// An empty set of flags
        const EMPTY = 0;

        /// We allocated this frame on the heap, we need to deallocated it when popping
        const ALLOCATED = 1 << 0;

        /// This value was moved out of — it's not part of the value we're building and
        /// we shouldn't error out when we build and we notice it's not initialized.
        /// In fact, it should not be tracked at all.
        const MOVED = 1 << 1;
    }

    // Note: there is no 'initialized' flag because initialization can be partial — it's tracked via `ISet`
}

impl<'shape> IState<'shape> {
    /// Creates a new `IState` with the given depth.
    pub fn new(depth: usize, mode: FrameMode, flags: FrameFlags) -> Self {
        Self {
            variant: None,
            fields: Default::default(),
            depth,
            mode,
            flags,
            list_index: None,
            map_key: None,
        }
    }

    /// Sets the list index and returns self for method chaining
    #[allow(dead_code)]
    pub fn with_list_index(mut self, index: usize) -> Self {
        self.list_index = Some(index);
        self
    }

    /// Sets the map key and returns self for method chaining
    #[allow(dead_code)]
    pub fn with_map_key(mut self, key: String) -> Self {
        self.map_key = Some(key);
        self
    }
}

/// Represents the special mode a frame can be in
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameMode {
    /// Root frame
    Root,
    /// Struct field
    Field,
    /// Frame represents a list element
    ListElement,
    /// Frame represents a map key
    MapKey,
    /// Frame represents a map value with the given key frame index
    MapValue {
        /// The index of the key frame associated with this map value
        index: usize,
    },
    /// Frame represents the Some variant of an option (that we allocated)
    OptionSome,
    /// Frame represents the None variant of an option (no allocation needed)
    /// Any `put` should fail
    OptionNone,
}

/// A work-in-progress heap-allocated value
///
/// # Lifetimes
///
/// * `'facet`: The lifetime of borrowed values within the structure
/// * `'shape`: The lifetime of the Shape structure itself (often 'static)
pub struct Wip<'facet, 'shape> {
    /// stack of frames to keep track of deeply nested initialization
    frames: alloc::vec::Vec<Frame<'shape>>,

    /// keeps track of initialization of out-of-tree frames
    istates: FlatMap<ValueId<'shape>, IState<'shape>>,

    invariant: PhantomData<fn(&'facet ()) -> &'facet ()>,
}

impl<'facet, 'shape> Wip<'facet, 'shape> {
    /// Puts the value from a Peek into the current frame.
    pub fn put_peek(
        self,
        peek: crate::Peek<'_, 'facet, 'shape>,
    ) -> Result<Wip<'facet, 'shape>, ReflectError<'shape>> {
        self.put_shape(peek.data, peek.shape)
    }

    /// Returns the number of frames on the stack
    pub fn frames_count(&self) -> usize {
        self.frames.len()
    }

    /// Allocates a new value of the given shape
    pub fn alloc_shape(shape: &'shape Shape<'shape>) -> Result<Self, ReflectError<'shape>> {
        let data = shape
            .allocate()
            .map_err(|_| ReflectError::Unsized { shape })?;
        Ok(Self {
            frames: alloc::vec![Frame {
                data,
                shape,
                field_index_in_parent: None,
                istate: IState::new(0, FrameMode::Root, FrameFlags::ALLOCATED),
            }],
            istates: Default::default(),
            invariant: PhantomData,
        })
    }

    /// Allocates a new value of type `S`
    pub fn alloc<S: Facet<'facet>>() -> Result<Self, ReflectError<'shape>> {
        Self::alloc_shape(S::SHAPE)
    }

    fn track(&mut self, frame: Frame<'shape>) {
        // fields might be partially initialized (in-place) and then
        // we might come back to them, so because they're popped off
        // the stack, we still need to track them _somewhere_
        //
        // the root also relies on being tracked in the drop impl
        if frame.istate.flags.contains(FrameFlags::MOVED) {
            // don't track those
            return;
        }

        self.istates.insert(frame.id(), frame.istate);
    }

    unsafe fn mark_moved_out_of(&mut self, frame: &mut Frame<'shape>) {
        // Recursively mark `istates` entries as MOVED and deallocate. Needed because
        // descendant values might be tracked separately in `istates`.
        unsafe fn mark_subtree_moved<'facet, 'shape>(
            wip: &mut Wip<'facet, 'shape>,
            id: ValueId<'shape>,
        ) {
            // Function requires unsafe due to pointer manipulation and potential deallocation.
            unsafe {
                // Process only if the value is still tracked off-stack.
                if let Some(mut istate) = wip.istates.remove(&id) {
                    // Ensure value is marked as MOVED.
                    istate.flags.insert(FrameFlags::MOVED);

                    // Ensure all owned fields within structs/enums are also marked.
                    match id.shape.ty {
                        Type::User(UserType::Struct(sd)) => {
                            let container_ptr = PtrUninit::new(id.ptr as *mut u8);
                            for field in sd.fields.iter() {
                                let field_ptr_uninit = container_ptr.field_uninit_at(field.offset);
                                let field_id =
                                    ValueId::new(field.shape(), field_ptr_uninit.as_byte_ptr());
                                // Recurse.
                                mark_subtree_moved(wip, field_id);
                            }
                        }
                        Type::User(UserType::Enum(_)) => {
                            // Use the variant info from the processed istate.
                            if let Some(variant) = &istate.variant {
                                let container_ptr = PtrUninit::new(id.ptr as *mut u8);
                                for field in variant.data.fields.iter() {
                                    let field_ptr_uninit =
                                        container_ptr.field_uninit_at(field.offset);
                                    let field_id =
                                        ValueId::new(field.shape(), field_ptr_uninit.as_byte_ptr());
                                    // Recurse.
                                    mark_subtree_moved(wip, field_id);
                                }
                            }
                        }
                        // Only recurse for direct fields (struct/enum). Other owned values
                        // (list elements, map entries, option Some payload) are handled
                        // individually when *their* ValueId is processed, if tracked.
                        _ => {}
                    }

                    // Prevent memory leaks for heap-allocated values that are now moved.
                    // Only deallocate AFTER recursively processing child fields to prevent use-after-free.
                    if istate.flags.contains(FrameFlags::ALLOCATED) {
                        // `dealloc_if_needed` needs a `Frame`.
                        let mut temp_frame = Frame::recompose(id, istate);
                        temp_frame.dealloc_if_needed();
                    }
                }
                // If istate wasn't found, value was already handled or not tracked off-stack.
            }
        }

        // Function requires unsafe due to pointer manipulation, potential deallocation,
        // and calling other unsafe functions/methods.
        unsafe {
            // 1. Process the primary frame being moved: mark MOVED, clear state
            let frame_id = frame.id();

            // Save variant information for recursive processing before we clear it
            let variant_opt = frame.istate.variant;

            // Mark as MOVED and clear any initialization progress.
            frame.istate.flags.insert(FrameFlags::MOVED);
            ISet::clear(&mut frame.istate.fields);

            // 2. Recursively mark descendants (struct/enum fields) in `istates` as MOVED.
            // This ensures consistency if fields were pushed/popped and stored in `istates`.
            match frame.shape.ty {
                Type::User(UserType::Struct(sd)) => {
                    let container_ptr = PtrUninit::new(frame_id.ptr as *mut u8);
                    for field in sd.fields.iter() {
                        let field_ptr_uninit = container_ptr.field_uninit_at(field.offset);
                        let field_id = ValueId::new(field.shape(), field_ptr_uninit.as_byte_ptr());
                        mark_subtree_moved(self, field_id);
                    }
                }
                Type::User(UserType::Enum(_)) => {
                    // Use the saved variant information for recursion
                    if let Some(variant) = &variant_opt {
                        let container_ptr = PtrUninit::new(frame_id.ptr as *mut u8);
                        for field in variant.data.fields.iter() {
                            let field_ptr_uninit = container_ptr.field_uninit_at(field.offset);
                            let field_id =
                                ValueId::new(field.shape(), field_ptr_uninit.as_byte_ptr());
                            mark_subtree_moved(self, field_id);
                        }
                    }
                }
                // Other types don't have direct fields requiring recursive marking here.
                _ => {}
            }

            // Now clear the variant after processing is done
            frame.istate.variant = None;

            // Untrack the frame in `istates`
            self.istates.remove(&frame_id);

            // Deallocate AFTER all processing is complete to prevent use-after-free
            if frame.istate.flags.contains(FrameFlags::ALLOCATED) {
                frame.dealloc_if_needed();
            }
        }
    }

    /// Returns the shape of the current frame
    pub fn shape(&self) -> &'shape Shape<'shape> {
        self.frames.last().expect("must have frames left").shape
    }

    /// Returns the innermost shape for the current frame
    /// If the current shape is a transparent wrapper, this returns the shape of the wrapped type
    /// Otherwise, returns the current shape
    pub fn innermost_shape(&self) -> &'shape Shape<'shape> {
        let mut current_shape = self.shape();

        // Keep unwrapping as long as we find inner shapes
        while let Some(inner_fn) = current_shape.inner {
            current_shape = inner_fn();
        }

        current_shape
    }

    /// Return true if the last frame is in option mode
    pub fn in_option(&self) -> bool {
        let Some(frame) = self.frames.last() else {
            return false;
        };
        matches!(frame.istate.mode, FrameMode::OptionSome)
    }

    /// Returns the mode of the current frame
    pub fn mode(&self) -> FrameMode {
        self.frames.last().unwrap().istate.mode
    }

    /// Asserts everything is initialized and that invariants are upheld (if any)
    pub fn build(mut self) -> Result<HeapValue<'facet, 'shape>, ReflectError<'shape>> {
        debug!("[{}] ⚒️ It's BUILD time", self.frames.len());

        // 1. Require that there is exactly one frame on the stack (the root frame)
        if self.frames.is_empty() {
            panic!("No frames in WIP during build: stack is empty (you popped too much)");
        }
        if self.frames.len() != 1 {
            panic!(
                "You must pop frames so that only the root frame remains before calling build (frames left: {})",
                self.frames.len()
            );
        }

        // now the root frame is at index 0
        let root_frame = &self.frames[0];

        enum FrameRef<'shape> {
            Root,
            ById(ValueId<'shape>),
        }
        let mut to_check = alloc::vec![FrameRef::Root];

        // 4. Traverse the tree
        while let Some(fr) = to_check.pop() {
            let (id, istate) = match fr {
                FrameRef::Root => (root_frame.id(), &root_frame.istate),
                FrameRef::ById(id) => {
                    // Look up the istate for the frame with this ValueId.
                    let istate = self.istates.get(&id).unwrap();
                    (id, istate)
                }
            };

            trace!(
                "Checking shape {} at {:p}, flags={:?}, mode={:?}, fully_initialized={}",
                id.shape.blue(),
                id.ptr,
                istate.flags.bright_magenta(),
                istate.mode.yellow(),
                if is_fully_initialized(id.shape, istate) {
                    "✅"
                } else {
                    "❌"
                }
            );

            // Skip moved frames
            if istate.flags.contains(FrameFlags::MOVED) {
                trace!(
                    "{}",
                    "Frame was moved out of, skipping initialization check".yellow()
                );
                continue;
            }

            // Check initialization for the current frame

            // Special handling for arrays - check that all elements were properly set
            if let Def::Array(array_def) = id.shape.def {
                // Get the number of items we've pushed to the array
                let pushed_count = istate.list_index.unwrap_or(0);

                // Make sure we pushed exactly the right number of items
                if pushed_count != array_def.n {
                    return Err(ReflectError::ArrayNotFullyInitialized {
                        shape: id.shape,
                        pushed_count,
                        expected_size: array_def.n,
                    });
                }
            }
            // For other types that manage their own contents (List, Map, Option, Scalar, etc.),
            // we just need to check if the *container* itself is marked as initialized.
            // The recursive check handles struct/enum *elements* within these containers if they exist.
            else if !matches!(id.shape.def, Def::Undefined) {
                if !istate.fields.are_all_set(1) {
                    // Check specific modes for better errors
                    match istate.mode {
                        FrameMode::OptionNone => {
                            // This should technically be marked initialized, but if not, treat as uninit Option
                            debug!("Found uninitialized value (option none) — {}", id.shape);
                            return Err(ReflectError::UninitializedValue { shape: id.shape });
                        }
                        // Add more specific checks if needed, e.g., for lists/maps that started but weren't finished?
                        _ => {
                            debug!(
                                "Found uninitialized value (list/map/option/etc. — {})",
                                id.shape
                            );
                            return Err(ReflectError::UninitializedValue { shape: id.shape });
                        }
                    }
                }
                // No children to push onto `to_check` from the perspective of the *container* frame itself.
                // If a List contains Structs, those struct frames would have been pushed/popped
                // and their states tracked individually in `istates`, and checked when encountered via
                // `to_check` if they were fields of another struct/enum.
                // The `Drop` logic handles cleaning these contained items based on the container's drop_in_place.
                // For `build`, we trust that if the container is marked initialized, its contents are valid
                // according to its type's rules.
            } else {
                match id.shape.ty {
                    Type::User(UserType::Struct(sd)) => {
                        // find the field that's not initialized
                        for i in 0..sd.fields.len() {
                            if !istate.fields.has(i) {
                                let field = &sd.fields[i];
                                trace!("Found uninitialized field: {}", field.name);
                                return Err(ReflectError::UninitializedField {
                                    shape: id.shape,
                                    field_name: field.name,
                                });
                            }
                        }

                        let container_ptr = PtrUninit::new(id.ptr as *mut u8);

                        // If initialized, push children to check stack
                        #[allow(clippy::unused_enumerate_index)]
                        for (_i, field) in sd.fields.iter().enumerate() {
                            let field_shape = field.shape();
                            let field_ptr = unsafe { container_ptr.field_init_at(field.offset) };
                            let field_id = ValueId::new(field_shape, field_ptr.as_byte_ptr());

                            if self.istates.contains_key(&field_id) {
                                debug!(
                                    "Queueing struct field check: #{} '{}' of {}: shape={}, ptr={:p}",
                                    _i.to_string().bright_cyan(),
                                    field.name.bright_blue(),
                                    id.shape.blue(),
                                    field_shape.green(),
                                    field_ptr.as_byte_ptr()
                                );
                                to_check.push(FrameRef::ById(field_id));
                            }
                        }
                    }
                    Type::User(UserType::Enum(_ed)) => {
                        if let Some(variant) = &istate.variant {
                            // Check each field, just like for structs
                            for (i, field) in variant.data.fields.iter().enumerate() {
                                if !istate.fields.has(i) {
                                    trace!("Found uninitialized field: {}", field.name);
                                    return Err(ReflectError::UninitializedEnumField {
                                        shape: id.shape,
                                        variant_name: variant.name,
                                        field_name: field.name,
                                    });
                                }
                            }

                            // All fields initialized, push children to check stack
                            #[allow(clippy::unused_enumerate_index)]
                            for (_i, field) in variant.data.fields.iter().enumerate() {
                                let field_shape = field.shape();
                                let container_ptr = PtrUninit::new(id.ptr as *mut u8);
                                // We're in an enum, so get the field ptr out of the variant's payload
                                let field_ptr =
                                    unsafe { container_ptr.field_init_at(field.offset) };
                                let field_id = ValueId::new(field_shape, field_ptr.as_byte_ptr());

                                if self.istates.contains_key(&field_id) {
                                    debug!(
                                        "Queueing enum field check: #{} '{}' of variant '{}' of {}: shape={}, ptr={:p}",
                                        _i.to_string().bright_cyan(),
                                        field.name.bright_blue(),
                                        variant.name.yellow(),
                                        id.shape.blue(),
                                        field_shape.green(),
                                        field_ptr.as_byte_ptr()
                                    );
                                    to_check.push(FrameRef::ById(field_id));
                                }
                            }
                        } else {
                            // No variant selected is an error during build
                            debug!("Found no variant selected for enum");
                            return Err(ReflectError::NoVariantSelected { shape: id.shape });
                        }
                    }
                    // Handle other Def variants if necessary
                    _ => {
                        // Default: Check if initialized using the standard method
                        if !istate.fields.are_all_set(1) {
                            debug!("Found uninitialized value (other)");
                            return Err(ReflectError::UninitializedValue { shape: id.shape });
                        }
                    }
                }
            }
        }

        // If we finished the loop, all reachable and non-moved frames are initialized.
        debug!("All reachable frames checked and initialized.");

        // 5. Check invariants on the root
        // We have already checked root is fully initialized above, so we only need to check its invariants.
        let root_shape = root_frame.shape;
        let root_data = unsafe { root_frame.data.assume_init() };
        if let Some(invariant_fn) = root_shape.vtable.invariants {
            debug!(
                "Checking invariants for root shape {} at {:p}",
                root_shape.green(),
                root_data.as_byte_ptr()
            );
            if !unsafe { invariant_fn(PtrConst::new(root_data.as_byte_ptr())) } {
                return Err(ReflectError::InvariantViolation {
                    invariant: "Custom validation function returned false",
                });
            }
        } else {
            debug!(
                "No invariants to check for root shape {}",
                root_shape.blue()
            );
        }

        // Prevent Drop from running on the successfully built value.
        {
            FlatMap::clear(&mut self.istates);
            self.frames.clear();
        }

        // Build the guard from the root data.
        let guard = Guard {
            ptr: root_data.as_mut_byte_ptr(),
            layout: match root_shape.layout {
                facet_core::ShapeLayout::Sized(layout) => layout,
                facet_core::ShapeLayout::Unsized => panic!("Unsized layout not supported"),
            },
        };

        Ok(HeapValue {
            guard: Some(guard),
            shape: root_shape,
            phantom: PhantomData,
        })
    }

    /// Selects a field of a struct or enum variant by index and pushes it onto the frame stack.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the field to select.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if the field was successfully selected and pushed.
    /// * `Err(ReflectError)` if the current frame is not a struct or an enum with a selected variant,
    ///   or if the field doesn't exist.
    pub fn field(mut self, index: usize) -> Result<Self, ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();
        let shape = frame.shape;

        let (field, field_offset) = match shape.ty {
            Type::User(UserType::Struct(def)) => {
                if index >= def.fields.len() {
                    return Err(ReflectError::FieldError {
                        shape,
                        field_error: FieldError::NoSuchField,
                    });
                }
                let field = &def.fields[index];
                (field, field.offset)
            }
            Type::User(UserType::Enum(_)) => {
                let Some(variant) = frame.istate.variant.as_ref() else {
                    return Err(ReflectError::OperationFailed {
                        shape,
                        operation: "tried to access a field but no variant was selected",
                    });
                };

                if index >= variant.data.fields.len() {
                    return Err(ReflectError::FieldError {
                        shape,
                        field_error: FieldError::NoSuchField,
                    });
                }

                let field = &variant.data.fields[index];
                (field, field.offset)
            }
            _ => {
                return Err(ReflectError::WasNotA {
                    expected: "struct or enum",
                    actual: shape,
                });
            }
        };

        let field_data = unsafe { frame.data.field_uninit_at(field_offset) };

        let mut frame = Frame {
            data: field_data,
            shape: field.shape(),
            field_index_in_parent: Some(index),
            // we didn't have to allocate that field, it's a struct field, so it's not allocated
            istate: IState::new(self.frames.len(), FrameMode::Field, FrameFlags::EMPTY),
        };
        debug!(
            "[{}] Selecting field {} ({}#{}) of {}",
            self.frames.len(),
            field.name.blue(),
            field.shape().green(),
            index.yellow(),
            shape.blue(),
        );
        if let Some(iset) = self.istates.remove(&frame.id()) {
            trace!(
                "[{}] Restoring saved state for {} (istate.mode = {:?}, istate.fields = {:?}, istate.flags = {:?}, istate.depth = {:?})",
                self.frames.len(),
                frame.id().shape.blue(),
                iset.mode,
                iset.fields,
                iset.flags,
                iset.depth
            );
            frame.istate = iset;
        } else {
            trace!(
                "[{}] no saved state for field {} ({}#{}) of {}",
                self.frames.len(),
                field.name.blue(),
                field.shape().green(),
                index.yellow(),
                shape.blue(),
            );
        }
        self.frames.push(frame);
        Ok(self)
    }

    /// Finds the index of a field in a struct or enum variant by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the field to find.
    ///
    /// # Returns
    ///
    /// * `Some(usize)` if the field was found.
    /// * `None` if the current frame is not a struct or an enum with a selected variant,
    ///   or if the field doesn't exist.
    pub fn field_index(&self, name: &str) -> Option<usize> {
        fn find_field_index(fields: &[facet_core::Field], name: &str) -> Option<usize> {
            fields.iter().position(|f| f.name == name)
        }

        let frame = self.frames.last()?;
        match frame.shape.ty {
            Type::User(UserType::Struct(def)) => find_field_index(def.fields, name),
            Type::User(UserType::Enum(_)) => {
                let variant = frame.istate.variant.as_ref()?;
                find_field_index(variant.data.fields, name)
            }
            _ => None,
        }
    }

    /// Selects a field of a struct or enum variant by name and pushes it onto the frame stack.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the field to select.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if the field was successfully selected and pushed.
    /// * `Err(ReflectError)` if the current frame is not a struct or an enum with a selected variant,
    ///   or if the field doesn't exist.
    pub fn field_named(self, name: &str) -> Result<Self, ReflectError<'shape>> {
        let frame = self.frames.last().unwrap();
        let shape = frame.shape;

        // For enums, ensure a variant is selected
        if let Type::User(UserType::Enum(_)) = shape.ty {
            if frame.istate.variant.is_none() {
                return Err(ReflectError::OperationFailed {
                    shape,
                    operation: "tried to access a field by name but no variant was selected",
                });
            }
        }

        let index = self.field_index(name).ok_or(ReflectError::FieldError {
            shape,
            field_error: FieldError::NoSuchField,
        })?;

        self.field(index)
    }

    /// Puts a value of type `T` into the current frame.
    ///
    /// # Arguments
    ///
    /// * `t` - The value to put into the frame.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if the value was successfully put into the frame.
    /// * `Err(ReflectError)` if there was an error putting the value into the frame.
    pub fn put<T: Facet<'facet>>(self, t: T) -> Result<Wip<'facet, 'shape>, ReflectError<'shape>> {
        let shape = T::SHAPE;
        let ptr_const = PtrConst::new(&t as *const T as *const u8);
        let res = self.put_shape(ptr_const, shape);
        core::mem::forget(t); // avoid double drop; ownership moved into Wip
        res
    }

    /// Puts a value of type `T` into the current frame.
    ///
    /// # Arguments
    ///
    /// * `t` - The value to put into the frame.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if the value was successfully put into the frame.
    /// * `Err(ReflectError)` if there was an error putting the value into the frame.
    pub fn try_put<T: Facet<'facet>>(
        self,
        t: T,
    ) -> Result<Wip<'facet, 'shape>, ReflectError<'shape>> {
        let shape = T::SHAPE;
        let ptr_const = PtrConst::new(&t as *const T as *const u8);
        let res = self.put_shape(ptr_const, shape);
        core::mem::forget(t); // avoid double drop; ownership moved into Wip
        res
    }

    /// Tries to parse the current frame's value from a string
    pub fn parse<'ínput>(mut self, s: &'ínput str) -> Result<Self, ReflectError<'shape>> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to parse value but there was no frame",
            });
        };

        let shape = frame.shape;
        let index = frame.field_index_in_parent;

        let Some(parse_fn) = frame.shape.vtable.parse else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "type does not implement Parse",
            });
        };
        match unsafe { (parse_fn)(s, frame.data) } {
            Ok(_res) => {
                unsafe {
                    frame.mark_fully_initialized();
                }

                // mark the field as initialized
                self.mark_field_as_initialized(shape, index)?;

                Ok(self)
            }
            Err(_) => Err(ReflectError::OperationFailed {
                shape,
                operation: "parsing",
            }),
        }
    }

    /// Puts a value using a provided DefaultInPlaceFn in the current frame.
    pub fn put_from_fn(
        mut self,
        default_in_place: DefaultInPlaceFn,
    ) -> Result<Self, ReflectError<'shape>> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to put value from fn but there was no frame",
            });
        };

        // Special handling for arrays - if this is for an array from default,
        // we need to set list_index to array size to mark it as fully initialized
        if let Def::Array(array_def) = frame.shape.def {
            trace!(
                "[{}] Setting array as default-initialized with {} elements",
                frame.istate.depth, array_def.n
            );
            // Set the index to the array size so it appears fully populated
            frame.istate.list_index = Some(array_def.n);
        }

        unsafe {
            default_in_place(frame.data);
            trace!("Marking frame as fully initialized...");
            frame.mark_fully_initialized();
            trace!("Marking frame as fully initialized... done!");
        }

        let shape = frame.shape;
        let index = frame.field_index_in_parent;

        // mark the field as initialized
        self.mark_field_as_initialized(shape, index)?;

        Ok(self)
    }

    /// Puts the default value in the current frame.
    pub fn put_default(self) -> Result<Self, ReflectError<'shape>> {
        let Some(frame) = self.frames.last() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to put default value but there was no frame",
            });
        };

        let vtable = frame.shape.vtable;
        let Some(default_in_place) = vtable.default_in_place else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "type does not implement Default",
            });
        };

        self.put_from_fn(default_in_place)
    }

    /// Marks a field as initialized in the parent frame.
    fn mark_field_as_initialized(
        &mut self,
        shape: &'shape Shape<'shape>,
        index: Option<usize>,
    ) -> Result<(), ReflectError<'shape>> {
        if let Some(index) = index {
            let parent_index = self.frames.len().saturating_sub(2);
            #[cfg(feature = "log")]
            let num_frames = self.frames.len();
            let Some(parent) = self.frames.get_mut(parent_index) else {
                return Err(ReflectError::OperationFailed {
                    shape,
                    operation: "was supposed to mark a field as initialized, but there was no parent frame",
                });
            };
            #[cfg(feature = "log")]
            let parent_shape = parent.shape;
            trace!(
                "[{}] {}.{} initialized with {}",
                num_frames,
                parent_shape.blue(),
                index.yellow(),
                shape.green()
            );

            if matches!(parent.shape.ty, Type::User(UserType::Enum(_)))
                && parent.istate.variant.is_none()
            {
                return Err(ReflectError::OperationFailed {
                    shape,
                    operation: "was supposed to mark a field as initialized, but the parent frame was an enum and didn't have a variant chosen",
                });
            }

            if parent.istate.fields.has(index) {
                return Err(ReflectError::OperationFailed {
                    shape,
                    operation: "was supposed to mark a field as initialized, but the parent frame already had it marked as initialized",
                });
            }

            parent.istate.fields.set(index);
        }
        Ok(())
    }

    /// Returns the shape of the element type for a list/array
    pub fn element_shape(&self) -> Result<&'shape Shape<'shape>, ReflectError<'shape>> {
        let frame = self.frames.last().unwrap();
        let shape = frame.shape;

        match shape.def {
            Def::List(list_def) => Ok(list_def.t()),
            _ => Err(ReflectError::WasNotA {
                expected: "list or array",
                actual: shape,
            }),
        }
    }

    /// Returns the shape of the key type for a map
    pub fn key_shape(&self) -> Result<&'shape Shape<'shape>, ReflectError<'shape>> {
        let frame = self.frames.last().unwrap();
        let shape = frame.shape;

        match shape.def {
            Def::Map(map_def) => Ok(map_def.k()),
            _ => Err(ReflectError::WasNotA {
                expected: "map",
                actual: shape,
            }),
        }
    }

    /// Creates an empty list without pushing any elements
    pub fn put_empty_list(mut self) -> Result<Self, ReflectError<'shape>> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to create empty list but there was no frame",
            });
        };

        if !matches!(frame.shape.def, Def::List(_)) {
            return Err(ReflectError::WasNotA {
                expected: "list or array",
                actual: frame.shape,
            });
        }

        let vtable = frame.shape.vtable;

        // Initialize an empty list
        let Some(default_in_place) = vtable.default_in_place else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "list type does not implement Default",
            });
        };

        unsafe {
            default_in_place(frame.data);
            frame.mark_fully_initialized();
        }

        let shape = frame.shape;
        let index = frame.field_index_in_parent;

        // Mark the field as initialized
        self.mark_field_as_initialized(shape, index)?;

        Ok(self)
    }

    /// Creates an empty map without pushing any entries
    pub fn put_empty_map(mut self) -> Result<Self, ReflectError<'shape>> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to create empty map but there was no frame",
            });
        };

        if !matches!(frame.shape.def, Def::Map(_)) {
            return Err(ReflectError::WasNotA {
                expected: "map or hash map",
                actual: frame.shape,
            });
        }

        let vtable = frame.shape.vtable;

        // Initialize an empty map
        let Some(default_in_place) = vtable.default_in_place else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "map type does not implement Default",
            });
        };

        unsafe {
            default_in_place(frame.data);
            frame.mark_fully_initialized();
        }

        // Mark the field as initialized directly
        if let Some(index) = frame.field_index_in_parent {
            let parent_index = self.frames.len().saturating_sub(2);
            if let Some(parent) = self.frames.get_mut(parent_index) {
                parent.istate.fields.set(index);
            }
        }

        Ok(self)
    }

    /// Begins pushback mode for a list, array, tuple struct, or enum variant tuple struct,
    /// allowing elements to be added one by one.
    /// For lists/arrays, initializes an empty container if needed.
    /// For tuple structs/variants, does nothing (expects subsequent `push` calls).
    pub fn begin_pushback(mut self) -> Result<Self, ReflectError<'shape>> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to begin pushback but there was no frame",
            });
        };

        let is_list = matches!(frame.shape.def, Def::List(_));
        let is_array = matches!(frame.shape.def, Def::Array(_));
        let is_tuple_struct_or_variant = match (frame.shape.ty, frame.shape.def) {
            (_, Def::Scalar(sd)) => matches!(sd.affinity, ScalarAffinity::Empty(_)),
            (Type::Sequence(_), _) => true,
            (Type::User(UserType::Struct(sd)), _) => sd.kind == facet_core::StructKind::Tuple,
            (Type::User(UserType::Enum(_)), _) => {
                // Check if a variant is selected and if that variant is a tuple-like struct
                if let Some(variant) = &frame.istate.variant {
                    variant.data.kind == facet_core::StructKind::Tuple
                } else {
                    // If no variant is selected yet, we can't determine if it's tuple-like.
                    // We allow beginning pushback here, assuming a tuple variant *will* be selected
                    // before pushing actual elements. The `push` operation will handle variant selection checks.
                    // Alternatively, we could error here if no variant is selected. Let's allow it for now.
                    // However, we definitely *don't* initialize anything if no variant is selected.
                    // UPDATE: Decided to be stricter. If it's an enum, a variant MUST be selected
                    // and it MUST be a tuple struct variant.
                    false // Require variant to be selected *and* be a tuple.
                }
            }
            _ => false,
        };

        if !is_list && !is_array && !is_tuple_struct_or_variant {
            return Err(ReflectError::WasNotA {
                expected: "list, array, or tuple-like struct/enum variant",
                actual: frame.shape,
            });
        }

        // Initialize a list if necessary
        if is_list {
            let vtable = frame.shape.vtable;
            // Initialize an empty list if it's not already marked as initialized (field 0)
            if !frame.istate.fields.has(0) {
                let Some(default_in_place) = vtable.default_in_place else {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "list type does not implement Default, cannot begin pushback",
                    });
                };

                unsafe {
                    default_in_place(frame.data);
                    // Mark the list itself as initialized (representing the container exists)
                    frame.istate.fields.set(0);
                }
            }
        }
        // For arrays, we don't need to call default_in_place - we'll initialize elements one by one
        else if is_array {
            // Initialize the list_index to track which array index we're on
            frame.istate.list_index = Some(0);
        }
        // For tuple structs/variants, do nothing here. Initialization happens field-by-field during `push`.

        Ok(self)
    }

    /// Begins insertion mode for a map, allowing key-value pairs to be added one by one
    pub fn begin_map_insert(mut self) -> Result<Self, ReflectError<'shape>> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to begin map insertion but there was no frame",
            });
        };

        if !matches!(frame.shape.def, Def::Map(_)) {
            return Err(ReflectError::WasNotA {
                expected: "map or hash map",
                actual: frame.shape,
            });
        }

        let vtable = frame.shape.vtable;

        // Initialize an empty map if it's not already initialized
        if !frame.istate.fields.has(0) {
            let Some(default_in_place) = vtable.default_in_place else {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "map type does not implement Default",
                });
            };

            unsafe {
                default_in_place(frame.data);
                frame.istate.fields.set(0);
            }
        }

        Ok(self)
    }

    /// Pushes a new element onto the list/array/tuple struct/tuple enum variant
    ///
    /// This creates a new frame for the element. When this frame is popped,
    /// the element will be added to the list or the corresponding tuple field will be set.
    pub fn push(mut self) -> Result<Self, ReflectError<'shape>> {
        // Get mutable access to the top frame early, we might need it for list_index
        let frame_len = self.frames.len();
        let frame = self
            .frames
            .last_mut()
            .ok_or(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to push but there was no frame",
            })?;
        let seq_shape = frame.shape;

        // Determine element shape and context string based on the container type
        let (element_shape, context_str): (&'shape Shape<'shape>, &'shape str) =
            match (seq_shape.ty, seq_shape.def) {
                (_, Def::List(list_def)) => {
                    // Check list initialization *before* getting element shape
                    if !frame.istate.fields.has(0) {
                        // Replicate original recursive call pattern to handle initialization
                        // Drop mutable borrow of frame before recursive call
                        return self.begin_pushback()?.push();
                    }
                    // Get element shape directly from the list definition
                    let shape = list_def.t();
                    (shape, "list")
                }
                (_, Def::Array(array_def)) => {
                    // For arrays, we need to check which index we're on and verify it's valid
                    let index = frame.istate.list_index.unwrap_or(0);

                    // Check if we're trying to push beyond the array bounds
                    if index >= array_def.n {
                        return Err(ReflectError::ArrayIndexOutOfBounds {
                            shape: seq_shape,
                            index,
                            size: array_def.n,
                        });
                    }

                    // Update the index for next push
                    frame.istate.list_index = Some(index + 1);

                    // Get the shape of the element type
                    let element_shape = array_def.t;
                    (element_shape, "array")
                }
                (Type::Sequence(SequenceType::Tuple(tt)), _) => {
                    // Handle tuples - similar to tuple struct handling
                    let field_index = {
                        // Borrow frame mutably (already done) to update list_index
                        let next_idx = frame.istate.list_index.unwrap_or(0);
                        frame.istate.list_index = Some(next_idx + 1);
                        next_idx
                    };
                    // Check if the field index is valid
                    if field_index >= tt.fields.len() {
                        return Err(ReflectError::FieldError {
                            shape: seq_shape,
                            field_error: FieldError::NoSuchField,
                        });
                    }
                    // Get the shape of the field at the calculated index
                    (tt.fields[field_index].shape(), "tuple")
                }
                (Type::User(UserType::Struct(sd)), _)
                    if sd.kind == facet_core::StructKind::Tuple =>
                {
                    // Handle tuple struct (requires mutable frame for list_index)
                    let field_index = {
                        // Borrow frame mutably (already done) to update list_index
                        let next_idx = frame.istate.list_index.unwrap_or(0);
                        frame.istate.list_index = Some(next_idx + 1);
                        next_idx
                    };
                    // Check if the field index is valid
                    if field_index >= sd.fields.len() {
                        return Err(ReflectError::FieldError {
                            shape: seq_shape,
                            field_error: FieldError::NoSuchField, // Or maybe SequenceError::OutOfBounds?
                        });
                    }
                    // Get the shape of the field at the calculated index
                    (sd.fields[field_index].shape(), "tuple struct")
                }

                (Type::User(UserType::Enum(_)), _) => {
                    // Handle tuple enum variant (requires mutable frame for list_index and variant check)
                    let variant =
                        frame
                            .istate
                            .variant
                            .as_ref()
                            .ok_or(ReflectError::OperationFailed {
                                shape: seq_shape,
                                operation: "tried to push onto enum but no variant was selected",
                            })?;
                    // Ensure the selected variant is tuple-like
                    if variant.data.kind != facet_core::StructKind::Tuple {
                        return Err(ReflectError::WasNotA {
                            expected: "tuple-like enum variant",
                            actual: seq_shape, // Could provide variant name here for clarity
                        });
                    }
                    // Get the next field index for the tuple variant
                    let field_index = {
                        // Borrow frame mutably (already done) to update list_index
                        let next_idx = frame.istate.list_index.unwrap_or(0);
                        frame.istate.list_index = Some(next_idx + 1);
                        next_idx
                    };
                    // Check if the field index is valid within the variant's fields
                    if field_index >= variant.data.fields.len() {
                        return Err(ReflectError::FieldError {
                            shape: seq_shape, // Could provide variant name here
                            field_error: FieldError::NoSuchField,
                        });
                    }
                    // Get the shape of the field at the calculated index within the variant
                    (
                        variant.data.fields[field_index].shape(),
                        "tuple enum variant",
                    )
                }
                (_, Def::Scalar(sd)) if matches!(sd.affinity, ScalarAffinity::Empty(_)) => {
                    // Handle empty tuple a.k.a. unit type () - cannot push elements
                    return Err(ReflectError::OperationFailed {
                        shape: seq_shape,
                        operation: "cannot push elements to unit type ()",
                    });
                }
                _ => {
                    // If it's not a list, tuple struct, or enum, it's an error
                    return Err(ReflectError::WasNotA {
                        expected: "list, array, tuple, tuple struct, or tuple enum variant",
                        actual: seq_shape,
                    });
                }
            };

        // Allocate memory for the element
        let element_data = element_shape
            .allocate()
            .map_err(|_| ReflectError::Unsized {
                shape: element_shape,
            })?;

        // Create a new frame for the element
        let element_frame = Frame {
            data: element_data,
            shape: element_shape,
            field_index_in_parent: None, // Mode distinguishes it, not field index
            istate: IState::new(
                frame_len,              // Use captured length (depth of the new frame)
                FrameMode::ListElement, // Keep using this mode for list/tuple elements
                FrameFlags::ALLOCATED,
            ),
        };

        trace!(
            "[{}] Pushing element of type {} to {} {}",
            frame_len,
            element_shape.green(),
            context_str, // Use the determined context string
            seq_shape.blue(),
        );
        let _ = context_str;

        self.frames.push(element_frame);
        Ok(self)
    }

    /// Prepare to push the `Some(T)` variant of an `Option<T>`.
    pub fn push_some(mut self) -> Result<Self, ReflectError<'shape>> {
        // Make sure we're initializing an option
        let frame = self.frames.last().unwrap();
        let option_shape = frame.shape;

        // Get the option definition
        let Def::Option(option_def) = option_shape.def else {
            return Err(ReflectError::WasNotA {
                expected: "option",
                actual: option_shape,
            });
        };

        // Get the inner type of the option
        let inner_shape = option_def.t();

        // Allocate memory for the inner value
        let inner_data = inner_shape
            .allocate()
            .map_err(|_| ReflectError::Unsized { shape: inner_shape })?;

        // Create a new frame for the inner value
        let inner_frame = Frame {
            data: inner_data,
            shape: inner_shape,
            // this is only set when we pop
            field_index_in_parent: None,
            istate: IState::new(
                self.frames.len(),
                FrameMode::OptionSome,
                // TODO: we could lazy-allocate it when something like `field` is called, tbh
                FrameFlags::ALLOCATED,
            ),
        };

        trace!(
            "[{}] Pushing option frame for {}",
            self.frames.len(),
            option_shape.blue(),
        );

        self.frames.push(inner_frame);
        Ok(self)
    }

    /// Pops a not-yet-initialized option frame, setting it to None in the parent
    ///
    /// This is used to set an option to None instead of Some.
    /// Steps:
    ///  1. Asserts the option frame is NOT initialized
    ///  2. Frees the memory for the pushed value
    ///  3. Pops the frame
    ///  4. Sets the parent option to its default value (i.e., None)
    ///  5. Pops the parent option (which is the actual `Option<T>`, but no longer in option mode)
    pub fn pop_some_push_none(mut self) -> Result<Self, ReflectError<'shape>> {
        // 1. Option frame must exist
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to pop_some_push_none but there was no frame",
            });
        };

        // 1. Make sure the current frame is an option inner frame in "Option" mode
        if frame.istate.mode != FrameMode::OptionSome {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "pop_some_push_none called, but frame was not in Option mode",
            });
        }

        // 1. Check not initialized
        if frame.is_fully_initialized() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "option frame already initialized, cannot pop_some_push_none",
            });
        }

        frame.dealloc_if_needed();

        // 3. Pop the frame (this discards, doesn't propagate up)
        let _frame = self.frames.pop().expect("frame already checked");

        // 4. Set parent option (which we just popped into) to default (None)
        let parent_frame = self
            .frames
            .last_mut()
            .ok_or(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to pop_some_push_none but there was no parent frame",
            })?;

        // Safety: option frames are correctly sized, and data is valid
        unsafe {
            if let Some(default_fn) = parent_frame.shape.vtable.default_in_place {
                default_fn(parent_frame.data);
            } else {
                return Err(ReflectError::OperationFailed {
                    shape: parent_frame.shape,
                    operation: "option type does not implement Default",
                });
            }
            parent_frame.mark_fully_initialized();
        }

        let Def::Option(od) = parent_frame.shape.def else {
            return Err(ReflectError::OperationFailed {
                shape: parent_frame.shape,
                operation: "pop_some_push_none and the parent isn't of type Option???",
            });
        };

        // Now push a `None` frame
        let data = parent_frame.data;

        let mut frame = Frame {
            data,
            shape: od.t(),
            field_index_in_parent: Some(0),
            istate: IState::new(self.frames.len(), FrameMode::OptionNone, FrameFlags::EMPTY),
        };
        unsafe {
            frame.mark_fully_initialized();
        }

        self.frames.push(frame);

        Ok(self)
    }

    /// Pushes a new key frame for a map entry
    ///
    /// This creates a new frame for the key. After setting the key value,
    /// call `push_map_value` to create a frame for the corresponding value.
    pub fn push_map_key(mut self) -> Result<Self, ReflectError<'shape>> {
        // Make sure we're initializing a map
        let frame = self.frames.last().unwrap();
        let map_shape = frame.shape;

        if !matches!(map_shape.def, Def::Map(_)) {
            return Err(ReflectError::WasNotA {
                expected: "map or hash map",
                actual: map_shape,
            });
        }

        // If the map isn't initialized yet, initialize it
        if !frame.istate.fields.has(0) {
            self = self.begin_map_insert()?;
        }

        // Get the key type directly from the map definition
        let key_shape = match map_shape.def {
            Def::Map(map_def) => map_def.k(),
            _ => unreachable!("Already checked map type above"),
        };

        // Allocate memory for the key
        let key_data = key_shape
            .allocate()
            .map_err(|_| ReflectError::Unsized { shape: key_shape })?;

        // Create a new frame for the key
        let key_frame = Frame {
            data: key_data,
            shape: key_shape,
            field_index_in_parent: None,
            istate: IState::new(self.frames.len(), FrameMode::MapKey, FrameFlags::ALLOCATED),
        };

        trace!(
            "[{}] Pushing key of type {} for map {}",
            self.frames.len(),
            key_shape.green(),
            map_shape.blue(),
        );

        self.frames.push(key_frame);
        Ok(self)
    }

    /// Pushes a new value frame for a map entry
    ///
    /// This should be called after pushing and initializing a key frame.
    /// When the value frame is popped, the key-value pair will be added to the map.
    pub fn push_map_value(mut self) -> Result<Self, ReflectError<'shape>> {
        trace!("Wants to push map value. Frames = ");
        #[cfg(feature = "log")]
        for (i, f) in self.frames.iter().enumerate() {
            trace!("Frame {}: {:?}", i, f);
        }

        // First, ensure we have a valid key frame
        if self.frames.len() < 2 {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to push map value but there was no key frame",
            });
        }

        // Check the frame before the last to ensure it's a map key
        let key_frame_index = self.frames.len() - 1;
        let key_frame = &self.frames[key_frame_index];

        // Verify the current frame is a key frame
        match key_frame.istate.mode {
            FrameMode::MapKey => {} // Valid - continue
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: key_frame.shape,
                    operation: "current frame is not a map key",
                });
            }
        }

        // Check that the key is fully initialized
        if !key_frame.is_fully_initialized() {
            return Err(ReflectError::OperationFailed {
                shape: key_frame.shape,
                operation: "map key is not fully initialized",
            });
        }

        // Get the parent map frame to verify we're working with a map
        let map_frame_index = self.frames.len() - 2;
        let map_frame = &self.frames[map_frame_index];
        let map_shape = map_frame.shape;

        let Def::Map(map_def) = map_shape.def else {
            return Err(ReflectError::WasNotA {
                expected: "map",
                actual: map_frame.shape,
            });
        };

        let value_shape = map_def.v();

        // Allocate memory for the value
        let value_data = value_shape
            .allocate()
            .map_err(|_| ReflectError::Unsized { shape: value_shape })?;

        // Create a new frame for the value
        let value_frame = Frame {
            data: value_data,
            shape: value_shape,
            field_index_in_parent: None,
            istate: IState::new(
                self.frames.len(),
                FrameMode::MapValue {
                    index: key_frame_index,
                },
                FrameFlags::ALLOCATED,
            ),
        };

        trace!(
            "[{}] Pushing value of type {} for map {} with key type {}",
            self.frames.len(),
            value_shape.green(),
            map_shape.blue(),
            key_frame.shape.yellow(),
        );

        self.frames.push(value_frame);
        Ok(self)
    }

    /// Evict a frame from istates, along with all its children
    /// (because we're about to use `drop_in_place` on it — not
    /// yet though, we need to know the variant for enums, etc.)
    pub(crate) fn evict_tree(&mut self, frame: Frame<'shape>) -> Frame<'shape> {
        match frame.shape.ty {
            Type::User(UserType::Struct(sd)) => {
                for f in sd.fields {
                    let id = ValueId {
                        shape: f.shape(),
                        ptr: unsafe { frame.data.field_uninit_at(f.offset) }.as_byte_ptr(),
                    };
                    if let Some(istate) = self.istates.remove(&id) {
                        let frame = Frame::recompose(id, istate);
                        self.evict_tree(frame);
                    } else {
                        trace!("No istate found for field {}", f.name);
                    }
                }
            }
            Type::User(UserType::Enum(_ed)) => {
                // Check if a variant is selected in the istate
                if let Some(variant) = &frame.istate.variant {
                    trace!(
                        "Evicting enum {} variant '{}' fields",
                        frame.shape.blue(),
                        variant.name.yellow()
                    );
                    // Iterate over the fields of the selected variant
                    for field in variant.data.fields {
                        // Calculate the pointer to the field within the enum's data payload
                        let field_ptr = unsafe { frame.data.field_uninit_at(field.offset) };
                        let field_shape = field.shape();
                        let field_id = ValueId::new(field_shape, field_ptr.as_byte_ptr());

                        // Try to remove the field's state from istates
                        if let Some(field_istate) = self.istates.remove(&field_id) {
                            trace!(
                                "Evicting field '{}' (shape {}) of enum variant '{}'",
                                field.name.bright_blue(),
                                field_shape.green(),
                                variant.name.yellow()
                            );
                            // Recompose the frame for the field
                            let field_frame = Frame::recompose(field_id, field_istate);
                            // Recursively evict the field's subtree
                            self.evict_tree(field_frame);
                        } else {
                            trace!(
                                "Field '{}' (shape {}) of enum variant '{}' not found in istates, skipping eviction",
                                field.name.red(),
                                field_shape.red(),
                                variant.name.yellow()
                            );
                        }
                    }
                } else {
                    // No variant selected, nothing to evict within the enum
                    trace!(
                        "Enum {} has no variant selected, no fields to evict.",
                        frame.shape.blue()
                    );
                }
            }
            _ => {}
        }
        frame
    }

    #[allow(rustdoc::broken_intra_doc_links)]
    /// Returns the current path in the JSON document as a string.
    /// For example: "$.users[0].name"
    pub fn path(&self) -> String {
        let mut path = String::from("$");

        for (i, frame) in self.frames.iter().enumerate() {
            // Skip the root frame
            if i == 0 {
                continue;
            }

            match frame.istate.mode {
                FrameMode::ListElement => {
                    // For arrays, we use bracket notation with index
                    if let Some(index) = frame.istate.list_index {
                        path.push_str(&format!("[{}]", index));
                    } else {
                        path.push_str("[?]");
                    }
                }
                FrameMode::MapKey => {
                    path.push_str(".key");
                }
                FrameMode::MapValue { index: _ } => {
                    path.push_str(".value");
                }
                FrameMode::OptionSome => {
                    path.push_str(".some");
                }
                FrameMode::OptionNone => {
                    path.push_str(".none");
                }
                FrameMode::Root => {
                    // Root doesn't add to the path
                }
                FrameMode::Field => {
                    // For struct fields, we use dot notation with field name
                    if let Some(index) = frame.field_index_in_parent {
                        // Find the parent frame to get the field name
                        if let Some(parent) = self.frames.get(i - 1) {
                            if let Type::User(UserType::Struct(sd)) = parent.shape.ty {
                                if index < sd.fields.len() {
                                    let field_name = sd.fields[index].name;
                                    path.push('.');
                                    path.push_str(field_name);
                                }
                            } else if let Type::User(UserType::Enum(_)) = parent.shape.ty {
                                if let Some(variant) = &parent.istate.variant {
                                    if index < variant.data.fields.len() {
                                        let field_name = variant.data.fields[index].name;
                                        path.push('.');
                                        path.push_str(field_name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        path
    }

    /// Returns true if the field at the given index is set (initialized) in the current frame.
    pub fn is_field_set(&self, index: usize) -> Result<bool, ReflectError<'shape>> {
        let frame = self.frames.last().ok_or(ReflectError::OperationFailed {
            shape: <()>::SHAPE,
            operation: "tried to check if field is set, but there was no frame",
        })?;

        match frame.shape.ty {
            Type::User(UserType::Struct(ref sd)) => {
                if index >= sd.fields.len() {
                    return Err(ReflectError::FieldError {
                        shape: frame.shape,
                        field_error: FieldError::NoSuchField,
                    });
                }
                Ok(frame.istate.fields.has(index))
            }
            Type::User(UserType::Enum(_)) => {
                let variant = frame.istate.variant.as_ref().ok_or(
                    ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "tried to check if field is set, but no variant was selected",
                    },
                )?;
                if index >= variant.data.fields.len() {
                    return Err(ReflectError::FieldError {
                        shape: frame.shape,
                        field_error: FieldError::NoSuchField,
                    });
                }
                Ok(frame.istate.fields.has(index))
            }
            _ => Err(ReflectError::WasNotA {
                expected: "struct or enum",
                actual: frame.shape,
            }),
        }
    }
}
