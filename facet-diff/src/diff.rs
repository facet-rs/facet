use std::collections::{HashMap, HashSet};

use facet::{Def, Shape, StructKind, Type, UserType};
use facet_core::Facet;
use facet_reflect::{HasFields, Peek};

use crate::sequences::{self, Updates};

/// The difference between two values.
///
/// The `from` value does not necessarily have to have the same type as the `to` value.
pub enum Diff<'mem, 'facet> {
    /// The two values are equal
    Equal,

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

        /// cf. [Value]
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
        updates: HashMap<&'static str, Diff<'mem, 'facet>>,

        /// The fields that are in `from` but not in `to`.
        deletions: HashMap<&'static str, Peek<'mem, 'facet>>,

        /// The fields that are in `to` but not in `from`.
        insertions: HashMap<&'static str, Peek<'mem, 'facet>>,

        /// The fields that are unchanged
        unchanged: HashSet<&'static str>,
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

/// Extension trait that provides a [`diff`] method for `Facet` types
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
        matches!(self, Self::Equal)
    }

    /// Computes the difference between two values that implement `Facet`
    pub fn new<T: Facet<'facet>, U: Facet<'facet>>(from: &'mem T, to: &'mem U) -> Self {
        Self::new_peek(Peek::new(from), Peek::new(to))
    }

    pub(crate) fn new_peek(from: Peek<'mem, 'facet>, to: Peek<'mem, 'facet>) -> Self {
        if from.shape().id == to.shape().id && from.shape().is_partial_eq() && from == to {
            return Diff::Equal;
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
                                    unchanged.insert(field.name);
                                } else {
                                    updates.insert(field.name, diff);
                                }
                            } else {
                                deletions.insert(field.name, from);
                            }
                        }

                        for (field, to) in to_ty.fields() {
                            if from_ty.field_by_name(field.name).is_err() {
                                insertions.insert(field.name, to);
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
                                unchanged.insert(field.name);
                            } else {
                                updates.insert(field.name, diff);
                            }
                        } else {
                            deletions.insert(field.name, from);
                        }
                    }

                    for (field, to) in to_enum.fields() {
                        if !from_enum
                            .field_by_name(field.name)
                            .is_ok_and(|x| x.is_some())
                        {
                            insertions.insert(field.name, to);
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

                let mut updates = Updates::default();

                let diff = Self::new_peek(from_value, to_value);
                if !diff.is_equal() {
                    updates.push_add(to_value);
                    updates.push_remove(from_value);
                }

                Diff::User {
                    from: from.shape(),
                    to: to.shape(),
                    variant: Some("Some"),
                    value: Value::Tuple { updates },
                }
            }
            (
                (Def::List(_), _) | (_, Type::Sequence(_)),
                (Def::List(_), _) | (_, Type::Sequence(_)),
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
            _ => Diff::Replace { from, to },
        }
    }

    pub(crate) fn closeness(&self) -> usize {
        match self {
            Self::Equal => 1, // This does not actually matter for flattening sequence diffs, because all diffs there are non-equal
            Self::Replace { .. } => 0,
            Self::Sequence { updates, .. } => updates.closeness(),
            Self::User {
                from, to, value, ..
            } => value.closeness() + (from == to) as usize,
        }
    }
}
