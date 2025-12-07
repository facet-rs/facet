# Examples

Concrete examples of what plugins need to generate.

## Example 1: `facet-display` (displaydoc replacement)

### User writes:

```rust
#[derive(Facet)]
#[facet(display)]
pub enum MyError {
    /// Failed to connect to {host}:{port}
    Connection { host: String, port: u16 },

    /// File not found: {0}
    NotFound(PathBuf),
}
```

### What we need to generate:

```rust
impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection { host, port } =>
                write!(f, "Failed to connect to {}:{}", host, port),
            Self::NotFound(v0) =>
                write!(f, "File not found: {}", v0),
        }
    }
}
```

### Script needs to express:

- "impl Display for this type"
- "match on variants"
- "use doc comment as format string, interpolating fields"

---

## Example 2: `facet-error` (thiserror replacement)

### User writes:

```rust
#[derive(Facet)]
#[facet(error)]  // implies display
pub enum DataStoreError {
    /// data store disconnected
    #[facet(from)]
    Disconnect(std::io::Error),

    /// invalid header (expected {expected:?}, found {found:?})
    InvalidHeader { expected: String, found: String },

    /// unknown error
    #[facet(source)]
    Unknown { source: Box<dyn std::error::Error> },
}
```

### What we need to generate:

```rust
// Display impl (same as facet-display)
impl std::fmt::Display for DataStoreError { ... }

// Error impl
impl std::error::Error for DataStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            // #[facet(from)] fields are implicitly sources
            Self::Disconnect(e) => Some(e),
            // #[facet(source)] explicitly marks source
            Self::Unknown { source } => Some(source.as_ref()),
            _ => None,
        }
    }
}

// From impl for #[facet(from)] fields
impl From<std::io::Error> for DataStoreError {
    fn from(source: std::io::Error) -> Self {
        Self::Disconnect(source)
    }
}
```

### Script needs to express:

- Everything from facet-display, plus:
- "impl Error for this type"
- "for source(), find fields with #[facet(from)] or #[facet(source)]"
- "for each #[facet(from)] field, generate a From impl"
