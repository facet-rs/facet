use picante::db::{DynIngredient, IngredientLookup, IngredientRegistry};
use picante::ingredient::InternedIngredient;
use picante::key::QueryKindId;
use picante::persist::{load_cache, save_cache};
use picante::runtime::{HasRuntime, Runtime};
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

#[tokio_test_lite::test]
async fn interned_dedups_and_persists() {
    init_tracing();

    let cache_path = temp_file("picante-interned-cache.bin");

    let mut db = TestDb::default();
    let strings: Arc<InternedIngredient<String>> =
        Arc::new(InternedIngredient::new(QueryKindId(1), "Strings"));
    db.register(strings.clone());

    let id1 = strings.intern("hello".to_string()).unwrap();
    let id2 = strings.intern("hello".to_string()).unwrap();
    assert_eq!(id1, id2);

    let v1 = strings.get(&db, id1).unwrap();
    let v2 = strings.get(&db, id2).unwrap();
    assert!(Arc::ptr_eq(&v1, &v2));
    assert_eq!(v1.as_str(), "hello");

    save_cache(&cache_path, db.runtime(), &[&*strings])
        .await
        .unwrap();

    let mut db2 = TestDb::default();
    let strings2: Arc<InternedIngredient<String>> =
        Arc::new(InternedIngredient::new(QueryKindId(1), "Strings"));
    db2.register(strings2.clone());

    let loaded = load_cache(&cache_path, db2.runtime(), &[&*strings2])
        .await
        .unwrap();
    assert!(loaded);

    let id3 = strings2.intern("hello".to_string()).unwrap();
    assert_eq!(id3, id1);

    let v3 = strings2.get(&db2, id3).unwrap();
    assert_eq!(v3.as_str(), "hello");

    let id4 = strings2.intern("world".to_string()).unwrap();
    assert_ne!(id4, id1);

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
