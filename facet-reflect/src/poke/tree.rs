use std::collections::HashMap;

use facet_core::{Def, Facet, FieldError, OpaqueConst, Variant};

use crate::{ReflectError, ValueId};

use super::{Guard, HeapVal, ISet, PokeValue, PokeValueUninit};

pub struct Frame<'mem> {
    /// The value we're initializing
    value: PokeValueUninit<'mem>,

    /// If set, when we're initialized, we must mark the
    /// parent's indexth field as initialized.
    index: Option<usize>,

    /// Tracking which of our fields are initialized
    istate: IState,
}

/// Initialization state
#[derive(Default)]
struct IState {
    /// Variant chosen — for everything except enums, this stays None
    variant: Option<Variant>,

    /// Fields that were initialized. For scalars, we only track 0
    fields: ISet,
}

pub struct Tree<'mem> {
    /// guarantees the memory allocation for the whole tree
    guard: Guard,

    /// the frames of the tree
    frames: Vec<Frame<'mem>>,

    /// Keeps track of field initialization
    isets: HashMap<ValueId, IState>,
}

impl<'mem> Tree<'mem> {
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
            guard,
            isets: Default::default(),
        }
    }

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
        unsafe { frame.value.data.write(OpaqueConst::new(&raw const t)) };
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
        let Some(frame) = self.frames.pop() else {
            return Err(ReflectError::InvariantViolation);
        };

        // we'll check if everything is initialized at the end
        self.isets.insert(frame.value.id(), frame.istate);

        Ok(self)
    }

    /// Asserts everything is initialized — get back a `HeapAlloc<PokeValue>`
    pub fn finish<T>(mut self) -> Result<HeapVal<PokeValue<'mem>>, ReflectError> {
        let Some(frame) = self.frames.pop() else {
            return Err(ReflectError::InvariantViolation);
        };

        // we'll check if everything is initialized at the end
        self.isets.insert(frame.value.id(), frame.istate);

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

        let shape = self.frames[0].value.shape;
        let data = unsafe { self.frames[0].value.data.assume_init() };

        Ok(HeapVal::Full {
            inner: PokeValue { data, shape },
            guard: self.guard,
        })
    }
}

impl Drop for Tree<'_> {
    fn drop(&mut self) {
        todo!()
    }
}
