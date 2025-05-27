#[cfg(test)]
mod tests;

mod iset;

use crate::{ReflectError, trace};

use core::marker::PhantomData;

mod heap_value;
use alloc::vec::Vec;
pub use heap_value::*;

use facet_core::{Def, Facet, KnownSmartPointer, PtrConst, PtrMut, PtrUninit, Shape, Variant};
use iset::ISet;

/// A work-in-progress heap-allocated value
///
/// # Lifetimes
///
/// * `'facet`: The lifetime of borrowed values within the structure (or 'static if it's owned)
/// * `'shape`: The lifetime of the Shape structure itself (often 'static)
pub struct Wip<'facet, 'shape> {
    /// stack of frames to keep track of deeply nested initialization
    frames: Vec<Frame<'shape>>,

    /// was this wip poisoned?
    poisoned: bool,

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

enum FrameOwnership {
    /// This frame owns the allocation and should deallocate it on drop
    Owned,
    /// This frame is a field pointer into a parent allocation
    Field,
}

struct Frame<'shape> {
    /// Address of the value being initialized
    data: PtrUninit<'static>,

    /// Shape of the value being initialized
    shape: &'shape Shape<'shape>,

    /// Tracks initialized fields
    tracker: Tracker<'shape>,

    /// Whether this frame owns the allocation or is just a field pointer
    ownership: FrameOwnership,
}

enum Tracker<'shape> {
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

    /// Partially initialized enum (but we picked a variant)
    Enum {
        variant: Variant<'shape>,
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
}

impl<'shape> Frame<'shape> {
    fn new(
        data: PtrUninit<'static>,
        shape: &'shape Shape<'shape>,
        ownership: FrameOwnership,
    ) -> Self {
        Self {
            data,
            shape,
            tracker: Tracker::Uninit,
            ownership,
        }
    }

    /// Returns an error if the value is not fully initialized
    fn require_full_initialization(&self) -> Result<(), ReflectError<'shape>> {
        match self.tracker {
            Tracker::Uninit => Err(ReflectError::UninitializedValue { shape: self.shape }),
            Tracker::Init => Ok(()),
            Tracker::Array { iset, .. } => {
                match self.shape.ty {
                    facet_core::Type::Sequence(facet_core::SequenceType::Array(array_def)) => {
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
                        facet_core::Type::User(facet_core::UserType::Struct(struct_type)) => {
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
                        Err(ReflectError::UninitializedField {
                            shape: self.shape,
                            field_name,
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
        }
    }
}

impl<'facet, 'shape> Wip<'facet, 'shape> {
    /// Allocates a new Wip instance with the given shape
    pub fn alloc_shape(shape: &'shape Shape<'shape>) -> Result<Self, ReflectError<'shape>> {
        let data = shape
            .allocate()
            .map_err(|_| ReflectError::Unsized { shape })?;

        Ok(Self {
            frames: vec![Frame::new(data, shape, FrameOwnership::Owned)],
            poisoned: false,
            invariant: PhantomData,
        })
    }

    /// Allocates a new TypedWip instance with the given shape and type
    pub fn alloc<T>() -> Result<TypedWip<'facet, 'shape, T>, ReflectError<'shape>>
    where
        T: Facet<'shape>,
    {
        Ok(TypedWip {
            wip: Self::alloc_shape(T::SHAPE)?,
            phantom: PhantomData,
        })
    }

    /// Creates a Wip from an existing pointer and shape (used for nested initialization)
    pub fn from_ptr(data: PtrUninit<'_>, shape: &'shape Shape<'shape>) -> Self {
        // We need to convert the lifetime, which is safe because we're storing it in a frame
        // that will manage the lifetime correctly
        let data_static = PtrUninit::new(data.as_mut_byte_ptr());
        Self {
            frames: vec![Frame::new(data_static, shape, FrameOwnership::Field)],
            poisoned: false,
            invariant: PhantomData,
        }
    }

    /// Sets a value wholesale into the current frame
    pub fn set<T>(&mut self, value: T) -> Result<(), ReflectError<'shape>>
    where
        T: Facet<'shape>,
    {
        // relay to set_shape — convert T into a ptr and shape, and call set_shape
        let ptr_const = PtrConst::new(&raw const value);
        let result = self.set_shape(ptr_const, T::SHAPE);
        // Prevent the value from being dropped since we've copied it
        core::mem::forget(value);
        result
    }

    /// Sets a value into the current frame by shape, for shape-based operations
    pub fn set_shape(
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'shape Shape<'shape>,
    ) -> Result<(), ReflectError<'shape>> {
        let fr = self.frames.last_mut().unwrap();

        if !fr.shape.is_shape(src_shape) {
            return Err(ReflectError::WrongShape {
                expected: src_shape,
                actual: fr.shape,
            });
        }

        unsafe {
            fr.data
                .copy_from(src_value, fr.shape)
                .map_err(|_| ReflectError::Unsized { shape: fr.shape })?;
        }

        fr.tracker = Tracker::Init;
        Ok(())
    }

    /// Sets the current frame to its default value
    pub fn set_default(&mut self) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();

        // Check if we need to drop an existing value
        if matches!(frame.tracker, Tracker::Init) {
            if let Some(drop_fn) = (frame.shape.vtable.drop_in_place)() {
                unsafe { drop_fn(PtrMut::new(frame.data.as_mut_byte_ptr())) };
            }
        }

        if let Some(default_fn) = (frame.shape.vtable.default_in_place)() {
            // Initialize with default value
            // SAFETY: frame.data points to uninitialized memory of the correct layout
            unsafe { default_fn(frame.data) };
            frame.tracker = Tracker::Init;
            Ok(())
        } else {
            Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "type does not implement Default",
            })
        }
    }

    /// Sets the current frame using a function that initializes the value
    pub fn set_from_function<F>(&mut self, f: F) -> Result<(), ReflectError<'shape>>
    where
        F: FnOnce(PtrUninit<'_>) -> Result<(), ReflectError<'shape>>,
    {
        let frame = self.frames.last_mut().unwrap();

        // Drop existing value if initialized
        if matches!(frame.tracker, Tracker::Init) {
            if let Some(drop_fn) = (frame.shape.vtable.drop_in_place)() {
                unsafe { drop_fn(PtrMut::new(frame.data.as_mut_byte_ptr())) };
            }
            frame.tracker = Tracker::Uninit;
        }

        // Call the function to initialize
        f(frame.data)?;
        frame.tracker = Tracker::Init;
        Ok(())
    }

    /// Pushes a variant for enum initialization
    pub fn push_variant(&mut self, discriminant: i64) -> Result<(), ReflectError<'shape>> {
        let fr = self.frames.last_mut().unwrap();

        // Check that we're dealing with an enum
        let enum_type = match fr.shape.ty {
            facet_core::Type::User(facet_core::UserType::Enum(e)) => e,
            _ => {
                return Err(ReflectError::WrongShape {
                    expected: fr.shape,
                    actual: fr.shape,
                });
            }
        };

        // Find the variant with the matching discriminant
        let variant = enum_type
            .variants
            .iter()
            .find(|v| v.discriminant == Some(discriminant))
            .ok_or_else(|| ReflectError::OperationFailed {
                shape: fr.shape,
                operation: "No variant found with the given discriminant",
            })?;

        // Write the discriminant to memory
        unsafe {
            match enum_type.enum_repr {
                facet_core::EnumRepr::U8 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut u8;
                    *ptr = discriminant as u8;
                }
                facet_core::EnumRepr::U16 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut u16;
                    *ptr = discriminant as u16;
                }
                facet_core::EnumRepr::U32 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut u32;
                    *ptr = discriminant as u32;
                }
                facet_core::EnumRepr::U64 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut u64;
                    *ptr = discriminant as u64;
                }
                facet_core::EnumRepr::I8 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i8;
                    *ptr = discriminant as i8;
                }
                facet_core::EnumRepr::I16 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i16;
                    *ptr = discriminant as i16;
                }
                facet_core::EnumRepr::I32 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i32;
                    *ptr = discriminant as i32;
                }
                facet_core::EnumRepr::I64 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i64;
                    *ptr = discriminant as i64;
                }
                facet_core::EnumRepr::USize => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut usize;
                    *ptr = discriminant as usize;
                }
                facet_core::EnumRepr::ISize => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut isize;
                    *ptr = discriminant as isize;
                }
                facet_core::EnumRepr::RustNPO => {
                    return Err(ReflectError::OperationFailed {
                        shape: fr.shape,
                        operation: "RustNPO enums are not supported for incremental building",
                    });
                }
                _ => {
                    return Err(ReflectError::OperationFailed {
                        shape: fr.shape,
                        operation: "Unknown enum representation",
                    });
                }
            }
        }

        // Update tracker to track the variant
        fr.tracker = Tracker::Enum {
            variant: *variant,
            data: ISet::new(variant.data.fields.len()),
            current_child: None,
        };

        Ok(())
    }

    /// Selects a field of a struct with a given name
    pub fn push_field(&mut self, field_name: &str) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();
        match frame.shape.ty {
            facet_core::Type::Primitive(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a primitive type",
            }),
            facet_core::Type::Sequence(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a sequence type",
            }),
            facet_core::Type::User(user_type) => match user_type {
                facet_core::UserType::Struct(struct_type) => {
                    let idx = struct_type.fields.iter().position(|f| f.name == field_name);
                    let idx = match idx {
                        Some(idx) => idx,
                        None => {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "field not found",
                            });
                        }
                    };
                    self.push_nth_field(idx)
                }
                facet_core::UserType::Enum(_) => {
                    // Check if we have a variant selected
                    match &frame.tracker {
                        Tracker::Enum { variant, .. } => {
                            let idx = variant
                                .data
                                .fields
                                .iter()
                                .position(|f| f.name == field_name);
                            let idx = match idx {
                                Some(idx) => idx,
                                None => {
                                    return Err(ReflectError::OperationFailed {
                                        shape: frame.shape,
                                        operation: "field not found in current enum variant",
                                    });
                                }
                            };
                            self.push_nth_enum_field(idx)
                        }
                        _ => Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "must call push_variant before selecting enum fields",
                        }),
                    }
                }
                facet_core::UserType::Union(_) => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "unions are not supported yet",
                }),
                facet_core::UserType::Opaque => todo!(),
            },
            facet_core::Type::Pointer(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a pointer type",
            }),
            _ => todo!(),
        }
    }

    /// Selects the nth field of a struct by index
    pub fn push_nth_field(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();
        match frame.shape.ty {
            facet_core::Type::User(user_type) => match user_type {
                facet_core::UserType::Struct(struct_type) => {
                    if idx >= struct_type.fields.len() {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "field index out of bounds",
                        });
                    }
                    let field = &struct_type.fields[idx];

                    match &mut frame.tracker {
                        Tracker::Uninit => {
                            frame.tracker = Tracker::Struct {
                                iset: ISet::new(struct_type.fields.len()),
                                current_child: Some(idx),
                            }
                        }
                        Tracker::Struct {
                            iset,
                            current_child,
                        } => {
                            // Check if this field was already initialized
                            if iset.get(idx) {
                                // Drop the existing value before re-initializing
                                let field_ptr = unsafe { frame.data.field_init_at(field.offset) };
                                if let Some(drop_fn) = (field.shape.vtable.drop_in_place)() {
                                    unsafe { drop_fn(field_ptr) };
                                }
                                // Unset the bit so we can re-initialize
                                iset.unset(idx);
                            }
                            *current_child = Some(idx);
                        }
                        _ => unreachable!(),
                    }

                    // Push a new frame for this field onto the frames stack.
                    let field_ptr = unsafe { frame.data.field_uninit_at(field.offset) };
                    let field_shape = field.shape;
                    self.frames
                        .push(Frame::new(field_ptr, field_shape, FrameOwnership::Field));

                    Ok(())
                }
                facet_core::UserType::Enum(_) => {
                    todo!("add support for selecting fields in enums")
                }
                facet_core::UserType::Union(_) => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "unions are not supported yet",
                }),
                facet_core::UserType::Opaque => todo!(),
            },
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from this type",
            }),
        }
    }

    /// Selects the nth element of an array by index
    pub fn push_nth_element(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();
        match frame.shape.ty {
            facet_core::Type::Sequence(seq_type) => match seq_type {
                facet_core::SequenceType::Array(array_def) => {
                    if idx >= array_def.n {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "array index out of bounds",
                        });
                    }

                    if array_def.n > 63 {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "arrays larger than 63 elements are not yet supported",
                        });
                    }

                    // Ensure frame is in Array state
                    if matches!(frame.tracker, Tracker::Uninit) {
                        frame.tracker = Tracker::Array {
                            iset: ISet::default(),
                            current_child: None,
                        };
                    }

                    match &mut frame.tracker {
                        Tracker::Array {
                            iset,
                            current_child,
                        } => {
                            // Calculate the offset for this array element
                            let element_layout = array_def
                                .t
                                .layout
                                .sized_layout()
                                .map_err(|_| ReflectError::Unsized { shape: array_def.t })?;
                            let offset = element_layout.size() * idx;

                            // Check if this element was already initialized
                            if iset.get(idx) {
                                // Drop the existing value before re-initializing
                                let element_ptr = unsafe { frame.data.field_init_at(offset) };
                                if let Some(drop_fn) = (array_def.t.vtable.drop_in_place)() {
                                    unsafe { drop_fn(element_ptr) };
                                }
                                // Unset the bit so we can re-initialize
                                iset.unset(idx);
                            }

                            *current_child = Some(idx);

                            // Create a new frame for the array element
                            let element_data = unsafe { frame.data.field_uninit_at(offset) };
                            self.frames.push(Frame::new(
                                element_data,
                                array_def.t,
                                FrameOwnership::Field,
                            ));

                            Ok(())
                        }
                        _ => Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "expected array tracker state",
                        }),
                    }
                }
                _ => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "can only select elements from arrays",
                }),
            },
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select an element from this type",
            }),
        }
    }

    /// Selects the nth field of an enum variant by index
    pub fn push_nth_enum_field(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();

        // Ensure we're in an enum with a variant selected
        let (variant, enum_type) = match (&frame.tracker, &frame.shape.ty) {
            (
                Tracker::Enum { variant, .. },
                facet_core::Type::User(facet_core::UserType::Enum(e)),
            ) => (variant, e),
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "push_nth_enum_field requires an enum with a variant selected",
                });
            }
        };

        // Check bounds
        if idx >= variant.data.fields.len() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "enum field index out of bounds",
            });
        }

        let field = &variant.data.fields[idx];

        // Update tracker
        match &mut frame.tracker {
            Tracker::Enum {
                data,
                current_child,
                ..
            } => {
                // Check if field was already initialized and drop if needed
                if data.get(idx) {
                    // Calculate the field offset, taking into account the discriminant
                    let _discriminant_size = match enum_type.enum_repr {
                        facet_core::EnumRepr::U8 | facet_core::EnumRepr::I8 => 1,
                        facet_core::EnumRepr::U16 | facet_core::EnumRepr::I16 => 2,
                        facet_core::EnumRepr::U32 | facet_core::EnumRepr::I32 => 4,
                        facet_core::EnumRepr::U64 | facet_core::EnumRepr::I64 => 8,
                        facet_core::EnumRepr::USize | facet_core::EnumRepr::ISize => {
                            std::mem::size_of::<usize>()
                        }
                        facet_core::EnumRepr::RustNPO => {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "RustNPO enums are not supported",
                            });
                        }
                        _ => {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "Unknown enum representation",
                            });
                        }
                    };

                    // The field offset already includes the discriminant offset
                    let field_ptr = unsafe { frame.data.as_mut_byte_ptr().add(field.offset) };

                    if let Some(drop_fn) = (field.shape.vtable.drop_in_place)() {
                        unsafe { drop_fn(PtrMut::new(field_ptr)) };
                    }

                    // Unset the bit so we can re-initialize
                    data.unset(idx);
                }

                // Set current_child to track which field we're initializing
                *current_child = Some(idx);
            }
            _ => unreachable!("Already checked that we have Enum tracker"),
        }

        // Extract data we need before pushing frame
        let field_ptr = unsafe { frame.data.as_mut_byte_ptr().add(field.offset) };
        let field_shape = field.shape;

        // Push new frame for the field
        self.frames.push(Frame::new(
            PtrUninit::new(field_ptr),
            field_shape,
            FrameOwnership::Field,
        ));

        Ok(())
    }

    /// Pushes a frame to initialize the inner value of a Box<T>
    pub fn push_box(&mut self) -> Result<(), ReflectError<'shape>> {
        self.push_smart_ptr()
    }

    /// Pushes a frame to initialize the inner value of a smart pointer (Box<T>, Arc<T>, etc.)
    pub fn push_smart_ptr(&mut self) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();

        // Check that we have a SmartPointer
        match &frame.shape.def {
            Def::SmartPointer(smart_ptr_def) => {
                // Check for supported smart pointer types
                match smart_ptr_def.known {
                    Some(KnownSmartPointer::Box) | Some(KnownSmartPointer::Arc) => {
                        // Supported types, continue
                    }
                    _ => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "only Box and Arc smart pointers are currently supported",
                        });
                    }
                }

                // Get the pointee shape
                let pointee_shape =
                    smart_ptr_def
                        .pointee()
                        .ok_or(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "Box must have a pointee shape",
                        })?;

                // Update tracker to SmartPointer state
                if matches!(frame.tracker, Tracker::Uninit) {
                    frame.tracker = Tracker::SmartPointer {
                        is_initialized: false,
                    };
                }

                // Allocate space for the inner value
                let inner_layout =
                    pointee_shape
                        .layout
                        .sized_layout()
                        .map_err(|_| ReflectError::Unsized {
                            shape: pointee_shape,
                        })?;
                let inner_ptr: *mut u8 = unsafe { std::alloc::alloc(inner_layout) };

                if inner_ptr.is_null() {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "failed to allocate memory for Box inner value",
                    });
                }

                // Push a new frame for the inner value
                self.frames.push(Frame::new(
                    PtrUninit::new(inner_ptr),
                    pointee_shape,
                    FrameOwnership::Owned,
                ));

                Ok(())
            }
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "push_box can only be called on Box types",
            }),
        }
    }

    /// Begins a pushback operation for a list (Vec, etc.)
    /// This initializes the list with default capacity and allows pushing elements
    pub fn begin_pushback(&mut self) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();

        // Check that we have a List
        let list_def = match &frame.shape.def {
            Def::List(list_def) => list_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "begin_pushback can only be called on List types",
                });
            }
        };

        // Check that we have init_in_place_with_capacity function
        let init_fn =
            list_def
                .vtable
                .init_in_place_with_capacity
                .ok_or(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "list type does not support initialization with capacity",
                })?;

        // Initialize the list with default capacity (0)
        unsafe {
            init_fn(frame.data, 0);
        }

        // Update tracker to List state
        frame.tracker = Tracker::List {
            is_initialized: true,
            current_child: false,
        };

        Ok(())
    }

    /// Begins a map initialization operation
    /// This initializes the map with default capacity and allows inserting key-value pairs
    pub fn begin_map(&mut self) -> Result<(), ReflectError<'shape>> {
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

        // Check that we have init_in_place_with_capacity function
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

        Ok(())
    }

    /// Begins inserting a key-value pair into the map
    /// After calling this, use push_key() and push_value() to set the key and value
    pub fn begin_insert(&mut self) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();

        // Check that we have a Map that's been initialized
        match &mut frame.tracker {
            Tracker::Map {
                is_initialized: true,
                insert_state,
            } => {
                if !matches!(insert_state, MapInsertState::Idle) {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "already in the middle of an insert operation",
                    });
                }
                *insert_state = MapInsertState::PushingKey { key_ptr: None };
                Ok(())
            }
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "must call begin_map() before begin_insert()",
            }),
        }
    }

    /// Pushes a frame for the map key
    /// Must be called after begin_insert()
    pub fn push_key(&mut self) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();

        // Check that we have a Map in PushingKey state
        let map_def = match (&frame.shape.def, &mut frame.tracker) {
            (
                Def::Map(map_def),
                Tracker::Map {
                    insert_state: MapInsertState::PushingKey { key_ptr },
                    ..
                },
            ) => {
                if key_ptr.is_some() {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "already pushing a key, call pop() first",
                    });
                }
                map_def
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "must call begin_insert() before push_key()",
                });
            }
        };

        // Get the key shape
        let key_shape = map_def.k();

        // Allocate space for the key
        let key_layout = key_shape
            .layout
            .sized_layout()
            .map_err(|_| ReflectError::Unsized { shape: key_shape })?;
        let key_ptr_raw: *mut u8 = unsafe { std::alloc::alloc(key_layout) };

        if key_ptr_raw.is_null() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "failed to allocate memory for map key",
            });
        }

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
            FrameOwnership::Owned,
        ));

        Ok(())
    }

    /// Pushes a frame for the map value
    /// Must be called after the key has been set and popped
    pub fn push_value(&mut self) -> Result<(), ReflectError<'shape>> {
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
        let value_layout = value_shape
            .layout
            .sized_layout()
            .map_err(|_| ReflectError::Unsized { shape: value_shape })?;
        let value_ptr_raw: *mut u8 = unsafe { std::alloc::alloc(value_layout) };

        if value_ptr_raw.is_null() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "failed to allocate memory for map value",
            });
        }

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
            FrameOwnership::Owned,
        ));

        Ok(())
    }

    /// Pushes an element to the list
    /// The element should be set using `set()` or similar methods, then `pop()` to complete
    pub fn push(&mut self) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();

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
                    operation: "must call begin_pushback() before push()",
                });
            }
        }

        // Get the element shape
        let element_shape = list_def.t();

        // Allocate space for the new element
        let element_layout =
            element_shape
                .layout
                .sized_layout()
                .map_err(|_| ReflectError::Unsized {
                    shape: element_shape,
                })?;
        let element_ptr: *mut u8 = unsafe { std::alloc::alloc(element_layout) };

        if element_ptr.is_null() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "failed to allocate memory for list element",
            });
        }

        // Push a new frame for the element
        self.frames.push(Frame::new(
            PtrUninit::new(element_ptr),
            element_shape,
            FrameOwnership::Owned,
        ));

        Ok(())
    }

    /// Pops the current frame off the stack, indicating we're done initializing the current field.
    pub fn pop(&mut self) -> Result<(), ReflectError<'shape>> {
        if self.frames.len() <= 1 {
            // Never pop the last/root frame.
            return Err(ReflectError::InvariantViolation {
                invariant: "Wip::pop() called with only one frame on the stack",
            });
        }

        // Require that the top frame is fully initialized before popping.
        {
            let frame = self.frames.last().unwrap();
            frame.require_full_initialization()?;
        }

        // Pop the frame and save its data pointer for SmartPointer handling
        let popped_frame = self.frames.pop().unwrap();

        // Update parent frame's tracking when popping from a child
        let parent_frame = self.frames.last_mut().unwrap();
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
                // We just popped the inner value frame, so now we need to create the Box
                if let Def::SmartPointer(smart_ptr_def) = parent_frame.shape.def {
                    if let Some(new_into_fn) = smart_ptr_def.vtable.new_into_fn {
                        // The child frame contained the inner value
                        let inner_ptr = PtrMut::new(popped_frame.data.as_mut_byte_ptr());

                        // Use new_into_fn to create the Box
                        unsafe {
                            new_into_fn(parent_frame.data, inner_ptr);
                        }

                        // Deallocate the inner value's memory since new_into_fn moved it
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

                        *is_initialized = true;
                    } else {
                        return Err(ReflectError::OperationFailed {
                            shape: parent_frame.shape,
                            operation: "SmartPointer missing new_into_fn",
                        });
                    }
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
                        if let Some(push_fn) = list_def.vtable.push {
                            // The child frame contained the element value
                            let element_ptr = PtrMut::new(popped_frame.data.as_mut_byte_ptr());

                            // Use push to add element to the list
                            unsafe {
                                push_fn(
                                    PtrMut::new(parent_frame.data.as_mut_byte_ptr()),
                                    element_ptr,
                                );
                            }

                            // Deallocate the element's memory since push moved it
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

                            *current_child = false;
                        } else {
                            return Err(ReflectError::OperationFailed {
                                shape: parent_frame.shape,
                                operation: "List missing push function",
                            });
                        }
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
                                    PtrMut::new(parent_frame.data.as_mut_byte_ptr()),
                                    PtrMut::new(key_ptr.as_mut_byte_ptr()),
                                    PtrMut::new(value_ptr.as_mut_byte_ptr()),
                                );
                            }

                            // Deallocate the key and value memory since insert moved them
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
            _ => {}
        }

        Ok(())
    }

    /// Builds the value
    pub fn build(mut self) -> Result<HeapValue<'facet, 'shape>, ReflectError<'shape>> {
        if self.frames.len() != 1 {
            return Err(ReflectError::InvariantViolation {
                invariant: "Wip::build() expects a single frame — pop until that's the case",
            });
        }

        let frame = self.frames.pop().unwrap();
        frame.require_full_initialization()?;

        Ok(HeapValue {
            guard: Some(Guard {
                ptr: frame.data.as_mut_byte_ptr(),
                layout: frame
                    .shape
                    .layout
                    .sized_layout()
                    .map_err(|_| ReflectError::Unsized { shape: frame.shape })?,
            }),
            shape: frame.shape,
            phantom: PhantomData,
        })
    }
}

/// A typed wrapper around `Wip`, for when you want to statically
/// ensure that `build` gives you the proper type.
pub struct TypedWip<'facet, 'shape, T> {
    wip: Wip<'facet, 'shape>,
    phantom: PhantomData<T>,
}
impl<'facet, 'shape, T> TypedWip<'facet, 'shape, T> {
    /// Builds the value and returns a Box<T>
    pub fn build(self) -> Result<Box<T>, ReflectError<'shape>>
    where
        T: Facet<'shape>,
        'facet: 'shape,
    {
        let heap_value = self.wip.build()?;
        // Safety: HeapValue was constructed from T and the shape layout is correct.
        unsafe { Ok(heap_value.into_box_unchecked::<T>()) }
    }

    /// Sets a value wholesale into the current frame
    pub fn set<U>(&mut self, value: U) -> Result<(), ReflectError<'shape>>
    where
        U: Facet<'shape>,
    {
        self.wip.set(value)
    }

    /// Sets a value into the current frame by shape, for shape-based operations
    pub fn set_shape(
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'shape Shape<'shape>,
    ) -> Result<(), ReflectError<'shape>> {
        self.wip.set_shape(src_value, src_shape)
    }

    /// Forwards field_named to the inner wip instance.
    pub fn push_field(&mut self, field_name: &str) -> Result<(), ReflectError<'shape>> {
        self.wip.push_field(field_name)
    }

    /// Forwards push_nth_field to the inner wip instance.
    pub fn push_nth_field(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        self.wip.push_nth_field(idx)
    }

    /// Forwards push_nth_element to the inner wip instance.
    pub fn push_nth_element(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        self.wip.push_nth_element(idx)
    }

    /// Forwards push_box to the inner wip instance.
    pub fn push_box(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.push_box()
    }

    /// Forwards push_smart_ptr to the inner wip instance.
    pub fn push_smart_ptr(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.push_smart_ptr()
    }

    /// Forwards pop to the inner wip instance.
    pub fn pop(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.pop()
    }

    /// Forwards set_default to the inner wip instance.
    pub fn set_default(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.set_default()
    }

    /// Forwards set_from_function to the inner wip instance.
    pub fn set_from_function<F>(&mut self, f: F) -> Result<(), ReflectError<'shape>>
    where
        F: FnOnce(PtrUninit<'_>) -> Result<(), ReflectError<'shape>>,
    {
        self.wip.set_from_function(f)
    }

    /// Forwards push_variant to the inner wip instance.
    pub fn push_variant(&mut self, discriminant: i64) -> Result<(), ReflectError<'shape>> {
        self.wip.push_variant(discriminant)
    }

    /// Forwards push_nth_enum_field to the inner wip instance.
    pub fn push_nth_enum_field(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        self.wip.push_nth_enum_field(idx)
    }

    /// Forwards begin_pushback to the inner wip instance.
    pub fn begin_pushback(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.begin_pushback()
    }

    /// Forwards push to the inner wip instance.
    pub fn push(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.push()
    }

    /// Forwards begin_map to the inner wip instance.
    pub fn begin_map(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.begin_map()
    }

    /// Forwards begin_insert to the inner wip instance.
    pub fn begin_insert(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.begin_insert()
    }

    /// Forwards push_key to the inner wip instance.
    pub fn push_key(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.push_key()
    }

    /// Forwards push_value to the inner wip instance.
    pub fn push_value(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.push_value()
    }
}

impl<'facet, 'shape> Drop for Wip<'facet, 'shape> {
    fn drop(&mut self) {
        trace!("🧹 Wip is being dropped");

        // We need to properly drop all initialized fields
        while let Some(frame) = self.frames.pop() {
            match &frame.tracker {
                Tracker::Uninit => {
                    // Nothing was initialized, nothing to drop
                }
                Tracker::Init => {
                    // Fully initialized, drop it
                    if let Some(drop_fn) = (frame.shape.vtable.drop_in_place)() {
                        unsafe { drop_fn(PtrMut::new(frame.data.as_mut_byte_ptr())) };
                    }
                }
                Tracker::Array { iset, .. } => {
                    // Drop initialized array elements
                    match frame.shape.ty {
                        facet_core::Type::Sequence(facet_core::SequenceType::Array(array_def)) => {
                            let element_layout = array_def.t.layout.sized_layout().ok();
                            if let Some(layout) = element_layout {
                                for idx in 0..array_def.n {
                                    if iset.get(idx) {
                                        let offset = layout.size() * idx;
                                        let element_ptr =
                                            unsafe { frame.data.field_init_at(offset) };
                                        if let Some(drop_fn) = (array_def.t.vtable.drop_in_place)()
                                        {
                                            unsafe { drop_fn(element_ptr) };
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Tracker::Struct { iset, .. } => {
                    // Drop initialized struct fields
                    match frame.shape.ty {
                        facet_core::Type::User(facet_core::UserType::Struct(struct_type)) => {
                            for (idx, field) in struct_type.fields.iter().enumerate() {
                                if iset.get(idx) {
                                    // This field was initialized, drop it
                                    let field_ptr =
                                        unsafe { frame.data.field_init_at(field.offset) };
                                    if let Some(drop_fn) = (field.shape.vtable.drop_in_place)() {
                                        unsafe { drop_fn(field_ptr) };
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Tracker::Enum { variant, data, .. } => {
                    // Drop initialized enum variant fields
                    for (idx, field) in variant.data.fields.iter().enumerate() {
                        if data.get(idx) {
                            // This field was initialized, drop it
                            let field_ptr =
                                unsafe { frame.data.as_mut_byte_ptr().add(field.offset) };
                            if let Some(drop_fn) = (field.shape.vtable.drop_in_place)() {
                                unsafe { drop_fn(PtrMut::new(field_ptr)) };
                            }
                        }
                    }
                }
                Tracker::SmartPointer { is_initialized } => {
                    // Drop the initialized Box
                    if *is_initialized {
                        if let Some(drop_fn) = (frame.shape.vtable.drop_in_place)() {
                            unsafe { drop_fn(PtrMut::new(frame.data.as_mut_byte_ptr())) };
                        }
                    }
                    // Note: we don't deallocate the inner value here because
                    // the Box's drop will handle that
                }
                Tracker::List { is_initialized, .. } => {
                    // Drop the initialized list
                    if *is_initialized {
                        if let Some(drop_fn) = (frame.shape.vtable.drop_in_place)() {
                            unsafe { drop_fn(PtrMut::new(frame.data.as_mut_byte_ptr())) };
                        }
                    }
                }
                Tracker::Map {
                    is_initialized,
                    insert_state,
                } => {
                    // Drop the initialized map
                    if *is_initialized {
                        if let Some(drop_fn) = (frame.shape.vtable.drop_in_place)() {
                            unsafe { drop_fn(PtrMut::new(frame.data.as_mut_byte_ptr())) };
                        }
                    }

                    // Clean up any in-progress insertion state
                    match insert_state {
                        MapInsertState::PushingKey { key_ptr } => {
                            if let Some(key_ptr) = key_ptr {
                                // Deallocate the key buffer
                                if let Def::Map(map_def) = frame.shape.def {
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
                            if let Def::Map(map_def) = frame.shape.def {
                                // Drop and deallocate the key
                                if let Some(drop_fn) = (map_def.k().vtable.drop_in_place)() {
                                    unsafe { drop_fn(PtrMut::new(key_ptr.as_mut_byte_ptr())) };
                                }
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

                                // Drop and deallocate the value if it exists
                                if let Some(value_ptr) = value_ptr {
                                    // Note: value_ptr being Some doesn't mean the value is initialized,
                                    // it just means we allocated space. We should only drop if we know
                                    // it was initialized, but since we're in Drop, we can't know that.
                                    // For safety, we'll just deallocate without dropping.
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
            }

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
