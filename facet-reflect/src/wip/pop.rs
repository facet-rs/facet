use crate::trace;
use facet_core::{
    Def, EnumType, PtrConst, PtrMut, PtrUninit, Repr, ScalarAffinity, SequenceType, StructType,
    Type, UserType, Variant,
};
#[allow(unused_imports)]
use owo_colors::OwoColorize;

use crate::{FrameMode, ReflectError};

use super::{Frame, Wip};

impl Wip<'_> {
    /// Pops the current frame — goes back up one level
    pub fn pop(mut self) -> Result<Self, ReflectError> {
        let frame = match self.pop_inner()? {
            Some(frame) => frame,
            None => {
                return Err(ReflectError::InvariantViolation {
                    invariant: "No frame to pop — it was time to call build()",
                });
            }
        };

        self.track(frame);
        Ok(self)
    }

    fn pop_inner(&mut self) -> Result<Option<Frame>, ReflectError> {
        let mut frame = match self.frames.pop() {
            Some(f) => f,
            None => return Ok(None),
        };
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
            // If this frame is fully initialized, mark it as such in all tracking states

            // 1. Mark this frame as fully initialized (if it isn't already)
            unsafe {
                frame.mark_fully_initialized();
            }

            // 2. Mark the parent's field as initialized (if this is a field)
            #[cfg(feature = "log")]
            let num_frames = self.frames.len();
            if let Some(parent) = self.frames.last_mut() {
                if let Some(index) = frame.field_index_in_parent {
                    trace!(
                        "[{}] Marking field #{} in parent {} as initialized",
                        num_frames,
                        index.to_string().yellow(),
                        parent.shape.blue()
                    );
                    parent.istate.fields.set(index);
                }
            }

            // 3. If this is a container type like an array, make sure its internal state reflects that it's complete
            match frame.shape.def {
                Def::Array(array_def) => {
                    // For arrays, they're only fully initialized if all elements are populated
                    let current_index = frame.istate.list_index.unwrap_or(0);
                    trace!(
                        "[{}] Array {} has {}/{} elements populated",
                        self.frames.len(),
                        frame_shape.blue(),
                        current_index.to_string().yellow(),
                        array_def.n.to_string().green()
                    );

                    if current_index == array_def.n {
                        trace!(
                            "[{}] Array {} fully populated with {} elements, marking as initialized",
                            self.frames.len(),
                            frame_shape.blue(),
                            array_def.n.to_string().green()
                        );
                        // Mark the array itself as initialized (field 0)
                        frame.istate.fields.set(0);
                    }
                }
                // Add other container types here if needed
                _ => {}
            }
        }

        // Handle special frame modes
        match frame.istate.mode {
            // Handle list element frames
            FrameMode::ListElement => {
                if frame.is_fully_initialized() {
                    // This was a list or tuple element, so we need to push it to the parent
                    #[cfg(feature = "log")]
                    let frame_len = self.frames.len();

                    // Get parent frame
                    let parent_frame = self.frames.last_mut().unwrap();
                    let parent_shape = parent_frame.shape;

                    match parent_shape.def {
                        // Handle List/Array
                        Def::List(list_def) => {
                            let list_vtable = list_def.vtable;
                            trace!(
                                "[{}] Pushing element to list {}",
                                frame_len,
                                parent_shape.blue()
                            );
                            unsafe {
                                (list_vtable.push)(
                                    PtrMut::new(parent_frame.data.as_mut_byte_ptr()),
                                    PtrMut::new(frame.data.as_mut_byte_ptr()),
                                );
                                self.mark_moved_out_of(&mut frame);
                            }
                        }
                        Def::Scalar(s) if matches!(s.affinity, ScalarAffinity::Empty(_)) => {
                            trace!(
                                "[{}] Handling scalar empty unit type {}",
                                frame_len,
                                parent_shape.blue()
                            );
                            // Mark the parent scalar unit as fully initialized
                            unsafe {
                                parent_frame.mark_fully_initialized();
                                self.mark_moved_out_of(&mut frame);
                            }
                        }
                        _ => match parent_shape.ty {
                            // Handle Empty Unit Types (including empty tuple structs and tuples)
                            Type::User(UserType::Struct(sd))
                                if sd.kind == facet_core::StructKind::Tuple
                                    && sd.fields.is_empty() =>
                            {
                                trace!(
                                    "[{}] Handling empty tuple struct unit type {}",
                                    frame_len,
                                    parent_shape.blue()
                                );
                                // Mark the parent unit struct as fully initialized
                                unsafe {
                                    parent_frame.mark_fully_initialized();
                                }
                                // Element frame is implicitly moved/consumed, but nothing to dealloc if it was also unit
                                unsafe { self.mark_moved_out_of(&mut frame) };
                            }

                            // Handle tuples (Type::Sequence(SequenceType::Tuple))
                            Type::Sequence(SequenceType::Tuple(tt)) => {
                                // Get the field index from list_index saved during push
                                let previous_index = parent_frame.istate.list_index.unwrap_or(1);
                                let field_index = previous_index - 1; // -1 because we incremented *after* using the index in push

                                if field_index >= tt.fields.len() {
                                    panic!(
                                        "Field index {} out of bounds for tuple {} with {} fields",
                                        field_index,
                                        parent_shape,
                                        tt.fields.len()
                                    );
                                }

                                let field = &tt.fields[field_index];
                                trace!(
                                    "[{}] Setting tuple field {} ({}) of {}",
                                    frame_len,
                                    field_index.to_string().yellow(),
                                    field.name.bright_blue(),
                                    parent_shape.blue()
                                );

                                unsafe {
                                    // Copy the element data to the tuple field
                                    let field_ptr = parent_frame.data.field_uninit_at(field.offset);
                                    field_ptr
                                        .copy_from(
                                            PtrConst::new(frame.data.as_byte_ptr()),
                                            field.shape(),
                                        )
                                        .map_err(|_| ReflectError::Unsized {
                                            shape: field.shape(),
                                        })?; // Use ? to propagate potential unsized error

                                    // Mark the specific field as initialized using its index
                                    parent_frame.istate.fields.set(field_index);

                                    // Mark the element as moved
                                    self.mark_moved_out_of(&mut frame);
                                }
                            }

                            // Handle Tuple Structs
                            Type::User(UserType::Struct(sd))
                                if sd.kind == facet_core::StructKind::Tuple =>
                            {
                                // Get the field index from list_index saved during push
                                let previous_index = parent_frame.istate.list_index.unwrap_or(1);
                                let field_index = previous_index - 1; // -1 because we incremented *after* using the index in push

                                if field_index >= sd.fields.len() {
                                    panic!(
                                        "Field index {} out of bounds for tuple struct {} with {} fields",
                                        field_index,
                                        parent_shape,
                                        sd.fields.len()
                                    );
                                }

                                let field = &sd.fields[field_index];
                                trace!(
                                    "[{}] Setting tuple struct field {} ({}) of {}",
                                    frame_len,
                                    field_index.to_string().yellow(),
                                    field.name.bright_blue(),
                                    parent_shape.blue()
                                );

                                unsafe {
                                    // Copy the element data to the tuple field
                                    let field_ptr = parent_frame.data.field_uninit_at(field.offset);
                                    field_ptr
                                        .copy_from(
                                            PtrConst::new(frame.data.as_byte_ptr()),
                                            field.shape(),
                                        )
                                        .map_err(|_| ReflectError::Unsized {
                                            shape: field.shape(),
                                        })?; // Use ? to propagate potential unsized error

                                    // Mark the specific field as initialized using its index
                                    parent_frame.istate.fields.set(field_index);

                                    // Mark the element as moved
                                    self.mark_moved_out_of(&mut frame);
                                }
                            }

                            // Handle Tuple Enum Variants
                            Type::User(UserType::Enum(_)) => {
                                // Ensure a variant is selected and it's a tuple variant
                                let variant =
                                parent_frame.istate.variant.as_ref().unwrap_or_else(|| {
                                    panic!(
                                    "Popping element for enum {} but no variant was selected",
                                    parent_shape
                                )
                                });

                                if variant.data.kind != facet_core::StructKind::Tuple {
                                    panic!(
                                        "Popping element for enum {}, but selected variant '{}' is not a tuple variant",
                                        parent_shape, variant.name
                                    );
                                }

                                // Get the field index from list_index saved during push
                                let previous_index = parent_frame.istate.list_index.unwrap_or(1);
                                let field_index = previous_index - 1; // -1 because we incremented *after* using the index in push

                                if field_index >= variant.data.fields.len() {
                                    panic!(
                                        "Field index {} out of bounds for tuple enum variant '{}' of {} with {} fields",
                                        field_index,
                                        variant.name,
                                        parent_shape,
                                        variant.data.fields.len()
                                    );
                                }

                                let field = &variant.data.fields[field_index];
                                trace!(
                                    "[{}] Setting tuple enum variant field {} ({}) of variant '{}' in {}",
                                    frame_len,
                                    field_index.to_string().yellow(),
                                    field.name.bright_blue(),
                                    variant.name.yellow(),
                                    parent_shape.blue()
                                );

                                unsafe {
                                    // Copy the element data to the tuple field within the enum's data payload
                                    let field_ptr = parent_frame.data.field_uninit_at(field.offset);
                                    field_ptr
                                        .copy_from(
                                            PtrConst::new(frame.data.as_byte_ptr()),
                                            field.shape(),
                                        )
                                        .map_err(|_| ReflectError::Unsized {
                                            shape: field.shape(),
                                        })?; // Use ? to propagate potential unsized error

                                    // Mark the specific field as initialized using its index
                                    parent_frame.istate.fields.set(field_index);

                                    // Mark the element as moved
                                    self.mark_moved_out_of(&mut frame);
                                }
                            }

                            // Handle Arrays
                            _ if matches!(parent_shape.def, Def::Array(_)) => {
                                // Get the element index from list_index saved during push
                                let previous_index = parent_frame.istate.list_index.unwrap_or(1);
                                let element_index = previous_index - 1; // -1 because we incremented *after* using the index in push

                                let array_def = match parent_shape.def {
                                    Def::Array(array_def) => array_def,
                                    _ => unreachable!("Already checked this is an array"),
                                };

                                if element_index >= array_def.n {
                                    panic!(
                                        "Element index {} out of bounds for array {} with {} elements",
                                        element_index, parent_shape, array_def.n
                                    );
                                }

                                trace!(
                                    "[{}] Setting array element {} of {}",
                                    frame_len,
                                    element_index.to_string().yellow(),
                                    parent_shape.blue()
                                );

                                unsafe {
                                    // Get raw pointer to the array data
                                    let array_ptr = (array_def.vtable.as_ptr)(PtrConst::new(
                                        parent_frame.data.as_byte_ptr(),
                                    ));

                                    // Calculate the element size and offset
                                    let element_size = array_def
                                        .t
                                        .layout
                                        .sized_layout()
                                        .map_err(|_| ReflectError::Unsized { shape: array_def.t })?
                                        .size();

                                    // Calculate pointer to the right element in the array
                                    let element_offset = element_size * element_index;
                                    let element_ptr = PtrUninit::new(
                                        array_ptr.as_byte_ptr().add(element_offset) as *mut u8,
                                    );

                                    // Copy the element data to the array
                                    element_ptr
                                        .copy_from(
                                            PtrConst::new(frame.data.as_byte_ptr()),
                                            frame.shape,
                                        )
                                        .map_err(|_| ReflectError::Unsized {
                                            shape: frame.shape,
                                        })?; // Use ? to propagate potential unsized error

                                    // Check if the array is fully populated and mark it specially if it is
                                    if previous_index == array_def.n {
                                        trace!(
                                            "[{}] Array {} fully populated with {} elements, marking as fully initialized",
                                            frame_len,
                                            parent_shape.blue(),
                                            array_def.n.to_string().green()
                                        );
                                        // Mark the array itself as fully initialized
                                        parent_frame.mark_fully_initialized();

                                        // For nested arrays, also mark the parent field as initialized
                                        if let Some(parent_field_index) =
                                            parent_frame.field_index_in_parent
                                        {
                                            // Find the grandparent (skip to before the parent frame) if it exists
                                            if self.frames.len() >= 3 {
                                                let grandparent_index = self.frames.len() - 2;
                                                if let Some(grandparent_frame) =
                                                    self.frames.get_mut(grandparent_index)
                                                {
                                                    trace!(
                                                        "[{}] Marking field {} in grandparent {} as initialized",
                                                        frame_len,
                                                        parent_field_index.to_string().yellow(),
                                                        grandparent_frame.shape.blue()
                                                    );
                                                    grandparent_frame
                                                        .istate
                                                        .fields
                                                        .set(parent_field_index);
                                                }
                                            }
                                        }
                                    }

                                    // Mark the element as moved
                                    self.mark_moved_out_of(&mut frame);
                                }
                            }

                            // Unexpected parent type
                            _ => {
                                panic!(
                                    "FrameMode::ListElement pop expected parent to be List, Tuple, Tuple Struct, Tuple Enum Variant, or Array, but got {}",
                                    parent_shape
                                );
                            }
                        },
                    }
                } else {
                    // Frame not fully initialized, just deallocate if needed (handled by Frame drop later)
                    trace!(
                        "Popping uninitialized ListElement frame ({}), potential leak if allocated resources are not managed",
                        frame.shape.yellow()
                    );
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
                                trace!(
                                    "Marking parent frame as fully initialized — its shape is {}",
                                    parent_frame.shape
                                );
                                let variant = match parent_frame.shape.ty {
                                    Type::User(UserType::Enum(EnumType { variants, .. })) => {
                                        variants[1]
                                    }
                                    _ => Variant::builder()
                                        .name("Some")
                                        .discriminant(1)
                                        .data(
                                            StructType::builder()
                                                .tuple()
                                                .repr(Repr::default())
                                                .build(),
                                        )
                                        .build(),
                                };
                                parent_frame.istate.variant = Some(variant); // the `Some` variant
                                parent_frame.mark_fully_initialized();
                                trace!(
                                    "After marking: shape={} at {:p}, flags={:?}, mode={:?}, fully_initialized={}",
                                    parent_frame.shape.blue(),
                                    parent_frame.data.as_byte_ptr(),
                                    parent_frame.istate.flags.bright_magenta(),
                                    parent_frame.istate.mode.yellow(),
                                    if parent_frame.is_fully_initialized() {
                                        "✅"
                                    } else {
                                        "❌"
                                    }
                                );

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

        Ok(Some(frame))
    }
}
