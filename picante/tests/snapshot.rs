use picante::PicanteResult;

#[picante::input]
pub struct Item {
    #[key]
    pub id: u32,
    pub value: String,
}

#[picante::interned]
pub struct Label {
    pub text: String,
}

#[picante::tracked]
pub async fn item_length<DB: DatabaseTrait>(db: &DB, item: Item) -> PicanteResult<u64> {
    Ok(item.value(db)?.len() as u64)
}

/// Singleton input (no key) — like dodeca's `SourceRegistry`.
#[picante::input]
pub struct Config {
    pub setting: String,
}

/// A *no-argument* tracked query reading a singleton input (keyed by `()`),
/// mirroring dodeca's `build_tree`.
#[picante::tracked]
pub async fn config_len<DB: DatabaseTrait>(db: &DB) -> PicanteResult<u64> {
    Ok(Config::setting(db)?.unwrap_or_default().len() as u64)
}

#[picante::db(
    inputs(Item, Config, Doc, Registry),
    interned(Label),
    tracked(item_length, config_len, item_length_via, doc_body_len, total_len)
)]
pub struct Database {}

#[tokio_test_lite::test]
async fn snapshot_sees_data_at_snapshot_time() -> PicanteResult<()> {
    let db = Database::new();

    // Create an item
    let item = Item::new(&db, 1, "hello".into())?;
    assert_eq!(item.value(&db)?, "hello".to_string());

    // Query it (will compute and cache)
    let len = item_length(&db, item).await?;
    assert_eq!(len, 5);

    // Create a snapshot
    let snapshot = DatabaseSnapshot::from_database(&db).await;

    // Snapshot sees the same item data
    assert_eq!(item.value(&snapshot)?, "hello".to_string());

    // Query on snapshot returns same result
    let len_snapshot = item_length(&snapshot, item).await?;
    assert_eq!(len_snapshot, 5);

    // Now modify the database
    let _item2 = Item::new(&db, 1, "hello world".into())?;

    // Database sees the new value
    assert_eq!(item.value(&db)?, "hello world".to_string());

    // Snapshot still sees the old value
    assert_eq!(item.value(&snapshot)?, "hello".to_string());

    // Query on database returns new length
    let len_new = item_length(&db, item).await?;
    assert_eq!(len_new, 11);

    // Query on snapshot still returns old length
    let len_snapshot2 = item_length(&snapshot, item).await?;
    assert_eq!(len_snapshot2, 5);

    Ok(())
}

#[tokio_test_lite::test]
async fn snapshot_shares_interned_values() -> PicanteResult<()> {
    let db = Database::new();

    // Intern a label
    let label = Label::new(&db, "tag".into())?;
    assert_eq!(label.text(&db)?, "tag".to_string());

    // Create snapshot
    let snapshot = DatabaseSnapshot::from_database(&db).await;

    // Snapshot can look up the same interned value
    assert_eq!(label.text(&snapshot)?, "tag".to_string());

    // New interned values after snapshot are still visible (append-only)
    let label2 = Label::new(&db, "new-tag".into())?;
    assert_eq!(label2.text(&snapshot)?, "new-tag".to_string());

    Ok(())
}

#[tokio_test_lite::test]
async fn snapshot_can_compute_new_queries() -> PicanteResult<()> {
    let db = Database::new();

    // Create items
    let item1 = Item::new(&db, 1, "foo".into())?;
    let item2 = Item::new(&db, 2, "bar".into())?;

    // Only compute item1 on database
    let len1 = item_length(&db, item1).await?;
    assert_eq!(len1, 3);

    // Create snapshot
    let snapshot = DatabaseSnapshot::from_database(&db).await;

    // item1 returns same result on snapshot
    let len1_snap = item_length(&snapshot, item1).await?;
    assert_eq!(len1_snap, 3);

    // item2 can be computed on snapshot
    let len2_snap = item_length(&snapshot, item2).await?;
    assert_eq!(len2_snap, 3);

    // item2 can also be computed on database (independent caches)
    let len2_db = item_length(&db, item2).await?;
    assert_eq!(len2_db, 3);

    Ok(())
}

#[tokio_test_lite::test]
async fn multiple_snapshots_are_independent() -> PicanteResult<()> {
    let db = Database::new();

    // Initial state
    let item = Item::new(&db, 1, "v1".into())?;
    let snap1 = DatabaseSnapshot::from_database(&db).await;

    // Modify and create another snapshot
    let _ = Item::new(&db, 1, "v2".into())?;
    let snap2 = DatabaseSnapshot::from_database(&db).await;

    // Modify again
    let _ = Item::new(&db, 1, "v3".into())?;

    // Each sees their respective version
    assert_eq!(item.value(&snap1)?, "v1".to_string());
    assert_eq!(item.value(&snap2)?, "v2".to_string());
    assert_eq!(item.value(&db)?, "v3".to_string());

    Ok(())
}

/// Overriding an input on a snapshot must invalidate the snapshot's deep-cloned
/// memo for derived queries that depend on it — otherwise "what-if" overlays on
/// a snapshot (e.g. an editor preview) silently render stale results.
#[tokio_test_lite::test]
async fn snapshot_input_override_invalidates_derived_query() -> PicanteResult<()> {
    let db = Database::new();
    let item = Item::new(&db, 1, "hello".into())?; // len 5
    assert_eq!(item_length(&db, item).await?, 5); // memoize on db

    let snapshot = DatabaseSnapshot::from_database(&db).await; // deep-clones the memo
    assert_eq!(item_length(&snapshot, item).await?, 5);

    // Override the input ON THE SNAPSHOT (isolated from db).
    Item::new(&snapshot, 1, "hello world!!!".into())?; // len 14

    // The input itself reflects the override...
    assert_eq!(item.value(&snapshot)?, "hello world!!!".to_string());
    // ...and so must the derived query (this is the one that currently fails).
    assert_eq!(item_length(&snapshot, item).await?, 14);

    // The host db is untouched.
    assert_eq!(item_length(&db, item).await?, 5);
    Ok(())
}

/// A *no-argument* tracked query over a singleton input must invalidate on a
/// snapshot when the singleton is overridden — this is what dodeca's build_tree
/// (over SourceRegistry) needs for editor previews.
#[tokio_test_lite::test]
async fn snapshot_override_invalidates_singleton_query() -> PicanteResult<()> {
    let db = Database::new();
    Config::set(&db, "hello".into())?; // len 5
    assert_eq!(config_len(&db).await?, 5); // memoize on db

    let snapshot = DatabaseSnapshot::from_database(&db).await; // deep-clones memo
    assert_eq!(config_len(&snapshot).await?, 5);

    // Override the singleton ON THE SNAPSHOT.
    Config::set(&snapshot, "much longer value".into())?; // len 17

    assert_eq!(
        Config::setting(&snapshot)?,
        Some("much longer value".into())
    );
    assert_eq!(config_len(&snapshot).await?, 17); // the query must recompute
    assert_eq!(config_len(&db).await?, 5); // host untouched
    Ok(())
}

/// A tracked query that calls ANOTHER tracked query (not just an input) must
/// also invalidate on a snapshot override. dodeca's build_tree calls parse_file,
/// which reads the source content — this is the transitive case.
#[picante::tracked]
pub async fn item_length_via<DB: DatabaseTrait>(db: &DB, item: Item) -> PicanteResult<u64> {
    item_length(db, item).await
}

#[tokio_test_lite::test]
async fn snapshot_override_invalidates_transitive_query() -> PicanteResult<()> {
    let db = Database::new();
    let item = Item::new(&db, 1, "hello".into())?;
    assert_eq!(item_length_via(&db, item).await?, 5); // memoize BOTH layers on db

    let snapshot = DatabaseSnapshot::from_database(&db).await;
    assert_eq!(item_length_via(&snapshot, item).await?, 5);

    Item::new(&snapshot, 1, "hello world!!!".into())?; // len 14

    // inner query recomputes...
    assert_eq!(item_length(&snapshot, item).await?, 14);
    // ...and so must the outer query that calls it (the dodeca case).
    assert_eq!(item_length_via(&snapshot, item).await?, 14);
    Ok(())
}

// ---- Exact dodeca shape: registry of keyed entities + iterate + sub-query ----
#[picante::input]
pub struct Doc {
    #[key]
    pub path: String,
    pub body: String,
}

#[picante::input]
pub struct Registry {
    pub docs: Vec<Doc>,
}

#[picante::tracked]
pub async fn doc_body_len<DB: DatabaseTrait>(db: &DB, doc: Doc) -> PicanteResult<u64> {
    Ok(doc.body(db)?.len() as u64)
}

/// build_tree analog: iterate a singleton registry, call a sub-query per entity.
#[picante::tracked]
pub async fn total_len<DB: DatabaseTrait>(db: &DB) -> PicanteResult<u64> {
    let docs = Registry::docs(db)?.unwrap_or_default();
    let mut total = 0;
    for doc in docs {
        total += doc_body_len(db, doc).await?;
    }
    Ok(total)
}

#[tokio_test_lite::test]
async fn snapshot_override_registry_entity_invalidates_outer() -> PicanteResult<()> {
    let db = Database::new();
    let doc = Doc::new(&db, "a".into(), "xx".into())?; // len 2
    Registry::set(&db, vec![doc])?;
    assert_eq!(total_len(&db).await?, 2); // memoize on db

    let snapshot = DatabaseSnapshot::from_database(&db).await;
    assert_eq!(total_len(&snapshot).await?, 2);

    // Override the doc's content (same key) and re-set the registry — exactly
    // what dodeca's preview_overlay does.
    let doc2 = Doc::new(&snapshot, "a".into(), "yyyy".into())?; // len 4
    Registry::set(&snapshot, vec![doc2])?;

    assert_eq!(total_len(&snapshot).await?, 4); // must recompute (4, not stale 2)
    assert_eq!(total_len(&db).await?, 2); // host untouched
    Ok(())
}

/// Two snapshots derived from the same db share a RuntimeId (for inflight dedup)
/// but have independent revision counters starting from the same base. A query
/// result computed in snapshot A must NOT leak into snapshot B and make B's own
/// override look stale. This mirrors dodeca's editor: an auto-preview snapshot
/// followed by a user-edit snapshot.
#[tokio_test_lite::test]
async fn two_snapshots_independent_overrides() -> PicanteResult<()> {
    let db = Database::new();
    Config::set(&db, "base".into())?; // len 4
    assert_eq!(config_len(&db).await?, 4); // compute on host

    // Snapshot A: override + compute (this is the "auto-preview").
    let snap_a = DatabaseSnapshot::from_database(&db).await;
    Config::set(&snap_a, "AAAAAAAAAAAAAAAAAAAA".into())?; // len 20
    assert_eq!(config_len(&snap_a).await?, 20);

    // Snapshot B: a DIFFERENT override. Must reflect B's value, not A's.
    let snap_b = DatabaseSnapshot::from_database(&db).await;
    Config::set(&snap_b, "BBB".into())?; // len 3
    assert_eq!(
        config_len(&snap_b).await?,
        3,
        "snapshot B must see its own override, not snapshot A's leaked result"
    );
    Ok(())
}
