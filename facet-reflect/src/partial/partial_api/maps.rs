use super::*;

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
        let frame = self.frames_mut().last_mut().unwrap();

        // Check tracker state before initializing
        match &frame.tracker {
            Tracker::Scalar if !frame.is_init => {
                // Good, will initialize below
            }
            Tracker::Scalar => {
                // is_init is true - already initialized (from a previous round), just update tracker
                if !matches!(frame.shape.def, Def::Map(_)) {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "begin_map can only be called on Map types",
                    });
                }
                frame.tracker = Tracker::Map {
                    insert_state: MapInsertState::Idle,
                };
                return Ok(self);
            }
            Tracker::Map { .. } => {
                if frame.is_init {
                    // Already initialized, nothing to do
                    return Ok(self);
                }
            }
            _ => {
                return Err(ReflectError::UnexpectedTracker {
                    message: "begin_map called but tracker isn't Scalar or Map",
                    current_tracker: frame.tracker.kind(),
                });
            }
        }

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

        // Update tracker to Map state and mark as initialized
        frame.tracker = Tracker::Map {
            insert_state: MapInsertState::Idle,
        };
        frame.is_init = true;

        Ok(self)
    }

    /// Pushes a frame for the map key. After that, `set()` should be called
    /// (or the key should be initialized somehow) and `end()` should be called
    /// to pop the frame.
    pub fn begin_key(&mut self) -> Result<&mut Self, ReflectError> {
        self.require_active()?;
        let frame = self.frames_mut().last_mut().unwrap();

        // Check that we have a Map in Idle state
        let map_def = match (&frame.shape.def, &frame.tracker) {
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
                    shape: frame.shape,
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
                    shape: frame.shape,
                    operation: "must complete current operation before begin_key()",
                });
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
        let key_ptr_raw: *mut u8 = unsafe { ::alloc::alloc::alloc(key_layout) };

        let Some(key_ptr_raw) = NonNull::new(key_ptr_raw) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "failed to allocate memory for map key",
            });
        };

        let key_ptr = PtrUninit::new(key_ptr_raw);

        // Store the key pointer in the insert state
        match &mut frame.tracker {
            Tracker::Map { insert_state, .. } => {
                *insert_state = MapInsertState::PushingKey {
                    key_ptr,
                    key_initialized: false,
                };
            }
            _ => unreachable!(),
        }

        // Push a new frame for the key
        self.frames_mut().push(Frame::new(
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
        let frame = self.frames_mut().last_mut().unwrap();

        // Check that we have a Map in PushingValue state with no value_ptr yet
        let (map_def, key_ptr) = match (&frame.shape.def, &frame.tracker) {
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
                    shape: frame.shape,
                    operation: "already pushing a value, call end() first",
                });
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
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
                shape: frame.shape,
                operation: "failed to allocate memory for map value",
            });
        };

        let value_ptr = PtrUninit::new(value_ptr_raw);

        // Store the value pointer in the insert state
        match &mut frame.tracker {
            Tracker::Map { insert_state, .. } => {
                *insert_state = MapInsertState::PushingValue {
                    key_ptr,
                    value_ptr: Some(value_ptr),
                    value_initialized: false,
                };
            }
            _ => unreachable!(),
        }

        // Push a new frame for the value
        self.frames_mut().push(Frame::new(
            value_ptr,
            value_shape,
            FrameOwnership::ManagedElsewhere, // Ownership tracked in MapInsertState
        ));

        Ok(self)
    }
}
