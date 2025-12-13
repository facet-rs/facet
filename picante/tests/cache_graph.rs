use picante::Revision;
use picante::db::{DynIngredient, IngredientLookup, IngredientRegistry};
use picante::ingredient::{DerivedIngredient, InputIngredient};
use picante::key::QueryKindId;
use picante::persist::{load_cache, save_cache};
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

#[tokio::test]
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

fn temp_file(name: &str) -> std::path::PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("{name}-{pid}-{nanos}"))
}
