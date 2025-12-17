use picante::PicanteResult;
use std::sync::atomic::{AtomicUsize, Ordering};

#[picante::input]
struct A {
    #[key]
    key: String,
    value: String,
}

#[picante::input]
struct B {
    #[key]
    key: String,
    value: String,
}

#[picante::tracked]
async fn compute_len<DB: Db>(db: &DB, a: A) -> PicanteResult<usize> {
    QUERY_CALLS.fetch_add(1, Ordering::Relaxed);
    let s = a.value(db)?;
    Ok(s.len())
}

#[picante::db(inputs(A, B), tracked(compute_len), db_trait(Db))]
struct Database {}

static QUERY_CALLS: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn shared_cache_reuses_across_snapshots_and_unrelated_revisions() -> PicanteResult<()> {
    // SAFETY: test sets process-global env; tests are single-process but can be parallel.
    // This setting is only a best-effort limit and doesn't affect correctness.
    unsafe { std::env::set_var("PICANTE_SHARED_CACHE_MAX_ENTRIES", "1000") };

    QUERY_CALLS.store(0, Ordering::Relaxed);
    let db = Database::new();

    let a = A::new(&db, "a".to_string(), "hello".to_string())?;
    let _b = B::new(&db, "b".to_string(), "world".to_string())?;

    // Snapshot 1 computes.
    let snap1 = DatabaseSnapshot::from_database(&db).await;
    let v1 = compute_len(&snap1, a).await?;
    assert_eq!(v1, 5);
    assert_eq!(QUERY_CALLS.load(Ordering::Relaxed), 1);

    // Snapshot 2 at the same revision should adopt from the shared cache (no recompute).
    let snap2 = DatabaseSnapshot::from_database(&db).await;
    let v2 = compute_len(&snap2, a).await?;
    assert_eq!(v2, 5);
    assert_eq!(QUERY_CALLS.load(Ordering::Relaxed), 1);

    // Bump an unrelated input (B) to a new revision. The query depends only on A,
    // so revalidation should succeed and adoption should still avoid recompute.
    let _ = B::new(&db, "b".to_string(), "world!!!".to_string())?;

    let snap3 = DatabaseSnapshot::from_database(&db).await;
    let v3 = compute_len(&snap3, a).await?;
    assert_eq!(v3, 5);
    assert_eq!(QUERY_CALLS.load(Ordering::Relaxed), 1);

    Ok(())
}
