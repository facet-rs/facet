// TODO: Consider using an approach similar to `morph` (bearcove's fork of difftastic)
// to compute and display the optimal diff path for complex structural changes.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use facet::{Def, DynValueKind, StructKind, Type, UserType};
use facet_core::Facet;
use facet_diff_core::{Diff, Path, PathSegment, Updates, Value};
use facet_reflect::{HasFields, Peek, ScalarType};

use crate::sequences;

/// Configuration options for diff computation
#[derive(Debug, Clone, Default)]
pub struct DiffOptions {
    /// Tolerance for floating-point comparisons.
    /// If set, two floats are considered equal if their absolute difference
    /// is less than or equal to this value.
    pub float_tolerance: Option<f64>,
}

impl DiffOptions {
    /// Create a new `DiffOptions` with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tolerance for floating-point comparisons.
    pub const fn with_float_tolerance(mut self, tolerance: f64) -> Self {
        self.float_tolerance = Some(tolerance);
        self
    }
}

/// Extension trait that provides a [`diff`](FacetDiff::diff) method for `Facet` types
pub trait FacetDiff<'f>: Facet<'f> {
    /// Computes the difference between two values that implement `Facet`
    fn diff<'a, U: Facet<'f>>(&'a self, other: &'a U) -> Diff<'a, 'f>;
}

impl<'f, T: Facet<'f>> FacetDiff<'f> for T {
    fn diff<'a, U: Facet<'f>>(&'a self, other: &'a U) -> Diff<'a, 'f> {
        diff_new(self, other)
    }
}

/// Computes the difference between two values that implement `Facet`
pub fn diff_new<'mem, 'facet, T: Facet<'facet>, U: Facet<'facet>>(
    from: &'mem T,
    to: &'mem U,
) -> Diff<'mem, 'facet> {
    diff_new_peek(Peek::new(from), Peek::new(to))
}

/// Computes the difference between two `Peek` values with options
pub fn diff_new_peek_with_options<'mem, 'facet>(
    from: Peek<'mem, 'facet>,
    to: Peek<'mem, 'facet>,
    options: &DiffOptions,
) -> Diff<'mem, 'facet> {
    // Dereference pointers/references to compare the underlying values
    let from = deref_if_pointer(from);
    let to = deref_if_pointer(to);

    // Check for equality if both shapes have the same type_identifier and implement PartialEq
    // This handles cases where shapes are structurally equivalent but have different IDs
    // (e.g., after deserialization)
    let same_type = from.shape().type_identifier == to.shape().type_identifier;
    let from_has_partialeq = from.shape().is_partial_eq();
    let to_has_partialeq = to.shape().is_partial_eq();
    let values_equal = from == to;

    // Check float tolerance if configured
    let float_equal = options
        .float_tolerance
        .map(|tol| check_float_tolerance(from, to, tol))
        .unwrap_or(false);

    // log::trace!(
    //     "diff_new_peek: type={} same_type={} from_has_partialeq={} to_has_partialeq={} values_equal={}",
    //     from.shape().type_identifier,
    //     same_type,
    //     from_has_partialeq,
    //     to_has_partialeq,
    //     values_equal
    // );

    if same_type && from_has_partialeq && to_has_partialeq && (values_equal || float_equal) {
        return Diff::Equal { value: Some(from) };
    }

    match (
        (from.shape().def, from.shape().ty),
        (to.shape().def, to.shape().ty),
    ) {
        ((_, Type::User(UserType::Struct(from_ty))), (_, Type::User(UserType::Struct(to_ty))))
            if from_ty.kind == to_ty.kind =>
        {
            let from_ty = from.into_struct().unwrap();
            let to_ty = to.into_struct().unwrap();

            let value = if [StructKind::Tuple, StructKind::TupleStruct].contains(&from_ty.ty().kind)
            {
                let from = from_ty.fields().map(|x| x.1).collect();
                let to = to_ty.fields().map(|x| x.1).collect();

                let updates = sequences::diff_with_options(from, to, options);

                Value::Tuple { updates }
            } else {
                let mut updates = HashMap::new();
                let mut deletions = HashMap::new();
                let mut insertions = HashMap::new();
                let mut unchanged = HashSet::new();

                for (field, from) in from_ty.fields() {
                    if let Ok(to) = to_ty.field_by_name(field.name) {
                        // Check for field-level proxy - if present, convert values through
                        // the proxy before comparing (needed for opaque types).
                        // Since OwnedPeek has a limited lifetime, we compare proxies for
                        // equality but return results referencing the original values.
                        let diff = if field.proxy.is_some() {
                            match (
                                from.custom_serialization(field),
                                to.custom_serialization(field),
                            ) {
                                (Ok(from_proxy), Ok(to_proxy)) => {
                                    let proxy_diff = diff_new_peek_with_options(
                                        from_proxy.as_peek(),
                                        to_proxy.as_peek(),
                                        options,
                                    );
                                    // Map the proxy diff result back to original values
                                    if proxy_diff.is_equal() {
                                        Diff::Equal { value: Some(from) }
                                    } else {
                                        Diff::Replace { from, to }
                                    }
                                }
                                // If proxy conversion fails, fall back to direct comparison
                                _ => diff_new_peek_with_options(from, to, options),
                            }
                        } else {
                            diff_new_peek_with_options(from, to, options)
                        };
                        if diff.is_equal() {
                            unchanged.insert(Cow::Borrowed(field.name));
                        } else {
                            updates.insert(Cow::Borrowed(field.name), diff);
                        }
                    } else {
                        deletions.insert(Cow::Borrowed(field.name), from);
                    }
                }

                for (field, to) in to_ty.fields() {
                    if from_ty.field_by_name(field.name).is_err() {
                        insertions.insert(Cow::Borrowed(field.name), to);
                    }
                }
                Value::Struct {
                    updates,
                    deletions,
                    insertions,
                    unchanged,
                }
            };

            // If there are no changes, return Equal instead of User
            let is_empty = match &value {
                Value::Tuple { updates } => updates.is_empty(),
                Value::Struct {
                    updates,
                    deletions,
                    insertions,
                    ..
                } => updates.is_empty() && deletions.is_empty() && insertions.is_empty(),
            };
            if is_empty {
                return Diff::Equal { value: Some(from) };
            }

            Diff::User {
                from: from.shape(),
                to: to.shape(),
                variant: None,
                value,
            }
        }
        ((_, Type::User(UserType::Enum(_))), (_, Type::User(UserType::Enum(_)))) => {
            let from_enum = from.into_enum().unwrap();
            let to_enum = to.into_enum().unwrap();

            let from_variant = from_enum.active_variant().unwrap();
            let to_variant = to_enum.active_variant().unwrap();

            if from_variant.name != to_variant.name
                || from_variant.data.kind != to_variant.data.kind
            {
                return Diff::Replace { from, to };
            }

            let value =
                if [StructKind::Tuple, StructKind::TupleStruct].contains(&from_variant.data.kind) {
                    let from = from_enum.fields().map(|x| x.1).collect();
                    let to = to_enum.fields().map(|x| x.1).collect();

                    let updates = sequences::diff_with_options(from, to, options);

                    Value::Tuple { updates }
                } else {
                    let mut updates = HashMap::new();
                    let mut deletions = HashMap::new();
                    let mut insertions = HashMap::new();
                    let mut unchanged = HashSet::new();

                    for (field, from) in from_enum.fields() {
                        if let Ok(Some(to)) = to_enum.field_by_name(field.name) {
                            // Check for field-level proxy - if present, convert values through
                            // the proxy before comparing (needed for opaque types).
                            // Since OwnedPeek has a limited lifetime, we compare proxies for
                            // equality but return results referencing the original values.
                            let diff = if field.proxy.is_some() {
                                match (
                                    from.custom_serialization(field),
                                    to.custom_serialization(field),
                                ) {
                                    (Ok(from_proxy), Ok(to_proxy)) => {
                                        let proxy_diff = diff_new_peek_with_options(
                                            from_proxy.as_peek(),
                                            to_proxy.as_peek(),
                                            options,
                                        );
                                        // Map the proxy diff result back to original values
                                        if proxy_diff.is_equal() {
                                            Diff::Equal { value: Some(from) }
                                        } else {
                                            Diff::Replace { from, to }
                                        }
                                    }
                                    // If proxy conversion fails, fall back to direct comparison
                                    _ => diff_new_peek_with_options(from, to, options),
                                }
                            } else {
                                diff_new_peek_with_options(from, to, options)
                            };
                            if diff.is_equal() {
                                unchanged.insert(Cow::Borrowed(field.name));
                            } else {
                                updates.insert(Cow::Borrowed(field.name), diff);
                            }
                        } else {
                            deletions.insert(Cow::Borrowed(field.name), from);
                        }
                    }

                    for (field, to) in to_enum.fields() {
                        if !from_enum
                            .field_by_name(field.name)
                            .is_ok_and(|x| x.is_some())
                        {
                            insertions.insert(Cow::Borrowed(field.name), to);
                        }
                    }

                    Value::Struct {
                        updates,
                        deletions,
                        insertions,
                        unchanged,
                    }
                };

            // If there are no changes, return Equal instead of User
            let is_empty = match &value {
                Value::Tuple { updates } => updates.is_empty(),
                Value::Struct {
                    updates,
                    deletions,
                    insertions,
                    ..
                } => updates.is_empty() && deletions.is_empty() && insertions.is_empty(),
            };
            if is_empty {
                return Diff::Equal { value: Some(from) };
            }

            Diff::User {
                from: from_enum.shape(),
                to: to_enum.shape(),
                variant: Some(from_variant.name),
                value,
            }
        }
        ((Def::Option(_), _), (Def::Option(_), _)) => {
            let from_option = from.into_option().unwrap();
            let to_option = to.into_option().unwrap();

            let (Some(from_value), Some(to_value)) = (from_option.value(), to_option.value())
            else {
                return Diff::Replace { from, to };
            };

            // Use sequences::diff to properly handle nested diffs
            let updates = sequences::diff_with_options(vec![from_value], vec![to_value], options);

            if updates.is_empty() {
                return Diff::Equal { value: Some(from) };
            }

            Diff::User {
                from: from.shape(),
                to: to.shape(),
                variant: Some("Some"),
                value: Value::Tuple { updates },
            }
        }
        (
            (Def::List(_) | Def::Slice(_), _) | (_, Type::Sequence(_)),
            (Def::List(_) | Def::Slice(_), _) | (_, Type::Sequence(_)),
        ) => {
            let from_list = from.into_list_like().unwrap();
            let to_list = to.into_list_like().unwrap();

            let updates = sequences::diff_with_options(
                from_list.iter().collect::<Vec<_>>(),
                to_list.iter().collect::<Vec<_>>(),
                options,
            );

            if updates.is_empty() {
                return Diff::Equal { value: Some(from) };
            }

            Diff::Sequence {
                from: from.shape(),
                to: to.shape(),
                updates,
            }
        }
        ((Def::Map(_), _), (Def::Map(_), _)) => {
            let from_map = from.into_map().unwrap();
            let to_map = to.into_map().unwrap();

            let mut updates = HashMap::new();
            let mut deletions = HashMap::new();
            let mut insertions = HashMap::new();
            let mut unchanged = HashSet::new();

            // Collect entries from `from` map with string keys for comparison
            let mut from_entries: HashMap<String, Peek<'mem, 'facet>> = HashMap::new();
            for (key, value) in from_map.iter() {
                let key_str = format!("{:?}", key);
                from_entries.insert(key_str, value);
            }

            // Collect entries from `to` map
            let mut to_entries: HashMap<String, Peek<'mem, 'facet>> = HashMap::new();
            for (key, value) in to_map.iter() {
                let key_str = format!("{:?}", key);
                to_entries.insert(key_str, value);
            }

            // Compare entries
            for (key, from_value) in &from_entries {
                if let Some(to_value) = to_entries.get(key) {
                    let diff = diff_new_peek_with_options(*from_value, *to_value, options);
                    if diff.is_equal() {
                        unchanged.insert(Cow::Owned(key.clone()));
                    } else {
                        updates.insert(Cow::Owned(key.clone()), diff);
                    }
                } else {
                    deletions.insert(Cow::Owned(key.clone()), *from_value);
                }
            }

            for (key, to_value) in &to_entries {
                if !from_entries.contains_key(key) {
                    insertions.insert(Cow::Owned(key.clone()), *to_value);
                }
            }

            let is_empty = updates.is_empty() && deletions.is_empty() && insertions.is_empty();
            if is_empty {
                return Diff::Equal { value: Some(from) };
            }

            Diff::User {
                from: from.shape(),
                to: to.shape(),
                variant: None,
                value: Value::Struct {
                    updates,
                    deletions,
                    insertions,
                    unchanged,
                },
            }
        }
        ((Def::Set(_), _), (Def::Set(_), _)) => {
            let from_set = from.into_set().unwrap();
            let to_set = to.into_set().unwrap();

            // Collect items from both sets using debug format for comparison
            let mut from_items: HashSet<String> = HashSet::new();
            for item in from_set.iter() {
                from_items.insert(format!("{:?}", item));
            }

            let mut to_items: HashSet<String> = HashSet::new();
            for item in to_set.iter() {
                to_items.insert(format!("{:?}", item));
            }

            // Sets are equal if they have the same items
            if from_items == to_items {
                return Diff::Equal { value: Some(from) };
            }

            Diff::Replace { from, to }
        }
        ((Def::DynamicValue(_), _), (Def::DynamicValue(_), _)) => {
            diff_dynamic_values(from, to, options)
        }
        // DynamicValue vs concrete type
        ((Def::DynamicValue(_), _), _) => diff_dynamic_vs_concrete(from, to, false, options),
        (_, (Def::DynamicValue(_), _)) => diff_dynamic_vs_concrete(to, from, true, options),
        _ => Diff::Replace { from, to },
    }
}

/// Computes the difference between two `Peek` values (backward compatibility wrapper)
pub fn diff_new_peek<'mem, 'facet>(
    from: Peek<'mem, 'facet>,
    to: Peek<'mem, 'facet>,
) -> Diff<'mem, 'facet> {
    diff_new_peek_with_options(from, to, &DiffOptions::default())
}

/// Diff two dynamic values (like `facet_value::Value`)
fn diff_dynamic_values<'mem, 'facet>(
    from: Peek<'mem, 'facet>,
    to: Peek<'mem, 'facet>,
    options: &DiffOptions,
) -> Diff<'mem, 'facet> {
    let from_dyn = from.into_dynamic_value().unwrap();
    let to_dyn = to.into_dynamic_value().unwrap();

    let from_kind = from_dyn.kind();
    let to_kind = to_dyn.kind();

    // If kinds differ, just return Replace
    if from_kind != to_kind {
        return Diff::Replace { from, to };
    }

    match from_kind {
        DynValueKind::Null => Diff::Equal { value: Some(from) },
        DynValueKind::Bool => {
            if from_dyn.as_bool() == to_dyn.as_bool() {
                Diff::Equal { value: Some(from) }
            } else {
                Diff::Replace { from, to }
            }
        }
        DynValueKind::Number => {
            // Compare numbers - try exact integer comparison first, then float
            let same = match (from_dyn.as_i64(), to_dyn.as_i64()) {
                (Some(l), Some(r)) => l == r,
                _ => match (from_dyn.as_u64(), to_dyn.as_u64()) {
                    (Some(l), Some(r)) => l == r,
                    _ => match (from_dyn.as_f64(), to_dyn.as_f64()) {
                        (Some(l), Some(r)) => l == r,
                        _ => false,
                    },
                },
            };
            if same {
                Diff::Equal { value: Some(from) }
            } else {
                Diff::Replace { from, to }
            }
        }
        DynValueKind::String => {
            if from_dyn.as_str() == to_dyn.as_str() {
                Diff::Equal { value: Some(from) }
            } else {
                Diff::Replace { from, to }
            }
        }
        DynValueKind::Bytes => {
            if from_dyn.as_bytes() == to_dyn.as_bytes() {
                Diff::Equal { value: Some(from) }
            } else {
                Diff::Replace { from, to }
            }
        }
        DynValueKind::Array => {
            // Use the sequence diff algorithm for arrays
            let from_iter = from_dyn.array_iter();
            let to_iter = to_dyn.array_iter();

            let from_elems: Vec<_> = from_iter.map(|i| i.collect()).unwrap_or_default();
            let to_elems: Vec<_> = to_iter.map(|i| i.collect()).unwrap_or_default();

            let updates = sequences::diff_with_options(from_elems, to_elems, options);

            if updates.is_empty() {
                return Diff::Equal { value: Some(from) };
            }

            Diff::Sequence {
                from: from.shape(),
                to: to.shape(),
                updates,
            }
        }
        DynValueKind::Object => {
            // Treat objects like struct diffs
            let from_len = from_dyn.object_len().unwrap_or(0);
            let to_len = to_dyn.object_len().unwrap_or(0);

            let mut updates = HashMap::new();
            let mut deletions = HashMap::new();
            let mut insertions = HashMap::new();
            let mut unchanged = HashSet::new();

            // Collect keys from `from`
            let mut from_keys: HashMap<String, Peek<'mem, 'facet>> = HashMap::new();
            for i in 0..from_len {
                if let Some((key, value)) = from_dyn.object_get_entry(i) {
                    from_keys.insert(key.to_owned(), value);
                }
            }

            // Collect keys from `to`
            let mut to_keys: HashMap<String, Peek<'mem, 'facet>> = HashMap::new();
            for i in 0..to_len {
                if let Some((key, value)) = to_dyn.object_get_entry(i) {
                    to_keys.insert(key.to_owned(), value);
                }
            }

            // Compare entries
            for (key, from_value) in &from_keys {
                if let Some(to_value) = to_keys.get(key) {
                    let diff = diff_new_peek_with_options(*from_value, *to_value, options);
                    if diff.is_equal() {
                        unchanged.insert(Cow::Owned(key.clone()));
                    } else {
                        updates.insert(Cow::Owned(key.clone()), diff);
                    }
                } else {
                    deletions.insert(Cow::Owned(key.clone()), *from_value);
                }
            }

            for (key, to_value) in &to_keys {
                if !from_keys.contains_key(key) {
                    insertions.insert(Cow::Owned(key.clone()), *to_value);
                }
            }

            let is_empty = updates.is_empty() && deletions.is_empty() && insertions.is_empty();
            if is_empty {
                return Diff::Equal { value: Some(from) };
            }

            Diff::User {
                from: from.shape(),
                to: to.shape(),
                variant: None,
                value: Value::Struct {
                    updates,
                    deletions,
                    insertions,
                    unchanged,
                },
            }
        }
        DynValueKind::DateTime => {
            // Compare datetime by their components
            if from_dyn.as_datetime() == to_dyn.as_datetime() {
                Diff::Equal { value: Some(from) }
            } else {
                Diff::Replace { from, to }
            }
        }
        DynValueKind::QName | DynValueKind::Uuid => {
            // For QName and Uuid, compare by their raw representation
            // Since they have the same kind, we can only compare by Replace semantics
            Diff::Replace { from, to }
        }
    }
}

/// Diff a DynamicValue against a concrete type
/// `dyn_peek` is the DynamicValue, `concrete_peek` is the concrete type
/// `swapped` indicates if the original from/to were swapped (true means dyn_peek is actually "to")
fn diff_dynamic_vs_concrete<'mem, 'facet>(
    dyn_peek: Peek<'mem, 'facet>,
    concrete_peek: Peek<'mem, 'facet>,
    swapped: bool,
    options: &DiffOptions,
) -> Diff<'mem, 'facet> {
    // Determine actual from/to based on swapped flag
    let (from_peek, to_peek) = if swapped {
        (concrete_peek, dyn_peek)
    } else {
        (dyn_peek, concrete_peek)
    };
    let dyn_val = dyn_peek.into_dynamic_value().unwrap();
    let dyn_kind = dyn_val.kind();

    // Try to match based on the DynamicValue's kind
    match dyn_kind {
        DynValueKind::Bool => {
            if concrete_peek
                .get::<bool>()
                .ok()
                .is_some_and(|&v| dyn_val.as_bool() == Some(v))
            {
                return Diff::Equal {
                    value: Some(from_peek),
                };
            }
        }
        DynValueKind::Number => {
            let is_equal =
                // Try signed integers
                concrete_peek.get::<i8>().ok().is_some_and(|&v| dyn_val.as_i64() == Some(v as i64))
                || concrete_peek.get::<i16>().ok().is_some_and(|&v| dyn_val.as_i64() == Some(v as i64))
                || concrete_peek.get::<i32>().ok().is_some_and(|&v| dyn_val.as_i64() == Some(v as i64))
                || concrete_peek.get::<i64>().ok().is_some_and(|&v| dyn_val.as_i64() == Some(v))
                || concrete_peek.get::<isize>().ok().is_some_and(|&v| dyn_val.as_i64() == Some(v as i64))
                // Try unsigned integers
                || concrete_peek.get::<u8>().ok().is_some_and(|&v| dyn_val.as_u64() == Some(v as u64))
                || concrete_peek.get::<u16>().ok().is_some_and(|&v| dyn_val.as_u64() == Some(v as u64))
                || concrete_peek.get::<u32>().ok().is_some_and(|&v| dyn_val.as_u64() == Some(v as u64))
                || concrete_peek.get::<u64>().ok().is_some_and(|&v| dyn_val.as_u64() == Some(v))
                || concrete_peek.get::<usize>().ok().is_some_and(|&v| dyn_val.as_u64() == Some(v as u64))
                // Try floats
                || concrete_peek.get::<f32>().ok().is_some_and(|&v| dyn_val.as_f64() == Some(v as f64))
                || concrete_peek.get::<f64>().ok().is_some_and(|&v| dyn_val.as_f64() == Some(v));
            if is_equal {
                return Diff::Equal {
                    value: Some(from_peek),
                };
            }
        }
        DynValueKind::String => {
            if concrete_peek
                .as_str()
                .is_some_and(|s| dyn_val.as_str() == Some(s))
            {
                return Diff::Equal {
                    value: Some(from_peek),
                };
            }
        }
        DynValueKind::Array => {
            // Try to diff as sequences if the concrete type is list-like
            if let Ok(concrete_list) = concrete_peek.into_list_like() {
                let dyn_elems: Vec<_> = dyn_val
                    .array_iter()
                    .map(|i| i.collect())
                    .unwrap_or_default();
                let concrete_elems: Vec<_> = concrete_list.iter().collect();

                // Use correct order based on swapped flag
                let (from_elems, to_elems) = if swapped {
                    (concrete_elems, dyn_elems)
                } else {
                    (dyn_elems, concrete_elems)
                };
                let updates = sequences::diff_with_options(from_elems, to_elems, options);

                if updates.is_empty() {
                    return Diff::Equal {
                        value: Some(from_peek),
                    };
                }

                return Diff::Sequence {
                    from: from_peek.shape(),
                    to: to_peek.shape(),
                    updates,
                };
            }
        }
        DynValueKind::Object => {
            // Try to diff as struct if the concrete type is a struct
            if let Ok(concrete_struct) = concrete_peek.into_struct() {
                let dyn_len = dyn_val.object_len().unwrap_or(0);

                let mut updates = HashMap::new();
                let mut deletions = HashMap::new();
                let mut insertions = HashMap::new();
                let mut unchanged = HashSet::new();

                // Collect keys from dynamic object
                let mut dyn_keys: HashMap<String, Peek<'mem, 'facet>> = HashMap::new();
                for i in 0..dyn_len {
                    if let Some((key, value)) = dyn_val.object_get_entry(i) {
                        dyn_keys.insert(key.to_owned(), value);
                    }
                }

                // Compare with concrete struct fields
                // When swapped, dyn is "to" and concrete is "from", so we need to swap the diff direction
                for (key, dyn_value) in &dyn_keys {
                    if let Ok(concrete_value) = concrete_struct.field_by_name(key) {
                        let diff = if swapped {
                            diff_new_peek_with_options(concrete_value, *dyn_value, options)
                        } else {
                            diff_new_peek_with_options(*dyn_value, concrete_value, options)
                        };
                        if diff.is_equal() {
                            unchanged.insert(Cow::Owned(key.clone()));
                        } else {
                            updates.insert(Cow::Owned(key.clone()), diff);
                        }
                    } else {
                        // Field in dyn but not in concrete
                        // If swapped: dyn is "to", so this is an insertion
                        // If not swapped: dyn is "from", so this is a deletion
                        if swapped {
                            insertions.insert(Cow::Owned(key.clone()), *dyn_value);
                        } else {
                            deletions.insert(Cow::Owned(key.clone()), *dyn_value);
                        }
                    }
                }

                for (field, concrete_value) in concrete_struct.fields() {
                    if !dyn_keys.contains_key(field.name) {
                        // Field in concrete but not in dyn
                        // If swapped: concrete is "from", so this is a deletion
                        // If not swapped: concrete is "to", so this is an insertion
                        if swapped {
                            deletions.insert(Cow::Borrowed(field.name), concrete_value);
                        } else {
                            insertions.insert(Cow::Borrowed(field.name), concrete_value);
                        }
                    }
                }

                let is_empty = updates.is_empty() && deletions.is_empty() && insertions.is_empty();
                if is_empty {
                    return Diff::Equal {
                        value: Some(from_peek),
                    };
                }

                return Diff::User {
                    from: from_peek.shape(),
                    to: to_peek.shape(),
                    variant: None,
                    value: Value::Struct {
                        updates,
                        deletions,
                        insertions,
                        unchanged,
                    },
                };
            }
        }
        // For other kinds (Null, Bytes, DateTime), fall through to Replace
        _ => {}
    }

    Diff::Replace {
        from: from_peek,
        to: to_peek,
    }
}

/// Extract a float value from a Peek, handling both f32 and f64
fn try_extract_float(peek: Peek) -> Option<f64> {
    match peek.scalar_type()? {
        ScalarType::F64 => Some(*peek.get::<f64>().ok()?),
        ScalarType::F32 => Some(*peek.get::<f32>().ok()? as f64),
        _ => None,
    }
}

/// Check if two Peek values are equal within the specified float tolerance
fn check_float_tolerance(from: Peek, to: Peek, tolerance: f64) -> bool {
    match (try_extract_float(from), try_extract_float(to)) {
        (Some(f1), Some(f2)) => (f1 - f2).abs() <= tolerance,
        _ => false,
    }
}

/// Dereference a pointer/reference to get the underlying value
fn deref_if_pointer<'mem, 'facet>(peek: Peek<'mem, 'facet>) -> Peek<'mem, 'facet> {
    if let Ok(ptr) = peek.into_pointer()
        && let Some(target) = ptr.borrow_inner()
    {
        return deref_if_pointer(target);
    }
    peek
}

/// Collect all leaf-level changes with their paths.
///
/// This walks the diff tree recursively and collects every terminal change
/// (scalar replacements) along with the path to reach them. This is useful
/// for compact display: if there's only one leaf change deep in a tree,
/// you can show `path.to.field: old → new` instead of nested structure.
pub fn collect_leaf_changes<'mem, 'facet>(
    diff: &Diff<'mem, 'facet>,
) -> Vec<LeafChange<'mem, 'facet>> {
    let mut changes = Vec::new();
    collect_leaf_changes_inner(diff, Path::new(), &mut changes);
    changes
}

fn collect_leaf_changes_inner<'mem, 'facet>(
    diff: &Diff<'mem, 'facet>,
    path: Path,
    changes: &mut Vec<LeafChange<'mem, 'facet>>,
) {
    match diff {
        Diff::Equal { .. } => {
            // No change
        }
        Diff::Replace { from, to } => {
            // This is a leaf change
            changes.push(LeafChange {
                path,
                kind: LeafChangeKind::Replace {
                    from: *from,
                    to: *to,
                },
            });
        }
        Diff::User {
            value,
            variant,
            from,
            ..
        } => {
            // For Option::Some, skip the variant in the path since it's implied
            // (the value exists, so it's Some)
            let is_option = matches!(from.def, Def::Option(_));

            let base_path = if let Some(v) = variant {
                if is_option && *v == "Some" {
                    path // Skip "::Some" for options
                } else {
                    path.with(PathSegment::Variant(Cow::Borrowed(*v)))
                }
            } else {
                path
            };

            match value {
                Value::Struct {
                    updates,
                    deletions,
                    insertions,
                    ..
                } => {
                    // Recurse into field updates
                    for (field, diff) in updates {
                        let field_path = base_path.with(PathSegment::Field(field.clone()));
                        collect_leaf_changes_inner(diff, field_path, changes);
                    }
                    // Deletions are leaf changes
                    for (field, peek) in deletions {
                        let field_path = base_path.with(PathSegment::Field(field.clone()));
                        changes.push(LeafChange {
                            path: field_path,
                            kind: LeafChangeKind::Delete { value: *peek },
                        });
                    }
                    // Insertions are leaf changes
                    for (field, peek) in insertions {
                        let field_path = base_path.with(PathSegment::Field(field.clone()));
                        changes.push(LeafChange {
                            path: field_path,
                            kind: LeafChangeKind::Insert { value: *peek },
                        });
                    }
                }
                Value::Tuple { updates } => {
                    // For single-element tuples (like Option::Some), skip the index
                    if is_option {
                        // Recurse directly without adding [0]
                        collect_from_updates_for_single_elem(&base_path, updates, changes);
                    } else {
                        collect_from_updates(&base_path, updates, changes);
                    }
                }
            }
        }
        Diff::Sequence { updates, .. } => {
            collect_from_updates(&path, updates, changes);
        }
    }
}

/// Special handling for single-element tuples (like Option::Some)
/// where we want to skip the `[0]` index in the path.
fn collect_from_updates_for_single_elem<'mem, 'facet>(
    base_path: &Path,
    updates: &Updates<'mem, 'facet>,
    changes: &mut Vec<LeafChange<'mem, 'facet>>,
) {
    // For single-element tuples, we expect exactly one change
    // Just use base_path directly instead of adding [0]
    if let Some(update_group) = &updates.0.first {
        // Process the first replace group if present
        if let Some(replace) = &update_group.0.first
            && replace.removals.len() == 1
            && replace.additions.len() == 1
        {
            let from = replace.removals[0];
            let to = replace.additions[0];
            let nested = diff_new_peek(from, to);
            if matches!(nested, Diff::Replace { .. }) {
                changes.push(LeafChange {
                    path: base_path.clone(),
                    kind: LeafChangeKind::Replace { from, to },
                });
            } else {
                collect_leaf_changes_inner(&nested, base_path.clone(), changes);
            }
            return;
        }
        // Handle nested diffs
        if let Some(diffs) = &update_group.0.last {
            for diff in diffs {
                collect_leaf_changes_inner(diff, base_path.clone(), changes);
            }
            return;
        }
    }
    // Fallback: use regular handling
    collect_from_updates(base_path, updates, changes);
}

fn collect_from_updates<'mem, 'facet>(
    base_path: &Path,
    updates: &Updates<'mem, 'facet>,
    changes: &mut Vec<LeafChange<'mem, 'facet>>,
) {
    // Walk through the interspersed structure to collect changes with correct indices
    let mut index = 0;

    // Process first update group if present
    if let Some(update_group) = &updates.0.first {
        collect_from_update_group(base_path, update_group, &mut index, changes);
    }

    // Process interleaved (unchanged, update) pairs
    for (unchanged, update_group) in &updates.0.values {
        index += unchanged.len();
        collect_from_update_group(base_path, update_group, &mut index, changes);
    }

    // Trailing unchanged items don't add changes
}

fn collect_from_update_group<'mem, 'facet>(
    base_path: &Path,
    group: &crate::UpdatesGroup<'mem, 'facet>,
    index: &mut usize,
    changes: &mut Vec<LeafChange<'mem, 'facet>>,
) {
    // Process first replace group if present
    if let Some(replace) = &group.0.first {
        collect_from_replace_group(base_path, replace, index, changes);
    }

    // Process interleaved (diffs, replace) pairs
    for (diffs, replace) in &group.0.values {
        for diff in diffs {
            let elem_path = base_path.with(PathSegment::Index(*index));
            collect_leaf_changes_inner(diff, elem_path, changes);
            *index += 1;
        }
        collect_from_replace_group(base_path, replace, index, changes);
    }

    // Process trailing diffs
    if let Some(diffs) = &group.0.last {
        for diff in diffs {
            let elem_path = base_path.with(PathSegment::Index(*index));
            collect_leaf_changes_inner(diff, elem_path, changes);
            *index += 1;
        }
    }
}

fn collect_from_replace_group<'mem, 'facet>(
    base_path: &Path,
    group: &crate::ReplaceGroup<'mem, 'facet>,
    index: &mut usize,
    changes: &mut Vec<LeafChange<'mem, 'facet>>,
) {
    // For replace groups, we have removals and additions
    // If counts match, treat as 1:1 replacements at the same index
    // Otherwise, show as deletions followed by insertions

    if group.removals.len() == group.additions.len() {
        // 1:1 replacements
        for (from, to) in group.removals.iter().zip(group.additions.iter()) {
            let elem_path = base_path.with(PathSegment::Index(*index));
            // Check if this is actually a nested diff
            let nested = diff_new_peek(*from, *to);
            if matches!(nested, Diff::Replace { .. }) {
                changes.push(LeafChange {
                    path: elem_path,
                    kind: LeafChangeKind::Replace {
                        from: *from,
                        to: *to,
                    },
                });
            } else {
                collect_leaf_changes_inner(&nested, elem_path, changes);
            }
            *index += 1;
        }
    } else {
        // Mixed deletions and insertions
        for from in &group.removals {
            let elem_path = base_path.with(PathSegment::Index(*index));
            changes.push(LeafChange {
                path: elem_path.clone(),
                kind: LeafChangeKind::Delete { value: *from },
            });
            *index += 1;
        }
        // Insertions happen at current index
        for to in &group.additions {
            let elem_path = base_path.with(PathSegment::Index(*index));
            changes.push(LeafChange {
                path: elem_path,
                kind: LeafChangeKind::Insert { value: *to },
            });
            *index += 1;
        }
    }
}

/// A single leaf-level change in a diff, with path information.
#[derive(Debug, Clone)]
pub struct LeafChange<'mem, 'facet> {
    /// The path from root to this change
    pub path: Path,
    /// The kind of change
    pub kind: LeafChangeKind<'mem, 'facet>,
}

/// The kind of leaf change.
#[derive(Debug, Clone)]
pub enum LeafChangeKind<'mem, 'facet> {
    /// A value was replaced
    Replace {
        /// The old value
        from: Peek<'mem, 'facet>,
        /// The new value
        to: Peek<'mem, 'facet>,
    },
    /// A value was deleted
    Delete {
        /// The deleted value
        value: Peek<'mem, 'facet>,
    },
    /// A value was inserted
    Insert {
        /// The inserted value
        value: Peek<'mem, 'facet>,
    },
}

impl<'mem, 'facet> LeafChange<'mem, 'facet> {
    /// Format this change without colors.
    pub fn format_plain(&self) -> String {
        use facet_pretty::PrettyPrinter;

        let printer = PrettyPrinter::default()
            .with_colors(facet_pretty::ColorMode::Never)
            .with_minimal_option_names(true);

        let mut out = String::new();

        // Show path if non-empty
        if !self.path.0.is_empty() {
            out.push_str(&format!("{}: ", self.path));
        }

        match &self.kind {
            LeafChangeKind::Replace { from, to } => {
                out.push_str(&format!(
                    "{} → {}",
                    printer.format_peek(*from),
                    printer.format_peek(*to)
                ));
            }
            LeafChangeKind::Delete { value } => {
                out.push_str(&format!("- {}", printer.format_peek(*value)));
            }
            LeafChangeKind::Insert { value } => {
                out.push_str(&format!("+ {}", printer.format_peek(*value)));
            }
        }

        out
    }

    /// Format this change with colors.
    pub fn format_colored(&self) -> String {
        use facet_pretty::{PrettyPrinter, tokyo_night};
        use owo_colors::OwoColorize;

        let printer = PrettyPrinter::default()
            .with_colors(facet_pretty::ColorMode::Never)
            .with_minimal_option_names(true);

        let mut out = String::new();

        // Show path if non-empty (in field name color)
        if !self.path.0.is_empty() {
            out.push_str(&format!(
                "{}: ",
                format!("{}", self.path).color(tokyo_night::FIELD_NAME)
            ));
        }

        match &self.kind {
            LeafChangeKind::Replace { from, to } => {
                out.push_str(&format!(
                    "{} {} {}",
                    printer.format_peek(*from).color(tokyo_night::DELETION),
                    "→".color(tokyo_night::COMMENT),
                    printer.format_peek(*to).color(tokyo_night::INSERTION)
                ));
            }
            LeafChangeKind::Delete { value } => {
                out.push_str(&format!(
                    "{} {}",
                    "-".color(tokyo_night::DELETION),
                    printer.format_peek(*value).color(tokyo_night::DELETION)
                ));
            }
            LeafChangeKind::Insert { value } => {
                out.push_str(&format!(
                    "{} {}",
                    "+".color(tokyo_night::INSERTION),
                    printer.format_peek(*value).color(tokyo_night::INSERTION)
                ));
            }
        }

        out
    }
}

impl<'mem, 'facet> std::fmt::Display for LeafChange<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_plain())
    }
}

/// Configuration for diff formatting.
#[derive(Debug, Clone)]
pub struct DiffFormat {
    /// Use colors in output
    pub colors: bool,
    /// Maximum number of changes before switching to summary mode
    pub max_inline_changes: usize,
    /// Whether to use compact (path-based) format for few changes
    pub prefer_compact: bool,
}

impl Default for DiffFormat {
    fn default() -> Self {
        Self {
            colors: true,
            max_inline_changes: 10,
            prefer_compact: true,
        }
    }
}

/// Format the diff with the given configuration.
///
/// This chooses between compact (path-based) and tree (nested) format
/// based on the number of changes and the configuration.
pub fn format_diff(diff: &Diff<'_, '_>, config: &DiffFormat) -> String {
    if matches!(diff, Diff::Equal { .. }) {
        return if config.colors {
            use facet_pretty::tokyo_night;
            use owo_colors::OwoColorize;
            "(no changes)".color(tokyo_night::MUTED).to_string()
        } else {
            "(no changes)".to_string()
        };
    }

    let changes = collect_leaf_changes(diff);

    if changes.is_empty() {
        return if config.colors {
            use facet_pretty::tokyo_night;
            use owo_colors::OwoColorize;
            "(no changes)".color(tokyo_night::MUTED).to_string()
        } else {
            "(no changes)".to_string()
        };
    }

    // Use compact format if preferred and we have few changes
    if config.prefer_compact && changes.len() <= config.max_inline_changes {
        let mut out = String::new();
        for (i, change) in changes.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            if config.colors {
                out.push_str(&change.format_colored());
            } else {
                out.push_str(&change.format_plain());
            }
        }
        return out;
    }

    // Fall back to tree format for many changes
    if changes.len() > config.max_inline_changes {
        let mut out = String::new();

        // Show first few changes
        for (i, change) in changes.iter().take(config.max_inline_changes).enumerate() {
            if i > 0 {
                out.push('\n');
            }
            if config.colors {
                out.push_str(&change.format_colored());
            } else {
                out.push_str(&change.format_plain());
            }
        }

        // Show summary of remaining
        let remaining = changes.len() - config.max_inline_changes;
        if remaining > 0 {
            out.push('\n');
            let summary = format!(
                "... and {} more change{}",
                remaining,
                if remaining == 1 { "" } else { "s" }
            );
            if config.colors {
                use facet_pretty::tokyo_night;
                use owo_colors::OwoColorize;
                out.push_str(&summary.color(tokyo_night::MUTED).to_string());
            } else {
                out.push_str(&summary);
            }
        }
        return out;
    }

    // Default: use Display impl (tree format)
    format!("{diff}")
}

/// Format the diff with default configuration.
pub fn format_diff_default(diff: &Diff<'_, '_>) -> String {
    format_diff(diff, &DiffFormat::default())
}

/// Format the diff in compact mode (path-based, no tree structure).
pub fn format_diff_compact(diff: &Diff<'_, '_>) -> String {
    format_diff(
        diff,
        &DiffFormat {
            prefer_compact: true,
            max_inline_changes: usize::MAX,
            ..Default::default()
        },
    )
}

/// Format the diff in compact mode without colors.
pub fn format_diff_compact_plain(diff: &Diff<'_, '_>) -> String {
    format_diff(
        diff,
        &DiffFormat {
            colors: false,
            prefer_compact: true,
            max_inline_changes: usize::MAX,
        },
    )
}
