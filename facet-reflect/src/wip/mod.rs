use crate::{ReflectError, ValueId};
use core::{alloc::Layout, fmt, marker::PhantomData};
use facet_ansi::Stylize;
use facet_core::{Def, Facet, FieldError, PtrConst, PtrMut, PtrUninit, Shape, Variant};
use flat_map::FlatMap;

mod enum_;
mod flat_map;

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
    index: Option<usize>,

    /// Tracking which of our fields are initialized
    /// TODO: I'm not sure we should track "ourselves" as initialized — we always have the
    /// parent to look out for, right now we're tracking children in two states, which isn't ideal
    istate: IState,
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
            .field("index", &self.index)
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
}

impl IState {
    /// Creates a new `IState` with the given depth.
    pub fn new(depth: usize) -> Self {
        Self {
            variant: None,
            fields: Default::default(),
            depth,
            mode: FrameMode::Normal,
        }
    }
}

/// Represents the special mode a frame can be in
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameMode {
    /// Normal frame
    Normal,
    /// Frame represents a list element
    ListElement,
    /// Frame represents a map key
    MapKey,
    /// Frame represents a map value with the given key frame index
    MapValue(usize),
}

/// A work-in-progress heap-allocated value
pub struct Wip<'a> {
    /// frees the memory when dropped
    guard: Option<Guard>,

    /// stack of frames to keep track of deeply nested initialization
    frames: alloc::vec::Vec<Frame>,

    /// keeps track of initialization of out-of-tree frames
    istates: FlatMap<ValueId, IState>,

    /// lifetime of the shortest reference we hold
    phantom: PhantomData<&'a ()>,
}

impl<'a> Wip<'a> {
    /// Returns the number of frames on the stack
    pub fn frames_count(&self) -> usize {
        self.frames.len()
    }

    /// Allocates a new value of the given shape
    pub fn alloc_shape(shape: &'static Shape) -> Self {
        let data = shape.allocate();
        let guard = Guard {
            ptr: data.as_mut_byte_ptr(),
            layout: shape.layout,
        };
        Self {
            guard: Some(guard),
            frames: alloc::vec![Frame {
                data,
                shape,
                index: None,
                istate: IState::new(0),
            }],
            istates: Default::default(),
            phantom: PhantomData,
        }
    }

    /// Allocates a new value of type `S`
    pub fn alloc<S: Facet>() -> Self {
        Self::alloc_shape(S::SHAPE)
    }

    fn pop_inner(&mut self) -> Option<Frame> {
        let frame = self.frames.pop()?;
        let frame_shape = frame.shape;

        let init = frame.is_fully_initialized();
        trace!(
            "[{}] {} popped, {} initialized",
            self.frames.len(),
            frame_shape.blue(),
            if init {
                "✅ fully".green()
            } else {
                "🚧 partially".red()
            }
        );
        if init {
            if let Some(parent) = self.frames.last_mut() {
                if let Some(index) = frame.index {
                    parent.istate.fields.set(index);
                }
            }
        }

        Some(frame)
    }

    fn track(&mut self, frame: Frame) {
        self.istates.insert(frame.id(), frame.istate);
    }

    /// Returns the shape of the current frame
    pub fn shape(&self) -> &'static Shape {
        self.frames.last().unwrap().shape
    }

    /// Returns the mode of the current frame
    pub fn mode(&self) -> FrameMode {
        self.frames.last().unwrap().istate.mode
    }

    /// Asserts everything is initialized and that invariants are upheld (if any)
    pub fn build(mut self) -> Result<HeapValue<'a>, ReflectError> {
        let mut root: Option<Frame> = None;
        while let Some(frame) = self.pop_inner() {
            if let Some(old_root) = root.replace(frame) {
                self.track(old_root);
            }
        }
        let Some(root) = root else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to build a value but there was no root frame",
            });
        };

        let shape = root.shape;
        let data = unsafe { root.data.assume_init() };

        self.istates.insert(root.id(), root.istate);

        for (id, is) in self.istates.iter() {
            let field_count = match id.shape.def {
                Def::Struct(def) => def.fields.len(),
                Def::Enum(_) => {
                    if let Some(variant) = &is.variant {
                        variant.data.fields.len()
                    } else {
                        // If no variant is selected, we should have zero fields initialized
                        0
                    }
                }
                _ => 1,
            };
            if !is.fields.are_all_set(field_count) {
                match id.shape.def {
                    Def::Struct(sd) => {
                        for (i, field) in sd.fields.iter().enumerate() {
                            if !is.fields.has(i) {
                                return Err(ReflectError::UninitializedField {
                                    shape: id.shape,
                                    field_name: field.name,
                                });
                            }
                        }
                    }
                    Def::Enum(_) => {
                        if let Some(variant) = &is.variant {
                            for (i, field) in variant.data.fields.iter().enumerate() {
                                if !is.fields.has(i) {
                                    return Err(ReflectError::UninitializedEnumField {
                                        shape: id.shape,
                                        field_name: field.name,
                                        variant_name: variant.name,
                                    });
                                }
                            }
                        } else {
                            return Err(ReflectError::NoVariantSelected { shape: id.shape });
                        }
                    }
                    Def::Scalar(_) => {
                        return Err(ReflectError::UninitializedScalar { shape: id.shape });
                    }
                    _ => {}
                }
            }
        }

        if let Some(invariant_fn) = shape.vtable.invariants {
            if !unsafe { invariant_fn(PtrConst::new(data.as_byte_ptr())) } {
                return Err(ReflectError::InvariantViolation {
                    invariant: "Custom validation function returned false",
                });
            }
        }

        let guard = self.guard.take().unwrap();

        // don't double-drop the fields!
        self.istates.clear();

        Ok(HeapValue {
            guard: Some(guard),
            shape,
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
            index: Some(index),
            istate: IState::new(self.frames.len()),
        };
        trace!(
            "[{}] Selecting field {} ({}#{}) of {}",
            self.frames.len(),
            field.name.blue(),
            field.shape().green(),
            index.yellow(),
            shape.blue(),
        );
        if let Some(iset) = self.istates.remove(&frame.id()) {
            trace!(
                "[{}] Restoring saved state for {}",
                self.frames.len(),
                frame.id().shape.blue()
            );
            frame.istate = iset;
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
    pub fn put<'val, T: Facet + 'val>(mut self, t: T) -> Result<Wip<'val>, ReflectError>
    where
        'a: 'val,
    {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: T::SHAPE,
                operation: "tried to put a T but there was no frame to put T into",
            });
        };

        // check that the type matches
        if !frame.shape.is_type::<T>() {
            return Err(ReflectError::WrongShape {
                expected: frame.shape,
                actual: T::SHAPE,
            });
        }

        // de-initialize partially initialized fields
        if frame.istate.variant.is_some() || frame.istate.fields.is_any_set() {
            trace!(
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
            frame.istate.fields.clear();
        }

        unsafe {
            frame.data.put(t);
            frame.mark_fully_initialized();
        }

        let shape = frame.shape;
        let index = frame.index;

        // mark the field as initialized
        self.mark_field_as_initialized(shape, index)?;

        trace!("[{}] Just put a {} value", self.frames.len(), shape.green());

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
        let index = frame.index;

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

    /// Puts the default value in the currrent frame.
    pub fn put_default(mut self) -> Result<Self, ReflectError> {
        let Some(frame) = self.frames.last_mut() else {
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
        unsafe {
            default_in_place(frame.data);
            frame.mark_fully_initialized();
        }

        let shape = frame.shape;
        let index = frame.index;

        // mark the field as initialized
        self.mark_field_as_initialized(shape, index)?;

        Ok(self)
    }

    /// Marks a field as initialized in the parent frame.
    fn mark_field_as_initialized(
        &mut self,
        shape: &'static Shape,
        index: Option<usize>,
    ) -> Result<(), ReflectError> {
        if let Some(index) = index {
            let parent_index = self.frames.len().saturating_sub(2);
            let num_frames = self.frames.len();
            let Some(parent) = self.frames.get_mut(parent_index) else {
                return Err(ReflectError::OperationFailed {
                    shape,
                    operation: "was supposed to mark a field as initialized, but there was no parent frame",
                });
            };
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
        let index = frame.index;

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
        let index = frame.index;

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
        let mut element_frame = Frame {
            data: element_data,
            shape: element_shape,
            index: None, // No need for an index since we're using mode
            istate: IState::new(self.frames.len()),
        };

        // Mark this as a list element
        element_frame.istate.mode = FrameMode::ListElement;

        trace!(
            "[{}] Pushing element of type {} to list {}",
            self.frames.len(),
            element_shape.green(),
            list_shape.blue(),
        );

        if let Some(iset) = self.istates.remove(&element_frame.id()) {
            trace!(
                "[{}] Restoring saved state for {}",
                self.frames.len(),
                element_frame.id().shape.blue()
            );
            element_frame.istate = iset;
        }

        self.frames.push(element_frame);
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
        let mut key_frame = Frame {
            data: key_data,
            shape: key_shape,
            index: None,
            istate: IState::new(self.frames.len()),
        };

        // Mark this as a map key
        key_frame.istate.mode = FrameMode::MapKey;

        trace!(
            "[{}] Pushing key of type {} for map {}",
            self.frames.len(),
            key_shape.green(),
            map_shape.blue(),
        );

        if let Some(iset) = self.istates.remove(&key_frame.id()) {
            trace!(
                "[{}] Restoring saved state for {}",
                self.frames.len(),
                key_frame.id().shape.blue()
            );
            key_frame.istate = iset;
        }

        self.frames.push(key_frame);
        Ok(self)
    }

    /// Pushes a new value frame for a map entry
    ///
    /// This should be called after pushing and initializing a key frame.
    /// When the value frame is popped, the key-value pair will be added to the map.
    pub fn push_map_value(mut self) -> Result<Self, ReflectError> {
        trace!("Wants to push map value. Frames = ");
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
        let mut value_frame = Frame {
            data: value_data,
            shape: value_shape,
            index: None,
            istate: IState::new(self.frames.len()),
        };

        // Mark this as a map value and store the key frame index
        value_frame.istate.mode = FrameMode::MapValue(key_frame_index);

        trace!(
            "[{}] Pushing value of type {} for map {} with key type {}",
            self.frames.len(),
            value_shape.green(),
            map_shape.blue(),
            key_frame.shape.yellow(),
        );

        if let Some(iset) = self.istates.remove(&value_frame.id()) {
            trace!(
                "[{}] Restoring saved state for {}",
                self.frames.len(),
                value_frame.id().shape.blue()
            );
            value_frame.istate = iset;
        }

        self.frames.push(value_frame);
        Ok(self)
    }

    /// Pops the current frame — goes back up one level
    pub fn pop(mut self) -> Result<Self, ReflectError> {
        let frame = match self.frames.len() {
            0 => Err(ReflectError::InvariantViolation {
                invariant: "No frame to pop",
            }),
            1 => Err(ReflectError::InvariantViolation {
                invariant: "The last frame should be popped through build",
            }),
            _ => Ok(self.pop_inner().unwrap()),
        }?;

        // Handle special frame modes
        match frame.istate.mode {
            // Handle list element frames
            FrameMode::ListElement if frame.is_fully_initialized() => {
                // This was a list element, so we need to push it to the parent list
                // Capture frame length and parent shape before mutable borrow
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

                                // now let's deallocate the value
                                alloc::alloc::dealloc(
                                    frame.data.as_mut_byte_ptr(),
                                    frame.shape.layout,
                                );

                                // and make sure we don't drop in place (it's been moved!)
                                return Ok(self);
                            }
                        } else {
                            return Err(ReflectError::OperationFailed {
                                shape: parent_frame.shape,
                                operation: "parent frame is not a list type",
                            });
                        }
                    }
                    _ => {
                        return Err(ReflectError::WasNotA {
                            expected: "list or array",
                            actual: frame.shape,
                        });
                    }
                }
            }

            // Handle map value frames
            FrameMode::MapValue(key_frame_index) if frame.is_fully_initialized() => {
                // This was a map value, so we need to insert the key-value pair into the map

                // Now let's remove the key frame from the frames array
                let key_frame = self.frames.remove(key_frame_index);
                let key_istate = key_frame.istate;

                // Make sure the key is fully initialized
                if !key_istate.fields.is_any_set() {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "key is not initialized when popping value frame",
                    });
                }

                // Get parent map frame
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

                                // now let's deallocate the key and the value
                                alloc::alloc::dealloc(
                                    key_frame.data.as_mut_byte_ptr(),
                                    key_frame.shape.layout,
                                );
                                alloc::alloc::dealloc(
                                    frame.data.as_mut_byte_ptr(),
                                    frame.shape.layout,
                                );
                            }

                            // now make sure the value frame doesn't accidentally end up tracked
                            return Ok(self);
                        } else {
                            return Err(ReflectError::OperationFailed {
                                shape: parent_frame.shape,
                                operation: "parent frame is not a map type",
                            });
                        }
                    }
                    _ => {
                        return Err(ReflectError::WasNotA {
                            expected: "map or hash map",
                            actual: frame.shape,
                        });
                    }
                }
            }

            // Map keys are just tracked, they don't need special handling when popped
            // FIXME: that's not true, we need to deallocate them at least??
            FrameMode::MapKey => {}

            // Normal frame
            FrameMode::Normal => {}

            // Uninitialized special frames
            _ => {}
        }

        self.track(frame);
        Ok(self)
    }
}

impl Drop for Wip<'_> {
    fn drop(&mut self) {
        while let Some(frame) = self.frames.pop() {
            self.track(frame);
        }

        log::trace!(
            "[{}] 🚯 Dropping, was tracking {} istates",
            self.frames.len(),
            self.istates.len()
        );
        for (id, is) in self.istates.iter() {
            log::trace!(
                "[{}]: variant={:?} initialized={:016b} {}",
                is.depth.yellow(),
                is.variant.green(),
                is.fields.0.bright_magenta(),
                id.shape.blue(),
            );
        }

        let mut depths: alloc::vec::Vec<usize> = self.istates.values().map(|is| is.depth).collect();
        depths.sort_unstable();
        depths.dedup();

        for depth in depths.iter().rev() {
            log::trace!("Dropping istates with depth {}", depth.yellow(),);

            // Find and drop values at this depth level
            self.istates.retain(|id, is| {
                if is.depth == *depth {
                    log::trace!(
                        "Dropping value ID with shape {} and fields {:016b}",
                        id.shape.blue(),
                        is.fields.0.bright_magenta()
                    );

                    if is.fields.is_any_set() {
                        if matches!(id.shape.def, Def::Struct(_) | Def::Enum(_)) {
                            // if it's a composite, rely on the fact that each individual field was deinitialized
                            log::trace!("  Skipping composite type drop: individual fields already deinitialized");
                            return false;
                        }

                        if let Some(drop_fn) = id.shape.vtable.drop_in_place {
                            // Only drop if some fields were initialized
                            if is.fields.is_any_set() {
                                log::trace!(
                                    "  Calling drop_in_place function for {}",
                                    id.shape.green()
                                );
                                unsafe {
                                    drop_fn(PtrMut::new(id.ptr as *mut u8));
                                }
                            }
                        } else {
                            log::trace!(
                                "  No drop_in_place function available for {}",
                                id.shape.red()
                            );
                        }
                    }

                    match is.mode {
                        FrameMode::MapKey | FrameMode::MapValue(_) | FrameMode::ListElement => {
                            // hey we initialized those, we have to free them
                            unsafe {
                                trace!("  Freeing {}", id.shape.green());
                                alloc::alloc::dealloc(id.ptr as *mut u8, id.shape.layout);
                            }
                        }
                        _ => {
                        }
                    }

                    return false;
                }
                true
            });
        }
    }
}

/// A guard structure to manage memory allocation and deallocation.
///
/// This struct holds a raw pointer to the allocated memory and the layout
/// information used for allocation. It's responsible for deallocating
/// the memory when dropped.
pub struct Guard {
    /// Raw pointer to the allocated memory.
    ptr: *mut u8,
    /// Layout information of the allocated memory.
    layout: Layout,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.layout.size() != 0 {
            // SAFETY: `ptr` has been allocated via the global allocator with the given layout
            unsafe { alloc::alloc::dealloc(self.ptr, self.layout) };
        }
    }
}

use facet_core::Field;
use log::trace;

/// Keeps track of which fields were initialized, up to 64 fields
#[derive(Clone, Copy, Default, Debug)]
pub struct ISet(u64);

impl ISet {
    /// The maximum index that can be tracked.
    pub const MAX_INDEX: usize = 63;

    /// Creates a new ISet with all (given) fields set.
    pub fn all(fields: &[Field]) -> Self {
        let mut iset = ISet::default();
        for (i, _field) in fields.iter().enumerate() {
            iset.set(i);
        }
        iset
    }

    /// Sets the bit at the given index.
    pub fn set(&mut self, index: usize) {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        self.0 |= 1 << index;
    }

    /// Unsets the bit at the given index.
    pub fn unset(&mut self, index: usize) {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        self.0 &= !(1 << index);
    }

    /// Checks if the bit at the given index is set.
    pub fn has(&self, index: usize) -> bool {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        (self.0 & (1 << index)) != 0
    }

    /// Checks if all bits up to the given count are set.
    pub fn are_all_set(&self, count: usize) -> bool {
        if count > 64 {
            panic!("ISet can only track up to 64 fields. Count {count} is out of bounds.");
        }
        let mask = (1 << count) - 1;
        self.0 & mask == mask
    }

    /// Checks if any bit in the ISet is set.
    pub fn is_any_set(&self) -> bool {
        self.0 != 0
    }

    /// Clears all bits in the ISet.
    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

/// A type-erased value stored on the heap
pub struct HeapValue<'a> {
    guard: Option<Guard>,
    shape: &'static Shape,
    phantom: PhantomData<&'a ()>,
}

impl Drop for HeapValue<'_> {
    fn drop(&mut self) {
        if let Some(guard) = self.guard.take() {
            if let Some(drop_fn) = self.shape.vtable.drop_in_place {
                unsafe { drop_fn(PtrMut::new(guard.ptr)) };
            }
            drop(guard);
        }
    }
}

impl<'a> HeapValue<'a> {
    /// Turn this heapvalue into a concrete type
    pub fn materialize<T: Facet + 'a>(mut self) -> Result<T, ReflectError> {
        if self.shape != T::SHAPE {
            return Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            });
        }

        let guard = self.guard.take().unwrap();
        let data = PtrConst::new(guard.ptr);
        let res = unsafe { data.read::<T>() };
        drop(guard); // free memory (but don't drop in place)
        Ok(res)
    }
}

impl HeapValue<'_> {
    /// Formats the value using its Display implementation, if available
    pub fn fmt_display(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(display_fn) = self.shape.vtable.display {
            unsafe { display_fn(PtrConst::new(self.guard.as_ref().unwrap().ptr), f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }

    /// Formats the value using its Debug implementation, if available
    pub fn fmt_debug(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(debug_fn) = self.shape.vtable.debug {
            unsafe { debug_fn(PtrConst::new(self.guard.as_ref().unwrap().ptr), f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }
}

impl core::fmt::Display for HeapValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.fmt_display(f)
    }
}

impl core::fmt::Debug for HeapValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.fmt_debug(f)
    }
}

impl PartialEq for HeapValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        if self.shape != other.shape {
            return false;
        }
        if let Some(eq_fn) = self.shape.vtable.eq {
            unsafe {
                eq_fn(
                    PtrConst::new(self.guard.as_ref().unwrap().ptr),
                    PtrConst::new(other.guard.as_ref().unwrap().ptr),
                )
            }
        } else {
            false
        }
    }
}

impl PartialOrd for HeapValue<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        if self.shape != other.shape {
            return None;
        }
        if let Some(partial_ord_fn) = self.shape.vtable.partial_ord {
            unsafe {
                partial_ord_fn(
                    PtrConst::new(self.guard.as_ref().unwrap().ptr),
                    PtrConst::new(other.guard.as_ref().unwrap().ptr),
                )
            }
        } else {
            None
        }
    }
}
