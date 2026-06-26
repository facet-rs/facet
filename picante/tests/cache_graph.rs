use picante::Revision;
use picante::db::{DynIngredient, IngredientLookup, IngredientRegistry};
use picante::ingredient::{DerivedIngredient, InputIngredient};
use picante::key::QueryKindId;
use picante::persist::{CacheFile, Section, SectionType, load_cache, save_cache};
use picante::runtime::{HasRuntime, Runtime, RuntimeEvent};
use std::sync::Arc;

fn init_tracing() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .try_init();
    });
}

#[derive(Default)]
struct TestDb {
    runtime: Runtime,
    ingredients: IngredientRegistry<TestDb>,
}

impl HasRuntime for TestDb {
    fn runtime(&self) -> &Runtime {
        &self.runtime
    }
}

impl IngredientLookup for TestDb {
    fn ingredient(&self, kind: QueryKindId) -> Option<&dyn DynIngredient<Self>> {
        self.ingredients.ingredient(kind)
    }
}

impl TestDb {
    fn register<I>(&mut self, ingredient: Arc<I>)
    where
        I: DynIngredient<Self> + 'static,
    {
        self.ingredients.register(ingredient);
    }
}

#[derive(Clone, Debug, facet::Facet)]
struct FacetOnlyKey {
    shard: u32,
    slot: u32,
}

#[derive(Clone, Debug, facet::Facet)]
struct LegacyInputRecord<K, V> {
    key: K,
    value: Option<V>,
    changed_at: u64,
}

#[derive(Clone, Debug, facet::Facet)]
struct LegacyDepRecord {
    kind_id: u32,
    key_bytes: Vec<u8>,
}

#[derive(Clone, Debug, facet::Facet)]
struct LegacyDerivedRecord<K, V> {
    key: K,
    value: V,
    verified_at: u64,
    changed_at: u64,
    deps: Vec<LegacyDepRecord>,
}

#[tokio_test_lite::test]
async fn load_cache_restores_reverse_deps_for_invalidation_events() {
    init_tracing();

    let cache_path = temp_file("picante-cache-graph.bin");

    let mut db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    db.register(input.clone());

    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = {
        let input = input.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(2),
            "Len",
            move |db, key| {
                let input = input.clone();
                Box::pin(async move {
                    let s = input.get(db, &key)?.unwrap_or_default();
                    Ok(s.len() as u64)
                })
            },
        ))
    };
    db.register(derived.clone());

    input.set(&db, "a".into(), "hello".into());
    let v1 = derived.get(&db, "a".into()).await.unwrap();
    assert_eq!(v1, 5);

    save_cache(&cache_path, db.runtime(), &[&*input, &*derived])
        .await
        .unwrap();

    let mut db2 = TestDb::default();
    let input2: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    db2.register(input2.clone());

    let derived2: Arc<DerivedIngredient<TestDb, String, u64>> = {
        let input2 = input2.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(2),
            "Len",
            move |db, key| {
                let input2 = input2.clone();
                Box::pin(async move {
                    let s = input2.get(db, &key)?.unwrap_or_default();
                    Ok(s.len() as u64)
                })
            },
        ))
    };
    db2.register(derived2.clone());

    let mut events = db2.runtime().subscribe_events();

    let loaded = load_cache(&cache_path, db2.runtime(), &[&*input2, &*derived2])
        .await
        .unwrap();
    assert!(loaded);

    match events.recv().await.unwrap() {
        RuntimeEvent::RevisionSet { revision } => assert_eq!(revision, Revision(1)),
        other => panic!("expected RevisionSet, got {other:?}"),
    }

    input2.set(&db2, "a".into(), "hello!".into());

    let mut saw = false;
    for _ in 0..8 {
        if let RuntimeEvent::QueryInvalidated {
            kind,
            key,
            by_kind,
            by_key,
            ..
        } = events.recv().await.unwrap()
        {
            assert_eq!(kind, QueryKindId(2));
            assert_eq!(key.decode_facet::<String>().unwrap(), "a");
            assert_eq!(by_kind, QueryKindId(1));
            assert_eq!(by_key.decode_facet::<String>().unwrap(), "a");
            saw = true;
            break;
        }
    }
    assert!(saw, "expected QueryInvalidated event after input set");

    let _ = tokio::fs::remove_file(&cache_path).await;
}

#[tokio_test_lite::test]
async fn typed_facet_keys_do_not_require_rust_hash_or_eq_after_cache_load() {
    init_tracing();

    let cache_path = temp_file("picante-typed-key-cache-graph.bin");

    let mut db = TestDb::default();
    let input: Arc<InputIngredient<FacetOnlyKey, String>> =
        Arc::new(InputIngredient::new(QueryKindId(10), "FacetOnlyInput"));
    db.register(input.clone());

    let derived: Arc<DerivedIngredient<TestDb, FacetOnlyKey, u64>> = {
        let input = input.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(11),
            "FacetOnlyLen",
            move |db, key| {
                let input = input.clone();
                Box::pin(async move {
                    let s = input.get(db, &key)?.unwrap_or_default();
                    Ok(s.len() as u64)
                })
            },
        ))
    };
    db.register(derived.clone());

    let key = FacetOnlyKey { shard: 7, slot: 3 };
    input.set(&db, key.clone(), "hello".to_string());
    assert_eq!(derived.get(&db, key.clone()).await.unwrap(), 5);

    save_cache(&cache_path, db.runtime(), &[&*input, &*derived])
        .await
        .unwrap();

    let mut db2 = TestDb::default();
    let input2: Arc<InputIngredient<FacetOnlyKey, String>> =
        Arc::new(InputIngredient::new(QueryKindId(10), "FacetOnlyInput"));
    db2.register(input2.clone());

    let derived2: Arc<DerivedIngredient<TestDb, FacetOnlyKey, u64>> = {
        let input2 = input2.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(11),
            "FacetOnlyLen",
            move |db, key| {
                let input2 = input2.clone();
                Box::pin(async move {
                    let s = input2.get(db, &key)?.unwrap_or_default();
                    Ok(s.len() as u64)
                })
            },
        ))
    };
    db2.register(derived2.clone());

    let mut events = db2.runtime().subscribe_events();

    let loaded = load_cache(&cache_path, db2.runtime(), &[&*input2, &*derived2])
        .await
        .unwrap();
    assert!(loaded);

    match events.recv().await.unwrap() {
        RuntimeEvent::RevisionSet { revision } => assert_eq!(revision, Revision(1)),
        other => panic!("expected RevisionSet, got {other:?}"),
    }

    input2.set(&db2, key.clone(), "hello!".to_string());

    let mut saw = false;
    for _ in 0..8 {
        if let RuntimeEvent::QueryInvalidated {
            kind,
            key,
            by_kind,
            by_key,
            ..
        } = events.recv().await.unwrap()
        {
            let key = key.decode_facet::<FacetOnlyKey>().unwrap();
            let by_key = by_key.decode_facet::<FacetOnlyKey>().unwrap();
            assert_eq!(kind, QueryKindId(11));
            assert_eq!(key.shard, 7);
            assert_eq!(key.slot, 3);
            assert_eq!(by_kind, QueryKindId(10));
            assert_eq!(by_key.shard, 7);
            assert_eq!(by_key.slot, 3);
            saw = true;
            break;
        }
    }
    assert!(saw, "expected typed-key invalidation after input set");

    assert_eq!(derived2.get(&db2, key).await.unwrap(), 6);

    let _ = tokio::fs::remove_file(&cache_path).await;
}

#[tokio_test_lite::test]
async fn legacy_persistent_key_bytes_rehydrate_into_current_runtime_keys() {
    init_tracing();

    let cache_path = temp_file("picante-legacy-key-bytes-cache-graph.bin");
    let key = FacetOnlyKey { shard: 42, slot: 9 };
    let key_bytes = facet_postcard::to_vec(&key).unwrap();

    let input_record = LegacyInputRecord {
        key: key.clone(),
        value: Some("hello".to_string()),
        changed_at: 1,
    };
    let derived_record = LegacyDerivedRecord {
        key: key.clone(),
        value: 5_u64,
        verified_at: 1,
        changed_at: 1,
        deps: vec![LegacyDepRecord {
            kind_id: 20,
            key_bytes,
        }],
    };

    let cache = CacheFile {
        format_version: 1,
        current_revision: 1,
        sections: vec![
            Section {
                kind_id: 20,
                kind_name: "LegacyFacetOnlyInput".to_string(),
                section_type: SectionType::Input,
                records: vec![facet_postcard::to_vec(&input_record).unwrap()],
            },
            Section {
                kind_id: 21,
                kind_name: "LegacyFacetOnlyLen".to_string(),
                section_type: SectionType::Derived,
                records: vec![facet_postcard::to_vec(&derived_record).unwrap()],
            },
        ],
    };
    tokio::fs::write(&cache_path, facet_postcard::to_vec(&cache).unwrap())
        .await
        .unwrap();

    let mut db = TestDb::default();
    let input: Arc<InputIngredient<FacetOnlyKey, String>> = Arc::new(InputIngredient::new(
        QueryKindId(20),
        "LegacyFacetOnlyInput",
    ));
    db.register(input.clone());

    let derived: Arc<DerivedIngredient<TestDb, FacetOnlyKey, u64>> = {
        let input = input.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(21),
            "LegacyFacetOnlyLen",
            move |db, key| {
                let input = input.clone();
                Box::pin(async move {
                    let s = input.get(db, &key)?.unwrap_or_default();
                    Ok(s.len() as u64)
                })
            },
        ))
    };
    db.register(derived.clone());

    let mut events = db.runtime().subscribe_events();

    let loaded = load_cache(&cache_path, db.runtime(), &[&*input, &*derived])
        .await
        .unwrap();
    assert!(loaded);

    match events.recv().await.unwrap() {
        RuntimeEvent::RevisionSet { revision } => assert_eq!(revision, Revision(1)),
        other => panic!("expected RevisionSet, got {other:?}"),
    }

    assert_eq!(derived.get(&db, key.clone()).await.unwrap(), 5);

    input.set(&db, key.clone(), "hello!".to_string());

    let mut saw = false;
    for _ in 0..8 {
        if let RuntimeEvent::QueryInvalidated {
            kind,
            key,
            by_kind,
            by_key,
            ..
        } = events.recv().await.unwrap()
        {
            let key = key.decode_facet::<FacetOnlyKey>().unwrap();
            let by_key = by_key.decode_facet::<FacetOnlyKey>().unwrap();
            assert_eq!(kind, QueryKindId(21));
            assert_eq!(key.shard, 42);
            assert_eq!(key.slot, 9);
            assert_eq!(by_kind, QueryKindId(20));
            assert_eq!(by_key.shard, 42);
            assert_eq!(by_key.slot, 9);
            saw = true;
            break;
        }
    }
    assert!(
        saw,
        "expected invalidation from rehydrated persistent key bytes"
    );

    assert_eq!(derived.get(&db, key).await.unwrap(), 6);

    let _ = tokio::fs::remove_file(&cache_path).await;
}

fn temp_file(name: &str) -> std::path::PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("{name}-{pid}-{nanos}"))
}
