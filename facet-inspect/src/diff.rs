use std::collections::HashMap;

use facet::{Def, Facet};
use facet_reflect::Peek;

use crate::{FacetInspect, FacetPath};

#[derive(Debug)]
pub struct ModificationPeek<'mem, 'facet, 'shape> {
    pub old_value: Peek<'mem, 'facet, 'shape>,
    pub new_value: Peek<'mem, 'facet, 'shape>,
}

#[derive(Debug, Facet, PartialEq)]
#[repr(C)]
pub enum Modification {
    U32 { before: u32, after: u32 },
    String { before: String, after: String },
}

impl<'a> From<ModificationPeek<'a, 'a, 'a>> for Modification {
    fn from(peek: ModificationPeek<'a, 'a, 'a>) -> Self {
        let old_value = peek.old_value;
        let new_value = peek.new_value;

        match old_value.shape().type_identifier {
            "u32" => {
                let before = *old_value.get::<u32>().expect("old_value should be u32");
                let after = *new_value.get::<u32>().expect("new_value should be u32");
                Modification::U32 { before, after }
            }
            "String" => {
                let before = old_value
                    .get::<String>()
                    .expect("old_value should be String")
                    .clone();
                let after = new_value
                    .get::<String>()
                    .expect("new_value should be String")
                    .clone();
                Modification::String { before, after }
            }
            _ => {
                // Handle other types as needed
                // For now, we will just panic if the type is not recognized
                panic!(
                    "Unsupported type for modification: {}",
                    old_value.shape().type_identifier
                );
            }
        }
    }
}

#[derive(Debug, Facet)]
pub struct Diff {
    pub changes: HashMap<FacetPath, Modification>,
}

impl Diff {
    pub fn compare<'a, T: Facet<'a>>(facet1: &'a T, facet2: &'a T) -> Self {
        let mut changes = HashMap::new();

        for ((path, peek1), (_, peek2)) in facet1.inspect().zip(facet2.inspect()) {
            if peek1 == peek2 {
                continue; // No change
            }

            if peek1.shape() == peek2.shape() && !matches!(peek1.shape().def, Def::Scalar) {
                // There is change, deeper in the shape
                // TODO: find a way to do the same without using Def::Scalar
                continue;
            }

            let modif = ModificationPeek {
                old_value: peek1,
                new_value: peek2,
            };

            changes.insert(path, Modification::from(modif));
        }

        Diff { changes }
    }
}

#[derive(Debug)]
pub struct ShapeDiff<'a> {
    pub changes: HashMap<FacetPath, DiffType<'a>>,
}

#[derive(Debug)]
pub enum DiffType<'a> {
    Added(Peek<'a, 'a, 'a>),
    Removed(Peek<'a, 'a, 'a>),
    Modified(ModificationPeek<'a, 'a, 'a>),
}

impl<'a> ShapeDiff<'a> {
    pub fn compare<A: Facet<'a>, B: Facet<'a>>(facet1: &'a A, facet2: &'a B) -> Self {
        let facet1_elements = facet1.inspect().collect::<HashMap<_, _>>();
        let facet2_elements = facet2.inspect().collect::<HashMap<_, _>>();

        let mut changes = HashMap::new();

        for path in facet1_elements.keys().chain(facet2_elements.keys()) {
            let peek1 = facet1_elements.get(path);
            let peek2 = facet2_elements.get(path);

            match (peek1, peek2) {
                (Some(p1), Some(p2)) if p1 == p2 => continue, // No change
                (Some(p1), Some(p2))
                    if p1.shape() == p2.shape() && !matches!(p1.shape().def, Def::Scalar) =>
                {
                    // There is change, deeper in the shape
                    // TODO: find a way to do the same without using Def::Scalar
                    continue;
                }
                (Some(p1), None) => {
                    changes.insert(path.clone(), DiffType::Removed(*p1));
                }
                (None, Some(p2)) => {
                    changes.insert(path.clone(), DiffType::Added(*p2));
                }
                (Some(p1), Some(p2)) => {
                    changes.insert(
                        path.clone(),
                        DiffType::Modified(ModificationPeek {
                            old_value: *p1,
                            new_value: *p2,
                        }),
                    );
                }
                (None, None) => unreachable!(), // This case should not happen
            }
        }

        ShapeDiff { changes }
    }
}

pub trait FacetDiff<'facet>: Facet<'facet> + Sized {
    fn diff(&'facet self, other: &'facet Self) -> Diff {
        Diff::compare(self, other)
    }

    fn shape_diff<T: Facet<'facet>>(&'facet self, other: &'facet T) -> ShapeDiff<'facet> {
        ShapeDiff::compare(self, other)
    }
}

impl<'facet, T: Facet<'facet>> FacetDiff<'facet> for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[derive(Facet, Clone)]
    struct TestFacet {
        field1: u32,
        field2: String,
    }

    #[derive(Facet)]
    struct NestedFacet {
        nested_field: TestFacet,
    }

    #[derive(Facet)]
    struct AnotherNestedFacet {
        nested_field: TestFacet,
        another_field: u32,
    }

    #[test]
    fn test_facet_diff() {
        let sub_facet1 = TestFacet {
            field1: 42,
            field2: "Hello".to_string(),
        };

        let facet1 = NestedFacet {
            nested_field: sub_facet1.clone(),
        };

        let facet2 = NestedFacet {
            nested_field: TestFacet {
                field1: 43,
                field2: "World".to_string(),
            },
        };

        let diff = facet1.diff(&facet2);

        assert_eq!(
            diff.changes.get(&"$.nested_field.field1".into()).unwrap(),
            &Modification::U32 {
                before: 42,
                after: 43
            }
        );
    }

    #[test]
    fn test_shape_diff() {
        let sub_facet1 = TestFacet {
            field1: 42,
            field2: "Hello".to_string(),
        };

        let facet1 = NestedFacet {
            nested_field: sub_facet1.clone(),
        };

        let facet2 = AnotherNestedFacet {
            nested_field: TestFacet {
                field1: 43,
                field2: "World".to_string(),
            },
            another_field: 100,
        };

        let shape_diff = facet1.shape_diff(&facet2);
        dbg!(shape_diff);
    }
}
