//! Structural sameness checking for Facet types.

use core::fmt;
use facet_core::{Def, Facet, Type, UserType};
use facet_pretty::PrettyPrinter;
use facet_reflect::{HasFields, Peek};

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
    let left_peek = Peek::new(left);
    let right_peek = Peek::new(right);

    let mut differ = Differ::new();
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
    fn new() -> Self {
        Self {
            diffs: Vec::new(),
            path: Vec::new(),
        }
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
        let printer = PrettyPrinter::default().with_colors(false);
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
        // Scalars are compared by their formatted representation.
        if matches!(left.shape().def, Def::Scalar) && matches!(right.shape().def, Def::Scalar) {
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
}
