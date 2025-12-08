//! Structural sameness checking for Facet types.

use core::fmt;
use facet_core::{Def, DynValueKind, Facet, Type, UserType};
use facet_pretty::PrettyPrinter;
use facet_reflect::{HasFields, Peek, ScalarType};

/// Options for customizing structural comparison behavior.
///
/// Use the builder pattern to configure options:
///
/// ```
/// use facet_assert::SameOptions;
///
/// let options = SameOptions::new()
///     .float_tolerance(1e-6);
/// ```
#[derive(Debug, Clone, Default)]
pub struct SameOptions {
    /// Tolerance for floating-point comparisons.
    /// If set, two floats are considered equal if their absolute difference
    /// is less than or equal to this value.
    float_tolerance: Option<f64>,
}

impl SameOptions {
    /// Create a new `SameOptions` with default settings (exact comparison).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tolerance for floating-point comparisons.
    ///
    /// When set, two `f32` or `f64` values are considered equal if:
    /// `|left - right| <= tolerance`
    ///
    /// # Example
    ///
    /// ```
    /// use facet_assert::{assert_same_with, SameOptions};
    ///
    /// let a = 1.0000001_f64;
    /// let b = 1.0000002_f64;
    ///
    /// // This would fail with exact comparison:
    /// // assert_same!(a, b);
    ///
    /// // But passes with tolerance:
    /// assert_same_with!(a, b, SameOptions::new().float_tolerance(1e-6));
    /// ```
    pub fn float_tolerance(mut self, tolerance: f64) -> Self {
        self.float_tolerance = Some(tolerance);
        self
    }
}

/// Result of checking if two values are structurally the same.
pub enum Sameness {
    /// The values are structurally the same.
    Same,
    /// The values differ - contains a formatted diff.
    Different(String),
    /// Encountered an opaque type that cannot be compared.
    Opaque {
        /// The type name of the opaque type.
        type_name: &'static str,
    },
}

/// Check if two Facet values are structurally the same.
///
/// This does NOT require `PartialEq` - it walks the structure via reflection.
/// Two values are "same" if they have the same structure and values, even if
/// they have different type names.
///
/// Returns [`Sameness::Opaque`] if either value contains an opaque type.
pub fn check_same<'f, T: Facet<'f>, U: Facet<'f>>(left: &T, right: &U) -> Sameness {
    check_same_with(left, right, SameOptions::default())
}

/// Check if two Facet values are structurally the same, with custom options.
///
/// Like [`check_same`], but allows configuring comparison behavior via [`SameOptions`].
///
/// # Example
///
/// ```
/// use facet_assert::{check_same_with, SameOptions, Sameness};
///
/// let a = 1.0000001_f64;
/// let b = 1.0000002_f64;
///
/// // With tolerance, these are considered the same
/// let options = SameOptions::new().float_tolerance(1e-6);
/// assert!(matches!(check_same_with(&a, &b, options), Sameness::Same));
/// ```
pub fn check_same_with<'f, T: Facet<'f>, U: Facet<'f>>(
    left: &T,
    right: &U,
    options: SameOptions,
) -> Sameness {
    let left_peek = Peek::new(left);
    let right_peek = Peek::new(right);

    let mut differ = Differ::new(options);
    match differ.check(left_peek, right_peek) {
        CheckResult::Same => Sameness::Same,
        CheckResult::Different => Sameness::Different(differ.into_diff()),
        CheckResult::Opaque { type_name } => Sameness::Opaque { type_name },
    }
}

enum CheckResult {
    Same,
    Different,
    Opaque { type_name: &'static str },
}

struct Differ {
    /// Differences found, stored as lines
    diffs: Vec<DiffLine>,
    /// Current path for context
    path: Vec<PathSegment>,
    /// Comparison options
    options: SameOptions,
}

enum PathSegment {
    Field(&'static str),
    Index(usize),
    Variant(&'static str),
    Key(String),
}

impl fmt::Display for PathSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathSegment::Field(name) => write!(f, ".{name}"),
            PathSegment::Index(i) => write!(f, "[{i}]"),
            PathSegment::Variant(name) => write!(f, "::{name}"),
            PathSegment::Key(k) => write!(f, "[{k:?}]"),
        }
    }
}

enum DiffLine {
    Changed {
        path: String,
        left: String,
        right: String,
    },
    OnlyLeft {
        path: String,
        value: String,
    },
    OnlyRight {
        path: String,
        value: String,
    },
}

impl Differ {
    fn new(options: SameOptions) -> Self {
        Self {
            diffs: Vec::new(),
            path: Vec::new(),
            options,
        }
    }

    /// Compare two f64 values, using tolerance if configured.
    fn floats_equal(&self, left: f64, right: f64) -> bool {
        if let Some(tolerance) = self.options.float_tolerance {
            (left - right).abs() <= tolerance
        } else {
            left == right
        }
    }

    /// Try to extract f64 values from two Peek values if they are both floats.
    /// Returns None if either value is not a float type.
    fn extract_floats(&self, left: Peek<'_, '_>, right: Peek<'_, '_>) -> Option<(f64, f64)> {
        let left_f64 = match left.scalar_type()? {
            ScalarType::F64 => *left.get::<f64>().ok()?,
            ScalarType::F32 => *left.get::<f32>().ok()? as f64,
            _ => return None,
        };
        let right_f64 = match right.scalar_type()? {
            ScalarType::F64 => *right.get::<f64>().ok()?,
            ScalarType::F32 => *right.get::<f32>().ok()? as f64,
            _ => return None,
        };
        Some((left_f64, right_f64))
    }

    fn current_path(&self) -> String {
        if self.path.is_empty() {
            "root".to_string()
        } else {
            let mut s = String::new();
            for seg in &self.path {
                s.push_str(&seg.to_string());
            }
            s
        }
    }

    fn format_value(peek: Peek<'_, '_>) -> String {
        let printer = PrettyPrinter::default()
            .with_colors(false)
            .with_minimal_option_names(true);
        printer.format_peek(peek).to_string()
    }

    fn record_changed(&mut self, left: Peek<'_, '_>, right: Peek<'_, '_>) {
        self.diffs.push(DiffLine::Changed {
            path: self.current_path(),
            left: Self::format_value(left),
            right: Self::format_value(right),
        });
    }

    fn record_only_left(&mut self, left: Peek<'_, '_>) {
        self.diffs.push(DiffLine::OnlyLeft {
            path: self.current_path(),
            value: Self::format_value(left),
        });
    }

    fn record_only_right(&mut self, right: Peek<'_, '_>) {
        self.diffs.push(DiffLine::OnlyRight {
            path: self.current_path(),
            value: Self::format_value(right),
        });
    }

    fn into_diff(self) -> String {
        use std::fmt::Write;

        let mut out = String::new();

        for diff in self.diffs {
            match diff {
                DiffLine::Changed { path, left, right } => {
                    writeln!(out, "\x1b[1m{path}\x1b[0m:").unwrap();
                    writeln!(out, "  \x1b[31m- {left}\x1b[0m").unwrap();
                    writeln!(out, "  \x1b[32m+ {right}\x1b[0m").unwrap();
                }
                DiffLine::OnlyLeft { path, value } => {
                    writeln!(out, "\x1b[1m{path}\x1b[0m (only in left):").unwrap();
                    writeln!(out, "  \x1b[31m- {value}\x1b[0m").unwrap();
                }
                DiffLine::OnlyRight { path, value } => {
                    writeln!(out, "\x1b[1m{path}\x1b[0m (only in right):").unwrap();
                    writeln!(out, "  \x1b[32m+ {value}\x1b[0m").unwrap();
                }
            }
        }

        out
    }

    fn check(&mut self, left: Peek<'_, '_>, right: Peek<'_, '_>) -> CheckResult {
        // Handle Option BEFORE innermost_peek (since Option's try_borrow_inner fails)
        if matches!(left.shape().def, Def::Option(_)) && matches!(right.shape().def, Def::Option(_))
        {
            return self.check_options(left, right);
        }

        // Unwrap transparent wrappers (like NonZero, newtype wrappers)
        let left = left.innermost_peek();
        let right = right.innermost_peek();

        // Try scalar comparison first (for leaf values like String, i32, etc.)
        // Scalars are compared by their formatted representation, except for floats
        // with tolerance configured.
        if matches!(left.shape().def, Def::Scalar) && matches!(right.shape().def, Def::Scalar) {
            // Try float comparison with tolerance if configured
            if self.options.float_tolerance.is_some()
                && let Some((left_f64, right_f64)) = self.extract_floats(left, right)
            {
                if self.floats_equal(left_f64, right_f64) {
                    return CheckResult::Same;
                } else {
                    self.record_changed(left, right);
                    return CheckResult::Different;
                }
            }

            // Default: compare by formatted representation
            let left_str = Self::format_value(left);
            let right_str = Self::format_value(right);
            if left_str == right_str {
                return CheckResult::Same;
            } else {
                self.record_changed(left, right);
                return CheckResult::Different;
            }
        }

        // Try to compare structurally based on type/def
        // Note: Many types are UserType::Opaque but still have a useful Def (like Vec -> Def::List)
        // So we check Def first before giving up on Opaque types.

        // Handle lists/arrays/slices (Vec is Opaque but has Def::List)
        if left.into_list_like().is_ok() && right.into_list_like().is_ok() {
            return self.check_lists(left, right);
        }

        // Handle maps
        if matches!(left.shape().def, Def::Map(_)) && matches!(right.shape().def, Def::Map(_)) {
            return self.check_maps(left, right);
        }

        // Handle smart pointers
        if matches!(left.shape().def, Def::Pointer(_))
            && matches!(right.shape().def, Def::Pointer(_))
        {
            return self.check_pointers(left, right);
        }

        // Handle structs
        if let (Type::User(UserType::Struct(_)), Type::User(UserType::Struct(_))) =
            (left.shape().ty, right.shape().ty)
        {
            return self.check_structs(left, right);
        }

        // Handle enums
        if let (Type::User(UserType::Enum(_)), Type::User(UserType::Enum(_))) =
            (left.shape().ty, right.shape().ty)
        {
            return self.check_enums(left, right);
        }

        // Handle dynamic values (like facet_value::Value) - compare based on their runtime kind
        // This allows comparing Value against concrete types (e.g., Value array vs Vec)
        if let Def::DynamicValue(_) = left.shape().def {
            return self.check_with_dynamic_value(left, right);
        }
        if let Def::DynamicValue(_) = right.shape().def {
            return self.check_with_dynamic_value(right, left);
        }

        // At this point, if either is Opaque and we haven't handled it above, fail
        if matches!(left.shape().ty, Type::User(UserType::Opaque)) {
            return CheckResult::Opaque {
                type_name: left.shape().type_identifier,
            };
        }
        if matches!(right.shape().ty, Type::User(UserType::Opaque)) {
            return CheckResult::Opaque {
                type_name: right.shape().type_identifier,
            };
        }

        // Fallback: format and compare
        let left_str = Self::format_value(left);
        let right_str = Self::format_value(right);
        if left_str == right_str {
            CheckResult::Same
        } else {
            self.record_changed(left, right);
            CheckResult::Different
        }
    }

    fn check_structs(&mut self, left: Peek<'_, '_>, right: Peek<'_, '_>) -> CheckResult {
        let left_struct = left.into_struct().unwrap();
        let right_struct = right.into_struct().unwrap();

        let mut any_different = false;
        let mut seen_fields = std::collections::HashSet::new();

        // Check all fields in left
        for (field, left_value) in left_struct.fields() {
            seen_fields.insert(field.name);
            self.path.push(PathSegment::Field(field.name));

            if let Ok(right_value) = right_struct.field_by_name(field.name) {
                match self.check(left_value, right_value) {
                    CheckResult::Same => {}
                    CheckResult::Different => any_different = true,
                    opaque @ CheckResult::Opaque { .. } => {
                        self.path.pop();
                        return opaque;
                    }
                }
            } else {
                // Field only in left
                self.record_only_left(left_value);
                any_different = true;
            }

            self.path.pop();
        }

        // Check fields only in right
        for (field, right_value) in right_struct.fields() {
            if !seen_fields.contains(field.name) {
                self.path.push(PathSegment::Field(field.name));
                self.record_only_right(right_value);
                any_different = true;
                self.path.pop();
            }
        }

        if any_different {
            CheckResult::Different
        } else {
            CheckResult::Same
        }
    }

    fn check_enums(&mut self, left: Peek<'_, '_>, right: Peek<'_, '_>) -> CheckResult {
        let left_enum = left.into_enum().unwrap();
        let right_enum = right.into_enum().unwrap();

        let left_variant = left_enum.active_variant().unwrap();
        let right_variant = right_enum.active_variant().unwrap();

        // Different variants = different
        if left_variant.name != right_variant.name {
            self.record_changed(left, right);
            return CheckResult::Different;
        }

        // Same variant - check fields
        self.path.push(PathSegment::Variant(left_variant.name));

        let mut any_different = false;
        let mut seen_fields = std::collections::HashSet::new();

        for (field, left_value) in left_enum.fields() {
            seen_fields.insert(field.name);
            self.path.push(PathSegment::Field(field.name));

            if let Ok(Some(right_value)) = right_enum.field_by_name(field.name) {
                match self.check(left_value, right_value) {
                    CheckResult::Same => {}
                    CheckResult::Different => any_different = true,
                    opaque @ CheckResult::Opaque { .. } => {
                        self.path.pop();
                        self.path.pop();
                        return opaque;
                    }
                }
            } else {
                self.record_only_left(left_value);
                any_different = true;
            }

            self.path.pop();
        }

        for (field, right_value) in right_enum.fields() {
            if !seen_fields.contains(field.name) {
                self.path.push(PathSegment::Field(field.name));
                self.record_only_right(right_value);
                any_different = true;
                self.path.pop();
            }
        }

        self.path.pop();

        if any_different {
            CheckResult::Different
        } else {
            CheckResult::Same
        }
    }

    fn check_options(&mut self, left: Peek<'_, '_>, right: Peek<'_, '_>) -> CheckResult {
        let left_opt = left.into_option().unwrap();
        let right_opt = right.into_option().unwrap();

        match (left_opt.value(), right_opt.value()) {
            (None, None) => CheckResult::Same,
            (Some(l), Some(r)) => self.check(l, r),
            (Some(_), None) | (None, Some(_)) => {
                self.record_changed(left, right);
                CheckResult::Different
            }
        }
    }

    fn check_lists(&mut self, left: Peek<'_, '_>, right: Peek<'_, '_>) -> CheckResult {
        let left_list = left.into_list_like().unwrap();
        let right_list = right.into_list_like().unwrap();

        let left_items: Vec<_> = left_list.iter().collect();
        let right_items: Vec<_> = right_list.iter().collect();

        let mut any_different = false;
        let min_len = left_items.len().min(right_items.len());

        // Compare common elements
        for i in 0..min_len {
            self.path.push(PathSegment::Index(i));

            match self.check(left_items[i], right_items[i]) {
                CheckResult::Same => {}
                CheckResult::Different => any_different = true,
                opaque @ CheckResult::Opaque { .. } => {
                    self.path.pop();
                    return opaque;
                }
            }

            self.path.pop();
        }

        // Elements only in left (removed)
        for (i, item) in left_items.iter().enumerate().skip(min_len) {
            self.path.push(PathSegment::Index(i));
            self.record_only_left(*item);
            any_different = true;
            self.path.pop();
        }

        // Elements only in right (added)
        for (i, item) in right_items.iter().enumerate().skip(min_len) {
            self.path.push(PathSegment::Index(i));
            self.record_only_right(*item);
            any_different = true;
            self.path.pop();
        }

        if any_different {
            CheckResult::Different
        } else {
            CheckResult::Same
        }
    }

    fn check_maps(&mut self, left: Peek<'_, '_>, right: Peek<'_, '_>) -> CheckResult {
        let left_map = left.into_map().unwrap();
        let right_map = right.into_map().unwrap();

        let mut any_different = false;
        let mut seen_keys = std::collections::HashSet::new();

        for (left_key, left_value) in left_map.iter() {
            let key_str = Self::format_value(left_key);
            seen_keys.insert(key_str.clone());
            self.path.push(PathSegment::Key(key_str));

            // Try to find matching key in right map
            let mut found = false;
            for (right_key, right_value) in right_map.iter() {
                if Self::format_value(left_key) == Self::format_value(right_key) {
                    found = true;
                    match self.check(left_value, right_value) {
                        CheckResult::Same => {}
                        CheckResult::Different => any_different = true,
                        opaque @ CheckResult::Opaque { .. } => {
                            self.path.pop();
                            return opaque;
                        }
                    }
                    break;
                }
            }

            if !found {
                self.record_only_left(left_value);
                any_different = true;
            }

            self.path.pop();
        }

        // Check keys only in right
        for (right_key, right_value) in right_map.iter() {
            let key_str = Self::format_value(right_key);
            if !seen_keys.contains(&key_str) {
                self.path.push(PathSegment::Key(key_str));
                self.record_only_right(right_value);
                any_different = true;
                self.path.pop();
            }
        }

        if any_different {
            CheckResult::Different
        } else {
            CheckResult::Same
        }
    }

    fn check_pointers(&mut self, left: Peek<'_, '_>, right: Peek<'_, '_>) -> CheckResult {
        let left_ptr = left.into_pointer().unwrap();
        let right_ptr = right.into_pointer().unwrap();

        match (left_ptr.borrow_inner(), right_ptr.borrow_inner()) {
            (Some(left_inner), Some(right_inner)) => self.check(left_inner, right_inner),
            (None, None) => CheckResult::Same,
            _ => {
                self.record_changed(left, right);
                CheckResult::Different
            }
        }
    }

    /// Compare a DynamicValue (left) against any other Peek (right) based on the DynamicValue's runtime kind.
    /// This enables comparing e.g. `Value::Array` against `Vec<i32>`.
    fn check_with_dynamic_value(
        &mut self,
        dyn_peek: Peek<'_, '_>,
        other: Peek<'_, '_>,
    ) -> CheckResult {
        let dyn_val = dyn_peek.into_dynamic_value().unwrap();
        let kind = dyn_val.kind();

        match kind {
            DynValueKind::Null => {
                // Null compares equal to () or Option::None
                let other_str = Self::format_value(other);
                if other_str == "()" || other_str == "None" {
                    CheckResult::Same
                } else {
                    self.record_changed(dyn_peek, other);
                    CheckResult::Different
                }
            }
            DynValueKind::Bool => {
                // Compare against bool
                let dyn_bool = dyn_val.as_bool();

                // Check if other is also a DynamicValue bool
                let other_bool = if let Ok(other_dyn) = other.into_dynamic_value() {
                    other_dyn.as_bool()
                } else {
                    let other_str = Self::format_value(other);
                    match other_str.as_str() {
                        "true" => Some(true),
                        "false" => Some(false),
                        _ => None,
                    }
                };

                if dyn_bool == other_bool {
                    CheckResult::Same
                } else {
                    self.record_changed(dyn_peek, other);
                    CheckResult::Different
                }
            }
            DynValueKind::Number => {
                // Check if other is also a DynamicValue number
                if let Ok(other_dyn) = other.into_dynamic_value() {
                    // Compare DynamicValue numbers directly
                    let same = match (dyn_val.as_i64(), other_dyn.as_i64()) {
                        (Some(l), Some(r)) => l == r,
                        _ => match (dyn_val.as_u64(), other_dyn.as_u64()) {
                            (Some(l), Some(r)) => l == r,
                            _ => match (dyn_val.as_f64(), other_dyn.as_f64()) {
                                (Some(l), Some(r)) => self.floats_equal(l, r),
                                _ => false,
                            },
                        },
                    };
                    if same {
                        return CheckResult::Same;
                    } else {
                        self.record_changed(dyn_peek, other);
                        return CheckResult::Different;
                    }
                }

                // Compare against scalar number by parsing formatted value
                let other_str = Self::format_value(other);

                let same = if let Some(dyn_i64) = dyn_val.as_i64() {
                    other_str.parse::<i64>().ok() == Some(dyn_i64)
                } else if let Some(dyn_u64) = dyn_val.as_u64() {
                    other_str.parse::<u64>().ok() == Some(dyn_u64)
                } else if let Some(dyn_f64) = dyn_val.as_f64() {
                    other_str
                        .parse::<f64>()
                        .ok()
                        .is_some_and(|other_f64| self.floats_equal(dyn_f64, other_f64))
                } else {
                    false
                };

                if same {
                    CheckResult::Same
                } else {
                    self.record_changed(dyn_peek, other);
                    CheckResult::Different
                }
            }
            DynValueKind::String => {
                // Compare against string types
                let dyn_str = dyn_val.as_str();

                // Check if other is also a DynamicValue string
                let other_str = if let Ok(other_dyn) = other.into_dynamic_value() {
                    other_dyn.as_str()
                } else {
                    other.as_str()
                };

                if dyn_str == other_str {
                    CheckResult::Same
                } else {
                    self.record_changed(dyn_peek, other);
                    CheckResult::Different
                }
            }
            DynValueKind::Bytes => {
                // Compare against byte slice types
                let dyn_bytes = dyn_val.as_bytes();

                // Check if other is also a DynamicValue bytes
                let other_bytes = if let Ok(other_dyn) = other.into_dynamic_value() {
                    other_dyn.as_bytes()
                } else {
                    other.as_bytes()
                };

                if dyn_bytes == other_bytes {
                    CheckResult::Same
                } else {
                    self.record_changed(dyn_peek, other);
                    CheckResult::Different
                }
            }
            DynValueKind::Array => {
                // Compare against any list-like type (Vec, array, slice, or another DynamicValue array)
                self.check_dyn_array_against_other(dyn_peek, dyn_val, other)
            }
            DynValueKind::Object => {
                // Compare against maps or structs
                self.check_dyn_object_against_other(dyn_peek, dyn_val, other)
            }
            DynValueKind::DateTime => {
                // Compare datetime values by their components
                let dyn_dt = dyn_val.as_datetime();

                // Check if other is also a DynamicValue datetime
                let other_dt = if let Ok(other_dyn) = other.into_dynamic_value() {
                    other_dyn.as_datetime()
                } else {
                    None
                };

                if dyn_dt == other_dt {
                    CheckResult::Same
                } else {
                    self.record_changed(dyn_peek, other);
                    CheckResult::Different
                }
            }
            DynValueKind::QName | DynValueKind::Uuid => {
                // For now, QName and Uuid are compared by formatted representation
                let dyn_str = Self::format_value(dyn_peek);
                let other_str = Self::format_value(other);
                if dyn_str == other_str {
                    CheckResult::Same
                } else {
                    self.record_changed(dyn_peek, other);
                    CheckResult::Different
                }
            }
        }
    }

    fn check_dyn_array_against_other(
        &mut self,
        dyn_peek: Peek<'_, '_>,
        dyn_val: facet_reflect::PeekDynamicValue<'_, '_>,
        other: Peek<'_, '_>,
    ) -> CheckResult {
        let dyn_len = dyn_val.array_len().unwrap_or(0);

        // Check if other is also a DynamicValue array
        if let Ok(other_dyn) = other.into_dynamic_value() {
            if other_dyn.kind() == DynValueKind::Array {
                let other_len = other_dyn.array_len().unwrap_or(0);
                return self
                    .check_two_dyn_arrays(dyn_peek, dyn_val, dyn_len, other, other_dyn, other_len);
            } else {
                self.record_changed(dyn_peek, other);
                return CheckResult::Different;
            }
        }

        // Check if other is list-like
        if let Ok(other_list) = other.into_list_like() {
            let other_len = other_list.len();
            let mut any_different = false;
            let min_len = dyn_len.min(other_len);

            // Compare common elements
            for i in 0..min_len {
                self.path.push(PathSegment::Index(i));

                if let (Some(dyn_elem), Some(other_elem)) =
                    (dyn_val.array_get(i), other_list.get(i))
                {
                    match self.check(dyn_elem, other_elem) {
                        CheckResult::Same => {}
                        CheckResult::Different => any_different = true,
                        opaque @ CheckResult::Opaque { .. } => {
                            self.path.pop();
                            return opaque;
                        }
                    }
                }

                self.path.pop();
            }

            // Elements only in dyn array
            for i in min_len..dyn_len {
                self.path.push(PathSegment::Index(i));
                if let Some(dyn_elem) = dyn_val.array_get(i) {
                    self.record_only_left(dyn_elem);
                    any_different = true;
                }
                self.path.pop();
            }

            // Elements only in other list
            for i in min_len..other_len {
                self.path.push(PathSegment::Index(i));
                if let Some(other_elem) = other_list.get(i) {
                    self.record_only_right(other_elem);
                    any_different = true;
                }
                self.path.pop();
            }

            if any_different {
                CheckResult::Different
            } else {
                CheckResult::Same
            }
        } else {
            // Other is not array-like, they're different
            self.record_changed(dyn_peek, other);
            CheckResult::Different
        }
    }

    fn check_two_dyn_arrays(
        &mut self,
        _left_peek: Peek<'_, '_>,
        left_dyn: facet_reflect::PeekDynamicValue<'_, '_>,
        left_len: usize,
        _right_peek: Peek<'_, '_>,
        right_dyn: facet_reflect::PeekDynamicValue<'_, '_>,
        right_len: usize,
    ) -> CheckResult {
        let mut any_different = false;
        let min_len = left_len.min(right_len);

        // Compare common elements
        for i in 0..min_len {
            self.path.push(PathSegment::Index(i));

            if let (Some(left_elem), Some(right_elem)) =
                (left_dyn.array_get(i), right_dyn.array_get(i))
            {
                match self.check(left_elem, right_elem) {
                    CheckResult::Same => {}
                    CheckResult::Different => any_different = true,
                    opaque @ CheckResult::Opaque { .. } => {
                        self.path.pop();
                        return opaque;
                    }
                }
            }

            self.path.pop();
        }

        // Elements only in left
        for i in min_len..left_len {
            self.path.push(PathSegment::Index(i));
            if let Some(left_elem) = left_dyn.array_get(i) {
                self.record_only_left(left_elem);
                any_different = true;
            }
            self.path.pop();
        }

        // Elements only in right
        for i in min_len..right_len {
            self.path.push(PathSegment::Index(i));
            if let Some(right_elem) = right_dyn.array_get(i) {
                self.record_only_right(right_elem);
                any_different = true;
            }
            self.path.pop();
        }

        if any_different {
            CheckResult::Different
        } else {
            CheckResult::Same
        }
    }

    fn check_dyn_object_against_other(
        &mut self,
        dyn_peek: Peek<'_, '_>,
        dyn_val: facet_reflect::PeekDynamicValue<'_, '_>,
        other: Peek<'_, '_>,
    ) -> CheckResult {
        let dyn_len = dyn_val.object_len().unwrap_or(0);

        // Check if other is also a DynamicValue object
        if let Ok(other_dyn) = other.into_dynamic_value() {
            if other_dyn.kind() == DynValueKind::Object {
                let other_len = other_dyn.object_len().unwrap_or(0);
                return self.check_two_dyn_objects(
                    dyn_peek, dyn_val, dyn_len, other, other_dyn, other_len,
                );
            } else {
                self.record_changed(dyn_peek, other);
                return CheckResult::Different;
            }
        }

        // Check if other is a map
        if let Ok(other_map) = other.into_map() {
            let mut any_different = false;
            let mut seen_keys = std::collections::HashSet::new();

            // Check all entries in dyn object
            for i in 0..dyn_len {
                if let Some((key, dyn_value)) = dyn_val.object_get_entry(i) {
                    seen_keys.insert(key.to_owned());
                    self.path.push(PathSegment::Key(key.to_owned()));

                    // Try to find key in map - need to compare by formatted key
                    let mut found = false;
                    for (map_key, map_value) in other_map.iter() {
                        if Self::format_value(map_key) == format!("{key:?}") {
                            found = true;
                            match self.check(dyn_value, map_value) {
                                CheckResult::Same => {}
                                CheckResult::Different => any_different = true,
                                opaque @ CheckResult::Opaque { .. } => {
                                    self.path.pop();
                                    return opaque;
                                }
                            }
                            break;
                        }
                    }

                    if !found {
                        self.record_only_left(dyn_value);
                        any_different = true;
                    }

                    self.path.pop();
                }
            }

            // Check keys only in map
            for (map_key, map_value) in other_map.iter() {
                let key_str = Self::format_value(map_key);
                // Remove quotes for comparison
                let key_unquoted = key_str.trim_matches('"');
                if !seen_keys.contains(key_unquoted) {
                    self.path.push(PathSegment::Key(key_unquoted.to_owned()));
                    self.record_only_right(map_value);
                    any_different = true;
                    self.path.pop();
                }
            }

            if any_different {
                CheckResult::Different
            } else {
                CheckResult::Same
            }
        } else if let Ok(other_struct) = other.into_struct() {
            // Compare DynamicValue object against struct fields
            let mut any_different = false;
            let mut seen_fields = std::collections::HashSet::new();

            // Check all entries in dyn object against struct fields
            for i in 0..dyn_len {
                if let Some((key, dyn_value)) = dyn_val.object_get_entry(i) {
                    seen_fields.insert(key.to_owned());
                    self.path.push(PathSegment::Key(key.to_owned()));

                    if let Ok(struct_value) = other_struct.field_by_name(key) {
                        match self.check(dyn_value, struct_value) {
                            CheckResult::Same => {}
                            CheckResult::Different => any_different = true,
                            opaque @ CheckResult::Opaque { .. } => {
                                self.path.pop();
                                return opaque;
                            }
                        }
                    } else {
                        self.record_only_left(dyn_value);
                        any_different = true;
                    }

                    self.path.pop();
                }
            }

            // Check struct fields not in dyn object
            for (field, struct_value) in other_struct.fields() {
                if !seen_fields.contains(field.name) {
                    self.path.push(PathSegment::Field(field.name));
                    self.record_only_right(struct_value);
                    any_different = true;
                    self.path.pop();
                }
            }

            if any_different {
                CheckResult::Different
            } else {
                CheckResult::Same
            }
        } else {
            // Other is not object-like, they're different
            self.record_changed(dyn_peek, other);
            CheckResult::Different
        }
    }

    fn check_two_dyn_objects(
        &mut self,
        _left_peek: Peek<'_, '_>,
        left_dyn: facet_reflect::PeekDynamicValue<'_, '_>,
        left_len: usize,
        _right_peek: Peek<'_, '_>,
        right_dyn: facet_reflect::PeekDynamicValue<'_, '_>,
        right_len: usize,
    ) -> CheckResult {
        let mut any_different = false;
        let mut seen_keys = std::collections::HashSet::new();

        // Check all entries in left
        for i in 0..left_len {
            if let Some((key, left_value)) = left_dyn.object_get_entry(i) {
                seen_keys.insert(key.to_owned());
                self.path.push(PathSegment::Key(key.to_owned()));

                if let Some(right_value) = right_dyn.object_get(key) {
                    match self.check(left_value, right_value) {
                        CheckResult::Same => {}
                        CheckResult::Different => any_different = true,
                        opaque @ CheckResult::Opaque { .. } => {
                            self.path.pop();
                            return opaque;
                        }
                    }
                } else {
                    self.record_only_left(left_value);
                    any_different = true;
                }

                self.path.pop();
            }
        }

        // Check entries only in right
        for i in 0..right_len {
            if let Some((key, right_value)) = right_dyn.object_get_entry(i)
                && !seen_keys.contains(key)
            {
                self.path.push(PathSegment::Key(key.to_owned()));
                self.record_only_right(right_value);
                any_different = true;
                self.path.pop();
            }
        }

        if any_different {
            CheckResult::Different
        } else {
            CheckResult::Same
        }
    }
}
