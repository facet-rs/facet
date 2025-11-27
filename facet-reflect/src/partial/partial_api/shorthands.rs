use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Shorthands
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Convenience shortcut: sets the field at index `idx` directly to value, popping after.
    ///
    /// Works on structs, enums (after selecting a variant) and arrays.
    pub fn set_nth_field<U>(&mut self, idx: usize, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_nth_field(idx)?.set(value)?.end()
    }

    /// Convenience shortcut: sets the named field to value, popping after.
    pub fn set_field<U>(&mut self, field_name: &str, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_field(field_name)?.set(value)?.end()
    }

    /// Convenience shortcut: sets the key for a map key-value insertion, then pops after.
    pub fn set_key<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_key()?.set(value)?.end()
    }

    /// Convenience shortcut: sets the value for a map key-value insertion, then pops after.
    pub fn set_value<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_value()?.set(value)?.end()
    }

    /// Shorthand for: begin_list_item(), set(), end()
    pub fn push<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_list_item()?.set(value)?.end()
    }

    /// Shorthand for: begin_set_item(), set(), end()
    pub fn insert<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.begin_set_item()?.set(value)?.end()
    }
}
