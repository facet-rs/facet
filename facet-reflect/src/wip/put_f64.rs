use crate::{ReflectError, Wip};
use core::num::{
    NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroIsize, NonZeroU8, NonZeroU16, NonZeroU32,
    NonZeroU64, NonZeroUsize,
};
use facet_core::{Def, ScalarAffinity};

impl<'facet_lifetime, 'shape> Wip<'facet_lifetime, 'shape>
where
    'facet_lifetime: 'shape, // Ensure 'facet_lifetime outlives 'shape
{
    /// Returns true if the current frame can accept a f64 into a supported numeric type.
    pub fn can_put_f64(&self) -> bool {
        let shape = self.shape();
        match shape.def {
            Def::Scalar(sd) => matches!(sd.affinity, ScalarAffinity::Number(_)),
            _ => false,
        }
    }

    /// Attempts to put a `f64` into the current frame, converting to the underlying numeric type.
    pub fn try_put_f64(self, number: f64) -> Result<Self, ReflectError<'shape>> {
        let shape = self.shape();
        // Ensure this is a numeric scalar
        match shape.def {
            Def::Scalar(sd) => match sd.affinity {
                ScalarAffinity::Number(_) => {}
                ScalarAffinity::String(_) => {
                    return Err(ReflectError::OperationFailed {
                        shape,
                        operation: "cannot parse number into string scalar",
                    });
                }
                _ => {
                    return Err(ReflectError::OperationFailed {
                        shape,
                        operation: "tried to put f64 into non-number scalar",
                    });
                }
            },
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape,
                    operation: "tried to put f64 into non-scalar type",
                });
            }
        }

        // Match on the concrete Rust type
        if shape.is_type::<u8>() {
            if (0.0..=u8::MAX as f64).contains(&number) {
                self.put(number as u8)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<u16>() {
            if (0.0..=u16::MAX as f64).contains(&number) {
                self.put(number as u16)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<u32>() {
            if (0.0..=u32::MAX as f64).contains(&number) {
                self.put(number as u32)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<u64>() {
            if (0.0..=u64::MAX as f64).contains(&number) {
                self.put(number as u64)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<usize>() {
            if (0.0..=usize::MAX as f64).contains(&number) {
                self.put(number as usize)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<i8>() {
            if (i8::MIN as f64..=i8::MAX as f64).contains(&number) {
                self.put(number as i8)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<i16>() {
            if (i16::MIN as f64..=i16::MAX as f64).contains(&number) {
                self.put(number as i16)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<i32>() {
            if (i32::MIN as f64..=i32::MAX as f64).contains(&number) {
                self.put(number as i32)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<i64>() {
            if (i64::MIN as f64..=i64::MAX as f64).contains(&number) {
                self.put(number as i64)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<isize>() {
            if (isize::MIN as f64..=isize::MAX as f64).contains(&number) {
                self.put(number as isize)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<f32>() {
            if (f32::MIN as f64..=f32::MAX as f64).contains(&number) {
                self.put(number as f32)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<f64>() {
            self.put(number)
        } else if shape.is_type::<NonZeroU8>() {
            if (1.0..=u8::MAX as f64).contains(&number) {
                let value = NonZeroU8::new(number as u8).unwrap();
                self.put(value)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<NonZeroU16>() {
            if (1.0..=u16::MAX as f64).contains(&number) {
                let value = NonZeroU16::new(number as u16).unwrap();
                self.put(value)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<NonZeroU32>() {
            if (1.0..=u32::MAX as f64).contains(&number) {
                let value = NonZeroU32::new(number as u32).unwrap();
                self.put(value)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<NonZeroU64>() {
            if (1.0..=u64::MAX as f64).contains(&number) {
                let value = NonZeroU64::new(number as u64).unwrap();
                self.put(value)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<NonZeroUsize>() {
            if (1.0..=usize::MAX as f64).contains(&number) {
                let value = NonZeroUsize::new(number as usize).unwrap();
                self.put(value)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<NonZeroI8>() {
            if (1.0..=i8::MAX as f64).contains(&number) {
                let value = NonZeroI8::new(number as i8).unwrap();
                self.put(value)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<NonZeroI16>() {
            if (1.0..=i16::MAX as f64).contains(&number) {
                let value = NonZeroI16::new(number as i16).unwrap();
                self.put(value)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<NonZeroI32>() {
            if (1.0..=i32::MAX as f64).contains(&number) {
                let value = NonZeroI32::new(number as i32).unwrap();
                self.put(value)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<NonZeroI64>() {
            if (1.0..=i64::MAX as f64).contains(&number) {
                let value = NonZeroI64::new(number as i64).unwrap();
                self.put(value)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else if shape.is_type::<NonZeroIsize>() {
            if (1.0..=isize::MAX as f64).contains(&number) {
                let value = NonZeroIsize::new(number as isize).unwrap();
                self.put(value)
            } else {
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "number out of range",
                })
            }
        } else {
            Err(ReflectError::OperationFailed {
                shape,
                operation: "number type not supported by try_put_f64",
            })
        }
    }
}
