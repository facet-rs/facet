use divan::{Bencher, black_box};
use facet::Facet;
use picante::db::{DynIngredient, IngredientLookup, IngredientRegistry};
use picante::ingredient::{DerivedIngredient, InputIngredient};
use picante::key::QueryKindId;
use picante::runtime::{HasRuntime, Runtime};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[derive(Default)]
struct BenchDb {
    runtime: Runtime,
    ingredients: IngredientRegistry<BenchDb>,
}

impl BenchDb {
    fn register<I>(&mut self, ingredient: Arc<I>)
    where
        I: DynIngredient<Self> + 'static,
    {
        self.ingredients.register(ingredient);
    }
}

impl HasRuntime for BenchDb {
    fn runtime(&self) -> &Runtime {
        &self.runtime
    }
}

impl IngredientLookup for BenchDb {
    fn ingredient(&self, kind: QueryKindId) -> Option<&dyn DynIngredient<Self>> {
        self.ingredients.ingredient(kind)
    }
}

#[derive(Clone, Debug, Facet)]
struct LargeRow {
    id: u32,
    weight: u64,
    label: String,
}

#[derive(Clone, Debug, Facet)]
struct LargeValue {
    name: String,
    rows: Vec<LargeRow>,
    checksum: u64,
}

#[derive(Clone, Debug, Facet)]
struct LargeKey {
    shard: u32,
    label: String,
    rows: Vec<LargeRow>,
    flags: [u16; 4],
}

type WideFixture = (
    tokio::runtime::Runtime,
    BenchDb,
    Arc<InputIngredient<u32, u64>>,
    Arc<InputIngredient<(), u64>>,
    Arc<DerivedIngredient<BenchDb, (), u64>>,
);

type LargeOutputFixture = (
    tokio::runtime::Runtime,
    BenchDb,
    Arc<InputIngredient<(), u64>>,
    Arc<DerivedIngredient<BenchDb, (), LargeValue>>,
);

type DeepChainFixture = (
    tokio::runtime::Runtime,
    BenchDb,
    Arc<InputIngredient<u32, u64>>,
    Arc<InputIngredient<(), u64>>,
    Arc<DerivedIngredient<BenchDb, u32, u64>>,
);

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn large_value(rows: usize, seed: u64) -> LargeValue {
    let mut checksum = seed;
    let rows = (0..rows)
        .map(|i| {
            let id = i as u32;
            let weight = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(i as u64);
            checksum = checksum.wrapping_add(weight ^ id as u64);
            LargeRow {
                id,
                weight,
                label: format!("row-{seed}-{i:04}"),
            }
        })
        .collect();

    LargeValue {
        name: format!("large-{seed}-{checksum}"),
        rows,
        checksum,
    }
}

fn large_key(rows: usize, seed: u64) -> LargeKey {
    LargeKey {
        shard: seed as u32,
        label: format!("key-{seed}-{rows}"),
        rows: large_value(rows, seed).rows,
        flags: [
            seed as u16,
            seed.wrapping_mul(3) as u16,
            seed.wrapping_mul(5) as u16,
            seed.wrapping_mul(7) as u16,
        ],
    }
}

#[divan::bench]
fn input_get_hit(bencher: Bencher) {
    let db = BenchDb::default();
    let input: InputIngredient<u32, u64> = InputIngredient::new(QueryKindId(1), "Number");
    input.set(&db, 7, 42);

    bencher.bench(|| {
        let value = input.get(&db, black_box(&7)).unwrap();
        black_box(value);
    });
}

#[divan::bench]
fn input_get_hit_large_key(bencher: Bencher) {
    let db = BenchDb::default();
    let input: InputIngredient<LargeKey, u64> = InputIngredient::new(QueryKindId(5), "LargeKey");
    let key = large_key(128, 7);
    input.set(&db, key.clone(), 42);

    bencher.bench(|| {
        let value = input.get(&db, black_box(&key)).unwrap();
        black_box(value);
    });
}

#[divan::bench]
fn input_set_noop_small_value(bencher: Bencher) {
    let db = BenchDb::default();
    let input: InputIngredient<u32, u64> = InputIngredient::new(QueryKindId(2), "Number");
    input.set(&db, 7, 42);

    bencher.bench(|| {
        let revision = input.set(&db, black_box(7), black_box(42));
        black_box(revision);
    });
}

#[divan::bench]
fn input_set_noop_large_key(bencher: Bencher) {
    let db = BenchDb::default();
    let input: InputIngredient<LargeKey, u64> = InputIngredient::new(QueryKindId(6), "LargeKey");
    let key = large_key(128, 7);
    input.set(&db, key.clone(), 42);

    bencher.bench(|| {
        let revision = input.set(&db, black_box(key.clone()), black_box(42));
        black_box(revision);
    });
}

#[divan::bench]
fn input_set_noop_large_value(bencher: Bencher) {
    let db = BenchDb::default();
    let input: InputIngredient<u32, LargeValue> = InputIngredient::new(QueryKindId(3), "Large");
    let value = large_value(1024, 1);
    input.set(&db, 7, value.clone());

    bencher.bench(|| {
        let revision = input.set(&db, black_box(7), black_box(value.clone()));
        black_box(revision);
    });
}

#[divan::bench]
fn input_set_changed_large_value(bencher: Bencher) {
    let db = BenchDb::default();
    let input: InputIngredient<u32, LargeValue> = InputIngredient::new(QueryKindId(4), "Large");
    input.set(&db, 7, large_value(1024, 1));

    let seed = AtomicU64::new(2);
    bencher.bench(|| {
        let next_seed = seed.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let revision = input.set(&db, black_box(7), black_box(large_value(1024, next_seed)));
        black_box(revision);
    });
}

#[divan::bench]
fn derived_hot_hit(bencher: Bencher) {
    let rt = runtime();
    let mut db = BenchDb::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(10), "Text"));
    db.register(input.clone());
    input.set(&db, "a".into(), "hello".into());

    let input_for_compute = input.clone();
    let derived: Arc<DerivedIngredient<BenchDb, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(11),
        "Len",
        move |db, key| {
            let input = input_for_compute.clone();
            Box::pin(async move {
                let text = input.get(db, &key)?.unwrap_or_default();
                Ok(text.len() as u64)
            })
        },
    ));
    db.register(derived.clone());

    rt.block_on(async {
        let _ = derived.get(&db, "a".into()).await.unwrap();
    });

    let key = "a".to_string();
    bencher.bench(|| {
        rt.block_on(async {
            let value = derived.get(&db, black_box(key.clone())).await.unwrap();
            black_box(value);
        })
    });
}

#[divan::bench]
fn derived_cold_unique_keys(bencher: Bencher) {
    let rt = runtime();
    let db = BenchDb::default();
    let derived: DerivedIngredient<BenchDb, u64, u64> =
        DerivedIngredient::new(QueryKindId(12), "Identity", |_db: &BenchDb, key: u64| {
            Box::pin(async move { Ok(key.wrapping_mul(3).wrapping_add(1)) })
        });

    let key = AtomicU64::new(0);
    bencher.bench(|| {
        let next_key = key.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        rt.block_on(async {
            let value = derived.get(&db, black_box(next_key)).await.unwrap();
            black_box(value);
        })
    });
}

fn wide_sum_fixture(leaf_count: u32) -> WideFixture {
    let rt = runtime();
    let mut db = BenchDb::default();
    let leaves: Arc<InputIngredient<u32, u64>> =
        Arc::new(InputIngredient::new(QueryKindId(20), "Leaf"));
    let noise: Arc<InputIngredient<(), u64>> =
        Arc::new(InputIngredient::new(QueryKindId(21), "Noise"));
    db.register(leaves.clone());
    db.register(noise.clone());

    for i in 0..leaf_count {
        leaves.set(&db, i, (i as u64).wrapping_mul(3));
    }
    noise.set(&db, (), 0);

    let leaves_for_compute = leaves.clone();
    let wide: Arc<DerivedIngredient<BenchDb, (), u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(22),
        "WideSum",
        move |db, ()| {
            let leaves = leaves_for_compute.clone();
            Box::pin(async move {
                let mut sum = 0u64;
                for i in 0..leaf_count {
                    sum = sum.wrapping_add(leaves.get(db, &i)?.unwrap_or_default());
                }
                Ok(sum)
            })
        },
    ));
    db.register(wide.clone());

    rt.block_on(async {
        let _ = wide.get(&db, ()).await.unwrap();
    });

    (rt, db, leaves, noise, wide)
}

#[divan::bench]
fn derived_wide_revalidate_64_deps(bencher: Bencher) {
    let (rt, db, _leaves, noise, wide) = wide_sum_fixture(64);
    bench_wide_revalidate(bencher, rt, db, noise, wide);
}

#[divan::bench]
fn derived_wide_recompute_64_deps(bencher: Bencher) {
    let (rt, db, leaves, _noise, wide) = wide_sum_fixture(64);
    bench_wide_recompute(bencher, rt, db, leaves, wide);
}

#[divan::bench]
fn derived_wide_revalidate_512_deps(bencher: Bencher) {
    let (rt, db, _leaves, noise, wide) = wide_sum_fixture(512);
    bench_wide_revalidate(bencher, rt, db, noise, wide);
}

#[divan::bench]
fn derived_wide_recompute_512_deps(bencher: Bencher) {
    let (rt, db, leaves, _noise, wide) = wide_sum_fixture(512);
    bench_wide_recompute(bencher, rt, db, leaves, wide);
}

#[divan::bench]
fn derived_wide_revalidate_2048_deps(bencher: Bencher) {
    let (rt, db, _leaves, noise, wide) = wide_sum_fixture(2048);
    bench_wide_revalidate(bencher, rt, db, noise, wide);
}

#[divan::bench]
fn derived_wide_recompute_2048_deps(bencher: Bencher) {
    let (rt, db, leaves, _noise, wide) = wide_sum_fixture(2048);
    bench_wide_recompute(bencher, rt, db, leaves, wide);
}

fn bench_wide_revalidate(
    bencher: Bencher,
    rt: tokio::runtime::Runtime,
    db: BenchDb,
    noise: Arc<InputIngredient<(), u64>>,
    wide: Arc<DerivedIngredient<BenchDb, (), u64>>,
) {
    let revision_noise = AtomicU64::new(0);
    bencher.bench(|| {
        let next_noise = revision_noise
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        noise.set(&db, (), next_noise);
        rt.block_on(async {
            let value = wide.get(&db, ()).await.unwrap();
            black_box(value);
        })
    });
}

fn bench_wide_recompute(
    bencher: Bencher,
    rt: tokio::runtime::Runtime,
    db: BenchDb,
    leaves: Arc<InputIngredient<u32, u64>>,
    wide: Arc<DerivedIngredient<BenchDb, (), u64>>,
) {
    let leaf_value = AtomicU64::new(0);
    bencher.bench(|| {
        let next_value = leaf_value.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        leaves.set(&db, black_box(0), next_value);
        rt.block_on(async {
            let value = wide.get(&db, ()).await.unwrap();
            black_box(value);
        })
    });
}

fn equal_cutoff_fixture(rows: usize, vary_output: bool) -> LargeOutputFixture {
    let rt = runtime();
    let mut db = BenchDb::default();
    let trigger: Arc<InputIngredient<(), u64>> =
        Arc::new(InputIngredient::new(QueryKindId(30), "Trigger"));
    db.register(trigger.clone());
    trigger.set(&db, (), 0);

    let stable = large_value(rows, 77);
    let trigger_for_compute = trigger.clone();
    let derived: Arc<DerivedIngredient<BenchDb, (), LargeValue>> = Arc::new(
        DerivedIngredient::new(QueryKindId(31), "LargeOutput", move |db, ()| {
            let trigger = trigger_for_compute.clone();
            let stable = stable.clone();
            Box::pin(async move {
                let tick = trigger.get(db, &())?.unwrap_or_default();
                if vary_output {
                    Ok(large_value(rows, tick))
                } else {
                    Ok(stable)
                }
            })
        }),
    );
    db.register(derived.clone());

    rt.block_on(async {
        let _ = derived.get(&db, ()).await.unwrap();
    });

    (rt, db, trigger, derived)
}

#[divan::bench]
fn derived_equal_cutoff_large_value(bencher: Bencher) {
    let (rt, db, trigger, derived) = equal_cutoff_fixture(1024, false);
    bench_large_output_recompute(bencher, rt, db, trigger, derived);
}

#[divan::bench]
fn derived_changed_large_value(bencher: Bencher) {
    let (rt, db, trigger, derived) = equal_cutoff_fixture(1024, true);
    bench_large_output_recompute(bencher, rt, db, trigger, derived);
}

fn bench_large_output_recompute(
    bencher: Bencher,
    rt: tokio::runtime::Runtime,
    db: BenchDb,
    trigger: Arc<InputIngredient<(), u64>>,
    derived: Arc<DerivedIngredient<BenchDb, (), LargeValue>>,
) {
    let tick = AtomicU64::new(0);
    bencher.bench(|| {
        let next_tick = tick.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        trigger.set(&db, (), next_tick);
        rt.block_on(async {
            let value = derived.get(&db, ()).await.unwrap();
            black_box(value.checksum);
        })
    });
}

fn deep_chain_fixture(levels: usize) -> DeepChainFixture {
    let rt = runtime();
    let mut db = BenchDb::default();
    let leaf: Arc<InputIngredient<u32, u64>> =
        Arc::new(InputIngredient::new(QueryKindId(40), "Leaf"));
    let noise: Arc<InputIngredient<(), u64>> =
        Arc::new(InputIngredient::new(QueryKindId(41), "Noise"));
    db.register(leaf.clone());
    db.register(noise.clone());

    for i in 0..4096u32 {
        leaf.set(&db, i, (i as u64).wrapping_mul(5));
    }
    noise.set(&db, (), 0);

    let mut chain: Vec<Arc<DerivedIngredient<BenchDb, u32, u64>>> = Vec::with_capacity(levels + 1);
    for level in 0..=levels {
        let previous = chain.last().cloned();
        let leaf_for_compute = leaf.clone();
        let ingredient: Arc<DerivedIngredient<BenchDb, u32, u64>> =
            Arc::new(DerivedIngredient::new(
                QueryKindId(100 + level as u32),
                "DeepChain",
                move |db, key| {
                    let previous = previous.clone();
                    let leaf = leaf_for_compute.clone();
                    Box::pin(async move {
                        match previous {
                            Some(previous) => Ok(previous.get(db, key).await?.wrapping_add(1)),
                            None => Ok(leaf.get(db, &key)?.unwrap_or_default()),
                        }
                    })
                },
            ));
        db.register(ingredient.clone());
        chain.push(ingredient);
    }

    let top = chain.last().unwrap().clone();
    rt.block_on(async {
        let _ = top.get(&db, 0).await.unwrap();
    });

    (rt, db, leaf, noise, top)
}

#[divan::bench]
fn derived_deep_chain_hot_hit_32(bencher: Bencher) {
    let (rt, db, _leaf, _noise, top) = deep_chain_fixture(32);

    bencher.bench(|| {
        rt.block_on(async {
            let value = top.get(&db, black_box(0)).await.unwrap();
            black_box(value);
        })
    });
}

#[divan::bench]
fn derived_deep_chain_revalidate_32(bencher: Bencher) {
    let (rt, db, _leaf, noise, top) = deep_chain_fixture(32);

    let revision_noise = AtomicU64::new(0);
    bencher.bench(|| {
        let next_noise = revision_noise
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        noise.set(&db, (), next_noise);
        rt.block_on(async {
            let value = top.get(&db, black_box(0)).await.unwrap();
            black_box(value);
        })
    });
}

#[divan::bench]
fn derived_deep_chain_cold_unique_keys_32(bencher: Bencher) {
    let (rt, db, _leaf, _noise, top) = deep_chain_fixture(32);

    let key = AtomicU32::new(0);
    bencher.bench(|| {
        let next_key = key.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        rt.block_on(async {
            let value = top.get(&db, black_box(next_key)).await.unwrap();
            black_box(value);
        })
    });
}

fn main() {
    divan::main();
}
