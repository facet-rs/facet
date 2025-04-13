use std::collections::HashMap;

use facet_core::{Def, Facet, FieldError, Variant};

use crate::{ReflectError, ValueId};

use super::{Guard, HeapVal, ISet, PokeValue, PokeValueUninit};

/// Represents a frame in the initialization stack
pub struct Frame<'mem> {
    /// The value we're initializing
    value: PokeValueUninit<'mem>,

    /// If set, when we're initialized, we must mark the
    /// parent's indexth field as initialized.
    index: Option<usize>,

    /// Tracking which of our fields are initialized
    istate: IState,
}

impl Frame<'_> {
    /// Returns true if the frame is fully initialized
    fn is_fully_initialized(&self) -> bool {
        match self.value.shape.def {
            Def::Struct(sd) => self.istate.fields.are_all_set(sd.fields.len()),
            Def::Enum(_) => match self.istate.variant.as_ref() {
                None => false,
                Some(v) => self.istate.fields.are_all_set(v.data.fields.len()),
            },
            _ => {
                todo!()
            }
        }
    }
}

/// Initialization state
#[derive(Default)]
struct IState {
    /// Variant chosen — for everything except enums, this stays None
    variant: Option<Variant>,

    /// Fields that were initialized. For scalars, we only track 0
    fields: ISet,
}

/// A builder for constructing and initializing complex data structures
pub struct Builder<'mem> {
    /// guarantees the memory allocation for the whole tree
    guard: Option<Guard>,

    /// the frames of the tree
    frames: Vec<Frame<'mem>>,

    /// Keeps track of field initialization
    isets: HashMap<ValueId, IState>,
}

impl<'mem> Builder<'mem> {
    /// Creates a new Tree
    pub fn new(value: HeapVal<PokeValueUninit<'mem>>) -> Self {
        let HeapVal::Full { inner, guard } = value else {
            panic!()
        };

        Self {
            frames: vec![Frame {
                value: inner,
                index: None,
                istate: Default::default(),
            }],
            guard: Some(guard),
            isets: Default::default(),
        }
    }

    /// Selects a field of a struct by name and pushes it onto the frame stack.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the field to select.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if the field was successfully selected and pushed.
    /// * `Err(ReflectError)` if the current frame is not a struct or the field doesn't exist.
    pub fn field_named(mut self, name: &str) -> Result<Self, ReflectError> {
        let frame = self.frames.last_mut().unwrap();
        let shape = frame.value.shape();
        let Def::Struct(def) = shape.def else {
            return Err(ReflectError::WasNotA { name: "struct" });
        };
        let (index, field) = def
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == name)
            .ok_or(ReflectError::FieldError {
                shape,
                field_error: FieldError::NoSuchField,
            })?;
        let field_data = unsafe { frame.value.data.field_uninit_at(field.offset) };

        self.frames.push(Frame {
            value: PokeValueUninit {
                data: field_data,
                shape: field.shape,
            },
            index: Some(index),
            istate: Default::default(),
        });
        Ok(self)
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
    pub fn put<T: Facet + 'mem>(mut self, t: T) -> Result<Self, ReflectError> {
        let Some(frame) = self.frames.pop() else {
            return Err(ReflectError::OperationFailed {
                shape: T::SHAPE,
                operation: "tried to put a T but there was no frame to put T into",
            });
        };

        // check that the type matches
        if !frame.value.shape.is_type::<T>() {
            return Err(ReflectError::WrongShape {
                expected: frame.value.shape,
                actual: T::SHAPE,
            });
        }

        // de-initialize partially initialized fields
        if frame.istate.variant.is_some() || frame.istate.fields.is_any_set() {
            todo!(
                "we should de-initialize partially initialized fields for {}",
                frame.value.shape
            );
        }

        // move the value into the frame
        unsafe {
            let size = core::mem::size_of::<T>();
            core::ptr::copy_nonoverlapping(
                &raw const t as *const u8,
                frame.value.data.as_mut_byte_ptr(),
                size,
            );
        };
        core::mem::forget(t);

        // mark the field as initialized
        if let Some(index) = frame.index {
            let Some(parent) = self.frames.last_mut() else {
                return Err(ReflectError::OperationFailed {
                    shape: frame.value.shape,
                    operation: "put was supposed to mark a field as initialized, but there was no parent frame",
                });
            };

            if matches!(parent.value.shape.def, Def::Enum(_)) && parent.istate.variant.is_none() {
                return Err(ReflectError::OperationFailed {
                    shape: frame.value.shape,
                    operation: "put was supposed to mark a field as initialized, but the parent frame was an enum and didn't have a variant chosen",
                });
            }

            if parent.istate.fields.has(index) {
                // TODO: just drop the field in place
                return Err(ReflectError::OperationFailed {
                    shape: frame.value.shape,
                    operation: "put was supposed to mark a field as initialized, but the parent frame already had it marked as initialized",
                });
            }

            parent.istate.fields.set(index);
        }
        Ok(self)
    }

    /// Pops the current frame — goes back up one level
    pub fn pop(mut self) -> Result<Self, ReflectError> {
        let Some(_) = self.pop_inner() else {
            return Err(ReflectError::InvariantViolation);
        };
        Ok(self)
    }

    fn pop_inner(&mut self) -> Option<PokeValueUninit<'mem>> {
        let frame = self.frames.pop()?;

        if frame.is_fully_initialized() {
            if let Some(parent) = self.frames.last_mut() {
                parent.istate.fields.set(frame.index.unwrap());
            };
        }

        // we'll check if everything is initialized at the end
        self.isets.insert(frame.value.id(), frame.istate);

        Some(frame.value)
    }

    /// Asserts everything is initialized — get back a `HeapAlloc<PokeValue>`
    pub fn build(mut self) -> Result<HeapVal<PokeValue<'mem>>, ReflectError> {
        let mut root: Option<PokeValueUninit<'mem>> = None;

        while let Some(frame) = self.pop_inner() {
            root = Some(frame);
        }
        let Some(root) = root else {
            return Err(ReflectError::InvariantViolation);
        };

        for (id, is) in self.isets.drain() {
            let field_count = match id.shape.def {
                Def::Struct(def) => def.fields.len(),
                Def::Enum(_) => todo!(),
                _ => 1,
            };
            if !is.fields.are_all_set(field_count) {
                match id.shape.def {
                    Def::Struct(sd) => {
                        eprintln!("fields were not initialized for struct {}", id.shape);
                        for (i, field) in sd.fields.iter().enumerate() {
                            if !is.fields.has(i) {
                                eprintln!("  {}", field.name);
                            }
                        }
                    }
                    Def::Enum(_) => {
                        todo!()
                    }
                    Def::Scalar(_) => {
                        eprintln!("fields were not initialized for scalar {}", id.shape);
                    }
                    _ => {}
                }
                panic!("some fields were not initialized")
            }
        }

        let shape = root.shape;
        let data = unsafe { root.data.assume_init() };

        Ok(HeapVal::Full {
            inner: PokeValue { data, shape },
            guard: self.guard.take().unwrap(),
        })
    }
}

impl Drop for Builder<'_> {
    fn drop(&mut self) {
        for (id, is) in self.isets.drain() {
            let field_count = match id.shape.def {
                Def::Struct(def) => def.fields.len(),
                Def::Enum(_) => todo!(),
                _ => 1,
            };
            if !is.fields.are_all_set(field_count) {
                match id.shape.def {
                    Def::Struct(sd) => {
                        eprintln!("fields were not initialized for struct {}", id.shape);
                        for (i, field) in sd.fields.iter().enumerate() {
                            if !is.fields.has(i) {
                                eprintln!("  {}", field.name);
                            }
                        }
                    }
                    Def::Enum(_) => {
                        todo!()
                    }
                    Def::Scalar(_) => {
                        eprintln!("fields were not initialized for scalar {}", id.shape);
                    }
                    _ => {}
                }
                panic!("some fields were not initialized")
            }
        }
    }
}
