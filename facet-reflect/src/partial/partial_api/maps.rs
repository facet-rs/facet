use super::*;
use crate::AllocatedShape;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Maps
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<const BORROW: bool> Partial<'_, BORROW> {
    /// Begins a map initialization operation
    ///
    /// This initializes the map with default capacity and allows inserting key-value pairs
    /// It does _not_ push a new frame onto the stack.
    ///
    /// For `Def::DynamicValue` types, this initializes as an object instead of a map.
    pub fn begin_map(mut self) -> Result<Self, ReflectError> {
        let frame = self.frames_mut().last_mut().unwrap();

        // Check tracker state before initializing
        match &frame.tracker {
            Tracker::Scalar if !frame.is_init => {
                // Good, will initialize below
            }
            Tracker::Scalar => {
                // is_init is true - already initialized (from a previous round)
                match frame.allocated.shape().def {
                    Def::Map(_) => {
                        // For Map, just update tracker - the map is already initialized
                        frame.tracker = Tracker::Map {
                            insert_state: MapInsertState::Idle,
                        };
                        return Ok(self);
                    }
                    Def::DynamicValue(_) => {
                        // For DynamicValue, we need to reinitialize as an object.
                        frame.deinit();
                    }
                    _ => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.allocated.shape(),
                            operation: "begin_map can only be called on Map or DynamicValue types",
                        });
                    }
                }
            }
            Tracker::Map { .. } => {
                if frame.is_init {
                    // Already initialized, nothing to do
                    return Ok(self);
                }
            }
            Tracker::DynamicValue { state } => {
                // Already initialized as a dynamic object
                if matches!(state, DynamicValueState::Object { .. }) {
                    return Ok(self);
                }
                // Otherwise (Scalar or Array state), we need to deinit before reinitializing.
                frame.deinit();
            }
            _ => {
                return Err(ReflectError::UnexpectedTracker {
                    message: "begin_map called but tracker isn't Scalar, Map, or DynamicValue",
                    current_tracker: frame.tracker.kind(),
                });
            }
        }

        // Check that we have a Map or DynamicValue
        match &frame.allocated.shape().def {
            Def::Map(map_def) => {
                let init_fn = map_def.vtable.init_in_place_with_capacity;

                // Initialize the map with default capacity (0)
                unsafe {
                    init_fn(frame.data, 0);
                }

                // Update tracker to Map state and mark as initialized
                frame.tracker = Tracker::Map {
                    insert_state: MapInsertState::Idle,
                };
                frame.is_init = true;
            }
            Def::DynamicValue(dyn_def) => {
                // Initialize as a dynamic object
                unsafe {
                    (dyn_def.vtable.begin_object)(frame.data);
                }

                // Update tracker to DynamicValue object state and mark as initialized
                frame.tracker = Tracker::DynamicValue {
                    state: DynamicValueState::Object {
                        insert_state: DynamicObjectInsertState::Idle,
                    },
                };
                frame.is_init = true;
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "begin_map can only be called on Map or DynamicValue types",
                });
            }
        }

        Ok(self)
    }

    /// Pushes a frame for the map key. After that, `set()` should be called
    /// (or the key should be initialized somehow) and `end()` should be called
    /// to pop the frame.
    pub fn begin_key(mut self) -> Result<Self, ReflectError> {
        let frame = self.frames_mut().last_mut().unwrap();

        // Check that we have a Map in Idle state
        let map_def = match (&frame.allocated.shape().def, &frame.tracker) {
            (
                Def::Map(map_def),
                Tracker::Map {
                    insert_state: MapInsertState::Idle,
                },
            ) if frame.is_init => map_def,
            (
                Def::Map(_),
                Tracker::Map {
                    insert_state: MapInsertState::PushingKey { .. },
                },
            ) => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "already pushing a key, call end() first",
                });
            }
            (
                Def::Map(_),
                Tracker::Map {
                    insert_state: MapInsertState::PushingValue { .. },
                },
            ) => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "must complete current operation before begin_key()",
                });
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
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
        let key_ptr_raw: *mut u8 = unsafe { ::alloc::alloc::alloc(key_layout) };

        let Some(key_ptr_raw) = NonNull::new(key_ptr_raw) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.allocated.shape(),
                operation: "failed to allocate memory for map key",
            });
        };

        let key_ptr = PtrUninit::new(key_ptr_raw.as_ptr());

        // Store the key pointer in the insert state
        match &mut frame.tracker {
            Tracker::Map { insert_state, .. } => {
                *insert_state = MapInsertState::PushingKey {
                    key_ptr,
                    key_initialized: false,
                    key_frame_on_stack: true, // TrackedBuffer frame is now on the stack
                };
            }
            _ => unreachable!(),
        }

        // Push a new frame for the key
        self.frames_mut().push(Frame::new(
            PtrUninit::new(key_ptr_raw.as_ptr()),
            AllocatedShape::new(key_shape, key_layout.size()),
            FrameOwnership::TrackedBuffer,
        ));

        Ok(self)
    }

    /// Pushes a frame for the map value
    /// Must be called after the key has been set and popped
    pub fn begin_value(mut self) -> Result<Self, ReflectError> {
        let frame = self.frames_mut().last_mut().unwrap();

        // Check that we have a Map in PushingValue state with no value_ptr yet
        let (map_def, key_ptr) = match (&frame.allocated.shape().def, &frame.tracker) {
            (
                Def::Map(map_def),
                Tracker::Map {
                    insert_state:
                        MapInsertState::PushingValue {
                            value_ptr: None,
                            key_ptr,
                            ..
                        },
                    ..
                },
            ) => (map_def, *key_ptr),
            (
                Def::Map(_),
                Tracker::Map {
                    insert_state:
                        MapInsertState::PushingValue {
                            value_ptr: Some(_), ..
                        },
                    ..
                },
            ) => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "already pushing a value, call end() first",
                });
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "must complete key before begin_value()",
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
        let value_ptr_raw: *mut u8 = unsafe { ::alloc::alloc::alloc(value_layout) };

        let Some(value_ptr_raw) = NonNull::new(value_ptr_raw) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.allocated.shape(),
                operation: "failed to allocate memory for map value",
            });
        };

        let value_ptr = PtrUninit::new(value_ptr_raw.as_ptr());

        // Store the value pointer in the insert state
        match &mut frame.tracker {
            Tracker::Map { insert_state, .. } => {
                *insert_state = MapInsertState::PushingValue {
                    key_ptr,
                    value_ptr: Some(value_ptr),
                    value_initialized: false,
                    value_frame_on_stack: true, // TrackedBuffer frame is now on the stack
                };
            }
            _ => unreachable!(),
        }

        // Push a new frame for the value
        self.frames_mut().push(Frame::new(
            value_ptr,
            AllocatedShape::new(value_shape, value_layout.size()),
            FrameOwnership::TrackedBuffer,
        ));

        Ok(self)
    }

    /// Begins an object entry for a DynamicValue object.
    ///
    /// This is a simpler API than begin_key/begin_value for DynamicValue objects,
    /// where keys are always strings. The key is stored and a frame is pushed for
    /// the value. After setting the value and calling `end()`, the key-value pair
    /// will be inserted into the object.
    ///
    /// For `Def::Map` types, use `begin_key()` / `begin_value()` instead.
    pub fn begin_object_entry(mut self, key: &str) -> Result<Self, ReflectError> {
        crate::trace!("begin_object_entry({key:?})");
        let frame = self.frames_mut().last_mut().unwrap();

        // Check that we have a DynamicValue in Object state with Idle insert_state
        let dyn_def = match (&frame.allocated.shape().def, &frame.tracker) {
            (
                Def::DynamicValue(dyn_def),
                Tracker::DynamicValue {
                    state:
                        DynamicValueState::Object {
                            insert_state: DynamicObjectInsertState::Idle,
                        },
                },
            ) if frame.is_init => {
                // Good, proceed
                dyn_def
            }
            (
                Def::DynamicValue(_),
                Tracker::DynamicValue {
                    state:
                        DynamicValueState::Object {
                            insert_state: DynamicObjectInsertState::BuildingValue { .. },
                        },
                },
            ) => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "already building a value, call end() first",
                });
            }
            (Def::DynamicValue(_), _) => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "must call begin_map() before begin_object_entry()",
                });
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "begin_object_entry can only be called on DynamicValue types",
                });
            }
        };

        // For DynamicValue objects, the value shape is the same DynamicValue shape
        let value_shape = frame.allocated.shape();

        // Check if key already exists using object_get_mut (for "get or create" semantics)
        // This is needed for formats like TOML with implicit tables: [a] followed by [a.b.c]
        if let Some(get_mut_fn) = dyn_def.vtable.object_get_mut {
            let object_ptr = unsafe { frame.data.assume_init() };
            if let Some(existing_ptr) = unsafe { get_mut_fn(object_ptr, key) } {
                // Key exists - push a frame pointing to existing value
                // Leave insert_state as Idle (no insertion needed on end())
                // Use ManagedElsewhere since parent object owns this value
                let value_size = value_shape
                    .layout
                    .sized_layout()
                    .expect("value must be sized")
                    .size();
                let mut new_frame = Frame::new(
                    existing_ptr.as_uninit(),
                    AllocatedShape::new(value_shape, value_size),
                    FrameOwnership::BorrowedInPlace,
                );
                new_frame.is_init = true;
                // Set tracker to reflect it's an initialized DynamicValue
                // For DynamicValue, we need to peek at the value to determine the state.
                // However, we don't know yet what operations will be called (begin_map, begin_list, etc.)
                // So we set Scalar tracker and let begin_map/begin_list handle the conversion.
                // begin_list will convert Scalar->List if shape is Def::List, or handle DynamicValue directly.
                new_frame.tracker = Tracker::Scalar;
                self.frames_mut().push(new_frame);
                return Ok(self);
            }
        }

        // Key doesn't exist - allocate new value
        let value_layout = match value_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return Err(ReflectError::Unsized {
                    shape: value_shape,
                    operation: "begin_object_entry: calculating value layout",
                });
            }
        };

        let value_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(value_layout) };
        let Some(value_ptr) = NonNull::new(value_ptr) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.allocated.shape(),
                operation: "failed to allocate memory for object value",
            });
        };

        // Update the insert state with the key
        match &mut frame.tracker {
            Tracker::DynamicValue {
                state: DynamicValueState::Object { insert_state },
            } => {
                *insert_state = DynamicObjectInsertState::BuildingValue {
                    key: String::from(key),
                };
            }
            _ => unreachable!(),
        }

        // Push a new frame for the value
        self.frames_mut().push(Frame::new(
            PtrUninit::new(value_ptr.as_ptr()),
            AllocatedShape::new(value_shape, value_layout.size()),
            FrameOwnership::Owned,
        ));

        Ok(self)
    }
}
