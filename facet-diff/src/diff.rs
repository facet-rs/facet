use std::collections::HashMap;

use facet::{Def, Shape, Type, UserType};
use facet_core::Facet;
use facet_reflect::{HasFields, Peek};

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
        from: Peek<'mem, 'facet, 'static>,
        to: Peek<'mem, 'facet, 'static>,
    },

    /// The two values are both structures or both enums with similar variants.
    User {
        /// The shape of the `from` struct.
        from: &'static Shape<'static>,

        /// The name of the variant, this is [`None`] if the values are structs
        variant: Option<&'static str>,

        /// The shape of the `to` struct.
        to: &'static Shape<'static>,

        /// The fields that are updated between the structs
        updates: HashMap<&'static str, Diff<'mem, 'facet>>,

        /// THe fields that are in `from` but not in `to`.
        deletions: HashMap<&'static str, Peek<'mem, 'facet, 'static>>,

        /// THe fields that are in `to` but not in `from`.
        insertions: HashMap<&'static str, Peek<'mem, 'facet, 'static>>,
    },
}

pub trait FacetDiff<'f>: Facet<'f> {
    fn diff<'a, U: Facet<'f>>(&'a self, other: &'a U) -> Diff<'a, 'f>;
}

impl<'f, T: Facet<'f>> FacetDiff<'f> for T {
    fn diff<'a, U: Facet<'f>>(&'a self, other: &'a U) -> Diff<'a, 'f> {
        Diff::new(Peek::new(self), Peek::new(other))
    }
}

impl<'mem, 'facet> Diff<'mem, 'facet> {
    pub fn is_equal(&self) -> bool {
        matches!(self, Self::Equal)
    }

    fn new(from: Peek<'mem, 'facet, 'static>, to: Peek<'mem, 'facet, 'static>) -> Self {
        if from.shape().id == to.shape().id && from.shape().is_partial_eq() && from == to {
            return Diff::Equal;
        }

        match (
            from.shape().def,
            from.shape().ty,
            to.shape().def,
            to.shape().ty,
        ) {
            (_, Type::User(UserType::Struct(from_ty)), _, Type::User(UserType::Struct(to_ty)))
                if from_ty.kind == to_ty.kind =>
            {
                let from_ty = from.into_struct().unwrap();
                let to_ty = to.into_struct().unwrap();

                let mut updates = HashMap::new();
                let mut deletions = HashMap::new();
                let mut insertions = HashMap::new();

                for (field, from) in from_ty.fields() {
                    if let Ok(to) = to_ty.field_by_name(field.name) {
                        let diff = Diff::new(from, to);
                        if !diff.is_equal() {
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

                Diff::User {
                    from: from.shape(),
                    to: to.shape(),
                    variant: None,
                    updates,
                    deletions,
                    insertions,
                }
            }
            (_, Type::User(UserType::Enum(_)), _, Type::User(UserType::Enum(_))) => {
                let from_enum = from.into_enum().unwrap();
                let to_enum = to.into_enum().unwrap();

                let from_variant = from_enum.active_variant().unwrap();
                let to_variant = to_enum.active_variant().unwrap();

                if from_variant.name != to_variant.name
                    || from_variant.data.kind != to_variant.data.kind
                {
                    return Diff::Replace { from, to };
                }

                let mut updates = HashMap::new();
                let mut deletions = HashMap::new();
                let mut insertions = HashMap::new();

                for (field, from) in from_enum.fields() {
                    if let Ok(Some(to)) = to_enum.field_by_name(field.name) {
                        let diff = Diff::new(from, to);
                        if !diff.is_equal() {
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

                Diff::User {
                    from: from_enum.shape(),
                    to: to_enum.shape(),
                    variant: Some(from_variant.name),
                    updates,
                    deletions,
                    insertions,
                }
            }
            (Def::Option(_), _, Def::Option(_), _) => {
                let from_option = from.into_option().unwrap();
                let to_option = to.into_option().unwrap();

                let (Some(from_value), Some(to_value)) = (from_option.value(), to_option.value())
                else {
                    return Diff::Replace { from, to };
                };

                let mut updates = HashMap::default();

                let diff = Self::new(from_value, to_value);
                if !diff.is_equal() {
                    updates.insert("0", diff);
                }

                Diff::User {
                    from: from.shape(),
                    to: to.shape(),
                    variant: Some("Some"),
                    updates,
                    deletions: Default::default(),
                    insertions: Default::default(),
                }
            }
            _ => Diff::Replace { from, to },
        }
    }
}
