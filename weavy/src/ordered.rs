//! Persistent canonical ordered collections.
//!
//! This is the ownership-neutral tree core used by the verified task substrate.
//! Its public API consumes the previous root: `Arc::make_mut` may update an
//! unshared path in place, while shared roots retain immutable descendants.
//! The semantic identity is always [`PersistentMap::entries`], never tree
//! topology or an arena handle.

use std::cmp::Ordering;
use std::sync::Arc;

#[derive(Clone, Debug)]
enum Node<K, V> {
    Empty,
    Branch {
        key: K,
        value: V,
        left: Arc<Node<K, V>>,
        right: Arc<Node<K, V>>,
        height: u8,
    },
}

impl<K, V> Node<K, V> {
    fn height(node: &Arc<Self>) -> u8 {
        match node.as_ref() {
            Self::Empty => 0,
            Self::Branch { height, .. } => *height,
        }
    }
}

/// A persistent AVL map. The tree may have insertion-history-dependent shape,
/// but its canonical identity and iteration are the key-sorted entry sequence.
#[derive(Clone, Debug)]
pub struct PersistentMap<K, V> {
    root: Arc<Node<K, V>>,
    len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertError<K> {
    Duplicate(K),
}

impl<K: Clone + Ord, V: Clone> Default for PersistentMap<K, V> {
    fn default() -> Self {
        Self {
            root: Arc::new(Node::Empty),
            len: 0,
        }
    }
}

impl<K: Clone + Ord, V: Clone> PersistentMap<K, V> {
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn has(&self, key: &K) -> bool {
        let mut node = &self.root;
        loop {
            match node.as_ref() {
                Node::Empty => return false,
                Node::Branch {
                    key: candidate,
                    left,
                    right,
                    ..
                } => match key.cmp(candidate) {
                    Ordering::Less => node = left,
                    Ordering::Equal => return true,
                    Ordering::Greater => node = right,
                },
            }
        }
    }

    #[must_use]
    pub fn get(&self, key: &K) -> Option<&V> {
        let mut node = &self.root;
        loop {
            match node.as_ref() {
                Node::Empty => return None,
                Node::Branch {
                    key: candidate,
                    value,
                    left,
                    right,
                    ..
                } => match key.cmp(candidate) {
                    Ordering::Less => node = left,
                    Ordering::Equal => return Some(value),
                    Ordering::Greater => node = right,
                },
            }
        }
    }

    pub fn add(self, key: K, value: V) -> Result<Self, InsertError<K>> {
        let (root, inserted) = insert(self.root, key, value, false)?;
        Ok(Self {
            root,
            len: self.len + usize::from(inserted),
        })
    }

    #[must_use]
    pub fn with(self, key: K, value: V) -> Self {
        let (root, inserted) = match insert(self.root, key, value, true) {
            Ok(result) => result,
            Err(InsertError::Duplicate(_)) => unreachable!("replacement insertion is total"),
        };
        Self {
            root,
            len: self.len + usize::from(inserted),
        }
    }

    pub fn union(self, other: &Self) -> Result<Self, InsertError<K>> {
        let mut out = self;
        for (key, value) in other.entries() {
            out = out.add(key, value)?;
        }
        Ok(out)
    }

    #[must_use]
    pub fn entries(&self) -> Vec<(K, V)> {
        let mut entries = Vec::with_capacity(self.len);
        collect(&self.root, &mut entries);
        entries
    }
}

fn collect<K: Clone, V: Clone>(node: &Arc<Node<K, V>>, out: &mut Vec<(K, V)>) {
    if let Node::Branch {
        key,
        value,
        left,
        right,
        ..
    } = node.as_ref()
    {
        collect(left, out);
        out.push((key.clone(), value.clone()));
        collect(right, out);
    }
}

fn insert<K: Clone + Ord, V: Clone>(
    mut node: Arc<Node<K, V>>,
    key: K,
    value: V,
    replace: bool,
) -> Result<(Arc<Node<K, V>>, bool), InsertError<K>> {
    match Arc::make_mut(&mut node) {
        Node::Empty => Ok((
            Arc::new(Node::Branch {
                key,
                value,
                left: Arc::new(Node::Empty),
                right: Arc::new(Node::Empty),
                height: 1,
            }),
            true,
        )),
        Node::Branch {
            key: candidate,
            value: candidate_value,
            left,
            right,
            height,
        } => match key.cmp(candidate) {
            Ordering::Equal if replace => {
                *candidate_value = value;
                Ok((node, false))
            }
            Ordering::Equal => Err(InsertError::Duplicate(key)),
            Ordering::Less => {
                let (child, inserted) = insert(left.clone(), key, value, replace)?;
                *left = child;
                *height = 1 + Node::height(left).max(Node::height(right));
                Ok((balance(node), inserted))
            }
            Ordering::Greater => {
                let (child, inserted) = insert(right.clone(), key, value, replace)?;
                *right = child;
                *height = 1 + Node::height(left).max(Node::height(right));
                Ok((balance(node), inserted))
            }
        },
    }
}

fn balance<K: Clone, V: Clone>(node: Arc<Node<K, V>>) -> Arc<Node<K, V>> {
    let Node::Branch { left, right, .. } = node.as_ref() else {
        return node;
    };
    let skew = i16::from(Node::height(left)) - i16::from(Node::height(right));
    if skew > 1 {
        return rotate_right(node);
    }
    if skew < -1 {
        return rotate_left(node);
    }
    node
}

fn rotate_left<K: Clone, V: Clone>(node: Arc<Node<K, V>>) -> Arc<Node<K, V>> {
    let Node::Branch { right, .. } = node.as_ref() else {
        return node;
    };
    let Node::Branch {
        left: pivot_left,
        right: pivot_right,
        ..
    } = right.as_ref()
    else {
        return node;
    };
    if Node::height(pivot_left) > Node::height(pivot_right) {
        return rotate_left_right(node);
    }
    rotate_left_single(node)
}

fn rotate_right<K: Clone, V: Clone>(node: Arc<Node<K, V>>) -> Arc<Node<K, V>> {
    let Node::Branch { left, .. } = node.as_ref() else {
        return node;
    };
    let Node::Branch {
        left: pivot_left,
        right: pivot_right,
        ..
    } = left.as_ref()
    else {
        return node;
    };
    if Node::height(pivot_right) > Node::height(pivot_left) {
        return rotate_right_left(node);
    }
    rotate_right_single(node)
}

fn rotate_left_single<K: Clone, V: Clone>(node: Arc<Node<K, V>>) -> Arc<Node<K, V>> {
    let Node::Branch {
        key,
        value,
        left,
        right,
        ..
    } = node.as_ref()
    else {
        return node;
    };
    let Node::Branch {
        key: rk,
        value: rv,
        left: rl,
        right: rr,
        ..
    } = right.as_ref()
    else {
        return node;
    };
    let left_height = 1 + Node::height(left).max(Node::height(rl));
    Arc::new(Node::Branch {
        key: rk.clone(),
        value: rv.clone(),
        left: Arc::new(Node::Branch {
            key: key.clone(),
            value: value.clone(),
            left: left.clone(),
            right: rl.clone(),
            height: left_height,
        }),
        right: rr.clone(),
        height: 1 + left_height.max(Node::height(rr)),
    })
}

fn rotate_right_single<K: Clone, V: Clone>(node: Arc<Node<K, V>>) -> Arc<Node<K, V>> {
    let Node::Branch {
        key,
        value,
        left,
        right,
        ..
    } = node.as_ref()
    else {
        return node;
    };
    let Node::Branch {
        key: lk,
        value: lv,
        left: ll,
        right: lr,
        ..
    } = left.as_ref()
    else {
        return node;
    };
    let right_height = 1 + Node::height(lr).max(Node::height(right));
    Arc::new(Node::Branch {
        key: lk.clone(),
        value: lv.clone(),
        left: ll.clone(),
        right: Arc::new(Node::Branch {
            key: key.clone(),
            value: value.clone(),
            left: lr.clone(),
            right: right.clone(),
            height: right_height,
        }),
        height: 1 + Node::height(ll).max(right_height),
    })
}

fn rotate_left_right<K: Clone, V: Clone>(node: Arc<Node<K, V>>) -> Arc<Node<K, V>> {
    let Node::Branch {
        key,
        value,
        left,
        right,
        ..
    } = node.as_ref()
    else {
        return node;
    };
    let Node::Branch {
        key: rk,
        value: rv,
        left: rl,
        right: rr,
        ..
    } = right.as_ref()
    else {
        return node;
    };
    let Node::Branch {
        key: pk,
        value: pv,
        left: pl,
        right: pr,
        ..
    } = rl.as_ref()
    else {
        return rotate_left_single(node);
    };
    let left_height = 1 + Node::height(left).max(Node::height(pl));
    let right_height = 1 + Node::height(pr).max(Node::height(rr));
    Arc::new(Node::Branch {
        key: pk.clone(),
        value: pv.clone(),
        left: Arc::new(Node::Branch {
            key: key.clone(),
            value: value.clone(),
            left: left.clone(),
            right: pl.clone(),
            height: left_height,
        }),
        right: Arc::new(Node::Branch {
            key: rk.clone(),
            value: rv.clone(),
            left: pr.clone(),
            right: rr.clone(),
            height: right_height,
        }),
        height: 1 + left_height.max(right_height),
    })
}

fn rotate_right_left<K: Clone, V: Clone>(node: Arc<Node<K, V>>) -> Arc<Node<K, V>> {
    let Node::Branch {
        key,
        value,
        left,
        right,
        ..
    } = node.as_ref()
    else {
        return node;
    };
    let Node::Branch {
        key: lk,
        value: lv,
        left: ll,
        right: lr,
        ..
    } = left.as_ref()
    else {
        return node;
    };
    let Node::Branch {
        key: pk,
        value: pv,
        left: pl,
        right: pr,
        ..
    } = lr.as_ref()
    else {
        return rotate_right_single(node);
    };
    let left_height = 1 + Node::height(ll).max(Node::height(pl));
    let right_height = 1 + Node::height(pr).max(Node::height(right));
    Arc::new(Node::Branch {
        key: pk.clone(),
        value: pv.clone(),
        left: Arc::new(Node::Branch {
            key: lk.clone(),
            value: lv.clone(),
            left: ll.clone(),
            right: pl.clone(),
            height: left_height,
        }),
        right: Arc::new(Node::Branch {
            key: key.clone(),
            value: value.clone(),
            left: pr.clone(),
            right: right.clone(),
            height: right_height,
        }),
        height: 1 + left_height.max(right_height),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertion_order_does_not_change_canonical_entries() {
        let a = PersistentMap::default()
            .add(2, 20)
            .unwrap()
            .add(1, 10)
            .unwrap();
        let b = PersistentMap::default()
            .add(1, 10)
            .unwrap()
            .add(2, 20)
            .unwrap();
        assert_eq!(a.entries(), b.entries());
    }

    #[test]
    fn replacement_and_duplicate_are_distinct_and_persistent() {
        let source = PersistentMap::default().add(1, 10).unwrap();
        assert!(matches!(
            source.clone().add(1, 99),
            Err(InsertError::Duplicate(1))
        ));
        let descendant = source.clone().with(1, 99).add(2, 20).unwrap();
        assert_eq!(source.entries(), vec![(1, 10)]);
        assert_eq!(descendant.entries(), vec![(1, 99), (2, 20)]);
    }

    #[test]
    fn union_reports_the_first_structural_conflict() {
        let left = PersistentMap::default()
            .add(3, 30)
            .unwrap()
            .add(1, 10)
            .unwrap();
        let right = PersistentMap::default()
            .add(2, 20)
            .unwrap()
            .add(3, 300)
            .unwrap();
        assert!(matches!(left.union(&right), Err(InsertError::Duplicate(3))));
    }

    #[test]
    fn membership_does_not_read_values() {
        #[derive(Clone)]
        struct Poison;
        let map = PersistentMap::default().add(7, Poison).unwrap();
        assert!(map.has(&7));
        assert!(!map.has(&8));
    }

    #[test]
    fn accumulator_keeps_a_logarithmic_spine() {
        let mut map = PersistentMap::default();
        for key in 0..200_000 {
            map = map.add(key, key * 2).expect("distinct key");
        }
        assert_eq!(map.get(&123_456), Some(&246_912));
        assert!(Node::height(&map.root) < 32, "AVL height is logarithmic");
    }
}
