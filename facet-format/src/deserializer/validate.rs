use facet_reflect::Partial;

use crate::{DeserializeError, DeserializeErrorKind, FormatDeserializer};

impl<'parser, 'input, const BORROW: bool> FormatDeserializer<'parser, 'input, BORROW> {
    /// Run validation on a field value.
    ///
    /// This checks for `validate::*` attributes on the field and runs
    /// the appropriate validators on the deserialized value.
    #[allow(unsafe_code)]
    pub(crate) fn run_field_validators(
        &self,
        field: &facet_core::Field,
        wip: &Partial<'input, BORROW>,
    ) -> Result<(), DeserializeError> {
        use facet_core::ValidatorFn;

        // Check for validation attributes
        for attr in field.attributes.iter() {
            if attr.ns != Some("validate") {
                continue;
            }

            let validation_result: Result<(), String> = match attr.key {
                "custom" => {
                    // Custom validators need a raw pointer - this is the only case that requires unsafe.
                    // Get the data pointer, ensuring it's fully initialized first.
                    let data_ptr = wip
                        .initialized_data_ptr()
                        .expect("cannot run validator on uninitialized value");
                    // Custom validators store a ValidatorFn function pointer.
                    // ValidatorFn is a function pointer type alias and doesn't implement Facet,
                    // so we need to use unsafe access here.
                    // SAFETY: The validate::custom attribute is defined to store a ValidatorFn.
                    let validator_fn = unsafe { *attr.data.ptr().get::<ValidatorFn>() };
                    // SAFETY: validator_fn was registered by the user and data_ptr points to the field value
                    unsafe { validator_fn(data_ptr) }
                }
                "min" => {
                    let &min_val = attr
                        .get_as::<i64>()
                        .expect("validate::min attribute must contain i64");
                    Self::validate_min(wip, min_val)
                }
                "max" => {
                    let &max_val = attr
                        .get_as::<i64>()
                        .expect("validate::max attribute must contain i64");
                    Self::validate_max(wip, max_val)
                }
                "min_length" => {
                    let &min_len = attr
                        .get_as::<usize>()
                        .expect("validate::min_length attribute must contain usize");
                    Self::validate_min_length(wip, min_len)
                }
                "max_length" => {
                    let &max_len = attr
                        .get_as::<usize>()
                        .expect("validate::max_length attribute must contain usize");
                    Self::validate_max_length(wip, max_len)
                }
                "email" => Self::validate_email(wip),
                "url" => Self::validate_url(wip),
                "regex" => {
                    let &pattern = attr
                        .get_as::<&'static str>()
                        .expect("validate::regex attribute must contain &'static str");
                    Self::validate_regex(wip, pattern)
                }
                "contains" => {
                    let &needle = attr
                        .get_as::<&'static str>()
                        .expect("validate::contains attribute must contain &'static str");
                    Self::validate_contains(wip, needle)
                }
                other => {
                    return Err(DeserializeError {
                        span: Some(self.last_span),
                        path: Some(wip.path()),
                        kind: DeserializeErrorKind::Validation {
                            field: field.name,
                            message: format!("unknown validator '{}'", other).into(),
                        },
                    });
                }
            };

            if let Err(message) = validation_result {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::Validation {
                        field: field.name,
                        message: message.into(),
                    },
                });
            }
        }

        Ok(())
    }

    fn validate_min(wip: &Partial<'input, BORROW>, min_val: i64) -> Result<(), String> {
        use facet_core::ScalarType;
        let actual = match wip.shape().scalar_type() {
            Some(ScalarType::I8) => (*wip.read_as::<i8>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::I16) => (*wip.read_as::<i16>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::I32) => (*wip.read_as::<i32>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::I64) => *wip.read_as::<i64>().ok_or("value not initialized")?,
            Some(ScalarType::U8) => (*wip.read_as::<u8>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::U16) => (*wip.read_as::<u16>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::U32) => (*wip.read_as::<u32>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::U64) => {
                let v = *wip.read_as::<u64>().ok_or("value not initialized")?;
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

    fn validate_max(wip: &Partial<'input, BORROW>, max_val: i64) -> Result<(), String> {
        use facet_core::ScalarType;
        let actual = match wip.shape().scalar_type() {
            Some(ScalarType::I8) => (*wip.read_as::<i8>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::I16) => (*wip.read_as::<i16>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::I32) => (*wip.read_as::<i32>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::I64) => *wip.read_as::<i64>().ok_or("value not initialized")?,
            Some(ScalarType::U8) => (*wip.read_as::<u8>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::U16) => (*wip.read_as::<u16>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::U32) => (*wip.read_as::<u32>().ok_or("value not initialized")?) as i64,
            Some(ScalarType::U64) => {
                let v = *wip.read_as::<u64>().ok_or("value not initialized")?;
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

    fn validate_min_length(wip: &Partial<'input, BORROW>, min_len: usize) -> Result<(), String> {
        let len = Self::get_length(wip)?;
        if len < min_len {
            Err(format!("length must be >= {}, got {}", min_len, len))
        } else {
            Ok(())
        }
    }

    fn validate_max_length(wip: &Partial<'input, BORROW>, max_len: usize) -> Result<(), String> {
        let len = Self::get_length(wip)?;
        if len > max_len {
            Err(format!("length must be <= {}, got {}", max_len, len))
        } else {
            Ok(())
        }
    }

    fn get_length(wip: &Partial<'input, BORROW>) -> Result<usize, String> {
        // Check if it's a String
        if let Some(s) = wip.read_as::<String>() {
            return Ok(s.len());
        }
        // Check if it's a &str
        if let Some(&s) = wip.read_as::<&str>() {
            return Ok(s.len());
        }
        // For Vec and other list types, we'd need to check shape.def
        // For now, return 0 for unknown types
        Ok(0)
    }

    fn validate_email(wip: &Partial<'input, BORROW>) -> Result<(), String> {
        let s = Self::get_string(wip)?;
        if facet_validate::is_valid_email(s) {
            Ok(())
        } else {
            Err(format!("'{}' is not a valid email address", s))
        }
    }

    fn validate_url(wip: &Partial<'input, BORROW>) -> Result<(), String> {
        let s = Self::get_string(wip)?;
        if facet_validate::is_valid_url(s) {
            Ok(())
        } else {
            Err(format!("'{}' is not a valid URL", s))
        }
    }

    fn validate_regex(wip: &Partial<'input, BORROW>, pattern: &str) -> Result<(), String> {
        let s = Self::get_string(wip)?;
        if facet_validate::matches_pattern(s, pattern) {
            Ok(())
        } else {
            Err(format!("'{}' does not match pattern '{}'", s, pattern))
        }
    }

    fn validate_contains(wip: &Partial<'input, BORROW>, needle: &str) -> Result<(), String> {
        let s = Self::get_string(wip)?;
        if s.contains(needle) {
            Ok(())
        } else {
            Err(format!("'{}' does not contain '{}'", s, needle))
        }
    }

    fn get_string<'s>(wip: &'s Partial<'input, BORROW>) -> Result<&'s str, String> {
        if let Some(s) = wip.read_as::<String>() {
            return Ok(s.as_str());
        }
        if let Some(&s) = wip.read_as::<&str>() {
            return Ok(s);
        }
        Err("expected string type".to_string())
    }
}
