use facet::{Def, Facet, PointerType, Type, UserType};
use facet_reflect::{HasFields, Peek};

// TODO: Discuss if we should use a indexes as path to reduce the size of the path, this would make it more efficient to
// send through the network, but less readable.
/// A structure representing the path to a sub-object in a [`Facet`] object.
#[derive(Facet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FacetPath {
    pub segments: Vec<String>,
}

impl FacetPath {
    pub fn root() -> Self {
        FacetPath {
            segments: vec!["$".to_string()],
        }
    }

    pub fn push(&mut self, path: &FacetPath) {
        self.segments.extend(path.segments.iter().cloned());
    }

    pub fn join(&self, path: &FacetPath) -> Self {
        let mut new_path = self.clone();
        new_path.push(path);
        new_path
    }
}

impl From<&str> for FacetPath {
    fn from(path: &str) -> Self {
        FacetPath {
            segments: path.split('.').map(|s| s.to_string()).collect(),
        }
    }
}

/// A structure allowing to iterate over the shape of a [`Facet`] object.
/// This iterator will yield the [`Peek`]s of the [`Facet`] sub-objects composing the inspected object.
/// Each [`Peek`] will be associated to the path leading to it, allowing to reconstruct the structure of the object.
#[derive(Clone)]
pub struct FacetIterator<'mem, 'facet, 'shape> {
    stack: Vec<(FacetPath, Peek<'mem, 'facet, 'shape>)>,
}

impl<'mem, 'facet, 'shape> Iterator for FacetIterator<'mem, 'facet, 'shape> {
    type Item = (FacetPath, Peek<'mem, 'facet, 'shape>);

    fn next(&mut self) -> Option<Self::Item> {
        let Some((path, peek)) = self.stack.pop() else {
            return None; // If the stack is empty, we are done
        };

        let def = peek.shape().def;
        let ty = peek.shape().ty;

        match (def, ty) {
            (Def::Scalar, _) => {} // Scalars do not have sub-objects, so we skip them
            (Def::Array(_), _) | (Def::List(_), _) | (Def::Slice(_), _) => {
                self.push_list_items_to_stack(
                    &path,
                    peek.into_list_like().expect("Expected a list").iter(),
                );
            }
            (Def::Map(_), _) => {
                self.push_map_items_to_stack(
                    &path,
                    peek.into_map().expect("Expected a map").iter(),
                );
            }
            (_, Type::User(UserType::Struct(_))) => {
                self.push_fields_to_stack(&path, peek.into_struct().expect("Expected a struct"));
            }
            (_, Type::User(UserType::Enum(_))) => {
                self.push_fields_to_stack(&path, peek.into_enum().expect("Expected an enum"));
            }
            (_, Type::Sequence(_)) => {
                // Sequences are treated like lists, so we push their items to the stack
                self.push_list_items_to_stack(
                    &path,
                    peek.into_list_like().expect("Expected a sequence").iter(),
                );
            }
            (_, Type::Pointer(PointerType::Reference(r))) => {
                let target = (r.target)();
                if let Type::Sequence(_) = target.ty {
                    self.push_list_items_to_stack(
                        &path,
                        peek.into_list_like().expect("Expected a sequence").iter(),
                    );
                }
            }
            (_, _) => {
                // TODO: discuss behavior here as I don't think runtime crash is the best option
                todo!(
                    "this type is not yet supported for inspection\ndef:{:?}\nty:{:?}",
                    def,
                    ty
                );
            }
        }

        Some((path, peek))
    }
}

impl<'mem, 'facet, 'shape> FacetIterator<'mem, 'facet, 'shape> {
    fn push_fields_to_stack(
        &mut self,
        parent_path: &FacetPath,
        object: impl HasFields<'mem, 'facet, 'shape>,
    ) {
        // TODO: discuss if the performance trade-off of having the fields in reverse order is worth it
        // We reverse the fields to maintain the order of fields as they are defined in the struct
        for (field, peek) in object.fields().rev() {
            let new_path = parent_path.join(&field.name.into());
            self.stack.push((new_path, peek));
        }
    }

    fn push_list_items_to_stack(
        &mut self,
        parent_path: &FacetPath,
        list: impl Iterator<Item = Peek<'mem, 'facet, 'shape>>,
    ) {
        for (index, item) in list.enumerate() {
            let new_path = parent_path.join(&FacetPath::from(index.to_string().as_str()));
            self.stack.push((new_path, item));
        }
    }

    fn push_map_items_to_stack(
        &mut self,
        parent_path: &FacetPath,
        map: impl Iterator<Item = (Peek<'mem, 'facet, 'shape>, Peek<'mem, 'facet, 'shape>)>,
    ) {
        for (key, value_peek) in map {
            let new_path = parent_path.join(&FacetPath::from(format!("{}", key).as_str()));
            self.stack.push((new_path, value_peek));
        }
    }
}

pub trait FacetInspect<'a>: Facet<'a> {
    /// Returns an iterator over the shape of the [`Facet`] object.
    ///
    /// The iterator will yield tuples containing the path to the sub-object and its corresponding [`Peek`].
    fn inspect(&'a self) -> FacetIterator<'a, 'a, 'a> {
        FacetIterator {
            stack: vec![(FacetPath::root(), Peek::new(self))], // Start with the root path and a Peek of self
        }
    }

    /// Returns a [`Peek`] for the sub-object at the specified path.
    ///
    /// If the path does not lead to a valid sub-object, `None` is returned.
    fn get(&'a self, path: &FacetPath) -> Option<Peek<'a, 'a, 'a>> {
        self.inspect()
            .find(|(p, _)| p == path)
            .map(|(_, peek)| peek)
    }
}

impl<'a, T: Facet<'a>> FacetInspect<'a> for T {}

#[cfg(test)]
mod tests {
    use super::FacetInspect;
    use super::*;
    use std::collections::HashMap;

    #[derive(Facet)]
    struct TestFacet {
        field1: u32,
        field2: String,
    }

    #[derive(Facet)]
    struct NestedFacet {
        nested_field: TestFacet,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum MyEnum {
        Unit,
        Tuple(u32, String),
        Struct { x: u32, y: String },
    }

    #[test]
    fn test_facet_iterator_struct() {
        let facet = NestedFacet {
            nested_field: TestFacet {
                field1: 42,
                field2: "Hello".to_string(),
            },
        };

        let mut iter = facet.inspect();

        let iterator_contains_field1 = iter.any(|(path, peek)| {
            path == FacetPath::from("$.nested_field.field1")
                && matches!(
                    peek.partial_eq(&Peek::new(&facet.nested_field.field1)),
                    Some(true)
                )
        });

        let iterator_contains_field2 = iter.any(|(path, peek)| {
            path == FacetPath::from("$.nested_field.field2")
                && matches!(
                    peek.partial_eq(&Peek::new(&facet.nested_field.field2)),
                    Some(true)
                )
        });

        assert!(iterator_contains_field1);
        assert!(iterator_contains_field2);
    }

    #[test]
    fn test_get_peek_by_path_struct() {
        let facet = TestFacet {
            field1: 42,
            field2: "Hello".to_string(),
        };

        let peek1 = facet.get(&FacetPath::from("$.field1")).unwrap();
        let peek2 = facet.get(&FacetPath::from("$.field2")).unwrap();

        assert_eq!(peek1.partial_eq(&Peek::new(&facet.field1)), Some(true));
        assert_eq!(peek2.partial_eq(&Peek::new(&facet.field2)), Some(true));
    }

    #[test]
    fn test_facet_iterator_enum_unit() {
        let facet = MyEnum::Unit;
        let mut iter = facet.inspect();
        // Should contain only the root path
        assert!(iter.any(|(p, _)| p == FacetPath::from("$")));
    }

    #[test]
    fn test_facet_iterator_enum_tuple() {
        let facet = MyEnum::Tuple(42, "hello".to_string());
        let mut iter = facet.inspect();
        // Should contain root, and tuple fields
        assert!(iter.any(|(p, peek)| p == FacetPath::from("$.0")
            && peek.partial_eq(&Peek::new(&42)).unwrap_or(false)));
        assert!(iter.any(|(p, peek)| {
            p == FacetPath::from("$.1")
                && peek
                    .partial_eq(&Peek::new(&"hello".to_string()))
                    .unwrap_or(false)
        }));
    }

    #[test]
    fn test_facet_iterator_enum_struct() {
        let facet = MyEnum::Struct {
            x: 7,
            y: "abc".to_string(),
        };
        let mut iter = facet.inspect();

        assert!(iter.any(|(p, peek)| p == FacetPath::from("$.x")
            && peek.partial_eq(&Peek::new(&7)).unwrap_or(false)));
        assert!(iter.any(|(p, peek)| {
            p == FacetPath::from("$.y")
                && peek
                    .partial_eq(&Peek::new(&"abc".to_string()))
                    .unwrap_or(false)
        }));
    }

    #[test]
    fn test_get_peek_by_path_enum_variants() {
        // Struct variant
        let facet = MyEnum::Struct {
            x: 99,
            y: "zzz".to_string(),
        };
        let peek_x = facet.get(&FacetPath::from("$.x")).unwrap();
        let peek_y = facet.get(&FacetPath::from("$.y")).unwrap();
        assert_eq!(peek_x.partial_eq(&Peek::new(&99)), Some(true));
        assert_eq!(
            peek_y.partial_eq(&Peek::new(&"zzz".to_string())),
            Some(true)
        );

        // Tuple variant
        let facet = MyEnum::Tuple(123, "tupleval".to_string());
        let peek_0 = facet.get(&FacetPath::from("$.0")).unwrap();
        let peek_1 = facet.get(&FacetPath::from("$.1")).unwrap();
        assert_eq!(peek_0.partial_eq(&Peek::new(&123)), Some(true));
        assert_eq!(
            peek_1.partial_eq(&Peek::new(&"tupleval".to_string())),
            Some(true)
        );

        // Unit variant (should only have root path)
        let facet = MyEnum::Unit;
        let peek_root = facet.get(&FacetPath::from("$")).unwrap();
        // For unit, just check that we get a Peek and it matches itself
        assert!(peek_root.partial_eq(&Peek::new(&facet)).unwrap_or(false));
    }

    #[test]
    fn test_facet_iterator_array() {
        let arr = [10, 20, 30];
        let iter = arr.inspect();

        let items: Vec<_> = iter.clone().collect();
        dbg!(items);

        assert!(iter.clone().any(|(p, peek)| p == FacetPath::from("$.0")
            && peek.partial_eq(&Peek::new(&10)).unwrap_or(false)));
        assert!(iter.clone().any(|(p, peek)| p == FacetPath::from("$.1")
            && peek.partial_eq(&Peek::new(&20)).unwrap_or(false)));
        assert!(iter.clone().any(|(p, peek)| p == FacetPath::from("$.2")
            && peek.partial_eq(&Peek::new(&30)).unwrap_or(false)));
    }

    #[test]
    fn test_facet_iterator_vec() {
        let vec = vec!["a".to_string(), "b".to_string()];
        let iter = vec.inspect();
        assert!(iter.clone().any(|(p, peek)| {
            p == FacetPath::from("$.0")
                && peek
                    .partial_eq(&Peek::new(&"a".to_string()))
                    .unwrap_or(false)
        }));
        assert!(iter.clone().any(|(p, peek)| {
            p == FacetPath::from("$.1")
                && peek
                    .partial_eq(&Peek::new(&"b".to_string()))
                    .unwrap_or(false)
        }));
    }

    #[test]
    fn test_facet_iterator_slice() {
        let slice: &[u32] = &[5, 6, 7];
        let iter = slice.inspect();

        dbg!(iter.clone().collect::<Vec<_>>());

        assert!(iter.clone().any(|(p, peek)| p == FacetPath::from("$.0")
            && peek.partial_eq(&Peek::new(&5)).unwrap_or(false)));
        assert!(iter.clone().any(|(p, peek)| p == FacetPath::from("$.1")
            && peek.partial_eq(&Peek::new(&6)).unwrap_or(false)));
        assert!(iter.clone().any(|(p, peek)| p == FacetPath::from("$.2")
            && peek.partial_eq(&Peek::new(&7)).unwrap_or(false)));
    }

    #[test]
    fn test_facet_iterator_map() {
        let mut map = HashMap::new();
        map.insert("foo".to_string(), 123);
        map.insert("bar".to_string(), 456);
        let iter = map.inspect();
        assert!(iter.clone().any(|(p, peek)| p == FacetPath::from("$.foo")
            && peek.partial_eq(&Peek::new(&123)).unwrap_or(false)));
        assert!(iter.clone().any(|(p, peek)| p == FacetPath::from("$.bar")
            && peek.partial_eq(&Peek::new(&456)).unwrap_or(false)));
    }

    #[test]
    fn test_get_peek_by_path_array_vec_slice_map() {
        let arr = [1, 2, 3];
        assert_eq!(
            arr.get(&FacetPath::from("$.0"))
                .unwrap()
                .partial_eq(&Peek::new(&1)),
            Some(true)
        );
        assert_eq!(
            arr.get(&FacetPath::from("$.2"))
                .unwrap()
                .partial_eq(&Peek::new(&3)),
            Some(true)
        );

        let vec = vec![9, 8, 7];
        assert_eq!(
            vec.get(&FacetPath::from("$.1"))
                .unwrap()
                .partial_eq(&Peek::new(&8)),
            Some(true)
        );

        let slice: &[u32] = &[4, 5];
        assert_eq!(
            FacetInspect::get(&slice, &FacetPath::from("$.0"))
                .unwrap()
                .partial_eq(&Peek::new(&4)),
            Some(true)
        );

        let mut map = HashMap::new();
        map.insert("k1".to_string(), 11);
        map.insert("k2".to_string(), 22);
        assert_eq!(
            FacetInspect::get(&map, &FacetPath::from("$.k1"))
                .unwrap()
                .partial_eq(&Peek::new(&11)),
            Some(true)
        );
        assert_eq!(
            FacetInspect::get(&map, &FacetPath::from("$.k2"))
                .unwrap()
                .partial_eq(&Peek::new(&22)),
            Some(true)
        );
    }
}
