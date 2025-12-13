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

// Define a tracked query - automatically async and memoized
#[picante::tracked]
pub async fn line_count<DB: Db>(db: &DB, file: SourceFile) -> PicanteResult<usize> {
    let content = file.content(db);
    Ok(content.lines().count())
}

#[tokio::main]
async fn main() -> PicanteResult<()> {
    let db = Database::new();

    // Create an input
    let file = SourceFile::new(&db, "main.rs".into(), "fn main() {\n}\n".into());

    // Query is computed and cached
    assert_eq!(line_count(&db, file).await?, 2);

    // Update the input - dependents automatically invalidated
    file.set_content(&db, "fn main() {\n    println!(\"hello\");\n}\n".into());

    // Re-query: only recomputes what changed
    assert_eq!(line_count(&db, file).await?, 3);

    Ok(())
}
```

