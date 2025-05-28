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
//! # API Changes
//!
//! Recent API improvements include:
//! - **Renamed from `Wip` to `Partial`** for better clarity about its purpose
//! - **Method naming changes**:
//!   - `push()` → `begin()` - Start working on a nested value
//!   - `pop()` → `end()` - Finish working on a nested value
//!   - camelCase methods → snake_case (Rust convention)
//! - **New convenience methods**:
//!   - `set_nth_field()` - Set a field by index
//!   - `set_named_field()` - Set a field by name
//!   - `set_variant()` - Set enum variant
//!   - `begin_nth_field()` - Begin working on a field by index
//!   - `begin_named_field()` - Begin working on a field by name
//!
//! # Basic Usage
//!
//! ```no_run
//! # use facet_reflect::Partial;
//! # use facet_core::Shape;
//! # fn example(shape: &'static Shape<'static>) -> Result<(), Box<dyn std::error::Error>> {
//! // Allocate memory for a struct
//! let mut partial = Partial::alloc(shape)?;
//!
//! // Set simple fields
//! partial.set_named_field("name", "Alice")?;
//! partial.set_named_field("age", 30u32)?;
//!
//! // Work with nested structures
//! partial.begin_named_field("address")?;
//! partial.set_named_field("street", "123 Main St")?;
//! partial.set_named_field("city", "Springfield")?;
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
//! # use facet_core::Shape;
//! # fn example(shape: &'static Shape<'static>) -> Result<(), Box<dyn std::error::Error>> {
//! let value = Partial::alloc(shape)?
//!     .set_named_field("name", "Bob")?
//!     .begin_named_field("scores")?
//!         .put(vec![95, 87, 92])?
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
//! # use facet_core::Shape;
//! # fn example(shape: &'static Shape<'static>) -> Result<(), Box<dyn std::error::Error>> {
//! let mut partial = Partial::alloc(shape)?; // Assuming shape is for Vec<String>
//!
//! // Add items to a list
//! partial.begin_item()?;
//! partial.put("first")?;
//! partial.end()?;
//!
//! partial.begin_item()?;
//! partial.put("second")?;
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
//! # use facet_core::Shape;
//! # fn example(shape: &'static Shape<'static>) -> Result<(), Box<dyn std::error::Error>> {
//! let mut partial = Partial::alloc(shape)?; // Assuming shape is for HashMap<String, i32>
//!
//! // Insert key-value pairs
//! partial.begin_key()?;
//! partial.put("score")?;
//! partial.end()?;
//! partial.begin_value()?;
//! partial.put(100i32)?;
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

#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;

mod iset;

use crate::{ReflectError, trace};

use core::marker::PhantomData;

mod heap_value;
use alloc::vec::Vec;
pub use heap_value::*;

use facet_core::{
    Def, EnumRepr, Facet, KnownSmartPointer, PtrConst, PtrMut, PtrUninit, Shape, Type, UserType,
    Variant,
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

/// A work-in-progress heap-allocated value
///
/// # Lifetimes
///
/// * `'facet`: The lifetime of borrowed values within the structure (or 'static if it's owned)
/// * `'shape`: The lifetime of the Shape structure itself (often 'static)
pub struct Partial<'facet, 'shape> {
    /// stack of frames to keep track of deeply nested initialization
    frames: Vec<Frame<'shape>>,

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

impl<'facet, 'shape> Partial<'facet, 'shape> {
    /// Allocates a new Partial instance with the given shape
    pub fn alloc_shape(shape: &'shape Shape<'shape>) -> Result<Self, ReflectError<'shape>> {
        let data = shape
            .allocate()
            .map_err(|_| ReflectError::Unsized { shape })?;

        Ok(Self {
            frames: vec![Frame::new(data, shape, FrameOwnership::Owned)],
            state: PartialState::Active,
            invariant: PhantomData,
        })
    }

    /// Allocates a new TypedPartial instance with the given shape and type
    pub fn alloc<T>() -> Result<TypedPartial<'facet, 'shape, T>, ReflectError<'shape>>
    where
        T: Facet<'facet>,
    {
        Ok(TypedPartial {
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
            state: PartialState::Active,
            invariant: PhantomData,
        }
    }

    /// Require that the partial is active
    fn require_active(&self) -> Result<(), ReflectError<'shape>> {
        if self.state == PartialState::Active {
            Ok(())
        } else {
            Err(ReflectError::InvariantViolation {
                invariant: "Cannot use Partial after it has been built or poisoned",
            })
        }
    }

    /// Sets a value wholesale into the current frame
    pub fn set<U>(&mut self, value: U) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.require_active()?;

        // relay to set_shape — convert U into a ptr and shape, and call set_shape
        let ptr_const = PtrConst::new(&raw const value);
        unsafe {
            // Safety: We are calling set_shape with a valid shape and a valid pointer
            self.set_shape(ptr_const, U::SHAPE)?
        };

        // Prevent the value from being dropped since we've copied it
        core::mem::forget(value);
        Ok(self)
    }

    /// Sets a value into the current frame by shape, for shape-based operations
    ///
    /// If this returns Ok, then `src_value` has been moved out of
    ///
    /// # Safety
    ///
    /// The caller must ensure that `src_value` points to a valid instance of a value
    /// whose memory layout and type matches `src_shape`, and that this value can be
    /// safely copied (bitwise) into the destination specified by the Partial's current frame.
    /// No automatic drop will be performed for any existing value, so calling this on an
    /// already-initialized destination may result in leaks or double drops if misused.
    /// After a successful call, the ownership of the value at `src_value` is effectively moved
    /// into the Partial (i.e., the destination), and the original value should not be used
    /// or dropped by the caller; consider using `core::mem::forget` on the passed value.
    /// If an error is returned, the destination remains unmodified and safe for future operations.
    pub unsafe fn set_shape(
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'shape Shape<'shape>,
    ) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;

        let fr = self.frames.last_mut().unwrap();
        if !fr.shape.is_shape(src_shape) {
            let err = ReflectError::WrongShape {
                expected: src_shape,
                actual: fr.shape,
            };
            return Err(err);
        }

        if fr.shape.layout.sized_layout().is_err() {
            return Err(ReflectError::Unsized { shape: fr.shape });
        }

        unsafe {
            fr.data.copy_from(src_value, fr.shape).unwrap();
        }
        fr.tracker = Tracker::Init;
        Ok(self)
    }

    /// Sets the current frame to its default value
    pub fn set_default(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        let frame = self.frames.last().unwrap(); // Get frame to access vtable

        if let Some(default_fn) = (frame.shape.vtable.default_in_place)() {
            // Initialize with default value using set_from_function
            // SAFETY: set_from_function handles the active check, dropping,
            // and setting tracker. The closure passes the correct pointer type
            // and casts to 'static which is safe within the context of calling
            // the vtable function. The closure returns Ok(()) because the
            // default_in_place function does not return errors.
            self.set_from_function(move |ptr: PtrUninit<'_>| {
                unsafe { default_fn(PtrUninit::new(ptr.as_mut_byte_ptr())) };
                Ok(())
            })
        } else {
            // Default function not available, set state and return error
            Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "type does not implement Default",
            })
        }
    }

    /// Sets the current frame using a function that initializes the value
    pub fn set_from_function<F>(&mut self, f: F) -> Result<&mut Self, ReflectError<'shape>>
    where
        F: FnOnce(PtrUninit<'_>) -> Result<(), ReflectError<'shape>>,
    {
        self.require_active()?;

        let frame = self.frames.last_mut().unwrap();

        // Check if we need to drop an existing value
        if matches!(frame.tracker, Tracker::Init) {
            if let Some(drop_fn) = (frame.shape.vtable.drop_in_place)() {
                unsafe { drop_fn(PtrMut::new(frame.data.as_mut_byte_ptr())) };
            }
        }

        // Call the function to initialize the value
        match f(frame.data) {
            Ok(()) => {
                // FIXME: what about finding out the discriminant of enums?
                frame.tracker = Tracker::Init;
                Ok(self)
            }
            Err(e) => Err(e),
        }
    }

    /// Pushes a variant for enum initialization by name
    pub fn select_variant_named(
        &mut self,
        variant_name: &str,
    ) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;

        let fr = self.frames.last_mut().unwrap();

        // Check that we're dealing with an enum
        let enum_type = match fr.shape.ty {
            Type::User(UserType::Enum(e)) => e,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: fr.shape,
                    operation: "push_variant_named requires an enum type",
                });
            }
        };

        // Find the variant with the matching name
        let variant = match enum_type.variants.iter().find(|v| v.name == variant_name) {
            Some(v) => v,
            None => {
                return Err(ReflectError::OperationFailed {
                    shape: fr.shape,
                    operation: "No variant found with the given name",
                });
            }
        };

        // Get the discriminant value
        let discriminant = match variant.discriminant {
            Some(d) => d,
            None => {
                return Err(ReflectError::OperationFailed {
                    shape: fr.shape,
                    operation: "Variant has no discriminant value",
                });
            }
        };

        // Delegate to push_variant
        self.select_variant(discriminant)
    }

    /// Pushes a variant for enum initialization
    pub fn select_variant(&mut self, discriminant: i64) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;

        // Check all invariants early before making any changes
        let fr = self.frames.last().unwrap();

        // Check that we're dealing with an enum
        let enum_type = match fr.shape.ty {
            Type::User(UserType::Enum(e)) => e,
            _ => {
                return Err(ReflectError::WrongShape {
                    expected: fr.shape,
                    actual: fr.shape,
                });
            }
        };

        // Find the variant with the matching discriminant
        let variant = match enum_type
            .variants
            .iter()
            .find(|v| v.discriminant == Some(discriminant))
        {
            Some(v) => v,
            None => {
                return Err(ReflectError::OperationFailed {
                    shape: fr.shape,
                    operation: "No variant found with the given discriminant",
                });
            }
        };

        // Check enum representation early
        match enum_type.enum_repr {
            EnumRepr::RustNPO => {
                return Err(ReflectError::OperationFailed {
                    shape: fr.shape,
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
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: fr.shape,
                    operation: "Unknown enum representation",
                });
            }
        }

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
            variant: *variant,
            data: ISet::new(variant.data.fields.len()),
            current_child: None,
        };

        Ok(self)
    }

    /// Selects a field of a struct with a given name
    pub fn begin_field(&mut self, field_name: &str) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;

        let frame = self.frames.last_mut().unwrap();
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
                UserType::Struct(struct_type) => {
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
                    self.begin_nth_field(idx)
                }
                UserType::Enum(_) => {
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
                            self.begin_nth_enum_field(idx)
                        }
                        _ => Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "must call push_variant before selecting enum fields",
                        }),
                    }
                }
                UserType::Union(_) => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "unions are not supported",
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
            _ => todo!(),
        }
    }

    /// Begins a variant for enum initialization, by variant index in the enum's variant list (0-based)
    pub fn begin_nth_variant(&mut self, index: usize) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;

        let fr = self.frames.last().unwrap();

        // Check that we're dealing with an enum
        let enum_type = match fr.shape.ty {
            Type::User(UserType::Enum(e)) => e,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: fr.shape,
                    operation: "begin_nth_variant requires an enum type",
                });
            }
        };

        if index >= enum_type.variants.len() {
            return Err(ReflectError::OperationFailed {
                shape: fr.shape,
                operation: "variant index out of bounds",
            });
        }
        let variant = &enum_type.variants[index];

        // Get the discriminant value
        let discriminant = match variant.discriminant {
            Some(d) => d,
            None => {
                return Err(ReflectError::OperationFailed {
                    shape: fr.shape,
                    operation: "Variant has no discriminant value",
                });
            }
        };

        // Delegate to begin_variant
        self.select_variant(discriminant)
    }

    /// Selects the nth field of a struct by index
    pub fn begin_nth_field(&mut self, idx: usize) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();
        match frame.shape.ty {
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => {
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

                    Ok(self)
                }
                UserType::Enum(_) => {
                    todo!("add support for selecting fields in enums")
                }
                UserType::Union(_) => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "unions are not supported",
                }),
                UserType::Opaque => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "opaque types cannot be reflected upon",
                }),
            },
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from this type",
            }),
        }
    }

    /// Selects the nth element of an array by index
    pub fn begin_nth_element(&mut self, idx: usize) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();
        match frame.shape.ty {
            Type::Sequence(seq_type) => match seq_type {
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
                            let element_layout = match array_def.t.layout.sized_layout() {
                                Ok(layout) => layout,
                                Err(_) => {
                                    return Err(ReflectError::Unsized { shape: array_def.t });
                                }
                            };
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

                            Ok(self)
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
    pub fn begin_nth_enum_field(&mut self, idx: usize) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        // Ensure we're in an enum with a variant selected
        let (variant, enum_type) = match (&frame.tracker, &frame.shape.ty) {
            (Tracker::Enum { variant, .. }, Type::User(UserType::Enum(e))) => (variant, e),
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
                        EnumRepr::U8 | EnumRepr::I8 => 1,
                        EnumRepr::U16 | EnumRepr::I16 => 2,
                        EnumRepr::U32 | EnumRepr::I32 => 4,
                        EnumRepr::U64 | EnumRepr::I64 => 8,
                        EnumRepr::USize | EnumRepr::ISize => core::mem::size_of::<usize>(),
                        EnumRepr::RustNPO => {
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

        Ok(self)
    }

    /// Pushes a frame to initialize the inner value of a smart pointer (Box<T>, Arc<T>, etc.)
    pub fn begin_smart_ptr(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;
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
                let pointee_shape = match smart_ptr_def.pointee() {
                    Some(shape) => shape,
                    None => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "Box must have a pointee shape",
                        });
                    }
                };

                // Update tracker to SmartPointer state
                if matches!(frame.tracker, Tracker::Uninit) {
                    frame.tracker = Tracker::SmartPointer {
                        is_initialized: false,
                    };
                }

                // Allocate space for the inner value
                let inner_layout = match pointee_shape.layout.sized_layout() {
                    Ok(layout) => layout,
                    Err(_) => {
                        return Err(ReflectError::Unsized {
                            shape: pointee_shape,
                        });
                    }
                };
                let inner_ptr: *mut u8 = unsafe { alloc::alloc::alloc(inner_layout) };

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

                Ok(self)
            }
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "push_smart_ptr can only be called on compatible types",
            }),
        }
    }

    /// Begins a pushback operation for a list (Vec, etc.)
    /// This initializes the list with default capacity and allows pushing elements
    pub fn begin_list(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;
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

    /// Begins a map initialization operation
    /// This initializes the map with default capacity and allows inserting key-value pairs
    pub fn begin_map(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
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

        Ok(self)
    }

    /// Begins inserting a key-value pair into the map
    /// After calling this, use push_key() and push_value() to set the key and value
    pub fn begin_insert(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;
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
                Ok(self)
            }
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "must call begin_map() before begin_insert()",
            }),
        }
    }

    /// Pushes a frame for the map key
    /// Must be called after begin_insert()
    pub fn begin_key(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;
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
        let key_layout = match key_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return Err(ReflectError::Unsized { shape: key_shape });
            }
        };
        let key_ptr_raw: *mut u8 = unsafe { alloc::alloc::alloc(key_layout) };

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
            FrameOwnership::ManagedElsewhere, // Ownership tracked in MapInsertState
        ));

        Ok(self)
    }

    /// Pushes a frame for the map value
    /// Must be called after the key has been set and popped
    pub fn begin_value(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
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
                return Err(ReflectError::Unsized { shape: value_shape });
            }
        };
        let value_ptr_raw: *mut u8 = unsafe { alloc::alloc::alloc(value_layout) };

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
            FrameOwnership::ManagedElsewhere, // Ownership tracked in MapInsertState
        ));

        Ok(self)
    }

    /// Pushes an element to the list
    /// The element should be set using `set()` or similar methods, then `pop()` to complete
    pub fn begin_list_item(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;
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
        let element_layout = match element_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return Err(ReflectError::Unsized {
                    shape: element_shape,
                });
            }
        };
        let element_ptr: *mut u8 = unsafe { alloc::alloc::alloc(element_layout) };

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

        Ok(self)
    }

    /// Pops the current frame off the stack, indicating we're done initializing the current field.
    pub fn end(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.require_active()?;
        if self.frames.len() <= 1 {
            // Never pop the last/root frame.
            return Err(ReflectError::InvariantViolation {
                invariant: "Wip::end() called with only one frame on the stack",
            });
        }

        // Require that the top frame is fully initialized before popping.
        {
            let frame = self.frames.last().unwrap();
            frame.require_full_initialization()?
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
            _ => {}
        }

        Ok(self)
    }

    /// Builds the value
    pub fn build(&mut self) -> Result<HeapValue<'facet, 'shape>, ReflectError<'shape>> {
        self.require_active()?;
        if self.frames.len() != 1 {
            self.state = PartialState::BuildFailed;
            return Err(ReflectError::InvariantViolation {
                invariant: "Wip::build() expects a single frame — pop until that's the case",
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
        if let Some(invariants_fn) = (frame.shape.vtable.invariants)() {
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
            .map_err(|_| ReflectError::Unsized { shape: frame.shape })
        {
            Ok(layout) => Ok(HeapValue {
                guard: Some(Guard {
                    ptr: frame.data.as_mut_byte_ptr(),
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

    /// Returns a human-readable path representing the current traversal in the builder,
    /// e.g., "RootStruct.fieldName[index].subfield".
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
                            path_components.push(format!(".{}", field_name));
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
                            path_components.push(format!("[{}]", idx));
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

    /// Returns the shape of the current frame.
    pub fn shape(&self) -> &'shape Shape<'shape> {
        self.frames
            .last()
            .expect("Partial always has at least one frame")
            .shape
    }

    /// Convenience shortcut: sets the nth element of an array directly to value, popping after.
    pub fn set_nth_element<U>(
        &mut self,
        idx: usize,
        value: U,
    ) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.begin_nth_element(idx)?.set(value)?.end()
    }

    /// Convenience shortcut: sets the field at index `idx` directly to value, popping after.
    pub fn set_nth_field<U>(
        &mut self,
        idx: usize,
        value: U,
    ) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.begin_nth_field(idx)?.set(value)?.end()
    }

    /// Convenience shortcut: sets the named field to value, popping after.
    pub fn set_field<U>(
        &mut self,
        field_name: &str,
        value: U,
    ) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.begin_field(field_name)?.set(value)?.end()
    }

    /// Convenience shortcut: sets the nth field of an enum variant directly to value, popping after.
    pub fn set_nth_enum_field<U>(
        &mut self,
        idx: usize,
        value: U,
    ) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.begin_nth_enum_field(idx)?.set(value)?.end()
    }

    /// Convenience shortcut: sets the key for a map key-value insertion, then pops after.
    pub fn set_key<U>(&mut self, value: U) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.begin_key()?.set(value)?.end()
    }

    /// Convenience shortcut: sets the value for a map key-value insertion, then pops after.
    pub fn set_value<U>(&mut self, value: U) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.begin_value()?.set(value)?.end()
    }

    /// Shorthand for: begin_list_item(), set, end
    pub fn push<U>(&mut self, value: U) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.begin_list_item()?.set(value)?.end()
    }
}

/// A typed wrapper around `Wip`, for when you want to statically
/// ensure that `build` gives you the proper type.
pub struct TypedPartial<'facet, 'shape, T> {
    wip: Partial<'facet, 'shape>,
    phantom: PhantomData<T>,
}

impl<'facet, 'shape, T> TypedPartial<'facet, 'shape, T> {
    /// Builds the value and returns a Box<T>
    pub fn build(&mut self) -> Result<Box<T>, ReflectError<'shape>>
    where
        T: Facet<'facet>,
        'facet: 'shape,
    {
        let heap_value = self.wip.build()?;
        // Safety: HeapValue was constructed from T and the shape layout is correct.
        unsafe { Ok(heap_value.into_box_unchecked::<T>()) }
    }

    /// Sets a value wholesale into the current frame
    pub fn set<U>(&mut self, value: U) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.wip.set(value)?;
        Ok(self)
    }

    /// Sets a value into the current frame by shape, for shape-based operations
    pub fn set_shape(
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'shape Shape<'shape>,
    ) -> Result<&mut Self, ReflectError<'shape>> {
        unsafe { self.wip.set_shape(src_value, src_shape)? };
        Ok(self)
    }

    /// Forwards begin_field to the inner wip instance.
    pub fn begin_field(&mut self, field_name: &str) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_field(field_name)?;
        Ok(self)
    }

    /// Forwards begin_nth_field to the inner wip instance.
    pub fn begin_nth_field(&mut self, idx: usize) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_nth_field(idx)?;
        Ok(self)
    }

    /// Forwards begin_nth_element to the inner wip instance.
    pub fn begin_nth_element(&mut self, idx: usize) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_nth_element(idx)?;
        Ok(self)
    }

    /// Forwards begin_smart_ptr to the inner wip instance.
    pub fn begin_smart_ptr(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_smart_ptr()?;
        Ok(self)
    }

    /// Forwards end to the inner wip instance.
    pub fn end(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.end()?;
        Ok(self)
    }

    /// Forwards set_default to the inner wip instance.
    pub fn set_default(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.set_default()?;
        Ok(self)
    }

    /// Forwards set_from_function to the inner wip instance.
    pub fn set_from_function<F>(&mut self, f: F) -> Result<&mut Self, ReflectError<'shape>>
    where
        F: FnOnce(PtrUninit<'_>) -> Result<(), ReflectError<'shape>>,
    {
        self.wip.set_from_function(f)?;
        Ok(self)
    }

    /// Forwards begin_variant to the inner wip instance.
    pub fn select_variant(&mut self, discriminant: i64) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.select_variant(discriminant)?;
        Ok(self)
    }

    /// Forwards begin_variant_named to the inner wip instance.
    pub fn select_variant_named(
        &mut self,
        variant_name: &str,
    ) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.select_variant_named(variant_name)?;
        Ok(self)
    }

    /// Forwards begin_nth_enum_field to the inner wip instance.
    pub fn begin_nth_enum_field(&mut self, idx: usize) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_nth_enum_field(idx)?;
        Ok(self)
    }

    /// Forwards begin_pushback to the inner wip instance.
    pub fn begin_list(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_list()?;
        Ok(self)
    }

    /// Forwards begin_list_item to the inner wip instance.
    pub fn begin_list_item(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_list_item()?;
        Ok(self)
    }

    /// Forwards begin_map to the inner wip instance.
    pub fn begin_map(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_map()?;
        Ok(self)
    }

    /// Forwards begin_insert to the inner wip instance.
    pub fn begin_insert(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_insert()?;
        Ok(self)
    }

    /// Forwards begin_key to the inner wip instance.
    pub fn begin_key(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_key()?;
        Ok(self)
    }

    /// Forwards begin_value to the inner wip instance.
    pub fn begin_value(&mut self) -> Result<&mut Self, ReflectError<'shape>> {
        self.wip.begin_value()?;
        Ok(self)
    }

    /// Returns a human-readable path representing the current traversal in the builder,
    /// e.g., "RootStruct.fieldName[index].subfield".
    pub fn path(&self) -> String {
        self.wip.path()
    }

    /// Returns the shape of the current frame.
    pub fn shape(&self) -> &'shape Shape<'shape> {
        self.wip.shape()
    }

    /// Convenience shortcut: sets the nth element of an array directly to value, popping after.
    pub fn set_nth_element<U>(
        &mut self,
        idx: usize,
        value: U,
    ) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.wip.set_nth_element(idx, value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the field at index `idx` directly to value, popping after.
    pub fn set_nth_field<U>(
        &mut self,
        idx: usize,
        value: U,
    ) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.wip.set_nth_field(idx, value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the named field to value, popping after.
    pub fn set_field<U>(
        &mut self,
        field_name: &str,
        value: U,
    ) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.wip.set_field(field_name, value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the nth field of an enum variant directly to value, popping after.
    pub fn set_nth_enum_field<U>(
        &mut self,
        idx: usize,
        value: U,
    ) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.wip.set_nth_enum_field(idx, value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the key for a map key-value insertion, then pops after.
    pub fn set_key<U>(&mut self, value: U) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.wip.set_key(value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the value for a map key-value insertion, then pops after.
    pub fn set_value<U>(&mut self, value: U) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.wip.set_value(value)?;
        Ok(self)
    }

    /// Forwards push to the inner wip instance.
    pub fn push<U>(&mut self, value: U) -> Result<&mut Self, ReflectError<'shape>>
    where
        U: Facet<'facet>,
    {
        self.wip.push(value)?;
        Ok(self)
    }
}

impl<'facet, 'shape, T> core::fmt::Debug for TypedPartial<'facet, 'shape, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TypedWip")
            .field("shape", &self.wip.frames.last().map(|frame| frame.shape))
            .finish()
    }
}

impl<'facet, 'shape> Drop for Partial<'facet, 'shape> {
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
                    if let Type::Sequence(facet_core::SequenceType::Array(array_def)) =
                        frame.shape.ty
                    {
                        let element_layout = array_def.t.layout.sized_layout().ok();
                        if let Some(layout) = element_layout {
                            for idx in 0..array_def.n {
                                if iset.get(idx) {
                                    let offset = layout.size() * idx;
                                    let element_ptr = unsafe { frame.data.field_init_at(offset) };
                                    if let Some(drop_fn) = (array_def.t.vtable.drop_in_place)() {
                                        unsafe { drop_fn(element_ptr) };
                                    }
                                }
                            }
                        }
                    }
                }
                Tracker::Struct { iset, .. } => {
                    // Drop initialized struct fields
                    if let Type::User(UserType::Struct(struct_type)) = frame.shape.ty {
                        for (idx, field) in struct_type.fields.iter().enumerate() {
                            if iset.get(idx) {
                                // This field was initialized, drop it
                                let field_ptr = unsafe { frame.data.field_init_at(field.offset) };
                                if let Some(drop_fn) = (field.shape.vtable.drop_in_place)() {
                                    unsafe { drop_fn(field_ptr) };
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
