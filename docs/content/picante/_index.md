+++
title = "picante"
description = "An async incremental query runtime (Tokio-first Salsa alternative)"
+++

picante is an async incremental query runtime for Rust.

It's motivated by **Dodeca's** main use-case: a large query graph where many nodes want to do async I/O (reading files, calling plugins, spawning work) while still benefiting from memoization, dependency tracking, and persistence across restarts.

picante provides:

- **Inputs** (`InputIngredient<K, V>`)
- **Derived** async queries (`DerivedIngredient<DB, K, V>`)
- **Dependency tracking** via Tokio task-local frames
- **Per-task cycle detection**
- **Persistence** using `facet` + `facet-postcard` (**no serde**)
- **Notifications** for live reload (`Runtime::subscribe_revisions` / `Runtime::subscribe_events`)

Minimal example (trimmed; see the crate docs for more):

```rust,noexec
use picante::{DerivedIngredient, HasRuntime, InputIngredient, QueryKindId, Runtime};
use std::sync::Arc;

#[derive(Default)]
struct Db {
    runtime: Runtime,
}

impl HasRuntime for Db {
    fn runtime(&self) -> &Runtime {
        &self.runtime
    }
}

#[tokio::main]
async fn main() -> picante::PicanteResult<()> {
    let text: Arc<InputIngredient<String, String>> =
        Arc::new(InputIngredient::new(QueryKindId(1), "Text"));

    let len: Arc<DerivedIngredient<Db, String, u64>> = {
        let text = text.clone();
        Arc::new(DerivedIngredient::new(QueryKindId(2), "Len", move |db, key| {
            let text = text.clone();
            Box::pin(async move {
                let s = text.get(db, &key)?.unwrap_or_default();
                Ok(s.len() as u64)
            })
        }))
    };

    let db = Db::default();
    text.set(&db, "a".into(), "hello".into());
    assert_eq!(len.get(&db, "a".into()).await?, 5);
    Ok(())
}
```

