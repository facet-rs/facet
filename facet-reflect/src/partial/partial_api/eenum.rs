use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Enum variant selection
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, const BORROW: bool> Partial<'facet, BORROW> {
    /// Get the currently selected variant for an enum
    pub fn selected_variant(&self) -> Option<Variant> {
        let frame = self.frames().last()?;

        match &frame.tracker {
            Tracker::Enum { variant, .. } => Some(**variant),
            _ => None,
        }
    }

    /// Find a variant by name in the current enum
    pub fn find_variant(&self, variant_name: &str) -> Option<(usize, &'static Variant)> {
        let frame = self.frames().last()?;

        if let Type::User(UserType::Enum(enum_def)) = frame.allocated.shape().ty {
            enum_def
                .variants
                .iter()
                .enumerate()
                .find(|(_, v)| v.name == variant_name)
        } else {
            None
        }
    }

    /// Assuming the current frame is an enum, this selects a variant by index
    /// (0-based, in declaration order).
    ///
    /// For example:
    ///
    /// ```rust,no_run
    /// enum E { A, B, C }
    /// ```
    ///
    /// Calling `select_nth_variant(2)` would select variant `C`.
    ///
    /// This will return an error if the current frame is anything other than fully-uninitialized.
    /// In other words, it's not possible to "switch to a different variant" once you've selected one.
    ///
    /// This does _not_ push a frame on the stack.
    pub fn select_nth_variant(mut self, index: usize) -> Result<Self, ReflectError> {
        let frame = self.frames().last().unwrap();
        let enum_type = frame.get_enum_type()?;

        if index >= enum_type.variants.len() {
            return Err(ReflectError::OperationFailed {
                shape: frame.allocated.shape(),
                operation: "variant index out of bounds",
            });
        }
        let variant = &enum_type.variants[index];

        self.select_variant_internal(&enum_type, variant)?;
        Ok(self)
    }

    /// Pushes a variant for enum initialization by name
    ///
    /// See [Self::select_nth_variant] for more notes.
    pub fn select_variant_named(mut self, variant_name: &str) -> Result<Self, ReflectError> {
        let frame = self.frames_mut().last_mut().unwrap();
        let enum_type = frame.get_enum_type()?;

        let Some(variant) = enum_type.variants.iter().find(|v| v.name == variant_name) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.allocated.shape(),
                operation: "No variant found with the given name",
            });
        };

        self.select_variant_internal(&enum_type, variant)?;
        Ok(self)
    }

    /// Selects a given enum variant by discriminant. If none of the variants
    /// of the frame's enum have that discriminant, this returns an error.
    ///
    /// See [Self::select_nth_variant] for more notes.
    pub fn select_variant(mut self, discriminant: i64) -> Result<Self, ReflectError> {
        // Check all invariants early before making any changes
        let frame = self.frames().last().unwrap();

        // Check that we're dealing with an enum
        let enum_type = match frame.allocated.shape().ty {
            Type::User(UserType::Enum(e)) => e,
            _ => {
                return Err(ReflectError::WasNotA {
                    expected: "enum",
                    actual: frame.allocated.shape(),
                });
            }
        };

        // Find the variant with the matching discriminant
        let Some(variant) = enum_type
            .variants
            .iter()
            .find(|v| v.discriminant == Some(discriminant))
        else {
            return Err(ReflectError::OperationFailed {
                shape: frame.allocated.shape(),
                operation: "No variant found with the given discriminant",
            });
        };

        // Update the frame tracker to select the variant
        self.select_variant_internal(&enum_type, variant)?;

        Ok(self)
    }
}
