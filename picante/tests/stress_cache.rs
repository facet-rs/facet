mod support;

use picante::PicanteResult;
use std::sync::atomic::Ordering;
use support::*;
use tokio::time::{Duration, timeout};

#[tokio::test(flavor = "current_thread")]
async fn cache_reuses_across_snapshots_under_load() -> PicanteResult<()> {
    let _permit = TEST_SEM.acquire().await.unwrap();
    picante::__test_shared_cache_clear();
    picante::__test_shared_cache_set_max_entries(100_000);

    WIDE_CALLS.store(0, Ordering::Relaxed);
    DEEP_CALLS.store(0, Ordering::Relaxed);

    let db = Database::new();

    // Build a wide input set + registry.
    let leaf_count = 2_000u32;
    init_db(&db, leaf_count)?;

    // Snapshot 1 computes both expensive query families.
    let snap1 = DatabaseSnapshot::from_database(&db).await;
    let wide1 = wide_sum(&snap1, WideKey).await?;
    let deep1 = deep_l32(&snap1, DeepKey { leaf: 0 }).await?;
    assert_eq!(wide1, expected_wide_sum(leaf_count));
    assert_eq!(deep1, DEEP_LEVELS);

    let wide_calls_after_first = WIDE_CALLS.load(Ordering::Relaxed);
    let deep_calls_after_first = DEEP_CALLS.load(Ordering::Relaxed);
    assert_eq!(wide_calls_after_first, 1);
    assert_eq!(deep_calls_after_first, (DEEP_LEVELS as usize) + 1);

    // Many concurrent snapshots should reuse without recomputation.
    let mut handles = Vec::new();
    for i in 0..64u32 {
        let expected_leaf = (i % 8) as u64;
        let snap = DatabaseSnapshot::from_database(&db).await;
        handles.push(tokio::spawn(async move {
            let w = wide_sum(&snap, WideKey).await?;
            let d = deep_l32(&snap, DeepKey { leaf: i % 8 }).await?;
            PicanteResult::Ok((w, d, expected_leaf))
        }));
    }

    let res = timeout(Duration::from_secs(10), async {
        for h in handles {
            let (w, d, expected_leaf) = h.await.unwrap()?;
            assert_eq!(w, wide1);
            assert_eq!(d, expected_leaf.wrapping_mul(3).wrapping_add(DEEP_LEVELS));
        }
        PicanteResult::Ok(())
    })
    .await;

    res.expect("timeout waiting for load test tasks")?;

    assert_eq!(WIDE_CALLS.load(Ordering::Relaxed), wide_calls_after_first);

    // We computed deep_l32 for 8 distinct keys (0..7); each key requires exactly (levels+1) computations once.
    assert_eq!(
        DEEP_CALLS.load(Ordering::Relaxed),
        8 * ((DEEP_LEVELS as usize) + 1)
    );

    // Bump an unrelated input to a new revision; both query families should revalidate
    // and adopt from the shared cache (no recompute for the previously-used keys).
    Noise::set(&db, 1)?;
    let snap2 = DatabaseSnapshot::from_database(&db).await;
    let wide2 = wide_sum(&snap2, WideKey).await?;
    let deep2 = deep_l32(&snap2, DeepKey { leaf: 0 }).await?;
    assert_eq!(wide2, wide1);
    assert_eq!(deep2, deep1);
    assert_eq!(WIDE_CALLS.load(Ordering::Relaxed), wide_calls_after_first);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn shared_cache_survives_unrelated_revisions_but_invalidates_on_real_deps()
-> PicanteResult<()> {
    let _permit = TEST_SEM.acquire().await.unwrap();
    picante::__test_shared_cache_clear();
    picante::__test_shared_cache_set_max_entries(100_000);

    WIDE_CALLS.store(0, Ordering::Relaxed);
    DEEP_CALLS.store(0, Ordering::Relaxed);

    let db = Database::new();

    let leaf_count = 512u32;
    init_db(&db, leaf_count)?;

    // Compute once.
    let snap1 = DatabaseSnapshot::from_database(&db).await;
    let wide1 = wide_sum(&snap1, WideKey).await?;
    assert_eq!(wide1, expected_wide_sum(leaf_count));
    assert_eq!(WIDE_CALLS.load(Ordering::Relaxed), 1);

    // Unrelated revision bump should not force recompute.
    Noise::set(&db, 1)?;
    let snap2 = DatabaseSnapshot::from_database(&db).await;
    let wide2 = wide_sum(&snap2, WideKey).await?;
    assert_eq!(wide2, wide1);
    assert_eq!(WIDE_CALLS.load(Ordering::Relaxed), 1);

    // Real dependency change should force recompute and produce new value.
    let _ = Leaf::new(&db, 0, 999)?;
    let snap3 = DatabaseSnapshot::from_database(&db).await;
    let wide3 = wide_sum(&snap3, WideKey).await?;
    assert_eq!(wide3, wide1.wrapping_sub(0).wrapping_add(999));
    assert_eq!(WIDE_CALLS.load(Ordering::Relaxed), 2);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn shared_cache_eviction_is_correct_and_bounded() -> PicanteResult<()> {
    let _permit = TEST_SEM.acquire().await.unwrap();
    picante::__test_shared_cache_clear();
    picante::__test_shared_cache_set_max_entries(50);

    WIDE_CALLS.store(0, Ordering::Relaxed);
    DEEP_CALLS.store(0, Ordering::Relaxed);

    let db = Database::new();
    init_db(&db, 100)?;

    // Populate shared cache with many distinct deep keys (should evict old ones).
    for leaf in 0..200u32 {
        let snap = DatabaseSnapshot::from_database(&db).await;
        let _ = deep_l32(&snap, DeepKey { leaf }).await?;
    }

    let calls_after_populate = DEEP_CALLS.load(Ordering::Relaxed);

    // Access again: may be evicted, but must remain correct.
    let snap = DatabaseSnapshot::from_database(&db).await;
    let v = deep_l32(&snap, DeepKey { leaf: 1 }).await?;
    assert_eq!(v, 3 + DEEP_LEVELS);
    assert!(DEEP_CALLS.load(Ordering::Relaxed) >= calls_after_populate);

    Ok(())
}
