use picante::PicanteResult;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Semaphore;
use tokio::time::{Duration, sleep};

#[picante::input]
pub struct Leaf {
    #[key]
    pub key: u32,
    pub value: u64,
}

/// Singleton registry that references many leaf inputs (wide dependency fan-in).
#[picante::input]
pub struct LeafRegistry {
    pub leaves: Vec<Leaf>,
}

/// Singleton input used to bump the global revision without affecting most queries.
#[picante::input]
pub struct Noise {
    pub value: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, facet::Facet)]
pub struct WideKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, facet::Facet)]
pub struct DeepKey {
    pub leaf: u32,
}

pub static WIDE_CALLS: AtomicUsize = AtomicUsize::new(0);
pub static DEEP_CALLS: AtomicUsize = AtomicUsize::new(0);
pub static TEST_SEM: Semaphore = Semaphore::const_new(1);

#[picante::tracked]
pub async fn wide_sum<DB: DatabaseTrait>(db: &DB, _key: WideKey) -> PicanteResult<u64> {
    WIDE_CALLS.fetch_add(1, Ordering::Relaxed);
    // Artificially expensive but deterministic.
    sleep(Duration::from_millis(10)).await;

    let registry = LeafRegistry::get(db)?.ok_or_else(|| {
        std::sync::Arc::new(picante::PicanteError::Panic {
            message: "missing LeafRegistry".to_string(),
        })
    })?;

    let mut sum = 0u64;
    for leaf in registry.leaves.iter().copied() {
        sum = sum.wrapping_add(leaf.value(db)?);
    }
    Ok(sum)
}

macro_rules! define_deep_chain {
    ($base:ident, $($name:ident => $prev:ident),+ $(,)?) => {
        #[picante::tracked]
        pub async fn $base<DB: DatabaseTrait>(db: &DB, key: DeepKey) -> PicanteResult<u64> {
            DEEP_CALLS.fetch_add(1, Ordering::Relaxed);
            // Base depends on one leaf selected from the registry.
            let registry = LeafRegistry::get(db)?.ok_or_else(|| {
                std::sync::Arc::new(picante::PicanteError::Panic {
                    message: "missing LeafRegistry".to_string(),
                })
            })?;
            let idx = (key.leaf as usize) % registry.leaves.len().max(1);
            let leaf = registry.leaves[idx];
            leaf.value(db)
        }

        $(
            #[picante::tracked]
            pub async fn $name<DB: DatabaseTrait>(db: &DB, key: DeepKey) -> PicanteResult<u64> {
                DEEP_CALLS.fetch_add(1, Ordering::Relaxed);
                // Simulate per-layer work.
                if (key.leaf + (stringify!($name).len() as u32)) % 7 == 0 {
                    sleep(Duration::from_millis(2)).await;
                }
                let v = $prev(db, key).await?;
                Ok(v.wrapping_add(1))
            }
        )+
    };
}

define_deep_chain!(
    deep_l0,
    deep_l1 => deep_l0,
    deep_l2 => deep_l1,
    deep_l3 => deep_l2,
    deep_l4 => deep_l3,
    deep_l5 => deep_l4,
    deep_l6 => deep_l5,
    deep_l7 => deep_l6,
    deep_l8 => deep_l7,
    deep_l9 => deep_l8,
    deep_l10 => deep_l9,
    deep_l11 => deep_l10,
    deep_l12 => deep_l11,
    deep_l13 => deep_l12,
    deep_l14 => deep_l13,
    deep_l15 => deep_l14,
    deep_l16 => deep_l15,
    deep_l17 => deep_l16,
    deep_l18 => deep_l17,
    deep_l19 => deep_l18,
    deep_l20 => deep_l19,
    deep_l21 => deep_l20,
    deep_l22 => deep_l21,
    deep_l23 => deep_l22,
    deep_l24 => deep_l23,
    deep_l25 => deep_l24,
    deep_l26 => deep_l25,
    deep_l27 => deep_l26,
    deep_l28 => deep_l27,
    deep_l29 => deep_l28,
    deep_l30 => deep_l29,
    deep_l31 => deep_l30,
    deep_l32 => deep_l31,
);

pub const DEEP_LEVELS: u64 = 32;

#[picante::db(
    inputs(Leaf, LeafRegistry, Noise),
    tracked(
        wide_sum, deep_l0, deep_l1, deep_l2, deep_l3, deep_l4, deep_l5, deep_l6, deep_l7, deep_l8,
        deep_l9, deep_l10, deep_l11, deep_l12, deep_l13, deep_l14, deep_l15, deep_l16, deep_l17,
        deep_l18, deep_l19, deep_l20, deep_l21, deep_l22, deep_l23, deep_l24, deep_l25, deep_l26,
        deep_l27, deep_l28, deep_l29, deep_l30, deep_l31, deep_l32
    )
)]
pub struct Database {}

#[allow(dead_code)]
pub fn expected_wide_sum(leaf_count: u32) -> u64 {
    (0..leaf_count as u64)
        .map(|i| i.wrapping_mul(3))
        .sum::<u64>()
}

pub fn init_db(db: &Database, leaf_count: u32) -> PicanteResult<()> {
    let mut leaves = Vec::with_capacity(leaf_count as usize);
    for i in 0..leaf_count {
        leaves.push(Leaf::new(db, i, (i as u64).wrapping_mul(3))?);
    }
    LeafRegistry::set(db, leaves)?;
    Noise::set(db, 0)?;
    Ok(())
}
