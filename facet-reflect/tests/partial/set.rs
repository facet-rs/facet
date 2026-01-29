use alloc::collections::BTreeSet;
use facet::Facet;
use facet_reflect::Partial;
use facet_testhelpers::{IPanic, test};
use std::collections::HashSet;

extern crate alloc;

#[test]
fn set_hashset_basic() -> Result<(), IPanic> {
    let set = Partial::alloc::<HashSet<i32>>()?
        .init_set()?
        .insert(42)?
        .insert(84)?
        .insert(126)?
        .build()?
        .materialize::<HashSet<i32>>()?;
    assert_eq!(set.len(), 3);
    assert!(set.contains(&42));
    assert!(set.contains(&84));
    assert!(set.contains(&126));
    Ok(())
}

#[test]
fn set_hashset_strings() -> Result<(), IPanic> {
    let set = Partial::alloc::<HashSet<String>>()?
        .init_set()?
        .insert("foo".to_string())?
        .insert("bar".to_string())?
        .insert("baz".to_string())?
        .build()?
        .materialize::<HashSet<String>>()?;
    assert_eq!(set.len(), 3);
    assert!(set.contains("foo"));
    assert!(set.contains("bar"));
    assert!(set.contains("baz"));
    Ok(())
}

#[test]
fn set_hashset_empty() -> Result<(), IPanic> {
    let set = Partial::alloc::<HashSet<String>>()?
        .init_set()?
        .build()?
        .materialize::<HashSet<String>>()?;
    assert_eq!(set.len(), 0);
    Ok(())
}

#[test]
fn set_hashset_duplicates() -> Result<(), IPanic> {
    let set = Partial::alloc::<HashSet<i32>>()?
        .init_set()?
        .insert(42)?
        .insert(42)?
        .insert(42)?
        .build()?
        .materialize::<HashSet<i32>>()?;
    assert_eq!(set.len(), 1);
    assert!(set.contains(&42));
    Ok(())
}

#[test]
fn set_btreeset_basic() -> Result<(), IPanic> {
    let set = Partial::alloc::<BTreeSet<i32>>()?
        .init_set()?
        .insert(3)?
        .insert(1)?
        .insert(2)?
        .build()?
        .materialize::<BTreeSet<i32>>()?;
    assert_eq!(set.len(), 3);
    let vec: Vec<_> = set.iter().copied().collect();
    assert_eq!(vec, vec![1, 2, 3]);
    Ok(())
}

#[test]
fn set_using_begin_set_item() -> Result<(), IPanic> {
    let set = Partial::alloc::<HashSet<i32>>()?
        .init_set()?
        .begin_set_item()?
        .set(100)?
        .end()?
        .begin_set_item()?
        .set(200)?
        .end()?
        .build()?
        .materialize::<HashSet<i32>>()?;
    assert_eq!(set.len(), 2);
    assert!(set.contains(&100));
    assert!(set.contains(&200));
    Ok(())
}

#[test]
fn set_duplicate_drops_new_value() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct DropTracker {
        id: u64,
    }

    impl PartialEq for DropTracker {
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id
        }
    }
    impl Eq for DropTracker {}
    impl core::hash::Hash for DropTracker {
        fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
            self.id.hash(state);
        }
    }
    impl Drop for DropTracker {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    DROP_COUNT.store(0, Ordering::SeqCst);

    {
        let set = Partial::alloc::<HashSet<DropTracker>>()?
            .init_set()?
            .insert(DropTracker { id: 1 })?
            .insert(DropTracker { id: 2 })?
            .insert(DropTracker { id: 1 })?
            .insert(DropTracker { id: 3 })?
            .insert(DropTracker { id: 2 })?
            .build()?
            .materialize::<HashSet<DropTracker>>()?;

        assert_eq!(set.len(), 3);
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
    }

    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 5);
    Ok(())
}

#[test]
fn set_partial_initialization_drop() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct DropTracker {
        id: u64,
    }

    impl PartialEq for DropTracker {
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id
        }
    }
    impl Eq for DropTracker {}
    impl core::hash::Hash for DropTracker {
        fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
            self.id.hash(state);
        }
    }
    impl Drop for DropTracker {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    DROP_COUNT.store(0, Ordering::SeqCst);

    {
        let partial = Partial::alloc::<HashSet<DropTracker>>()?;
        let _partial = partial
            .init_set()?
            .insert(DropTracker { id: 1 })?
            .insert(DropTracker { id: 2 })?;
    }

    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
    Ok(())
}
