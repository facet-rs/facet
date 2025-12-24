//! Tests for in-flight query deduplication across concurrent snapshots.
//!
//! These tests verify that concurrent requests for the same tracked query
//! with identical parameters coalesce into a single computation.

use futures_util::future;
use picante::PicanteResult;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[picante::input]
pub struct Config {
    #[key]
    pub id: u32,
    pub value: u32,
}

/// Shared counter to track how many times the compute function runs.
static SLOW_COMPUTE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// A tracked query that intentionally takes time to compute.
/// Used to create a window where concurrent requests can overlap.
#[picante::tracked]
pub async fn slow_compute<DB: DatabaseTrait>(db: &DB, config: Config) -> PicanteResult<u64> {
    SLOW_COMPUTE_COUNT.fetch_add(1, Ordering::SeqCst);

    // Simulate expensive computation
    tokio::time::sleep(Duration::from_millis(50)).await;

    let value = config.value(db)?;
    Ok(value as u64 * 2)
}

#[picante::db(inputs(Config), tracked(slow_compute))]
pub struct Database {}

/// Test that concurrent queries from different snapshots coalesce into a single computation.
///
/// This test reproduces the bug described in issue #27: when multiple concurrent tasks
/// each create a DatabaseSnapshot and query the same tracked function with the same key,
/// the computation runs once per snapshot instead of being shared.
#[tokio_test_lite::test]
async fn concurrent_snapshot_queries_should_deduplicate() -> PicanteResult<()> {
    // Reset the counter
    SLOW_COMPUTE_COUNT.store(0, Ordering::SeqCst);

    let db = Arc::new(Database::new());

    // Create an input
    let config = Config::new(&*db, 1, 42)?;

    // Number of concurrent tasks
    let num_tasks = 10;

    // Create all snapshots upfront (we need to do this because from_database takes &db)
    let mut snapshots = Vec::new();
    for _ in 0..num_tasks {
        snapshots.push(Arc::new(DatabaseSnapshot::from_database(&db).await));
    }

    // Spawn concurrent tasks that query the same key on different snapshots
    let handles: Vec<_> = snapshots
        .into_iter()
        .map(|snapshot| {
            tokio::spawn(async move {
                // All tasks query the same key
                slow_compute(&*snapshot, config).await
            })
        })
        .collect();

    // Wait for all tasks to complete
    let results: Vec<_> = future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("task panicked"))
        .collect();

    // All tasks should get the same result
    for result in &results {
        let value = result.as_ref().expect("query failed");
        assert_eq!(*value, 84, "unexpected result");
    }

    // The compute function should have run exactly once, not once per task
    let compute_count = SLOW_COMPUTE_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        compute_count, 1,
        "expected compute to run exactly once, but it ran {} times",
        compute_count
    );

    Ok(())
}

/// Control test: verify that within a single DB instance, the existing Running
/// state deduplication already works.
#[tokio_test_lite::test]
async fn single_db_queries_already_deduplicate() -> PicanteResult<()> {
    // Use a separate counter for this test
    static SINGLE_DB_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[picante::input]
    pub struct SingleConfig {
        #[key]
        pub id: u32,
        pub value: u32,
    }

    #[picante::tracked]
    pub async fn single_slow_compute<DB: SingleDatabaseTrait>(
        db: &DB,
        config: SingleConfig,
    ) -> PicanteResult<u64> {
        SINGLE_DB_COUNT.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let value = config.value(db)?;
        Ok(value as u64 * 2)
    }

    #[picante::db(inputs(SingleConfig), tracked(single_slow_compute))]
    pub struct SingleDatabase {}

    SINGLE_DB_COUNT.store(0, Ordering::SeqCst);

    let db = Arc::new(SingleDatabase::new());

    let config = SingleConfig::new(&*db, 1, 42)?;

    let num_tasks = 10;

    // All tasks share the same DB instance
    let handles: Vec<_> = (0..num_tasks)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move { single_slow_compute(&*db, config).await })
        })
        .collect();

    let results: Vec<_> = future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("task panicked"))
        .collect();

    for result in &results {
        let value = result.as_ref().expect("query failed");
        assert_eq!(*value, 84);
    }

    // With a single DB, the Running state should coalesce concurrent queries
    let compute_count = SINGLE_DB_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        compute_count, 1,
        "single DB: expected compute to run exactly once, but it ran {} times",
        compute_count
    );

    Ok(())
}

/// Test that when the leader task is cancelled, followers can retry and one
/// becomes the new leader to complete the computation.
#[tokio_test_lite::test]
async fn cancellation_allows_follower_retry() -> PicanteResult<()> {
    static CANCEL_COMPUTE_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[picante::input]
    pub struct CancelConfig {
        #[key]
        pub id: u32,
        pub value: u32,
    }

    #[picante::tracked]
    pub async fn cancel_slow_compute<DB: CancelDatabaseTrait>(
        db: &DB,
        config: CancelConfig,
    ) -> PicanteResult<u64> {
        let count = CANCEL_COMPUTE_COUNT.fetch_add(1, Ordering::SeqCst);

        // First computation will be cancelled
        tokio::time::sleep(Duration::from_millis(100)).await;

        let value = config.value(db)?;
        Ok(value as u64 * 2 + count as u64)
    }

    #[picante::db(inputs(CancelConfig), tracked(cancel_slow_compute))]
    pub struct CancelDatabase {}

    CANCEL_COMPUTE_COUNT.store(0, Ordering::SeqCst);

    let db = Arc::new(CancelDatabase::new());
    let config = CancelConfig::new(&*db, 1, 42)?;

    // Create snapshots for leader and followers
    let leader_snapshot = Arc::new(CancelDatabaseSnapshot::from_database(&db).await);
    let follower_snapshots: Vec<_> = (0..3)
        .map(|_| {
            let db = db.clone();
            async move { Arc::new(CancelDatabaseSnapshot::from_database(&db).await) }
        })
        .collect::<Vec<_>>();

    let mut follower_snapshots_vec = Vec::new();
    for f in follower_snapshots {
        follower_snapshots_vec.push(f.await);
    }

    // Start the leader task
    let leader_handle = {
        let snapshot = leader_snapshot.clone();
        tokio::spawn(async move { cancel_slow_compute(&*snapshot, config).await })
    };

    // Wait a bit for the leader to start computing
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Start follower tasks
    let follower_handles: Vec<_> = follower_snapshots_vec
        .into_iter()
        .map(|snapshot| tokio::spawn(async move { cancel_slow_compute(&*snapshot, config).await }))
        .collect();

    // Wait a bit more, then cancel the leader
    tokio::time::sleep(Duration::from_millis(20)).await;
    leader_handle.abort();

    // Wait for followers to complete
    let results: Vec<_> = future::join_all(follower_handles)
        .await
        .into_iter()
        .map(|r| r.expect("follower task panicked"))
        .collect();

    // All followers should get a successful result
    for result in &results {
        let value = result.as_ref().expect("query failed");
        // Value should be 84 + some small offset from retries
        assert!(*value >= 84, "unexpected result: {}", value);
    }

    // Compute should have been called at least twice:
    // 1. Initial leader (cancelled)
    // 2. New leader after cancellation
    // Could be more if there were races, but should be bounded.
    let compute_count = CANCEL_COMPUTE_COUNT.load(Ordering::SeqCst);
    assert!(
        (2..=5).contains(&compute_count),
        "expected compute to run 2-5 times (1 cancelled + 1 successful + possible races), but it ran {} times",
        compute_count
    );

    Ok(())
}

/// Test that errors from the leader are propagated to followers.
#[tokio_test_lite::test]
async fn error_propagation_to_followers() -> PicanteResult<()> {
    static ERROR_COMPUTE_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[picante::input]
    pub struct ErrorConfig {
        #[key]
        pub id: u32,
        pub should_fail: bool,
    }

    #[picante::tracked]
    pub async fn error_compute<DB: ErrorDatabaseTrait>(
        db: &DB,
        config: ErrorConfig,
    ) -> PicanteResult<u64> {
        ERROR_COMPUTE_COUNT.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(50)).await;

        if config.should_fail(db)? {
            Err(std::sync::Arc::new(picante::PicanteError::Panic {
                message: "intentional test error".to_string(),
            }))
        } else {
            Ok(42)
        }
    }

    #[picante::db(inputs(ErrorConfig), tracked(error_compute))]
    pub struct ErrorDatabase {}

    ERROR_COMPUTE_COUNT.store(0, Ordering::SeqCst);

    let db = Arc::new(ErrorDatabase::new());
    let config = ErrorConfig::new(&*db, 1, true)?;

    // Create snapshots
    let mut snapshots = Vec::new();
    for _ in 0..5 {
        snapshots.push(Arc::new(ErrorDatabaseSnapshot::from_database(&db).await));
    }

    // Spawn concurrent tasks
    let handles: Vec<_> = snapshots
        .into_iter()
        .map(|snapshot| tokio::spawn(async move { error_compute(&*snapshot, config).await }))
        .collect();

    // Wait for all tasks
    let results: Vec<_> = future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("task panicked"))
        .collect();

    // All tasks should get the same error
    for result in &results {
        assert!(result.is_err(), "expected error, got {:?}", result);
    }

    // Compute should have run exactly once
    let compute_count = ERROR_COMPUTE_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        compute_count, 1,
        "expected compute to run exactly once, but it ran {} times",
        compute_count
    );

    Ok(())
}
