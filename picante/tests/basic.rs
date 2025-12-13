use picante::db::{DynIngredient, IngredientLookup, IngredientRegistry};
use picante::error::PicanteError;
use picante::ingredient::{DerivedIngredient, InputIngredient};
use picante::key::QueryKindId;
use picante::persist::{load_cache, save_cache};
use picante::runtime::{HasRuntime, Runtime};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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
async fn derived_caches_and_invalidates() {
    init_tracing();

    let mut db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    db.register(input.clone());

    input.set(&db, "a".into(), "hello".into());

    let executions = Arc::new(AtomicUsize::new(0));
    let input_for_compute = input.clone();
    let executions_for_compute = executions.clone();

    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(2),
        "Len",
        move |db, key| {
            let input = input_for_compute.clone();
            let executions = executions_for_compute.clone();
            Box::pin(async move {
                executions.fetch_add(1, Ordering::SeqCst);
                let text = input.get(db, &key)?.expect("missing input");
                Ok(text.len() as u64)
            })
        },
    ));
    db.register(derived.clone());

    let v1 = derived.get(&db, "a".into()).await.unwrap();
    let v2 = derived.get(&db, "a".into()).await.unwrap();
    assert_eq!(v1, 5);
    assert_eq!(v2, 5);
    assert_eq!(executions.load(Ordering::SeqCst), 1);

    input.set(&db, "a".into(), "hello!!!".into());

    let v3 = derived.get(&db, "a".into()).await.unwrap();
    assert_eq!(v3, 8);
    assert_eq!(executions.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn derived_singleflight_across_tasks() {
    init_tracing();

    let mut db = TestDb::default();
    let executions = Arc::new(AtomicUsize::new(0));
    let executions_for_compute = executions.clone();

    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(1),
        "Slow",
        move |_db, _key| {
            let executions = executions_for_compute.clone();
            Box::pin(async move {
                executions.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                Ok(42)
            })
        },
    ));
    db.register(derived.clone());
    let db = Arc::new(db);

    let mut joins = Vec::new();
    for _ in 0..10 {
        let db = db.clone();
        let derived = derived.clone();
        joins.push(tokio::spawn(async move {
            derived.get(db.as_ref(), "k".into()).await.unwrap()
        }));
    }

    for j in joins {
        assert_eq!(j.await.unwrap(), 42);
    }

    assert_eq!(executions.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn detects_cycles_within_task() {
    init_tracing();

    let mut db = TestDb::default();

    let ingredient: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new_cyclic(
        |weak: &std::sync::Weak<DerivedIngredient<TestDb, String, u64>>| {
            let weak = weak.clone();
            DerivedIngredient::new(QueryKindId(1), "Cycle", move |db, key| {
                let weak = weak.clone();
                Box::pin(async move {
                    let me = weak.upgrade().expect("ingredient dropped");
                    me.get(db, key).await
                })
            })
        },
    );
    db.register(ingredient.clone());

    let err = ingredient.get(&db, "k".into()).await.unwrap_err();
    match &*err {
        PicanteError::Cycle { .. } => {}
        other => panic!("expected cycle error, got {other:?}"),
    }
}

#[tokio::test]
async fn persistence_roundtrip() {
    init_tracing();

    let cache_path = {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("picante-cache-{pid}-{nanos}.bin"))
    };

    let mut db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    db.register(input.clone());

    input.set(&db, "a".into(), "hello".into());

    let exec1 = Arc::new(AtomicUsize::new(0));
    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = {
        let input = input.clone();
        let exec = exec1.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(2),
            "Len",
            move |db, key| {
                let input = input.clone();
                let exec = exec.clone();
                Box::pin(async move {
                    exec.fetch_add(1, Ordering::SeqCst);
                    let text = input.get(db, &key)?.expect("missing input");
                    Ok(text.len() as u64)
                })
            },
        ))
    };
    db.register(derived.clone());

    let v = derived.get(&db, "a".into()).await.unwrap();
    assert_eq!(v, 5);
    assert_eq!(exec1.load(Ordering::SeqCst), 1);

    save_cache(&cache_path, db.runtime(), &[&*input, &*derived])
        .await
        .unwrap();

    let mut db2 = TestDb::default();
    let input2: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    db2.register(input2.clone());

    let exec2 = Arc::new(AtomicUsize::new(0));
    let derived2: Arc<DerivedIngredient<TestDb, String, u64>> = {
        let input = input2.clone();
        let exec = exec2.clone();
        Arc::new(DerivedIngredient::new(
            QueryKindId(2),
            "Len",
            move |db, key| {
                let input = input.clone();
                let exec = exec.clone();
                Box::pin(async move {
                    exec.fetch_add(1, Ordering::SeqCst);
                    let text = input.get(db, &key)?.expect("missing input");
                    Ok(text.len() as u64)
                })
            },
        ))
    };
    db2.register(derived2.clone());

    let loaded = load_cache(&cache_path, db2.runtime(), &[&*input2, &*derived2])
        .await
        .unwrap();
    assert!(loaded);

    let v2 = derived2.get(&db2, "a".into()).await.unwrap();
    assert_eq!(v2, 5);
    assert_eq!(exec2.load(Ordering::SeqCst), 0);

    input2.set(&db2, "a".into(), "hello!".into());
    let v3 = derived2.get(&db2, "a".into()).await.unwrap();
    assert_eq!(v3, 6);
    assert_eq!(exec2.load(Ordering::SeqCst), 1);

    let _ = tokio::fs::remove_file(&cache_path).await;
}

#[tokio::test]
async fn poisoned_cells_recompute_after_revision_bump() {
    init_tracing();

    let mut db = TestDb::default();

    let executions = Arc::new(AtomicUsize::new(0));
    let executions_for_compute = executions.clone();

    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(1),
        "MaybePanic",
        move |_db, _key| {
            let executions = executions_for_compute.clone();
            Box::pin(async move {
                let n = executions.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    panic!("boom");
                }
                Ok(42)
            })
        },
    ));
    db.register(derived.clone());

    let err1 = derived.get(&db, "k".into()).await.unwrap_err();
    match &*err1 {
        PicanteError::Panic { .. } => {}
        other => panic!("expected panic error, got {other:?}"),
    }

    let err2 = derived.get(&db, "k".into()).await.unwrap_err();
    match &*err2 {
        PicanteError::Panic { .. } => {}
        other => panic!("expected panic error, got {other:?}"),
    }

    assert_eq!(executions.load(Ordering::SeqCst), 1);

    // Bump revision so the poisoned value becomes stale and can be recomputed.
    db.runtime().bump_revision();

    let v = derived.get(&db, "k".into()).await.unwrap();
    assert_eq!(v, 42);
    assert_eq!(executions.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn input_snapshot_captures_state_at_creation_time() {
    init_tracing();

    let db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));

    // Set initial values
    input.set(&db, "a".into(), "hello".into());
    input.set(&db, "b".into(), "world".into());

    // Take snapshot
    let snapshot = input.snapshot();

    // Snapshot contains the data
    assert_eq!(snapshot.len(), 2);
    assert_eq!(
        snapshot.get(&"a".to_string()).unwrap().value,
        Some("hello".to_string())
    );
    assert_eq!(
        snapshot.get(&"b".to_string()).unwrap().value,
        Some("world".to_string())
    );
}

#[tokio::test]
async fn input_snapshot_remains_valid_after_modification() {
    init_tracing();

    let db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));

    input.set(&db, "a".into(), "v1".into());

    // Take snapshot
    let snapshot = input.snapshot();

    // Modify live ingredient
    input.set(&db, "a".into(), "v2".into());
    input.set(&db, "b".into(), "new".into());

    // Snapshot still sees original data
    assert_eq!(snapshot.len(), 1);
    assert_eq!(
        snapshot.get(&"a".to_string()).unwrap().value,
        Some("v1".to_string())
    );
    assert!(snapshot.get(&"b".to_string()).is_none());

    // Live ingredient sees new data
    assert_eq!(input.get(&db, &"a".into()).unwrap(), Some("v2".to_string()));
    assert_eq!(
        input.get(&db, &"b".into()).unwrap(),
        Some("new".to_string())
    );
}

#[tokio::test]
async fn derived_snapshot_captures_cells() {
    init_tracing();

    let mut db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    db.register(input.clone());

    input.set(&db, "a".into(), "hello".into());

    let input_for_compute = input.clone();
    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(2),
        "Len",
        move |db, key| {
            let input = input_for_compute.clone();
            Box::pin(async move {
                let text = input.get(db, &key)?.expect("missing input");
                Ok(text.len() as u64)
            })
        },
    ));
    db.register(derived.clone());

    // Compute a value
    let _ = derived.get(&db, "a".into()).await.unwrap();

    // Take snapshot
    let snapshot = derived.snapshot();

    // Snapshot contains the cell
    assert_eq!(snapshot.len(), 1);
    assert!(snapshot.get(&"a".to_string()).is_some());
}

#[tokio::test]
async fn derived_snapshot_remains_valid_after_modification() {
    init_tracing();

    let mut db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    db.register(input.clone());

    input.set(&db, "a".into(), "hello".into());

    let input_for_compute = input.clone();
    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(2),
        "Len",
        move |db, key| {
            let input = input_for_compute.clone();
            Box::pin(async move {
                let text = input.get(db, &key)?.expect("missing input");
                Ok(text.len() as u64)
            })
        },
    ));
    db.register(derived.clone());

    // Compute initial value
    let _ = derived.get(&db, "a".into()).await.unwrap();

    // Take snapshot
    let snapshot = derived.snapshot();

    // Compute another value on live ingredient
    let _ = derived.get(&db, "b".into()).await;

    // Snapshot still has only the original cell
    assert_eq!(snapshot.len(), 1);
    assert!(snapshot.get(&"a".to_string()).is_some());
    assert!(snapshot.get(&"b".to_string()).is_none());

    // Live ingredient has both
    let live_snapshot = derived.snapshot();
    assert_eq!(live_snapshot.len(), 2);
}
