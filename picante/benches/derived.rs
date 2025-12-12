use divan::{Bencher, black_box};
use picante::db::{DynIngredient, IngredientLookup, IngredientRegistry};
use picante::ingredient::{DerivedIngredient, InputIngredient};
use picante::key::QueryKindId;
use picante::runtime::{HasRuntime, Runtime};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct Db {
    runtime: Runtime,
    ingredients: IngredientRegistry<Db>,
}

impl HasRuntime for Db {
    fn runtime(&self) -> &Runtime {
        &self.runtime
    }
}

impl IngredientLookup for Db {
    fn ingredient(&self, kind: QueryKindId) -> Option<&dyn DynIngredient<Self>> {
        self.ingredients.ingredient(kind)
    }
}

#[divan::bench]
fn derived_get_hit(bencher: Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let db = Db::default();
    let input: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
    input.set(&db, "a".into(), "hello".into());

    let executions = Arc::new(AtomicUsize::new(0));
    let input_for_compute = input.clone();
    let executions_for_compute = executions.clone();

    let derived: Arc<DerivedIngredient<Db, String, u64>> = Arc::new(DerivedIngredient::new(
        QueryKindId(2),
        "Len",
        move |db, key| {
            let input = input_for_compute.clone();
            let executions = executions_for_compute.clone();
            Box::pin(async move {
                executions.fetch_add(1, Ordering::Relaxed);
                let text = input.get(db, &key)?.unwrap_or_default();
                Ok(text.len() as u64)
            })
        },
    ));

    rt.block_on(async {
        let _ = derived.get(&db, "a".into()).await.unwrap();
    });

    let key = "a".to_string();
    bencher.bench(|| {
        rt.block_on(async {
            let v = derived.get(&db, key.clone()).await.unwrap();
            black_box(v);
        })
    });
}

fn main() {
    divan::main();
}
