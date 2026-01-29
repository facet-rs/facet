use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use facet::Facet;

#[derive(Eq, PartialEq, Facet)]
pub struct Key<I> {
    pub a: String,
    pub b: I,
}

impl<I: Hash> Hash for Key<I> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.a.hash(state);
        self.b.hash(state);
    }
}

#[derive(Facet)]
#[facet(where I: Eq + Hash + 'static, T: 'static)]
pub struct Container<I, T> {
    pub data: HashMap<Key<I>, T>,
}

#[test]
fn hashmap_key_with_custom_bounds() {
    let mut container: Container<u32, String> = Container {
        data: HashMap::new(),
    };

    container.data.insert(
        Key {
            a: "test".to_string(),
            b: 42u32,
        },
        "value".to_string(),
    );

    assert_eq!(container.data.len(), 1);
}
