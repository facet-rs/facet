// TODO: Consider using an approach similar to `morph` (bearcove's fork of difftastic)
// to compute and display the optimal diff path for complex structural changes.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use facet::{Def, DynValueKind, Shape, StructKind, Type, UserType};
use facet_core::Facet;
use facet_reflect::{HasFields, Peek};

use crate::sequences::{self, Updates};

/// The difference between two values.
///
/// The `from` value does not necessarily have to have the same type as the `to` value.
pub enum Diff<'mem, 'facet> {
    /// The two values are equal
    Equal {
        /// The value (stored for display purposes)
        value: Option<Peek<'mem, 'facet>>,
    },

    /// Fallback case.
    ///
    /// We do not know much about the values, apart from that they are unequal to each other.
    Replace {
        /// The `from` value.
        from: Peek<'mem, 'facet>,

        /// The `to` value.
        to: Peek<'mem, 'facet>,
    },

    /// The two values are both structures or both enums with similar variants.
    User {
        /// The shape of the `from` struct.
        from: &'static Shape,

        /// The shape of the `to` struct.
        to: &'static Shape,

        /// The name of the variant, this is [`None`] if the values are structs
        variant: Option<&'static str>,

        /// The value of the struct/enum variant (tuple or struct fields)
        value: Value<'mem, 'facet>,
    },

    /// A diff between two sequences
    Sequence {
        /// The shape of the `from` sequence.
        from: &'static Shape,

        /// The shape of the `to` sequence.
        to: &'static Shape,

        /// The updates on the sequence
        updates: Updates<'mem, 'facet>,
    },
}

/// A set of updates, additions, deletions, insertions etc. for a tuple or a struct
pub enum Value<'mem, 'facet> {
    Tuple {
        /// The updates on the sequence
        updates: Updates<'mem, 'facet>,
    },

    Struct {
        /// The fields that are updated between the structs
        updates: HashMap<Cow<'static, str>, Diff<'mem, 'facet>>,

        /// The fields that are in `from` but not in `to`.
        deletions: HashMap<Cow<'static, str>, Peek<'mem, 'facet>>,

        /// The fields that are in `to` but not in `from`.
        insertions: HashMap<Cow<'static, str>, Peek<'mem, 'facet>>,

        /// The fields that are unchanged
        unchanged: HashSet<Cow<'static, str>>,
    },
}

impl<'mem, 'facet> Value<'mem, 'facet> {
    fn closeness(&self) -> usize {
        match self {
            Self::Tuple { updates } => updates.closeness(),
            Self::Struct { unchanged, .. } => unchanged.len(),
        }
    }
}

/// Extension trait that provides a [`diff`](FacetDiff::diff) method for `Facet` types
pub trait FacetDiff<'f>: Facet<'f> {
    /// Computes the difference between two values that implement `Facet`
    fn diff<'a, U: Facet<'f>>(&'a self, other: &'a U) -> Diff<'a, 'f>;
}

impl<'f, T: Facet<'f>> FacetDiff<'f> for T {
    fn diff<'a, U: Facet<'f>>(&'a self, other: &'a U) -> Diff<'a, 'f> {
        Diff::new(self, other)
    }
}

impl<'mem, 'facet> Diff<'mem, 'facet> {
    /// Returns true if the two values were equal
    pub fn is_equal(&self) -> bool {
        matches!(self, Self::Equal { .. })
    }

    /// Computes the difference between two values that implement `Facet`
    pub fn new<T: Facet<'facet>, U: Facet<'facet>>(from: &'mem T, to: &'mem U) -> Self {
        Self::new_peek(Peek::new(from), Peek::new(to))
    }

    pub(crate) fn new_peek(from: Peek<'mem, 'facet>, to: Peek<'mem, 'facet>) -> Self {
        // Dereference pointers/references to compare the underlying values
        let from = Self::deref_if_pointer(from);
        let to = Self::deref_if_pointer(to);

        if from.shape().id == to.shape().id && from.shape().is_partial_eq() && from == to {
            return Diff::Equal { value: Some(from) };
        }

        match (
            (from.shape().def, from.shape().ty),
            (to.shape().def, to.shape().ty),
        ) {
            (
                (_, Type::User(UserType::Struct(from_ty))),
                (_, Type::User(UserType::Struct(to_ty))),
            ) if from_ty.kind == to_ty.kind => {
                let from_ty = from.into_struct().unwrap();
                let to_ty = to.into_struct().unwrap();

                let value =
                    if [StructKind::Tuple, StructKind::TupleStruct].contains(&from_ty.ty().kind) {
                        let from = from_ty.fields().map(|x| x.1).collect();
                        let to = to_ty.fields().map(|x| x.1).collect();

                        let updates = sequences::diff(from, to);

                        Value::Tuple { updates }
                    } else {
                        let mut updates = HashMap::new();
                        let mut deletions = HashMap::new();
                        let mut insertions = HashMap::new();
                        let mut unchanged = HashSet::new();

                        for (field, from) in from_ty.fields() {
                            if let Ok(to) = to_ty.field_by_name(field.name) {
                                let diff = Diff::new_peek(from, to);
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

                let value = if [StructKind::Tuple, StructKind::TupleStruct]
                    .contains(&from_variant.data.kind)
                {
                    let from = from_enum.fields().map(|x| x.1).collect();
                    let to = to_enum.fields().map(|x| x.1).collect();

                    let updates = sequences::diff(from, to);

                    Value::Tuple { updates }
                } else {
                    let mut updates = HashMap::new();
                    let mut deletions = HashMap::new();
                    let mut insertions = HashMap::new();
                    let mut unchanged = HashSet::new();

                    for (field, from) in from_enum.fields() {
                        if let Ok(Some(to)) = to_enum.field_by_name(field.name) {
                            let diff = Diff::new_peek(from, to);
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
                let updates = sequences::diff(vec![from_value], vec![to_value]);

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

                let updates = sequences::diff(
                    from_list.iter().collect::<Vec<_>>(),
                    to_list.iter().collect::<Vec<_>>(),
                );

                Diff::Sequence {
                    from: from.shape(),
                    to: to.shape(),
                    updates,
                }
            }
            ((Def::DynamicValue(_), _), (Def::DynamicValue(_), _)) => {
                Self::diff_dynamic_values(from, to)
            }
            // DynamicValue vs concrete type
            ((Def::DynamicValue(_), _), _) => Self::diff_dynamic_vs_concrete(from, to, false),
            (_, (Def::DynamicValue(_), _)) => Self::diff_dynamic_vs_concrete(to, from, true),
            _ => Diff::Replace { from, to },
        }
    }

    /// Diff two dynamic values (like `facet_value::Value`)
    fn diff_dynamic_values(from: Peek<'mem, 'facet>, to: Peek<'mem, 'facet>) -> Self {
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

                let updates = sequences::diff(from_elems, to_elems);

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
                        let diff = Self::new_peek(*from_value, *to_value);
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
    fn diff_dynamic_vs_concrete(
        dyn_peek: Peek<'mem, 'facet>,
        concrete_peek: Peek<'mem, 'facet>,
        swapped: bool,
    ) -> Self {
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
                    let updates = sequences::diff(from_elems, to_elems);

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
                                Self::new_peek(concrete_value, *dyn_value)
                            } else {
                                Self::new_peek(*dyn_value, concrete_value)
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

    /// Dereference a pointer/reference to get the underlying value
    fn deref_if_pointer(peek: Peek<'mem, 'facet>) -> Peek<'mem, 'facet> {
        if let Ok(ptr) = peek.into_pointer()
            && let Some(target) = ptr.borrow_inner()
        {
            return Self::deref_if_pointer(target);
        }
        peek
    }

    pub(crate) fn closeness(&self) -> usize {
        match self {
            Self::Equal { .. } => 1, // This does not actually matter for flattening sequence diffs, because all diffs there are non-equal
            Self::Replace { .. } => 0,
            Self::Sequence { updates, .. } => updates.closeness(),
            Self::User {
                from, to, value, ..
            } => value.closeness() + (from == to) as usize,
        }
    }
}
