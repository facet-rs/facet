+++
title = "Ecosystem Integration"
weight = 7
insert_anchor_links = "heading"
+++

Facet provides `Facet` trait implementations for many popular Rust crates via feature flags. Enable the feature, and those types work seamlessly with all facet format crates.

## Third-Party type support

Enable these features in your `Cargo.toml`:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", features = ["uuid", "chrono"] }
```

### Available features

| Feature | Crate | Types |
|---------|-------|-------|
| `uuid` | [uuid](https://docs.rs/uuid) | `Uuid` |
| `ulid` | [ulid](https://docs.rs/ulid) | `Ulid` |
| `url` | [url](https://docs.rs/url) | `Url` |
| `chrono` | [chrono](https://docs.rs/chrono) | `DateTime<Tz>`, `NaiveDate`, `NaiveTime`, `NaiveDateTime` |
| `time` | [time](https://docs.rs/time) | `Date`, `Time`, `PrimitiveDateTime`, `OffsetDateTime`, `Duration` |
| `jiff02` | [jiff](https://docs.rs/jiff) | `Timestamp`, `Zoned`, `DateTime`, `Date`, `Time`, `Span`, `SignedDuration` |
| `camino` | [camino](https://docs.rs/camino) | `Utf8Path`, `Utf8PathBuf` |
| `bytes` | [bytes](https://docs.rs/bytes) | `Bytes`, `BytesMut` |
| `iddqd` | [iddqd](https://docs.rs/iddqd) | `IdHashMap<T>`, `IdOrdMap<T>`[^1], `BiHashMap<T>`, `TriHashMap<T>` |
| `ordered-float` | [ordered-float](https://docs.rs/ordered-float) | `OrderedFloat<f32>`, `OrderedFloat<f64>`, `NotNan<f32>`, `NotNan<f64>` |
| `ruint` | [ruint](https://docs.rs/ruint) | `Uint<BITS, LIMBS>`, `Bits<BITS, LIMBS>` |
| `lock_api` | [lock_api](https://docs.rs/lock_api) | `Mutex<R, T>`, `RwLock<R, T>`, `MutexGuard`, `RwLockReadGuard`, `RwLockWriteGuard` |
| `yoke` | [yoke](https://docs.rs/yoke) | `Yoke<Y, C>` |

[^1]: `IdOrdMap` requires `std` feature

### Example: uUIDs

```rust,noexec
use facet::Facet;
use uuid::Uuid;

#[derive(Facet)]
struct User {
    id: Uuid,
    name: String,
}

let json = r#"{"id": "550e8400-e29b-41d4-a716-446655440000", "name": "Alice"}"#;
let user: User = facet_json::from_str(json)?;
```

### Example: dateTime with chrono

```rust,noexec
use facet::Facet;
use chrono::{DateTime, Utc};

#[derive(Facet)]
struct Event {
    name: String,
    timestamp: DateTime<Utc>,
}

let json = r#"{"name": "deploy", "timestamp": "2024-01-15T10:30:00Z"}"#;
let event: Event = facet_json::from_str(json)?;
```

### Example: UTF-8 paths with camino

```rust,noexec
use facet::Facet;
use camino::Utf8PathBuf;

#[derive(Facet)]
struct Config {
    data_dir: Utf8PathBuf,
}
```

### Example: zero-copy strings with yoke

```rust,noexec
use facet::Facet;
use std::borrow::Cow;
use std::sync::Arc;
use yoke::Yoke;

#[derive(Facet)]
struct Document {
    // Zero-copy string that borrows from an Arc<str> cart
    title: Yoke<Cow<'static, str>, Arc<str>>,
}
```

## Extended tuple support

By default, facet supports tuples up to 4 elements. Enable `tuples-12` for tuples up to 12 elements:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", features = ["tuples-12"] }
```

## Function pointer support

Enable `fn-ptr` for `Facet` implementations on function pointer types:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", features = ["fn-ptr"] }
```

## Standard library type support

Some standard library types require feature flags:

| Feature | Types |
|---------|-------|
| `nonzero` | `NonZero<T>` types (`NonZeroU8`, `NonZeroI32`, etc.) |
| `net` | `SocketAddr`, `IpAddr`, `Ipv4Addr`, `Ipv6Addr`, `SocketAddrV4`, `SocketAddrV6` |

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", features = ["nonzero", "net"] }
```

## Doc comment extraction

By default, doc comments (`/// ...`) are **not** included in generated `Shape`, `Field`, and `Variant` definitions to reduce compile times and binary size. Enable the `doc` feature to include them:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", features = ["doc"] }
```

This is useful for:
- **figue**: Including doc comments in CLI help text generation
- **facet-pretty**: Showing doc comments in pretty-printed output
- **Custom tooling**: Building documentation generators or IDE integrations

Without this feature, `.doc` fields will be empty slices (`&[]`).

## Typo suggestions in derive errors

By default, when you mistype an attribute name in `#[facet(...)]`, the derive macro suggests corrections using string similarity matching. This requires the `strsim` dependency.

To disable this and reduce compile times slightly, turn off the `helpful-derive` feature:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", default-features = false, features = ["std"] }
```

With `helpful-derive` enabled (default):
```
error: unknown attribute `renam`, did you mean `rename`?
```

Without it:
```
error: unknown attribute `renam`; expected one of: rename, skip, default, ...
```

## figue: CLI argument parsing

[figue](https://github.com/bearcove/figue) (previously facet-args) provides CLI argument parsing, environment variable parsing, and config file support.

See the [figue documentation](https://docs.rs/figue) for usage examples including help generation, shell completions, subcommands, layered configuration, and more.

## When a type doesn't implement Facet

If you have a type that doesn't implement `Facet`, you have several options:

### Your own type

If it's your type, just derive it:

```rust,noexec
#[derive(Facet)]
struct MyType {
    // ...
}
```

### Type with non-Facet fields

If your type contains fields that don't implement `Facet`, use `opaque` to hide them:

```rust,noexec
use some_crate::ExternalType;  // Doesn't implement Facet

#[derive(Facet)]
struct MyWrapper {
    name: String,
    #[facet(opaque)]
    internal: ExternalType,  // Hidden from serialization
}
```

Opaque fields can't be serialized on their own. If you need serialization, add a `proxy`:

```rust,noexec
#[derive(Facet)]
#[facet(transparent)]
struct ExternalTypeProxy(String);

impl TryFrom<ExternalTypeProxy> for ExternalType {
    type Error = &'static str;
    fn try_from(proxy: ExternalTypeProxy) -> Result<Self, Self::Error> {
        ExternalType::parse(&proxy.0).ok_or("invalid format")
    }
}

impl TryFrom<&ExternalType> for ExternalTypeProxy {
    type Error = std::convert::Infallible;
    fn try_from(val: &ExternalType) -> Result<Self, Self::Error> {
        Ok(ExternalTypeProxy(val.to_string()))
    }
}

#[derive(Facet)]
struct MyWrapper {
    name: String,
    #[facet(opaque, proxy = ExternalTypeProxy)]
    internal: ExternalType,
}

// Serialization: ExternalType → proxy → JSON string
let wrapper = MyWrapper {
    name: "example".into(),
    internal: ExternalType::new(),
};
let json = facet_json::to_string(&wrapper);
// {"name":"example","internal":"...serialized form..."}

// Deserialization: JSON → proxy → ExternalType
let parsed: MyWrapper = facet_json::from_str(&json).unwrap();
```

See the [Attributes Reference](@/reference/attributes/_index.md#container-attributes--opaque) for details on `opaque` and `proxy`.

### Third-party type you want full support for

If you want a third-party type to work seamlessly with facet (like `uuid::Uuid` does), you can contribute an implementation to facet. See [Implementing Facet for third-party types](@/contribute/adding-types.md).

## no_std support

Facet works in `no_std` environments. Disable default features and enable `alloc`:

```toml
[dependencies]
facet = { version = "{{ data.versions.facet }}", default-features = false, features = ["alloc"] }
```

Some format crates also support `no_std`:
- `facet-json` — with `alloc` feature
- `facet-postcard` — with `alloc` feature
- `facet-msgpack` — with `alloc` feature
