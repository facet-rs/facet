+++
title = "picante"
description = "An async incremental query runtime (Tokio-first Salsa alternative)"
template = "index.html"
+++

Add picante to your `Cargo.toml`:

```toml,noexec
[dependencies]
picante = "0.1"
tokio = { version = "1", features = ["full"] }
```

Then define your database and queries:

```rust,noexec
use picante::PicanteResult;

// Define an input with a key field
#[picante::input]
pub struct SourceFile {
    #[key]
    pub path: String,
    pub content: String,
}

// Define a tracked query — automatically async and memoized
// Use the combined trait (DatabaseTrait) instead of listing Has* traits
#[picante::tracked]
pub async fn line_count<DB: DatabaseTrait>(
    db: &DB,
    file: SourceFile,
) -> PicanteResult<usize> {
    let content = file.content(db)?;
    Ok(content.lines().count())
}

// Wire everything together
// The macro generates DatabaseTrait with all ingredient bounds
#[picante::db(inputs(SourceFile), tracked(line_count))]
pub struct Database {}

#[tokio::main]
async fn main() -> PicanteResult<()> {
    let db = Database::new();

    // Create an input
    let file = SourceFile::new(&db, "main.rs".into(), "fn main() {\n}\n".into())?;

    // Query is computed and cached
    assert_eq!(line_count(&db, file).await?, 2);

    Ok(())
}
```

## Singleton Inputs

Inputs without a `#[key]` field are treated as singletons — only one instance exists:

```rust,noexec
#[picante::input]
pub struct Config {
    pub debug: bool,
    pub timeout: u64,
}

#[picante::db(inputs(Config))]
pub struct Database {}

fn main() -> PicanteResult<()> {
    let db = Database::new();

    // Set the singleton
    Config::set(&db, true, 30)?;

    // Get returns Option (None if not set)
    let config = Config::get(&db)?.unwrap();
    assert!(config.debug);

    Ok(())
}
```

## Custom Trait Name

Use `db_trait(Name)` to customize the generated combined trait name:

```rust,noexec
// Generates trait named `Db` instead of `AppDatabaseTrait`
#[picante::db(inputs(SourceFile), db_trait(Db))]
pub struct AppDatabase {}

fn use_db<DB: Db>(db: &DB) { /* ... */ }
```

