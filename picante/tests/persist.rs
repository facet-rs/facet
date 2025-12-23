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

impl TestDb {
    fn persistable_ingredients(&self) -> Vec<&dyn picante::persist::PersistableIngredient> {
        self.ingredients.persistable_ingredients()
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
async fn load_cache_kind_name_mismatch_is_warning() {
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

    // Kind name mismatches now produce warnings but still load the cache
    // This allows for ingredient renames without invalidating the cache
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

    assert!(ok);
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

// Test using the convenience methods generated by #[picante::db] macro

use picante::PicanteResult;

#[picante::input]
pub struct DbTestText {
    #[key]
    pub key: String,
    pub value: String,
}

#[picante::interned]
pub struct DbTestWord {
    pub text: String,
}

#[picante::tracked]
pub async fn db_test_len<DB: HasDbTestTextIngredient>(
    db: &DB,
    text: DbTestText,
) -> PicanteResult<u64> {
    Ok(text.value(db)?.len() as u64)
}

#[picante::db(inputs(DbTestText), interned(DbTestWord), tracked(db_test_len))]
struct ConvenienceDb {}

#[tokio::test]
async fn save_and_load_with_convenience_methods() -> PicanteResult<()> {
    init_tracing();

    let cache_path = temp_file("picante-convenience-cache.bin");

    // Create db and populate it
    let db = ConvenienceDb::new();

    // Add some input data
    let text1 = DbTestText::new(&db, "hello".to_string(), "world".to_string())?;
    let text2 = DbTestText::new(&db, "foo".to_string(), "bar".to_string())?;

    // Intern some words
    let word1 = DbTestWord::new(&db, "test".to_string())?;
    let word2 = DbTestWord::new(&db, "example".to_string())?;

    // Execute some queries
    let len1 = db_test_len(&db, text1).await?;
    assert_eq!(len1, 5); // "world".len()

    // Save using convenience method
    db.save_to_cache(&cache_path).await?;

    // Create a new database and load the cache
    let db2 = ConvenienceDb::new();
    let loaded = db2.load_from_cache(&cache_path).await?;
    assert!(loaded);

    // Verify data was restored - the values should be in the cache now
    assert_eq!(text1.value(&db2)?, "world");
    assert_eq!(text2.value(&db2)?, "bar");

    // Verify interned values
    let word1_restored = DbTestWord::new(&db2, "test".to_string())?;
    let word2_restored = DbTestWord::new(&db2, "example".to_string())?;
    assert_eq!(word1, word1_restored);
    assert_eq!(word2, word2_restored);

    // Verify cached query results
    let len1_cached = db_test_len(&db2, text1).await?;
    assert_eq!(len1_cached, 5);

    let _ = tokio::fs::remove_file(&cache_path).await;
    Ok(())
}

#[tokio::test]
async fn save_with_options_using_convenience_methods() -> PicanteResult<()> {
    init_tracing();

    let cache_path = temp_file("picante-convenience-options-cache.bin");

    let db = ConvenienceDb::new();

    // Add lots of data
    for i in 0..100u32 {
        DbTestText::new(&db, format!("k{i}"), "x".repeat(50))?;
    }

    // Save with size limit
    db.save_to_cache_with_options(
        &cache_path,
        &CacheSaveOptions {
            max_bytes: Some(2048),
            max_records_per_section: None,
            max_record_bytes: None,
        },
    )
    .await?;

    let bytes = tokio::fs::read(&cache_path).await.unwrap();
    assert!(bytes.len() <= 2048);

    let _ = tokio::fs::remove_file(&cache_path).await;
    Ok(())
}

#[tokio::test]
async fn load_with_options_using_convenience_methods() -> PicanteResult<()> {
    init_tracing();

    let cache_path = temp_file("picante-convenience-load-options.bin");

    // Write corrupt cache
    tokio::fs::write(&cache_path, b"not a valid cache")
        .await
        .unwrap();

    let db = ConvenienceDb::new();

    // Load with ignore on corrupt
    let loaded = db
        .load_from_cache_with_options(
            &cache_path,
            &CacheLoadOptions {
                max_bytes: None,
                on_corrupt: OnCorruptCache::Ignore,
            },
        )
        .await?;

    assert!(!loaded);

    let _ = tokio::fs::remove_file(&cache_path).await;
    Ok(())
}

#[tokio::test]
async fn persistable_ingredients_returns_all_ingredients() -> PicanteResult<()> {
    init_tracing();

    let db = ConvenienceDb::new();
    let ingredients = db.persistable_ingredients();

    // Should have: 2 input ingredients (keys + data), 1 interned, 1 tracked = 4 total
    assert_eq!(ingredients.len(), 4);

    // Verify they have the expected kinds
    let kind_names: Vec<&str> = ingredients.iter().map(|i| i.kind_name()).collect();
    assert!(kind_names.contains(&"DbTestText::keys"));
    assert!(kind_names.contains(&"DbTestText::data"));
    assert!(kind_names.contains(&"DbTestWord"));
    assert!(kind_names.contains(&"db_test_len"));
    Ok(())
}

#[tokio::test]
async fn test_wal_incremental_persistence() {
    use picante::persist::{append_to_wal, compact_wal, replay_wal};
    use picante::wal::WalWriter;

    init_tracing();

    let cache_path = temp_file("wal-test-cache.bin");
    let wal_path = temp_file("wal-test.wal");

    // Create database and add initial data
    let mut db = TestDb::default();
    let text: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    let numbers: Arc<InputIngredient<u64, u64>> =
        Arc::new(InputIngredient::new(QueryKindId(2), "Numbers"));

    db.ingredients.register(text.clone());
    db.ingredients.register(numbers.clone());

    // Set initial values (revision 1)
    text.set(&db, "a".into(), "hello".into());
    text.set(&db, "b".into(), "world".into());
    numbers.set(&db, 1, 100);
    numbers.set(&db, 2, 200);

    assert_eq!(db.runtime.current_revision().0, 4);

    // Save snapshot at revision 4
    let ingredients = db.persistable_ingredients();
    picante::persist::save_cache(&cache_path, &db.runtime, &ingredients)
        .await
        .unwrap();

    // Create WAL at base revision 4
    let mut wal = WalWriter::create(&wal_path, 4).unwrap();

    // Make more changes (revisions 5-8)
    text.set(&db, "a".into(), "goodbye".into()); // revision 5
    text.set(&db, "c".into(), "new".into()); // revision 6
    numbers.set(&db, 1, 101); // revision 7
    numbers.set(&db, 3, 300); // revision 8

    assert_eq!(db.runtime.current_revision().0, 8);

    // Append changes to WAL
    let count = append_to_wal(&mut wal, &db.runtime, &ingredients)
        .await
        .unwrap();
    assert_eq!(count, 4); // 4 changes since revision 4
    wal.flush().unwrap();
    drop(wal);

    // Create a new database and load snapshot + WAL
    let mut db2 = TestDb::default();
    let text2: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    let numbers2: Arc<InputIngredient<u64, u64>> =
        Arc::new(InputIngredient::new(QueryKindId(2), "Numbers"));

    db2.ingredients.register(text2.clone());
    db2.ingredients.register(numbers2.clone());

    let ingredients2 = db2.persistable_ingredients();

    // Load base snapshot
    let loaded = picante::persist::load_cache(&cache_path, &db2.runtime, &ingredients2)
        .await
        .unwrap();
    assert!(loaded);
    assert_eq!(db2.runtime.current_revision().0, 4);

    // Verify snapshot loaded correctly
    assert_eq!(text2.get(&db2, &"a".into()).unwrap(), Some("hello".into()));
    assert_eq!(text2.get(&db2, &"b".into()).unwrap(), Some("world".into()));
    assert_eq!(text2.get(&db2, &"c".into()).unwrap(), None);
    assert_eq!(numbers2.get(&db2, &1).unwrap(), Some(100));
    assert_eq!(numbers2.get(&db2, &2).unwrap(), Some(200));
    assert_eq!(numbers2.get(&db2, &3).unwrap(), None);

    // Replay WAL
    let replayed = replay_wal(&wal_path, &db2.runtime, &ingredients2)
        .await
        .unwrap();
    assert_eq!(replayed, 4);
    assert_eq!(db2.runtime.current_revision().0, 8);

    // Verify WAL changes were applied
    assert_eq!(
        text2.get(&db2, &"a".into()).unwrap(),
        Some("goodbye".into())
    ); // updated
    assert_eq!(text2.get(&db2, &"b".into()).unwrap(), Some("world".into())); // unchanged
    assert_eq!(text2.get(&db2, &"c".into()).unwrap(), Some("new".into())); // new
    assert_eq!(numbers2.get(&db2, &1).unwrap(), Some(101)); // updated
    assert_eq!(numbers2.get(&db2, &2).unwrap(), Some(200)); // unchanged
    assert_eq!(numbers2.get(&db2, &3).unwrap(), Some(300)); // new

    // Test compaction (delete old WAL, don't create new one)
    let new_revision = compact_wal(
        &cache_path,
        &wal_path,
        &db.runtime,
        &ingredients,
        &CacheSaveOptions::default(),
        false, // don't create new WAL after compaction
    )
    .await
    .unwrap();
    assert_eq!(new_revision, 8);

    // WAL should be deleted after compaction
    assert!(!wal_path.exists());

    // New snapshot should have all data
    let mut db3 = TestDb::default();
    let text3: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    let numbers3: Arc<InputIngredient<u64, u64>> =
        Arc::new(InputIngredient::new(QueryKindId(2), "Numbers"));

    db3.ingredients.register(text3.clone());
    db3.ingredients.register(numbers3.clone());

    let ingredients3 = db3.persistable_ingredients();
    let loaded = picante::persist::load_cache(&cache_path, &db3.runtime, &ingredients3)
        .await
        .unwrap();
    assert!(loaded);
    assert_eq!(db3.runtime.current_revision().0, 8);

    // All data should be in the snapshot
    assert_eq!(
        text3.get(&db3, &"a".into()).unwrap(),
        Some("goodbye".into())
    );
    assert_eq!(text3.get(&db3, &"b".into()).unwrap(), Some("world".into()));
    assert_eq!(text3.get(&db3, &"c".into()).unwrap(), Some("new".into()));
    assert_eq!(numbers3.get(&db3, &1).unwrap(), Some(101));
    assert_eq!(numbers3.get(&db3, &2).unwrap(), Some(200));
    assert_eq!(numbers3.get(&db3, &3).unwrap(), Some(300));
}
