use picante::Revision;
use picante::db::{DynIngredient, IngredientLookup, IngredientRegistry};
use picante::error::PicanteError;
use picante::ingredient::{DerivedIngredient, InputIngredient};
use picante::key::QueryKindId;
use picante::persist::{
    CacheFile, CacheLoadOptions, CacheSaveOptions, OnCorruptCache, Section, SectionType,
    load_cache_with_options, save_cache_with_options,
};
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

#[tokio::test]
async fn load_corrupt_cache_ignored() {
    init_tracing();

    let cache_path = temp_file("picante-corrupt-cache.bin");
    tokio::fs::write(&cache_path, b"not a valid cache")
        .await
        .unwrap();

    let db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(2),
        "Len",
        |_db, _key| Box::pin(async { Ok(0) }),
    ));

    let ok = load_cache_with_options(
        &cache_path,
        db.runtime(),
        &[&*input, &*derived],
        &CacheLoadOptions {
            max_bytes: None,
            on_corrupt: OnCorruptCache::Ignore,
        },
    )
    .await
    .unwrap();

    assert!(!ok);
    let _ = tokio::fs::remove_file(&cache_path).await;
}

#[tokio::test]
async fn load_corrupt_cache_deleted() {
    init_tracing();

    let cache_path = temp_file("picante-corrupt-cache-delete.bin");
    tokio::fs::write(&cache_path, b"not a valid cache")
        .await
        .unwrap();

    let db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(2),
        "Len",
        |_db, _key| Box::pin(async { Ok(0) }),
    ));

    let ok = load_cache_with_options(
        &cache_path,
        db.runtime(),
        &[&*input, &*derived],
        &CacheLoadOptions {
            max_bytes: None,
            on_corrupt: OnCorruptCache::Delete,
        },
    )
    .await
    .unwrap();

    assert!(!ok);
    assert!(!cache_path.exists());
}

#[tokio::test]
async fn save_cache_respects_max_bytes() {
    init_tracing();

    let cache_path = temp_file("picante-small-cache.bin");

    let db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));

    // Make the input section quite large.
    for i in 0..200u32 {
        input.set(&db, format!("k{i}"), "x".repeat(50));
    }

    let input_for_derived = input.clone();
    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(2),
        "Len",
        move |db, key| {
            let input = input_for_derived.clone();
            Box::pin(async move {
                let text = input.get(db, &key)?.unwrap_or_default();
                Ok(text.len() as u64)
            })
        },
    ));

    // Populate some derived values too.
    for i in 0..200u32 {
        let _ = derived.get(&db, format!("k{i}")).await.unwrap();
    }

    let max_bytes = 4096;
    save_cache_with_options(
        &cache_path,
        db.runtime(),
        &[&*input, &*derived],
        &CacheSaveOptions {
            max_bytes: Some(max_bytes),
            max_records_per_section: None,
            max_record_bytes: None,
        },
    )
    .await
    .unwrap();

    let bytes = tokio::fs::read(&cache_path).await.unwrap();
    assert!(
        bytes.len() <= max_bytes,
        "cache was {} bytes, expected <= {max_bytes}",
        bytes.len()
    );

    let _ = tokio::fs::remove_file(&cache_path).await;
}

#[tokio::test]
async fn load_cache_respects_max_bytes() {
    init_tracing();

    let cache_path = temp_file("picante-too-big-cache.bin");
    tokio::fs::write(&cache_path, vec![0u8; 16]).await.unwrap();

    let db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(2),
        "Len",
        |_db, _key| Box::pin(async { Ok(0) }),
    ));

    let err = load_cache_with_options(
        &cache_path,
        db.runtime(),
        &[&*input, &*derived],
        &CacheLoadOptions {
            max_bytes: Some(8),
            on_corrupt: OnCorruptCache::Error,
        },
    )
    .await
    .unwrap_err();

    match &*err {
        PicanteError::Cache { message } => {
            assert!(message.contains("cache file too large"));
        }
        other => panic!("expected cache error, got {other:?}"),
    }

    let _ = tokio::fs::remove_file(&cache_path).await;
}

#[tokio::test]
async fn load_cache_ignores_unknown_sections() {
    init_tracing();

    let cache_path = temp_file("picante-unknown-section.bin");

    let cache = CacheFile {
        format_version: 1,
        current_revision: 123,
        sections: vec![
            Section {
                kind_id: 999,
                kind_name: "Unknown".to_string(),
                section_type: SectionType::Input,
                records: vec![b"ignored".to_vec()],
            },
            Section {
                kind_id: 1,
                kind_name: "Text".to_string(),
                section_type: SectionType::Input,
                records: Vec::new(),
            },
        ],
    };

    let bytes = facet_postcard::to_vec(&cache).unwrap();
    tokio::fs::write(&cache_path, bytes).await.unwrap();

    let db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    let derived: Arc<DerivedIngredient<TestDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(2),
        "Len",
        |_db, _key| Box::pin(async { Ok(0) }),
    ));

    let ok = load_cache_with_options(
        &cache_path,
        db.runtime(),
        &[&*input, &*derived],
        &CacheLoadOptions {
            max_bytes: None,
            on_corrupt: OnCorruptCache::Error,
        },
    )
    .await
    .unwrap();

    assert!(ok);
    assert_eq!(db.runtime().current_revision(), Revision(123));

    let _ = tokio::fs::remove_file(&cache_path).await;
}

#[tokio::test]
async fn load_cache_kind_name_mismatch_is_corrupt() {
    init_tracing();

    let cache_path = temp_file("picante-kind-name-mismatch.bin");

    let cache = CacheFile {
        format_version: 1,
        current_revision: 1,
        sections: vec![Section {
            kind_id: 1,
            kind_name: "NotText".to_string(),
            section_type: SectionType::Input,
            records: Vec::new(),
        }],
    };

    let bytes = facet_postcard::to_vec(&cache).unwrap();
    tokio::fs::write(&cache_path, bytes).await.unwrap();

    let db = TestDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));

    let ok = load_cache_with_options(
        &cache_path,
        db.runtime(),
        &[&*input],
        &CacheLoadOptions {
            max_bytes: None,
            on_corrupt: OnCorruptCache::Ignore,
        },
    )
    .await
    .unwrap();

    assert!(!ok);
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
