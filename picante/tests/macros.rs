use picante::PicanteResult;
use std::sync::atomic::{AtomicUsize, Ordering};

static LEN_CALLS: AtomicUsize = AtomicUsize::new(0);
static SUM_CALLS: AtomicUsize = AtomicUsize::new(0);
static UNIT_CALLS: AtomicUsize = AtomicUsize::new(0);

#[picante::input]
pub struct Text {
    #[key]
    pub key: String,
    pub value: String,
}

#[picante::tracked]
pub async fn len<DB: HasTextIngredient>(db: &DB, text: Text) -> PicanteResult<u64> {
    LEN_CALLS.fetch_add(1, Ordering::Relaxed);
    Ok(text.value(db)?.len() as u64)
}

#[picante::tracked]
pub async fn sum<DB>(db: &DB, x: u32, y: u32) -> u64 {
    let _ = db;
    SUM_CALLS.fetch_add(1, Ordering::Relaxed);
    (x as u64) + (y as u64)
}

#[picante::tracked]
pub async fn unit_key<DB>(db: &DB) -> u64 {
    let _ = db;
    UNIT_CALLS.fetch_add(1, Ordering::Relaxed);
    42
}

#[picante::interned]
pub struct Word {
    pub text: String,
}

#[picante::db(inputs(Text), interned(Word), tracked(len, sum, unit_key))]
struct Db {
    pub config: u32,
    pub enabled: bool,
}

#[tokio::test(flavor = "current_thread")]
async fn macros_basic_flow() -> PicanteResult<()> {
    LEN_CALLS.store(0, Ordering::Relaxed);
    let db = Db::new(123, true);
    assert_eq!(db.config, 123);
    assert!(db.enabled);
    let text = Text::new(&db, "a".into(), "hello".into())?;

    assert_eq!(len(&db, text).await?, 5);
    assert_eq!(len(&db, text).await?, 5);
    assert_eq!(LEN_CALLS.load(Ordering::Relaxed), 1);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn macros_tuple_keys_and_unit_key() -> PicanteResult<()> {
    SUM_CALLS.store(0, Ordering::Relaxed);
    UNIT_CALLS.store(0, Ordering::Relaxed);

    let db = Db::new(0, false);

    assert_eq!(sum(&db, 1, 2).await?, 3);
    assert_eq!(sum(&db, 1, 2).await?, 3);
    assert_eq!(SUM_CALLS.load(Ordering::Relaxed), 1);

    assert_eq!(sum(&db, 2, 3).await?, 5);
    assert_eq!(sum(&db, 2, 3).await?, 5);
    assert_eq!(SUM_CALLS.load(Ordering::Relaxed), 2);

    assert_eq!(unit_key(&db).await?, 42);
    assert_eq!(unit_key(&db).await?, 42);
    assert_eq!(UNIT_CALLS.load(Ordering::Relaxed), 1);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn macros_interned_works() -> PicanteResult<()> {
    let db = Db::new(0, false);

    let w1 = Word::new(&db, "hello".to_string())?;
    let w2 = Word::new(&db, "hello".to_string())?;
    assert_eq!(w1, w2);
    assert_eq!(w1.text(&db)?, "hello".to_string());

    Ok(())
}

mod db_paths {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CALLS: AtomicUsize = AtomicUsize::new(0);

    #[picante::input]
    pub struct Text2 {
        #[key]
        pub key: String,
        pub value: String,
    }

    #[picante::tracked]
    pub async fn len2<DB: HasText2Ingredient>(db: &DB, text: Text2) -> PicanteResult<u64> {
        CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(text.value(db)?.len() as u64)
    }

    #[picante::interned]
    pub struct Word2 {
        pub text: String,
    }

    #[picante::db(inputs(self::Text2), interned(self::Word2), tracked(self::len2))]
    pub struct Db2 {}

    #[tokio::test(flavor = "current_thread")]
    async fn db_macro_accepts_paths() -> PicanteResult<()> {
        CALLS.store(0, Ordering::Relaxed);
        let db = Db2::new();
        let text = Text2::new(&db, "a".into(), "hello".into())?;

        assert_eq!(len2(&db, text).await?, 5);
        assert_eq!(len2(&db, text).await?, 5);
        assert_eq!(CALLS.load(Ordering::Relaxed), 1);

        let w1 = Word2::new(&db, "hello".to_string())?;
        let w2 = Word2::new(&db, "hello".to_string())?;
        assert_eq!(w1, w2);

        Ok(())
    }
}

/// Tests for the combined db trait feature (#7)
mod db_trait {
    use picante::PicanteResult;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static QUERY_CALLS: AtomicUsize = AtomicUsize::new(0);

    #[picante::input]
    pub struct Item {
        #[key]
        pub id: u32,
        pub name: String,
    }

    #[picante::interned]
    pub struct Tag {
        pub label: String,
    }

    // Tracked functions use the combined trait to access inputs/interned
    #[picante::tracked]
    pub async fn process_item<DB: DatabaseTrait>(db: &DB, item: Item) -> PicanteResult<String> {
        QUERY_CALLS.fetch_add(1, Ordering::Relaxed);
        let name = item.name(db)?;
        // We can access Tag via the combined trait
        let _tag = Tag::new(db, format!("tag-{}", name))?;
        Ok(format!("processed: {}", name))
    }

    // Default trait name: {DbName}Trait
    #[picante::db(inputs(Item), interned(Tag), tracked(process_item))]
    pub struct Database {}

    // Generic function that operates on data sources (inputs + interned)
    // doesn't need query access - just reads/creates data
    fn create_tagged_item<DB: DatabaseTrait>(
        db: &DB,
        id: u32,
        name: &str,
    ) -> PicanteResult<(Item, Tag)> {
        let item = Item::new(db, id, name.to_string())?;
        let tag = Tag::new(db, format!("tag-{}", name))?;
        Ok((item, tag))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn combined_trait_works() -> PicanteResult<()> {
        QUERY_CALLS.store(0, Ordering::Relaxed);

        let db = Database::new();

        // Use the generic function that uses DatabaseTrait bound
        let (item, tag) = create_tagged_item(&db, 1, "foo")?;
        assert_eq!(item.name(&db)?, "foo");
        assert_eq!(tag.label(&db)?, "tag-foo");

        // Call the tracked query (uses HasProcessItemQuery bound internally)
        let result = process_item(&db, item).await?;
        assert_eq!(result, "processed: foo");
        assert_eq!(QUERY_CALLS.load(Ordering::Relaxed), 1);

        // Cached on second call
        let result = process_item(&db, item).await?;
        assert_eq!(result, "processed: foo");
        assert_eq!(QUERY_CALLS.load(Ordering::Relaxed), 1);

        Ok(())
    }

    // Test custom trait name via db_trait()
    mod custom_name {
        use picante::PicanteResult;

        #[picante::input]
        pub struct Entry {
            #[key]
            pub key: String,
            pub value: String,
        }

        #[picante::tracked]
        pub async fn get_value<DB: Db>(db: &DB, entry: Entry) -> PicanteResult<String> {
            Ok(entry.value(db)?.to_string())
        }

        // Custom trait name: Db
        #[picante::db(inputs(Entry), tracked(get_value), db_trait(Db))]
        pub struct AppDatabase {}

        // Function using the custom trait name for data access
        fn create_entry<DB: Db>(db: &DB, key: &str, value: &str) -> PicanteResult<Entry> {
            Entry::new(db, key.to_string(), value.to_string())
        }

        #[tokio::test(flavor = "current_thread")]
        async fn custom_trait_name_works() -> PicanteResult<()> {
            let db = AppDatabase::new();

            // Use generic function with custom Db trait
            let entry = create_entry(&db, "test-key", "test-value")?;
            assert_eq!(entry.key(&db)?.as_ref(), "test-key");

            // Call tracked query
            let value = get_value(&db, entry).await?;
            assert_eq!(value, "test-value");

            Ok(())
        }
    }
}

/// Tests for singleton inputs (no #[key] field)
mod singleton {
    use picante::PicanteResult;

    /// A singleton config - no #[key] field means only one instance
    #[picante::input]
    pub struct Config {
        pub debug: bool,
        pub timeout: u64,
    }

    // Test singleton with tracked query
    #[picante::tracked]
    pub async fn get_timeout<DB: DatabaseTrait>(db: &DB) -> PicanteResult<Option<u64>> {
        Config::timeout(db)
    }

    #[picante::db(inputs(Config), tracked(get_timeout))]
    pub struct Database {}

    #[tokio::test(flavor = "current_thread")]
    async fn singleton_set_and_get() -> PicanteResult<()> {
        let db = Database::new();

        // Initially not set
        assert!(Config::get(&db)?.is_none());
        assert!(Config::debug(&db)?.is_none());

        // Set the singleton
        Config::set(&db, true, 30)?;

        // Now it's set
        let config = Config::get(&db)?.expect("config should be set");
        assert!(config.debug);
        assert_eq!(config.timeout, 30);

        // Field accessors work
        assert_eq!(Config::debug(&db)?, Some(true));
        assert_eq!(Config::timeout(&db)?, Some(30));

        // Update the singleton
        Config::set(&db, false, 60)?;
        assert_eq!(Config::debug(&db)?, Some(false));
        assert_eq!(Config::timeout(&db)?, Some(60));

        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn singleton_with_tracked() -> PicanteResult<()> {
        let db = Database::new();

        // Query returns None when not set
        assert_eq!(get_timeout(&db).await?, None);

        // Set and query
        Config::set(&db, true, 42)?;
        assert_eq!(get_timeout(&db).await?, Some(42));

        Ok(())
    }
}
