# Canonical cross‑format validation matrix for facet\*

This file is format‑agnostic. It avoids KDL‑specific markers like `#[facet(argument)]` / `#[facet(property)]` / `#[facet(children)]`. Treat every field as an ordinary struct field unless explicitly noted (e.g., `flatten`, `default`, `skip_*`). Map “nested” to whatever your format uses (JSON object, XML child element, binary embedded struct).

## Prerequisites / where to start
- Deserialization: implement `facet_deserialize::Format` (see the `next()` / `skip()` contract).
- Serialization: implement `facet_serialize::Serializer` (about 25 methods).
- Reference implementation: `facet-json` (tests cover most of these scenarios).

## Attribute translation cheatsheet
| Concept in examples | JSON/YAML/TOML/msgpack | XML / hierarchical | Binary (bincode/postcard) |
|---------------------|------------------------|--------------------|---------------------------|
| field               | object key/value       | attribute or child | field in declared order   |
| nested struct       | nested object          | child element      | embedded struct           |
| list of nested      | array of objects       | repeated elements  | len + repeated elements   |
| flatten             | merge fields inline    | merge attrs/elements| inline fields             |
| default             | fill missing with Default| same             | same                      |

## Quick API surface
- `from_str` / `from_slice` -> `Result<T, Error>`.
- `to_string` / `to_vec` / `to_writer`; deterministic output preferred.
- Errors: implement `std::error::Error`; `miette::Diagnostic` if possible. For binary, spans = byte offsets/lengths.

## Implementation tiers
- **Tier 1 (MVP):** scalars, structs, `Option<T>`, `Vec<T>`, round-trip equality.
- **Tier 2 (Complete):** enums (unit/newtype/struct/tuple), flatten (struct + enum), maps with string keys, deny‑unknown‑fields, tuple structs/variants, unit structs.
- **Tier 3 (Advanced):** spans/diagnostics, `deserialize_with`, non‑string map keys, value‑based disambiguation, `Option<flatten>`, flatten defaults, skip serialize/deserialize, 128‑bit ints, recursive types.

## Option & defaults
- Option without default: missing = error; explicit null = None.
- Option with default: missing = None; explicit null = None; explicit value = Some.

## Flatten solver (core cases)
1. Struct flatten, interleaved fields:
```rust
struct Root { svc: Svc }
struct Svc { name: String, enabled: bool, #[facet(flatten)] conn: Conn }
struct Conn { host: String, port: u16 }
```
2. Enum flatten by field presence:
```rust
enum Backend { File { path: String }, Db { url: String, branch: String } }
// url+branch -> Db; path only -> File; url without branch -> error.
```
3. Enum flatten by child presence:
```rust
enum Mode { Tuned { tuning: Tuning, gain: u8 }, Simple { level: u8 } }
// tuning present -> Tuned; gain without tuning -> error.
```
4. Nested flatten (struct -> struct -> enum):
```rust
struct Server { name: String, #[facet(flatten)] settings: Settings }
struct Settings { enabled: bool, #[facet(flatten)] backend: Backend }
enum Backend { Http { url: String }, Grpc { addr: String, tls: bool } }
```
5. Multiple flattens side‑by‑side:
```rust
struct Connection { name: String, #[facet(flatten)] auth: Auth, #[facet(flatten)] transport: Transport }
```
6. Unit variant vs data variant:
```rust
enum Output { Stdout, File { path: String } }
// empty -> Stdout; has path -> File; unknown + deny_unknown_fields -> error.
```
7. Ambiguity & mixed fields:
```rust
enum Kind { A { x: u8 }, B { x: u8 } }        // x only => error (ambiguous)
enum Mode2 { Simple { level: u8 }, Tuned { level: u8, tuning: u8 } } // level+gain(no tuning) => error
```
8. Value/type disambiguation:
```rust
enum Ints { Small { v: u8 }, Large { v: u16 } }   // v=255 -> Small; v=1000 -> Large
enum Signed { Signed { n: i8 }, Unsigned { n: u8 } } // n=-5 -> Signed; n=200 -> Unsigned
```
9. Option<flatten>:
```rust
struct Server { name: String, #[facet(flatten)] tuning: Option<Tuning> }
struct Tuning { ttl: u32, strategy: Option<String> }
// absent -> None; partial fills defaults/None.
```
10. Flatten with Default:
```rust
struct Server { name: String, #[facet(flatten, default)] limits: Limits }
#[derive(Default)] struct Limits { max: u32, burst: u32 }
```
11. Tuple structs / variants:
```rust
struct Point(i32, i32);
enum Pairish { Pair(u8, u8), Unit }
// Choose array representation for text formats; ordered fields for binary.
```
12. Unit structs / markers:
```rust
struct Marker;
```
13. Recursive:
```rust
struct Node { name: String, kids: Vec<Node> }
```
14. Result<T, E> (if supported):
```rust
struct Wrapper { outcome: Result<u8, String> }
```
15. Skip serialize / skip deserialize:
```rust
struct Hidden { visible: u8, #[facet(skip_serializing)] transient: u8, #[facet(skip_deserializing)] cache: u8 }
```

## Collections & maps
- Vec/sequence round‑trips.
- Sets (HashSet/BTreeSet) — BTreeSet ordering deterministic.
- Maps with string keys; non‑string keys via newtypes (e.g., path types).
- Node‑name‑as‑key pattern: for formats that support it; otherwise map key = field name.
- Tuple structs/variants: prefer arrays for text formats; document choice.

## Scalars & strings
- Escaping rules; raw/multiline where supported.
- Booleans, null, numeric boundaries; special floats policy (NaN/∞) explicit.

## Unknown data
- Default: unknown fields skipped.
- With deny_unknown_fields: reject unknown keys/elements/extra bytes; message should mention offending key.

## Diagnostics & spans
- Parse errors should be meaningful.
- Spans: text formats -> byte range/line+col; binary -> byte offset/len plus field/variant name.

## Binary formats (postcard/bincode, etc.)
- No field names: extra bytes => error; ordering fixed by type.
- Primary validation: encode → decode round‑trip; overflow/underflow must error (no wrapping).
- Spans as byte offsets/lengths; include field/variant name in diagnostics.
- Define endianness/varint and length‑prefix policy; keep it stable.

## Custom hooks
- `deserialize_with` on fields (scalars and inside flatten); test success + failing parse.

## Round‑trip guarantees
- Serialize → parse → serialize idempotence for: basic structs, maps, options, flatten (struct + enum), interleaved fields. When order is undefined (maps/sets), assert presence not exact text.
