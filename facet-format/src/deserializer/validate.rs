#[cfg(feature = "validate")]
use facet_reflect::Partial;

#[cfg(feature = "validate")]
use crate::DeserializeError;
use crate::{FormatDeserializer, FormatParser};

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    /// Run validation on a field value.
    ///
    /// This checks for `validate::*` attributes on the field and runs
    /// the appropriate validators on the deserialized value.
    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    pub(crate) fn run_field_validators(
        &self,
        field: &facet_core::Field,
        wip: &Partial<'input, BORROW>,
    ) -> Result<(), DeserializeError<P::Error>> {
        use facet_core::ValidatorFn;

        // Get the data pointer from the current frame
        let Some(data_ptr) = wip.data_ptr() else {
            return Ok(());
        };

        // Check for validation attributes
        for attr in field.attributes.iter() {
            if attr.ns != Some("validate") {
                continue;
            }

            let validation_result: Result<(), String> = match attr.key {
                "custom" => {
                    // Custom validators store a ValidatorFn function pointer
                    let validator_fn = unsafe { *attr.data.ptr().get::<ValidatorFn>() };
                    unsafe { validator_fn(data_ptr) }
                }
                "min" => {
                    let min_val = unsafe { *attr.data.ptr().get::<i64>() };
                    self.validate_min(data_ptr, wip.shape(), min_val)
                }
                "max" => {
                    let max_val = unsafe { *attr.data.ptr().get::<i64>() };
                    self.validate_max(data_ptr, wip.shape(), max_val)
                }
                "min_length" => {
                    let min_len = unsafe { *attr.data.ptr().get::<usize>() };
                    self.validate_min_length(data_ptr, wip.shape(), min_len)
                }
                "max_length" => {
                    let max_len = unsafe { *attr.data.ptr().get::<usize>() };
                    self.validate_max_length(data_ptr, wip.shape(), max_len)
                }
                "email" => self.validate_email(data_ptr, wip.shape()),
                "url" => self.validate_url(data_ptr, wip.shape()),
                "regex" => {
                    let pattern = unsafe { *attr.data.ptr().get::<&'static str>() };
                    self.validate_regex(data_ptr, wip.shape(), pattern)
                }
                "contains" => {
                    let needle = unsafe { *attr.data.ptr().get::<&'static str>() };
                    self.validate_contains(data_ptr, wip.shape(), needle)
                }
                _ => Ok(()), // Unknown validator, skip
            };

            if let Err(message) = validation_result {
                return Err(DeserializeError::Validation {
                    field: field.name,
                    message,
                    span: self.last_span,
                    path: Some(self.current_path.clone()),
                });
            }
        }

        Ok(())
    }

    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    fn validate_min(
        &self,
        ptr: facet_core::PtrConst,
        shape: &'static facet_core::Shape,
        min_val: i64,
    ) -> Result<(), String> {
        use facet_core::ScalarType;
        let actual = match shape.scalar_type() {
            Some(ScalarType::I8) => (unsafe { *ptr.get::<i8>() }) as i64,
            Some(ScalarType::I16) => (unsafe { *ptr.get::<i16>() }) as i64,
            Some(ScalarType::I32) => (unsafe { *ptr.get::<i32>() }) as i64,
            Some(ScalarType::I64) => unsafe { *ptr.get::<i64>() },
            Some(ScalarType::U8) => (unsafe { *ptr.get::<u8>() }) as i64,
            Some(ScalarType::U16) => (unsafe { *ptr.get::<u16>() }) as i64,
            Some(ScalarType::U32) => (unsafe { *ptr.get::<u32>() }) as i64,
            Some(ScalarType::U64) => {
                let v = unsafe { *ptr.get::<u64>() };
                if v > i64::MAX as u64 {
                    return Ok(()); // Value too large to compare, assume valid
                }
                v as i64
            }
            _ => return Ok(()), // Not a numeric type, skip validation
        };
        if actual < min_val {
            Err(format!("must be >= {}, got {}", min_val, actual))
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    fn validate_max(
        &self,
        ptr: facet_core::PtrConst,
        shape: &'static facet_core::Shape,
        max_val: i64,
    ) -> Result<(), String> {
        use facet_core::ScalarType;
        let actual = match shape.scalar_type() {
            Some(ScalarType::I8) => (unsafe { *ptr.get::<i8>() }) as i64,
            Some(ScalarType::I16) => (unsafe { *ptr.get::<i16>() }) as i64,
            Some(ScalarType::I32) => (unsafe { *ptr.get::<i32>() }) as i64,
            Some(ScalarType::I64) => unsafe { *ptr.get::<i64>() },
            Some(ScalarType::U8) => (unsafe { *ptr.get::<u8>() }) as i64,
            Some(ScalarType::U16) => (unsafe { *ptr.get::<u16>() }) as i64,
            Some(ScalarType::U32) => (unsafe { *ptr.get::<u32>() }) as i64,
            Some(ScalarType::U64) => {
                let v = unsafe { *ptr.get::<u64>() };
                if v > i64::MAX as u64 {
                    return Err(format!("must be <= {}, got {}", max_val, v));
                }
                v as i64
            }
            _ => return Ok(()), // Not a numeric type, skip validation
        };
        if actual > max_val {
            Err(format!("must be <= {}, got {}", max_val, actual))
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    fn validate_min_length(
        &self,
        ptr: facet_core::PtrConst,
        shape: &'static facet_core::Shape,
        min_len: usize,
    ) -> Result<(), String> {
        let len = self.get_length(ptr, shape)?;
        if len < min_len {
            Err(format!("length must be >= {}, got {}", min_len, len))
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    fn validate_max_length(
        &self,
        ptr: facet_core::PtrConst,
        shape: &'static facet_core::Shape,
        max_len: usize,
    ) -> Result<(), String> {
        let len = self.get_length(ptr, shape)?;
        if len > max_len {
            Err(format!("length must be <= {}, got {}", max_len, len))
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    fn get_length(
        &self,
        ptr: facet_core::PtrConst,
        shape: &'static facet_core::Shape,
    ) -> Result<usize, String> {
        // Check if it's a String
        if shape.is_type::<String>() {
            let s = unsafe { ptr.get::<String>() };
            return Ok(s.len());
        }
        // Check if it's a &str
        if shape.is_type::<&str>() {
            let s = unsafe { *ptr.get::<&str>() };
            return Ok(s.len());
        }
        // For Vec and other list types, we'd need to check shape.def
        // For now, return 0 for unknown types
        Ok(0)
    }

    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    fn validate_email(
        &self,
        ptr: facet_core::PtrConst,
        shape: &'static facet_core::Shape,
    ) -> Result<(), String> {
        let s = self.get_string(ptr, shape)?;
        if facet_validate::is_valid_email(s) {
            Ok(())
        } else {
            Err(format!("'{}' is not a valid email address", s))
        }
    }

    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    fn validate_url(
        &self,
        ptr: facet_core::PtrConst,
        shape: &'static facet_core::Shape,
    ) -> Result<(), String> {
        let s = self.get_string(ptr, shape)?;
        if facet_validate::is_valid_url(s) {
            Ok(())
        } else {
            Err(format!("'{}' is not a valid URL", s))
        }
    }

    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    fn validate_regex(
        &self,
        ptr: facet_core::PtrConst,
        shape: &'static facet_core::Shape,
        pattern: &str,
    ) -> Result<(), String> {
        let s = self.get_string(ptr, shape)?;
        if facet_validate::matches_pattern(s, pattern) {
            Ok(())
        } else {
            Err(format!("'{}' does not match pattern '{}'", s, pattern))
        }
    }

    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    fn validate_contains(
        &self,
        ptr: facet_core::PtrConst,
        shape: &'static facet_core::Shape,
        needle: &str,
    ) -> Result<(), String> {
        let s = self.get_string(ptr, shape)?;
        if s.contains(needle) {
            Ok(())
        } else {
            Err(format!("'{}' does not contain '{}'", s, needle))
        }
    }

    #[cfg(feature = "validate")]
    #[allow(unsafe_code)]
    fn get_string<'s>(
        &self,
        ptr: facet_core::PtrConst,
        shape: &'static facet_core::Shape,
    ) -> Result<&'s str, String> {
        if shape.is_type::<String>() {
            let s = unsafe { ptr.get::<String>() };
            return Ok(s.as_str());
        }
        if shape.is_type::<&str>() {
            let s = unsafe { *ptr.get::<&str>() };
            return Ok(s);
        }
        Err("expected string type".to_string())
    }
}
