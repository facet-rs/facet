mod support;

use picante::PicanteResult;
use std::sync::atomic::Ordering;
use support::*;
use tokio::time::{Duration, timeout};

#[derive(Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u32(&mut self) -> u32 {
        // LCG (deterministic, cheap, good enough for tests)
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 32) as u32
    }

    fn gen_range(&mut self, upper: u32) -> u32 {
        if upper == 0 {
            0
        } else {
            self.next_u32() % upper
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn chaos_mixed_ops_keeps_cache_correct() -> PicanteResult<()> {
    let _permit = TEST_SEM.acquire().await.unwrap();

    picante::__test_shared_cache_clear();
    picante::__test_shared_cache_set_max_entries(200_000);
    WIDE_CALLS.store(0, Ordering::Relaxed);
    DEEP_CALLS.store(0, Ordering::Relaxed);

    let db = Database::new();
    let leaf_count = 512u32;
    init_db(&db, leaf_count)?;

    // Track expected leaf values and sum for correctness checks.
    let mut leaf_values: Vec<u64> = (0..leaf_count as u64).map(|i| i * 3).collect();
    let mut expected_sum: u64 = leaf_values.iter().copied().sum();

    let mut rng = Rng::new(0xC0FFEE);

    let body = async {
        for step in 0..300u32 {
            let choice = rng.gen_range(100);
            match choice {
                // Query wide_sum (expensive)
                0..=24 => {
                    let snap = DatabaseSnapshot::from_database(&db).await;
                    let got = wide_sum(&snap, WideKey).await?;
                    assert_eq!(got, expected_sum, "wide_sum mismatch at step {step}");
                }
                // Query deep chain for a random leaf
                25..=74 => {
                    let leaf = rng.gen_range(leaf_count);
                    let snap = DatabaseSnapshot::from_database(&db).await;
                    let got = deep_l32(&snap, DeepKey { leaf }).await?;
                    let expected = leaf_values[leaf as usize].wrapping_add(DEEP_LEVELS);
                    assert_eq!(got, expected, "deep mismatch at step {step}");
                }
                // Bump unrelated input (revision noise)
                75..=84 => {
                    let v = rng.next_u32() as u64;
                    Noise::set(&db, v)?;
                }
                // Mutate a leaf value (real dependency change)
                _ => {
                    let leaf = rng.gen_range(leaf_count);
                    let new_val = (rng.next_u32() as u64) % 10_000;
                    let old = leaf_values[leaf as usize];
                    let _ = Leaf::new(&db, leaf, new_val)?;
                    leaf_values[leaf as usize] = new_val;
                    expected_sum = expected_sum.wrapping_sub(old).wrapping_add(new_val);
                }
            }
        }
        PicanteResult::Ok(())
    };

    timeout(Duration::from_secs(30), body)
        .await
        .expect("chaos test timed out")?;

    Ok(())
}
