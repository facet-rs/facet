use bumpalo::Bump;
use facet::Facet;
use facet_reflect::Partial;
use facet_testhelpers::{IPanic, test};
use std::collections::HashMap;

#[test]
fn wip_map_trivial() {
    let bump = Bump::new(); let mut partial = Partial::alloc::<HashMap<String, String>>(&bump).unwrap();
    partial = partial.init_map().unwrap();

    partial = partial.begin_key().unwrap();
    partial = partial.set::<String>("key".into()).unwrap();
    partial = partial.end().unwrap();
    partial = partial.begin_value().unwrap();
    partial = partial.set::<String>("value".into()).unwrap();
    partial = partial.end().unwrap();
    let wip: HashMap<String, String> = partial
        .build()
        .unwrap()
        .materialize::<HashMap<String, String>>()
        .unwrap();

    assert_eq!(
        wip,
        HashMap::from([("key".to_string(), "value".to_string())])
    );
}

// =============================================================================
// Tests migrated from src/partial/tests.rs
// =============================================================================

#[test]
fn list_vec_basic() -> Result<(), IPanic> {
    let vec = Partial::alloc::<Vec<i32>>()?
        .init_list()?
        .push(42)?
        .push(84)?
        .push(126)?
        .build()?
        .materialize::<Vec<i32>>()?;
    assert_eq!(vec, vec![42, 84, 126]);
    Ok(())
}

#[test]
fn list_vec_complex() -> Result<(), IPanic> {
    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Person {
        name: String,
        age: u32,
    }

    let vec = Partial::alloc::<Vec<Person>>()?
        .init_list()?
        .begin_list_item()?
        .set_field("name", "Alice".to_string())?
        .set_field("age", 30u32)?
        .end()?
        .begin_list_item()?
        .set_field("name", "Bob".to_string())?
        .set_field("age", 25u32)?
        .end()?
        .build()?
        .materialize::<Vec<Person>>()?;
    assert_eq!(
        vec,
        vec![
            Person {
                name: "Alice".to_string(),
                age: 30
            },
            Person {
                name: "Bob".to_string(),
                age: 25
            }
        ]
    );
    Ok(())
}

#[test]
fn list_vec_empty() -> Result<(), IPanic> {
    let vec = Partial::alloc::<Vec<String>>()?
        .init_list()?
        .build()?
        .materialize::<Vec<String>>()?;
    assert_eq!(vec, Vec::<String>::new());
    Ok(())
}

#[test]
fn list_vec_nested() -> Result<(), IPanic> {
    let vec = Partial::alloc::<Vec<Vec<i32>>>()?
        .init_list()?
        .begin_list_item()?
        .init_list()?
        .push(1)?
        .push(2)?
        .end()?
        .begin_list_item()?
        .init_list()?
        .push(3)?
        .push(4)?
        .push(5)?
        .end()?
        .build()?
        .materialize::<Vec<Vec<i32>>>()?;
    assert_eq!(vec, vec![vec![1, 2], vec![3, 4, 5]]);
    Ok(())
}

#[test]
fn list_vec_reinit() -> Result<(), IPanic> {
    let mut p = Partial::alloc::<Vec<i32>>()?;
    p = p.init_list()?;
    p = p.push(1)?;
    p = p.push(2)?;
    p = p.init_list()?;
    p = p.push(3)?;
    p = p.push(4)?;
    let vec = p.build()?.materialize::<Vec<i32>>()?;
    assert_eq!(vec, vec![1, 2, 3, 4]);
    Ok(())
}

#[test]
fn list_vec_field_reinit() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct S {
        s: Vec<i32>,
    }

    let mut p = Partial::alloc::<S>()?;
    p = p.begin_field("s")?;
    p = p.init_list()?;
    p = p.push(1)?;
    p = p.push(2)?;
    p = p.end()?;
    p = p.begin_field("s")?;
    p = p.init_list()?;
    p = p.push(3)?;
    p = p.push(4)?;
    p = p.end()?;
    let s = p.build()?.materialize::<S>()?;
    assert_eq!(
        s,
        S {
            s: vec![1, 2, 3, 4]
        }
    );
    Ok(())
}

#[test]
fn list_wrong_begin_list() -> Result<(), IPanic> {
    let hv = Partial::alloc::<HashMap<String, i32>>()?;
    let err_str = match hv.init_list() {
        Ok(_) => panic!("expected error"),
        Err(e) => e.to_string(),
    };
    assert!(err_str.contains("init_list can only be called on List or DynamicValue types"));
    Ok(())
}

#[test]
fn map_hashmap_simple() -> Result<(), IPanic> {
    let map = Partial::alloc::<HashMap<String, i32>>()?
        .init_map()?
        .begin_key()?
        .set("foo".to_string())?
        .end()?
        .begin_value()?
        .set(42)?
        .end()?
        .begin_key()?
        .set("bar".to_string())?
        .end()?
        .begin_value()?
        .set(123)?
        .end()?
        .build()?
        .materialize::<HashMap<String, i32>>()?;
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("foo"), Some(&42));
    assert_eq!(map.get("bar"), Some(&123));
    Ok(())
}

#[test]
fn map_hashmap_empty() -> Result<(), IPanic> {
    let map = Partial::alloc::<HashMap<String, String>>()?
        .init_map()?
        .build()?
        .materialize::<HashMap<String, String>>()?;
    assert_eq!(map.len(), 0);
    Ok(())
}

#[test]
fn map_hashmap_complex_values() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        name: String,
        age: u32,
    }

    let map = Partial::alloc::<HashMap<String, Person>>()?
        .init_map()?
        .set_key("alice".to_string())?
        .begin_value()?
        .set_field("name", "Alice".to_string())?
        .set_field("age", 30u32)?
        .end()?
        .set_key("bob".to_string())?
        .begin_value()?
        .set_field("name", "Bob".to_string())?
        .set_field("age", 25u32)?
        .end()?
        .build()?
        .materialize::<HashMap<String, Person>>()?;
    assert_eq!(map.len(), 2);
    assert_eq!(
        map.get("alice"),
        Some(&Person {
            name: "Alice".to_string(),
            age: 30
        })
    );
    assert_eq!(
        map.get("bob"),
        Some(&Person {
            name: "Bob".to_string(),
            age: 25
        })
    );
    Ok(())
}

#[test]
fn map_partial_initialization_drop() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct DropTracker {
        id: u64,
    }

    impl Drop for DropTracker {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    DROP_COUNT.store(0, Ordering::SeqCst);

    {
        let partial = Partial::alloc::<HashMap<String, DropTracker>>()?;
        let _partial = partial
            .init_map()?
            .begin_key()?
            .set("first".to_string())?
            .end()?
            .begin_value()?
            .set(DropTracker { id: 1 })?
            .end()?
            .begin_key()?
            .set("second".to_string())?
            .end()?;
    }

    assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
    Ok(())
}
