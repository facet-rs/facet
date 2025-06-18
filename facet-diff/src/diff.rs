use std::collections::HashMap;

use facet::{Shape, Type, UserType};
use facet_core::Facet;
use facet_reflect::{HasFields, Peek};

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

    Struct {
        from: &'static Shape<'static>,
        to: &'static Shape<'static>,
        updates: HashMap<&'static str, Diff<'mem, 'facet>>,
        deletions: HashMap<&'static str, Peek<'mem, 'facet, 'static>>,
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

        match (from.shape().ty, to.shape().ty) {
            (Type::User(UserType::Struct(from_ty)), Type::User(UserType::Struct(to_ty)))
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

                Diff::Struct {
                    from: from.shape(),
                    to: to.shape(),
                    updates,
                    deletions,
                    insertions,
                }
            }
            _ => Diff::Replace { from, to },
        }
    }
}
