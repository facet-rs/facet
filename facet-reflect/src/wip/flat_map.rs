#![allow(dead_code)]

use alloc::borrow::Borrow;

/// Flat (Vec) backed map
///
/// This preserves insertion order
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FlatMap<K, V> {
    inner: alloc::vec::Vec<(K, V)>,
}

impl<K: PartialEq + Eq, V> FlatMap<K, V> {
    pub(crate) fn new() -> Self {
        Default::default()
    }

    pub(crate) fn insert(&mut self, key: K, mut value: V) -> Option<V> {
        let Some(index) = self.keys().position(|k| *k == key) else {
            self.insert_unchecked(key, value);
            return None;
        };

        core::mem::swap(&mut self.inner[index].1, &mut value);
        Some(value)
    }

    pub(crate) fn insert_unchecked(&mut self, key: K, value: V) {
        self.inner.push((key, value));
    }

    pub(crate) fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Eq + ?Sized,
    {
        for existing in self.keys() {
            if existing.borrow() == key {
                return true;
            }
        }
        false
    }

    pub(crate) fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K, &mut V) -> bool,
    {
        self.inner.retain_mut(|(k, v)| f(k, v))
    }

    pub(crate) fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: core::hash::Hash + Eq + ?Sized,
    {
        self.remove_entry(key).map(|(_, v)| v)
    }

    pub(crate) fn remove_entry<Q>(&mut self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: core::hash::Hash + Eq + ?Sized,
    {
        let index = self.keys().position(|k| k.borrow() == key)?;
        let kv = self.inner.remove(index);
        Some(kv)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub(crate) fn len(&mut self) -> usize {
        self.inner.len()
    }

    pub(crate) fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Eq + ?Sized,
    {
        let index = self.keys().position(|k| k.borrow() == key)?;
        Some(&self.inner[index].1)
    }

    pub(crate) fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Eq + ?Sized,
    {
        let index = self.keys().position(|k| k.borrow() == key)?;
        Some(&mut self.inner[index].1)
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &K> {
        self.inner.iter().map(|(k, _)| k)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.inner.iter().map(|(_, v)| v)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter().map(|(k, v)| (k, v))
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.inner.iter_mut().map(|(k, v)| (&*k, v))
    }
}

impl<K: PartialEq + Eq, V> Default for FlatMap<K, V> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}
