use crate::{ReflectError, ValueId};
use crate::{debug, trace};
#[cfg(feature = "log")]
use alloc::string::ToString;
#[cfg(feature = "log")]
use owo_colors::OwoColorize;

use alloc::format;
use alloc::{vec, vec::Vec};
use bitflags::bitflags;
use core::{fmt, marker::PhantomData};
use facet_core::{
    Def, DefaultInPlaceFn, Facet, FieldError, PtrConst, PtrMut, PtrUninit, Shape, Variant,
};
use flat_map::FlatMap;

use alloc::string::String;

mod iset;
pub use iset::*;

mod put_f64;

mod enum_;
mod flat_map;

mod heap_value;
pub use heap_value::*;

fn def_kind(def: &Def) -> &'static str {
    match def {
        Def::Scalar(_) => "scalar",
        Def::Struct(_) => "struct",
        Def::Map(_) => "map",
        Def::List(_) => "list",
        Def::Enum(_) => "enum",
        Def::Option(_) => "option",
        Def::SmartPointer(_) => "smart_ptr",
        _ => "other",
    }
}

/// Represents a frame in the initialization stack
pub struct Frame {
    /// The value we're initializing
    data: PtrUninit<'static>,

    /// The shape of the value
    shape: &'static Shape,

    /// If set, when we're initialized, we must mark the
    /// parent's indexth field as initialized.
    field_index_in_parent: Option<usize>,

    /// Tracking which of our fields are initialized
    /// TODO: I'm not sure we should track "ourselves" as initialized — we always have the
    /// parent to look out for, right now we're tracking children in two states, which isn't ideal
    istate: IState,
}

impl Frame {
    /// Given a ValueId and an IState, recompose a Frame suitable for tracking
    fn recompose(id: ValueId, istate: IState) -> Self {
        Frame {
            data: PtrUninit::new(id.ptr as *mut u8),
            shape: id.shape,
            field_index_in_parent: None,
            istate,
        }
    }

    /// Deallocates the memory used by this frame if it was heap-allocated.
    fn dealloc_if_needed(&mut self) {
        if self.istate.flags.contains(FrameFlags::ALLOCATED) {
            trace!(
                "[{}] {:p} => deallocating {}",
                self.istate.depth,
                self.data.as_mut_byte_ptr().magenta(),
                self.shape.green(),
            );
            if self.shape.layout.size() != 0 {
                unsafe {
                    alloc::alloc::dealloc(self.data.as_mut_byte_ptr(), self.shape.layout);
                }
            }
            self.istate.flags.remove(FrameFlags::ALLOCATED);
        } else {
            trace!(
                "[{}] {:p} => NOT deallocating {} (not ALLOCATED)",
                self.istate.depth,
                self.data.as_mut_byte_ptr().magenta(),
                self.shape.green(),
            );
        }
    }
}

struct DebugToDisplay<T>(T);

impl<T> fmt::Debug for DebugToDisplay<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("shape", &DebugToDisplay(&self.shape))
            .field("kind", &def_kind(&self.shape.def))
            .field("index", &self.field_index_in_parent)
            .field("mode", &self.istate.mode)
            .finish()
    }
}

impl Frame {
    /// Returns the value ID for a frame
    fn id(&self) -> ValueId {
        ValueId::new(self.shape, self.data.as_byte_ptr())
    }

    /// Returns true if the frame is fully initialized
    fn is_fully_initialized(&self) -> bool {
        match self.shape.def {
            Def::Struct(sd) => self.istate.fields.are_all_set(sd.fields.len()),
            Def::Enum(_) => match self.istate.variant.as_ref() {
                None => false,
                Some(v) => self.istate.fields.are_all_set(v.data.fields.len()),
            },
            _ => self.istate.fields.are_all_set(1),
        }
    }

    // Safety: only call if is fully initialized
    unsafe fn drop_and_dealloc_if_needed(mut self) {
        trace!(
            "[Frame::drop] Dropping frame for shape {} at {:p}",
            self.shape.blue(),
            self.data.as_byte_ptr()
        );
        if let Some(drop_in_place) = self.shape.vtable.drop_in_place {
            unsafe {
                trace!(
                    "[Frame::drop] Invoking drop_in_place for shape {} at {:p}",
                    self.shape.green(),
                    self.data.as_byte_ptr()
                );
                drop_in_place(self.data.assume_init());
            }
        } else {
            trace!(
                "[Frame::drop] No drop_in_place function for shape {}",
                self.shape.blue(),
            );
        }
        self.dealloc_if_needed();
    }

    /// Marks the frame as fully initialized
    unsafe fn mark_fully_initialized(&mut self) {
        match self.shape.def {
            Def::Struct(sd) => {
                self.istate.fields = ISet::all(sd.fields);
            }
            Def::Enum(_) => {
                if let Some(variant) = &self.istate.variant {
                    self.istate.fields = ISet::all(variant.data.fields);
                }
            }
            _ => {
                self.istate.fields.set(0);
            }
        }
    }
}

/// Initialization state
struct IState {
    /// Variant chosen — for everything except enums, this stays None
    variant: Option<Variant>,

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

impl IState {
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
pub struct Wip<'a> {
    /// stack of frames to keep track of deeply nested initialization
    frames: alloc::vec::Vec<Frame>,

    /// keeps track of initialization of out-of-tree frames
    istates: FlatMap<ValueId, IState>,

    /// lifetime of the shortest reference we hold
    phantom: PhantomData<&'a ()>,
}

impl<'a> Wip<'a> {
    /// Puts the value from a Peek into the current frame.
    pub fn put_peek<'mem>(self, peek: crate::Peek<'mem>) -> Result<Wip<'mem>, ReflectError>
    where
        'a: 'mem,
    {
        self.put_shape(peek.data, peek.shape)
    }

    /// Returns the number of frames on the stack
    pub fn frames_count(&self) -> usize {
        self.frames.len()
    }

    /// Allocates a new value of the given shape
    pub fn alloc_shape(shape: &'static Shape) -> Self {
        let data = shape.allocate();
        Self {
            frames: alloc::vec![Frame {
                data,
                shape,
                field_index_in_parent: None,
                istate: IState::new(0, FrameMode::Root, FrameFlags::ALLOCATED),
            }],
            istates: Default::default(),
            phantom: PhantomData,
        }
    }

    /// Allocates a new value of type `S`
    pub fn alloc<S: Facet>() -> Self {
        Self::alloc_shape(S::SHAPE)
    }

    fn track(&mut self, frame: Frame) {
        // fields might be partially initialized (in-place) and then
        // we might come back to them, so because they're popped off
        // the stack, we still need to track them _somewhere_
        //
        // the root also relies on being tracked in the drop impl
        if !frame.istate.flags.contains(FrameFlags::MOVED) {
            self.istates.insert(frame.id(), frame.istate);
        }
    }

    unsafe fn mark_moved_out_of(&mut self, frame: &mut Frame) {
        frame.dealloc_if_needed();
        ISet::clear(&mut frame.istate.fields);
        frame.istate.variant = None;
        frame.istate.flags.insert(FrameFlags::MOVED);
        // make sure this isn't tracked here anymore — we don't want to have
        // any metadata associated with it that gets restored by mistake
        self.istates.remove(&frame.id());
    }

    /// Returns the shape of the current frame
    pub fn shape(&self) -> &'static Shape {
        self.frames.last().expect("must have shapes left").shape
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
    pub fn build(mut self) -> Result<HeapValue<'a>, ReflectError> {
        debug!("[{}] ⚒️ It's BUILD time", self.frames.len());

        // 1. Track all frames currently on the stack into istates
        while let Some(frame) = self.pop_inner() {
            self.track(frame);
        }

        // 2. Find the root frame
        let Some((root_id, _)) = self.istates.iter().find(|(_k, istate)| istate.depth == 0) else {
            debug!("No root found, possibly already built or empty WIP");
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to build a value but there was no root frame tracked",
            });
        };

        let root_id = *root_id;
        // We need to *keep* the root istate for the check, so we clone it or get it immutably.
        // Let's retrieve it immutably first. The `istates` map will be dropped with `Wip` anyway.
        let root_istate = self
            .istates
            .remove(&root_id)
            .expect("Root ID found but not present in istates, this is a bug"); // Clone needed to avoid borrowing issues later potentially

        let root_frame = Frame::recompose(root_id, root_istate);
        let root_shape = root_frame.shape;
        let root_data_ptr = root_frame.data; // Keep the root pointer for the final HeapValue

        // 6. Transfer ownership of the root data to the HeapValue
        // The root frame should have had the ALLOCATED flag if it was heap allocated.
        // We need to ensure the Guard takes ownership correctly.
        // Find the original root istate again to check the flag.
        let guard = Guard {
            ptr: root_data_ptr.as_mut_byte_ptr(),
            layout: root_shape.layout,
        };

        // 3. Initialize `to_check`
        let mut to_check = alloc::vec![root_frame];

        // 4. Traverse the tree
        while let Some(frame) = to_check.pop() {
            trace!(
                "Checking frame: shape={} at {:p}, flags={:?}, mode={:?}",
                frame.shape.blue(),
                frame.data.as_byte_ptr(),
                frame.istate.flags.bright_magenta(),
                frame.istate.mode,
            );

            // Skip moved frames
            if frame.istate.flags.contains(FrameFlags::MOVED) {
                trace!(
                    "{}",
                    "Frame was moved out of, skipping initialization check".yellow()
                );
                continue;
            }

            // Check initialization for the current frame
            match frame.shape.def {
                Def::Struct(sd) => {
                    if !frame.is_fully_initialized() {
                        // find the field that's not initialized
                        for i in 0..sd.fields.len() {
                            if !frame.istate.fields.has(i) {
                                let field = &sd.fields[i];
                                return Err(ReflectError::UninitializedField {
                                    shape: frame.shape,
                                    field_name: field.name,
                                });
                            }
                        }
                        // Should be unreachable
                        unreachable!(
                            "Enum variant not fully initialized but couldn't find which field"
                        );
                    }

                    // If initialized, push children to check stack
                    #[allow(clippy::unused_enumerate_index)]
                    for (_i, field) in sd.fields.iter().enumerate() {
                        let field_shape = field.shape();
                        let field_ptr = unsafe { frame.data.field_init_at(field.offset) };
                        let field_id = ValueId::new(field_shape, field_ptr.as_byte_ptr());

                        if let Some(field_istate) = self.istates.remove(&field_id) {
                            debug!(
                                "Queueing struct field check: #{} '{}' of {}: shape={}, ptr={:p}",
                                _i.to_string().bright_cyan(),
                                field.name.bright_blue(),
                                frame.shape.blue(),
                                field_shape.green(),
                                field_ptr.as_byte_ptr()
                            );
                            let field_frame = Frame::recompose(field_id, field_istate);
                            to_check.push(field_frame);
                        }
                    }
                }
                Def::Enum(_ed) => {
                    if let Some(variant) = &frame.istate.variant {
                        if !frame.istate.fields.are_all_set(variant.data.fields.len()) {
                            // Find the uninitialized field
                            for (i, field) in variant.data.fields.iter().enumerate() {
                                if !frame.istate.fields.has(i) {
                                    return Err(ReflectError::UninitializedEnumField {
                                        shape: frame.shape,
                                        variant_name: variant.name,
                                        field_name: field.name,
                                    });
                                }
                            }
                            // Should be unreachable
                            unreachable!(
                                "Enum variant not fully initialized but couldn't find which field"
                            );
                        }

                        // If initialized, push children to check stack
                        #[allow(clippy::unused_enumerate_index)]
                        for (_i, field) in variant.data.fields.iter().enumerate() {
                            let field_shape = field.shape();
                            // Enum fields are potentially at different offsets depending on the variant layout.
                            // We assume the `frame.data` points to the start of the enum's data payload
                            // (after the discriminant if applicable and handled by layout).
                            let field_ptr = unsafe { frame.data.field_init_at(field.offset) };
                            let field_id = ValueId::new(field_shape, field_ptr.as_byte_ptr());

                            if let Some(field_istate) = self.istates.remove(&field_id) {
                                debug!(
                                    "Queueing enum field check: #{} '{}' of variant '{}' of {}: shape={}, ptr={:p}",
                                    _i.to_string().bright_cyan(),
                                    field.name.bright_blue(),
                                    variant.name.yellow(),
                                    frame.shape.blue(),
                                    field_shape.green(),
                                    field_ptr.as_byte_ptr()
                                );
                                let field_frame = Frame::recompose(field_id, field_istate);
                                to_check.push(field_frame);
                            }
                        }
                    } else {
                        // No variant selected is an error during build
                        return Err(ReflectError::NoVariantSelected { shape: frame.shape });
                    }
                }
                // For types that manage their own contents (List, Map, Option, Scalar, etc.),
                // we just need to check if the *container* itself is marked as initialized.
                // The recursive check handles struct/enum *elements* within these containers if they exist.
                Def::List(_)
                | Def::Map(_)
                | Def::Option(_)
                | Def::Scalar(_)
                | Def::SmartPointer(_)
                | Def::Array(_)
                | Def::Slice(_) => {
                    if !frame.istate.fields.are_all_set(1) {
                        // Check specific modes for better errors
                        match frame.istate.mode {
                            FrameMode::OptionNone => {
                                // This should technically be marked initialized, but if not, treat as uninit Option
                                return Err(ReflectError::UninitializedValue {
                                    shape: frame.shape,
                                });
                            }
                            // Add more specific checks if needed, e.g., for lists/maps that started but weren't finished?
                            _ => {
                                return Err(ReflectError::UninitializedValue {
                                    shape: frame.shape,
                                });
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
                }
                // Handle other Def variants if necessary
                _ => {
                    // Default: Check if initialized using the standard method
                    if !frame.istate.fields.are_all_set(1) {
                        return Err(ReflectError::UninitializedValue { shape: frame.shape });
                    }
                }
            }
        }

        // If we finished the loop, all reachable and non-moved frames are initialized.
        debug!("All reachable frames checked and initialized.");

        // 5. Check invariants on the root
        let data = unsafe { root_data_ptr.assume_init() };
        if let Some(invariant_fn) = root_shape.vtable.invariants {
            debug!(
                "Checking invariants for root shape {} at {:p}",
                root_shape.green(),
                data.as_byte_ptr()
            );
            if !unsafe { invariant_fn(PtrConst::new(data.as_byte_ptr())) } {
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

        FlatMap::clear(&mut self.istates); // Prevent Drop from running on the successfully built value.

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
    pub fn field(mut self, index: usize) -> Result<Self, ReflectError> {
        let frame = self.frames.last_mut().unwrap();
        let shape = frame.shape;

        let (field, field_offset) = match shape.def {
            Def::Struct(def) => {
                if index >= def.fields.len() {
                    return Err(ReflectError::FieldError {
                        shape,
                        field_error: FieldError::NoSuchField,
                    });
                }
                let field = &def.fields[index];
                (field, field.offset)
            }
            Def::Enum(_) => {
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
        let frame = self.frames.last()?;
        match frame.shape.def {
            Def::Struct(def) => def.fields.iter().position(|f| f.name == name),
            Def::Enum(_) => {
                // Get the selected variant
                let variant = frame.istate.variant.as_ref()?;
                variant.data.fields.iter().position(|f| f.name == name)
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
    pub fn field_named(self, name: &str) -> Result<Self, ReflectError> {
        let frame = self.frames.last().unwrap();
        let shape = frame.shape;

        // For enums, ensure a variant is selected
        if let Def::Enum(_) = shape.def {
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
    pub fn put<'val, T: Facet + 'val>(self, t: T) -> Result<Wip<'val>, ReflectError>
    where
        'a: 'val,
    {
        let shape = T::SHAPE;
        let ptr_const = PtrConst::new(&t as *const T as *const u8);
        let res = self.put_shape(ptr_const, shape);
        core::mem::forget(t); // avoid double drop; ownership moved into Wip
        res
    }

    /// Checks if the current frame is of type `T`.
    pub fn current_is_type<T: Facet + 'static>(&self) -> bool {
        self.frames
            .last()
            .is_some_and(|frame| frame.shape == T::SHAPE)
    }

    /// Puts a value from a `PtrConst` with the given shape into the current frame.
    pub fn put_shape<'val>(
        mut self,
        src: PtrConst<'val>,
        src_shape: &'static Shape,
    ) -> Result<Wip<'val>, ReflectError>
    where
        'a: 'val,
    {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: src_shape,
                operation: "tried to put a value but there was no frame to put into",
            });
        };

        // Check that the type matches
        if frame.shape != src_shape {
            trace!(
                "Trying to put a {} into a {}",
                src_shape.yellow(),
                frame.shape.magenta()
            );

            // Maybe there's a `TryFrom` impl?
            if let Some(try_from) = frame.shape.vtable.try_from {
                match unsafe { try_from(src, src_shape, frame.data) } {
                    Ok(_) => {
                        unsafe {
                            frame.mark_fully_initialized();
                        }

                        let shape = frame.shape;
                        let index = frame.field_index_in_parent;

                        // mark the field as initialized
                        self.mark_field_as_initialized(shape, index)?;

                        debug!("[{}] Just put a {} value", self.frames.len(), shape.green());

                        return Ok(self);
                    }
                    Err(e) => return Err(ReflectError::TryFromError { inner: e }),
                }
            }

            // Maybe we're putting into an Option<T>?
            // Handle Option<Inner>
            if let Def::Option(od) = frame.shape.def {
                // Check if inner type matches
                if od.t() == src_shape {
                    debug!("Putting into an Option<T>!");
                    if frame.istate.fields.is_any_set() {
                        let data = unsafe { frame.data.assume_init() };
                        unsafe { (od.vtable.replace_with_fn)(data, Some(src)) };
                    } else {
                        let data = frame.data;
                        unsafe { (od.vtable.init_some_fn)(data, src) };
                    }
                    unsafe {
                        frame.mark_fully_initialized();
                    }

                    let shape = frame.shape;
                    let index = frame.field_index_in_parent;

                    // mark the field as initialized
                    self.mark_field_as_initialized(shape, index)?;

                    debug!("[{}] Just put a {} value", self.frames.len(), shape.green());

                    return Ok(self);
                }
            }

            return Err(ReflectError::WrongShape {
                expected: frame.shape,
                actual: src_shape,
            });
        }

        // de-initialize partially initialized fields, if any
        if frame.istate.variant.is_some() || frame.istate.fields.is_any_set() {
            debug!(
                "De-initializing partially initialized fields for {}",
                frame.shape
            );

            match frame.shape.def {
                Def::Struct(sd) => {
                    for (i, field) in sd.fields.iter().enumerate() {
                        if frame.istate.fields.has(i) {
                            if let Some(drop_fn) = field.shape().vtable.drop_in_place {
                                unsafe {
                                    let field_ptr = frame.data.as_mut_byte_ptr().add(field.offset);
                                    drop_fn(PtrMut::new(field_ptr));
                                }
                            }
                        }
                    }
                }
                Def::Enum(_) => {
                    if let Some(variant) = &frame.istate.variant {
                        for (i, field) in variant.data.fields.iter().enumerate() {
                            if frame.istate.fields.has(i) {
                                if let Some(drop_fn) = field.shape().vtable.drop_in_place {
                                    unsafe {
                                        let field_ptr =
                                            frame.data.as_mut_byte_ptr().add(field.offset);
                                        drop_fn(PtrMut::new(field_ptr));
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {
                    // For scalar types, nothing to do if not fully initialized
                }
            }

            // Reset initialization state
            frame.istate.variant = None;
            ISet::clear(&mut frame.istate.fields);
        }

        unsafe {
            // Copy the contents from src to destination
            frame.data.copy_from(src, frame.shape);
            frame.mark_fully_initialized();
        }

        let shape = frame.shape;
        let index = frame.field_index_in_parent;

        // mark the field as initialized
        self.mark_field_as_initialized(shape, index)?;

        debug!("[{}] Just put a {} value", self.frames.len(), shape.green());

        Ok(self)
    }

    /// Tries to parse the current frame's value from a string
    pub fn parse(mut self, s: &str) -> Result<Self, ReflectError> {
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
    pub fn put_from_fn(mut self, default_in_place: DefaultInPlaceFn) -> Result<Self, ReflectError> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to put value from fn but there was no frame",
            });
        };

        unsafe {
            default_in_place(frame.data);
            frame.mark_fully_initialized();
        }

        let shape = frame.shape;
        let index = frame.field_index_in_parent;

        // mark the field as initialized
        self.mark_field_as_initialized(shape, index)?;

        Ok(self)
    }

    /// Puts the default value in the current frame.
    pub fn put_default(self) -> Result<Self, ReflectError> {
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
        shape: &'static Shape,
        index: Option<usize>,
    ) -> Result<(), ReflectError> {
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

            if matches!(parent.shape.def, Def::Enum(_)) && parent.istate.variant.is_none() {
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
    pub fn element_shape(&self) -> Result<&'static Shape, ReflectError> {
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
    pub fn key_shape(&self) -> Result<&'static Shape, ReflectError> {
        let frame = self.frames.last().unwrap();
        let shape = frame.shape;

        match shape.def {
            Def::Map(map_def) => Ok(map_def.k),
            _ => Err(ReflectError::WasNotA {
                expected: "map",
                actual: shape,
            }),
        }
    }

    /// Creates an empty list without pushing any elements
    pub fn put_empty_list(mut self) -> Result<Self, ReflectError> {
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
    pub fn put_empty_map(mut self) -> Result<Self, ReflectError> {
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

        let shape = frame.shape;
        let index = frame.field_index_in_parent;

        // Mark the field as initialized
        self.mark_field_as_initialized(shape, index)?;

        Ok(self)
    }

    /// Begins pushback mode for a list/array, allowing elements to be added one by one
    pub fn begin_pushback(mut self) -> Result<Self, ReflectError> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to begin pushback but there was no frame",
            });
        };

        if !matches!(frame.shape.def, Def::List(_)) {
            return Err(ReflectError::WasNotA {
                expected: "list or array",
                actual: frame.shape,
            });
        }

        let vtable = frame.shape.vtable;

        // Initialize an empty list if it's not already initialized
        if !frame.istate.fields.has(0) {
            let Some(default_in_place) = vtable.default_in_place else {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "list type does not implement Default",
                });
            };

            unsafe {
                default_in_place(frame.data);
                frame.istate.fields.set(0);
            }
        }

        Ok(self)
    }

    /// Begins insertion mode for a map, allowing key-value pairs to be added one by one
    pub fn begin_map_insert(mut self) -> Result<Self, ReflectError> {
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

    /// Pushes a new element onto the list/array
    ///
    /// This creates a new frame for the element. When this frame is popped,
    /// the element will be added to the list.
    pub fn push(mut self) -> Result<Self, ReflectError> {
        // Make sure we're initializing a list
        let frame = self.frames.last().unwrap();
        let list_shape = frame.shape;

        if !matches!(list_shape.def, Def::List(_)) {
            return Err(ReflectError::WasNotA {
                expected: "list or array",
                actual: list_shape,
            });
        }

        // If the list isn't initialized yet, initialize it
        if !frame.istate.fields.has(0) {
            self = self.begin_pushback()?;
        }

        // Get the element type
        let element_shape = self.element_shape()?;

        // Allocate memory for the element
        let element_data = element_shape.allocate();

        // Create a new frame for the element
        let element_frame = Frame {
            data: element_data,
            shape: element_shape,
            field_index_in_parent: None, // No need for an index since we're using mode
            istate: IState::new(
                self.frames.len(),
                FrameMode::ListElement,
                FrameFlags::ALLOCATED,
            ),
        };

        trace!(
            "[{}] Pushing element of type {} to list {}",
            self.frames.len(),
            element_shape.green(),
            list_shape.blue(),
        );

        self.frames.push(element_frame);
        Ok(self)
    }

    /// Prepare to push the `Some(T)` variant of an `Option<T>`.
    pub fn push_some(mut self) -> Result<Self, ReflectError> {
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
        let inner_data = inner_shape.allocate();

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
    pub fn pop_some_push_none(mut self) -> Result<Self, ReflectError> {
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
    pub fn push_map_key(mut self) -> Result<Self, ReflectError> {
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

        // Get the key type
        let key_shape = self.key_shape()?;

        // Allocate memory for the key
        let key_data = key_shape.allocate();

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
    pub fn push_map_value(mut self) -> Result<Self, ReflectError> {
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

        let value_shape = map_def.v;

        // Allocate memory for the value
        let value_data = value_shape.allocate();

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

    /// Pops the current frame — goes back up one level
    pub fn pop(mut self) -> Result<Self, ReflectError> {
        let Some(frame) = self.pop_inner() else {
            return Err(ReflectError::InvariantViolation {
                invariant: "No frame to pop — it was time to call build()",
            });
        };
        self.track(frame);
        Ok(self)
    }

    fn pop_inner(&mut self) -> Option<Frame> {
        let mut frame = self.frames.pop()?;
        #[cfg(feature = "log")]
        let frame_shape = frame.shape;

        let init = frame.is_fully_initialized();
        trace!(
            "[{}] {} popped, {} initialized",
            self.frames.len(),
            frame_shape.blue(),
            if init {
                "✅ fully".style(owo_colors::Style::new().green())
            } else {
                "🚧 partially".style(owo_colors::Style::new().red())
            }
        );
        if init {
            if let Some(parent) = self.frames.last_mut() {
                if let Some(index) = frame.field_index_in_parent {
                    parent.istate.fields.set(index);
                }
            }
        }

        // Handle special frame modes
        match frame.istate.mode {
            // Handle list element frames
            FrameMode::ListElement => {
                if frame.is_fully_initialized() {
                    // This was a list element, so we need to push it to the parent list
                    // Capture frame length and parent shape before mutable borrow
                    #[cfg(feature = "log")]
                    let frame_len = self.frames.len();

                    // Get parent frame
                    let parent_frame = self.frames.last_mut().unwrap();
                    let parent_shape = parent_frame.shape;

                    // Make sure the parent is a list
                    match parent_shape.def {
                        Def::List(_) => {
                            // Get the list vtable from the ListDef
                            if let Def::List(list_def) = parent_shape.def {
                                let list_vtable = list_def.vtable;
                                trace!(
                                    "[{}] Pushing element to list {}",
                                    frame_len,
                                    parent_shape.blue()
                                );
                                unsafe {
                                    // Convert the frame data pointer to Opaque and call push function from vtable
                                    (list_vtable.push)(
                                        PtrMut::new(parent_frame.data.as_mut_byte_ptr()),
                                        PtrMut::new(frame.data.as_mut_byte_ptr()),
                                    );
                                    self.mark_moved_out_of(&mut frame);
                                }
                            } else {
                                panic!("parent frame is not a list type");
                            }
                        }
                        _ => {
                            panic!("Expected list or array, got {}", frame.shape);
                        }
                    }
                }
            }

            // Handle map value frames
            FrameMode::MapValue {
                index: key_frame_index,
            } if frame.is_fully_initialized() => {
                // This was a map value, so we need to insert the key-value pair into the map

                // Now let's remove the key frame from the frames array
                let mut key_frame = self.frames.remove(key_frame_index);

                // Make sure the key is fully initialized
                if !key_frame.istate.fields.is_any_set() {
                    panic!("key is not initialized when popping value frame");
                }

                // Get parent map frame
                #[cfg(feature = "log")]
                let frame_len = self.frames.len();
                let parent_frame = self.frames.last_mut().unwrap();
                let parent_shape = parent_frame.shape;

                // Make sure the parent is a map
                match parent_shape.def {
                    Def::Map(_) => {
                        // Get the map vtable from the MapDef
                        if let Def::Map(map_def) = parent_shape.def {
                            trace!(
                                "[{}] Inserting key-value pair into map {}",
                                frame_len,
                                parent_shape.blue()
                            );
                            unsafe {
                                // Call the map's insert function with the key and value
                                (map_def.vtable.insert_fn)(
                                    parent_frame.data.assume_init(),
                                    key_frame.data.assume_init(),
                                    PtrMut::new(frame.data.as_mut_byte_ptr()),
                                );
                                self.mark_moved_out_of(&mut key_frame);
                                self.mark_moved_out_of(&mut frame);
                            }
                        } else {
                            panic!("parent frame is not a map type");
                        }
                    }
                    _ => {
                        panic!("Expected map or hash map, got {}", frame.shape);
                    }
                }
            }

            // Handle option frames
            FrameMode::OptionSome => {
                if frame.is_fully_initialized() {
                    trace!("Popping OptionSome (fully init'd)");

                    // Get parent frame
                    #[cfg(feature = "log")]
                    let frames_len = self.frames.len();
                    let parent_frame = self.frames.last_mut().unwrap();
                    let parent_shape = parent_frame.shape;

                    // Make sure the parent is an option
                    match parent_shape.def {
                        Def::Option(option_def) => {
                            trace!(
                                "[{}] Setting Some value in option {}",
                                frames_len,
                                parent_shape.blue()
                            );
                            unsafe {
                                // Call the option's init_some function
                                (option_def.vtable.init_some_fn)(
                                    parent_frame.data,
                                    PtrConst::new(frame.data.as_byte_ptr()),
                                );
                                trace!("Marking parent frame as fully initialized");
                                parent_frame.mark_fully_initialized();

                                self.mark_moved_out_of(&mut frame);
                            }
                        }
                        _ => {
                            panic!(
                                "Expected parent frame to be an option type, got {}",
                                frame.shape
                            );
                        }
                    }
                } else {
                    trace!("Popping OptionSome (not fully init'd)");
                }
            }

            // Map keys are just tracked, they don't need special handling when popped
            // FIXME: that's not true, we need to deallocate them at least??
            FrameMode::MapKey => {}

            // Field frame
            FrameMode::Field => {}

            // Uninitialized special frames
            _ => {}
        }

        Some(frame)
    }

    /// Evict a frame from istates, along with all its children
    /// (because we're about to use `drop_in_place` on it — not
    /// yet though, we need to know the variant for enums, etc.)
    pub fn evict_tree(&mut self, frame: Frame) -> Frame {
        match frame.shape.def {
            Def::Struct(sd) => {
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
            Def::Enum(_ed) => {
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
                            if let Def::Struct(sd) = parent.shape.def {
                                if index < sd.fields.len() {
                                    let field_name = sd.fields[index].name;
                                    path.push('.');
                                    path.push_str(field_name);
                                }
                            } else if let Def::Enum(_) = parent.shape.def {
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
    pub fn is_field_set(&self, index: usize) -> Result<bool, ReflectError> {
        let frame = self.frames.last().ok_or(ReflectError::OperationFailed {
            shape: <()>::SHAPE,
            operation: "tried to check if field is set, but there was no frame",
        })?;

        match frame.shape.def {
            Def::Struct(ref sd) => {
                if index >= sd.fields.len() {
                    return Err(ReflectError::FieldError {
                        shape: frame.shape,
                        field_error: FieldError::NoSuchField,
                    });
                }
                Ok(frame.istate.fields.has(index))
            }
            Def::Enum(_) => {
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

impl Drop for Wip<'_> {
    fn drop(&mut self) {
        trace!("🧹🧹🧹 WIP is dropping");

        while let Some(frame) = self.frames.pop() {
            self.track(frame);
        }

        let Some((root_id, _)) = self.istates.iter().find(|(_k, istate)| istate.depth == 0) else {
            trace!("No root found, we probably built already");
            return;
        };

        let root_id = *root_id;
        let root_istate = self.istates.remove(&root_id).unwrap();
        let root = Frame::recompose(root_id, root_istate);
        let mut to_clean = vec![root];

        let mut _root_guard: Option<Guard> = None;

        while let Some(mut frame) = to_clean.pop() {
            trace!(
                "Cleaning frame: shape={} at {:p}, flags={:?}, mode={:?}, fully_initialized={}",
                frame.shape.blue(),
                frame.data.as_byte_ptr(),
                frame.istate.flags.bright_magenta(),
                frame.istate.mode.yellow(),
                if frame.is_fully_initialized() {
                    "✅"
                } else {
                    "❌"
                }
            );

            if frame.istate.flags.contains(FrameFlags::MOVED) {
                trace!(
                    "{}",
                    "Frame was moved out of, nothing to dealloc/drop_in_place".yellow()
                );
                continue;
            }

            match frame.shape.def {
                Def::Struct(sd) => {
                    if frame.is_fully_initialized() {
                        trace!(
                            "Dropping fully initialized struct: {} at {:p}",
                            frame.shape.green(),
                            frame.data.as_byte_ptr()
                        );
                        let frame = self.evict_tree(frame);
                        unsafe { frame.drop_and_dealloc_if_needed() };
                    } else {
                        let num_fields = sd.fields.len();
                        trace!(
                            "De-initializing struct {} at {:p} field-by-field ({} fields)",
                            frame.shape.yellow(),
                            frame.data.as_byte_ptr(),
                            num_fields.to_string().bright_cyan()
                        );
                        for i in 0..num_fields {
                            if frame.istate.fields.has(i) {
                                let field = sd.fields[i];
                                let field_shape = field.shape();
                                let field_ptr = unsafe { frame.data.field_init_at(field.offset) };
                                let field_id = ValueId::new(field_shape, field_ptr.as_byte_ptr());
                                trace!(
                                    "Recursively cleaning field #{} '{}' of {}: field_shape={}, field_ptr={:p}",
                                    i.to_string().bright_cyan(),
                                    field.name.bright_blue(),
                                    frame.shape.blue(),
                                    field_shape.green(),
                                    field_ptr.as_byte_ptr()
                                );
                                let istate = self.istates.remove(&field_id).unwrap();
                                let field_frame = Frame::recompose(field_id, istate);
                                to_clean.push(field_frame);
                            } else {
                                trace!(
                                    "Field #{} '{}' of {} was NOT initialized, skipping",
                                    i.to_string().bright_cyan(),
                                    sd.fields[i].name.bright_red(),
                                    frame.shape.red()
                                );
                            }
                        }

                        // we'll also need to clean up if we're root
                        if frame.istate.mode == FrameMode::Root {
                            _root_guard = Some(Guard {
                                ptr: frame.data.as_mut_byte_ptr(),
                                layout: frame.shape.layout,
                            });
                        }
                    }
                }
                Def::Enum(_ed) => {
                    trace!(
                        "{}",
                        format!(
                            "TODO: handle enum deallocation for {} at {:p}",
                            frame.shape.yellow(),
                            frame.data.as_byte_ptr()
                        )
                        .magenta()
                    );

                    // we'll also need to clean up if we're root
                    if frame.istate.mode == FrameMode::Root {
                        _root_guard = Some(Guard {
                            ptr: frame.data.as_mut_byte_ptr(),
                            layout: frame.shape.layout,
                        });
                    }
                }
                Def::Array(_)
                | Def::Slice(_)
                | Def::List(_)
                | Def::Map(_)
                | Def::SmartPointer(_)
                | Def::Scalar(_)
                | Def::Option(_) => {
                    trace!(
                        "Can drop all at once for shape {} (def variant: {:?}, frame mode {:?}) at {:p}",
                        frame.shape.cyan(),
                        frame.shape.def,
                        frame.istate.mode.yellow(),
                        frame.data.as_byte_ptr(),
                    );

                    if frame.is_fully_initialized() {
                        unsafe { frame.drop_and_dealloc_if_needed() }
                    } else {
                        frame.dealloc_if_needed();
                    }
                }
                _ => {}
            }
        }

        // We might have some frames left over to deallocate for temporary allocations for keymap insertion etc.
        let mut all_ids = self.istates.keys().copied().collect::<Vec<_>>();
        for frame_id in all_ids.drain(..) {
            let frame_istate = self.istates.remove(&frame_id).unwrap();

            trace!(
                "Checking leftover istate: id.shape={} id.ptr={:p} mode={:?}",
                frame_id.shape.cyan(),
                frame_id.ptr,
                frame_istate.mode.yellow()
            );
            let mut frame = Frame::recompose(frame_id, frame_istate);

            if frame.is_fully_initialized() {
                trace!("It's fully initialized, we can drop it");
                unsafe { frame.drop_and_dealloc_if_needed() };
            } else if frame.istate.flags.contains(FrameFlags::ALLOCATED) {
                trace!("Not initialized but allocated, let's free it");
                frame.dealloc_if_needed();
            }
        }
    }
}
