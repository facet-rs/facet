# Canonical cross‑format validation matrix for facet\*

This guide is format‑agnostic. Examples use plain Rust fields plus core facet flags (`flatten`, `default`, `skip_*`). Map “nested” to your format’s construct (JSON object, XML child element, binary embedded struct).

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

## [r.api] Quick API surface
- `[r.api.text.from]` Text formats:
```rust
pub fn from_str<T: Facet + ?Sized>(s: &str) -> Result<T, Error>;
```
- `[r.api.text.to]` Text formats:
```rust
pub fn to_string<T: Facet + ?Sized>(value: &T) -> Result<String, Error>;
```
- `[r.api.bin.from]` Binary formats:
```rust
pub fn from_slice<T: Facet + ?Sized>(bytes: &[u8]) -> Result<T, Error>;
```
- `[r.api.bin.to]` Binary formats:
```rust
pub fn to_vec<T: Facet + ?Sized>(value: &T) -> Result<Vec<u8>, Error>;
```
- `[r.api.errors]` Errors implement `std::error::Error`; `miette::Diagnostic` recommended for text formats. Binary spans use byte offsets/lengths.

## [r.deterministic] Deterministic output
- Structs serialize in declaration order.
- Maps follow iteration order of the map type (HashMap unordered, BTreeMap sorted, ordered maps preserve insertion). If the format/schema mandates an order, follow and document it.

## [r.api.errors.miette] Error reporting with miette (text formats)
- `facet-reflect` already tracks spans; surface them via `miette::Diagnostic`.
- Example:
```rust
#[derive(thiserror::Error, Debug, miette::Diagnostic)]
#[error("{msg}")]
struct ParseErr {
    #[source_code]
    src: String,
    #[label("here")]
    span: miette::SourceSpan,
    msg: String,
}

// When deserializing, fill span with offset/len from facet-reflect spans.
```

## Implementation tiers
- `[r.tier.m1]` **Tier 1 (MVP):** scalars, structs, `Option<T>`, `Vec<T>`, round-trip equality.
- `[r.tier.m2]` **Tier 2 (Complete):** enums (unit/newtype/struct/tuple), flatten (struct + enum), maps with string keys, deny‑unknown‑fields, tuple structs/variants, unit structs.
- `[r.tier.m3]` **Tier 3 (Advanced):** spans/diagnostics, `deserialize_with`, non‑string map keys, value‑based disambiguation, `Option<flatten>`, flatten defaults, skip serialize/deserialize, 128‑bit ints, recursive types.

## Option & defaults
- `[r.option.missing]` Option without default: missing = error; explicit null = None. (Matches facet-json; differs from serde defaulting Option to None when absent.)
- `[r.option.default]` Option with default: missing = None; explicit null = None; explicit value = Some.

## Flatten solver (core cases)
1. `[r.flatten.struct]` Struct flatten, interleaved fields:
```rust
struct Root { svc: Svc }
struct Svc { name: String, enabled: bool, #[facet(flatten)] conn: Conn }
struct Conn { host: String, port: u16 }
```
2. `[r.flatten.enum.fields]` Enum flatten by field presence:
```rust
enum Backend { File { path: String }, Db { url: String, branch: String } }
// url+branch -> Db; path only -> File; url without branch -> error.
```
3. `[r.flatten.enum.child]` Enum flatten by child presence:
```rust
enum Mode { Tuned { tuning: Tuning, gain: u8 }, Simple { level: u8 } }
// tuning present -> Tuned; gain without tuning -> error.
```
4. `[r.flatten.nested]` Nested flatten (struct -> struct -> enum):
```rust
struct Server { name: String, #[facet(flatten)] settings: Settings }
struct Settings { enabled: bool, #[facet(flatten)] backend: Backend }
enum Backend { Http { url: String }, Grpc { addr: String, tls: bool } }
```
5. `[r.flatten.multi]` Multiple flattens side‑by‑side:
```rust
struct Connection { name: String, #[facet(flatten)] auth: Auth, #[facet(flatten)] transport: Transport }
```
6. `[r.flatten.unit-data]` Unit variant vs data variant:
```rust
enum Output { Stdout, File { path: String } }
// empty -> Stdout; has path -> File; unknown + deny_unknown_fields -> error.
```
7. `[r.flatten.ambiguous]` Ambiguity & mixed fields:
```rust
enum Kind { A { x: u8 }, B { x: u8 } }        // x only => error (ambiguous)
enum Mode2 { Simple { level: u8 }, Tuned { level: u8, tuning: u8 } } // level without tuning => error
```
8. `[r.flatten.value-disambig]` Value/type disambiguation:
```rust
enum Ints { Small { v: u8 }, Large { v: u16 } }   // v=255 -> Small; v=1000 -> Large
enum Signed { Signed { n: i8 }, Unsigned { n: u8 } } // n=-5 -> Signed; n=200 -> Unsigned
```

`[r.solver.strategy]` Solver disambiguation guidance: prefer variants that fully satisfy required fields; among candidates, pick those whose value ranges/types fit; child presence can disambiguate; if multiple viable variants remain, report ambiguity instead of picking arbitrarily. Document your format’s tie-break rules (e.g., declaration order) if any.
9. `[r.flatten.option]` `Option<flatten>`:
```rust
struct Server { name: String, #[facet(flatten)] tuning: Option<Tuning> }
struct Tuning { ttl: u32, strategy: Option<String> }
// absent -> None; partial fills defaults/None.
```
10. `[r.flatten.default]` Flatten with Default:
```rust
struct Server { name: String, #[facet(flatten, default)] limits: Limits }
#[derive(Default)] struct Limits { max: u32, burst: u32 }
```
11. `[r.types.tuple]` Tuple structs / variants:
```rust
struct Point(i32, i32);
enum Pairish { Pair(u8, u8), Unit }
// Choose array representation for text formats; ordered fields for binary.
```
12. `[r.types.unit-struct]` Unit structs / markers:
```rust
struct Marker;
```
13. `[r.types.recursive]` Recursive:
```rust
struct Node { name: String, kids: Vec<Node> }
```
14. `[r.types.result]` Result<T, E> (if supported):
```rust
struct Wrapper { outcome: Result<u8, String> }
```
*Suggested representation:* tagged form `{ "Ok": <value> }` / `{ "Err": <error> }` for text formats; for binary, use enum variant index + payload.
15. `[r.attrs.skip]` Skip serialize / skip deserialize:
```rust
struct Hidden { visible: u8, #[facet(skip_serializing)] transient: u8, #[facet(skip_deserializing)] cache: u8 }
```
*Expectation:* skipped-on-deserialize fields must be initializable (e.g., Default); input values for them are ignored if present.

16. `[r.attrs.rename]` Rename / rename_all:
```rust
#[facet(rename_all = "camelCase")]
struct Config {
    #[facet(rename = "serverHost")]
    server_host: String,
}
```

17. `[r.attrs.transparent]` Transparent newtypes:
```rust
#[facet(transparent)]
struct PathBufLike(String); // serializes/deserializes as inner String directly
```

18. `[r.types.ptr]` Smart pointers serialize as inner `T`: `Box<T>`, `Rc<T>`, `Arc<T>` (and equivalents) behave transparently.

19. `[r.attrs.skip_if]` Conditional skip serialize:
```rust
struct Config {
    #[facet(skip_serializing_if = "Option::is_none")]
    maybe: Option<String>,
}
```

## Collections & maps
- `[r.collections.vec]` Vec/sequence round‑trips.
- `[r.collections.set]` Sets (HashSet/BTreeSet) — BTreeSet ordering deterministic.
- `[r.collections.map]` Maps with string keys; non‑string keys via newtypes (e.g., path types).
- `[r.collections.node-name-key]` Node‑name‑as‑key: hierarchical formats only (element name carries data). Flat formats: ignore; use normal key/field name.
- `[r.collections.tuple-repr]` Tuple structs/variants: prefer arrays for text formats; document choice.
- `[r.collections.array]` Fixed-size arrays `[T; N]`: test parsing/serialization; in binary, no length prefix; in text, represent as arrays with exact length enforced.

## Scalars & strings
- `[r.scalars.strings]` Escaping rules; raw/multiline where supported.
- `[r.scalars.numbers]` Booleans, null, numeric boundaries; special floats policy (NaN/∞) explicit.

## Binary data & temporal types
- `[r.bytes.text]` Bytes in text formats: choose a representation (array-of-u8 like facet-json, or base64/hex for compactness). Document and test both directions.
- `[r.bytes.binary]` Bytes in binary formats: raw bytes with length prefix; ensure overflow/length checks.
- `[r.temporal.iso8601]` Temporal types in text formats: use ISO‑8601 strings (e.g., `"2024-01-15T10:30:00Z"` for instants; `"2024-01-15"` for dates; `"10:30:00"` for times).
- `[r.temporal.msgpack]` If the format has native timestamp types (e.g., msgpack ext type), prefer them; otherwise fall back to strings.
- `[r.temporal.binary]` Binary formats: pick a canonical encoding (e.g., i64 seconds + u32 nanos, or format-native) and keep it stable.
- `[r.temporal.coverage]` Cover common crates: chrono (`DateTime<Utc>`, NaiveDate, NaiveDateTime, NaiveTime); time (OffsetDateTime, PrimitiveDateTime, Date, Time); jiff (Timestamp, Zoned, Date, Time; `Span` as a duration); std (SystemTime as instant, Duration as span).

## Unknown data
- `[r.unknown.default]` Default: unknown fields skipped.
- `[r.unknown.deny]` With deny_unknown_fields: reject unknown keys/elements/extra bytes; message should mention offending key.

## Diagnostics & spans
- `[r.diagnostics.errors]` Parse errors should be meaningful.
- `[r.diagnostics.spans]` Spans: text formats -> byte range/line+col; binary -> byte offset/len plus field/variant name.

## Binary formats (postcard/bincode, etc.)
- `[r.binary.extra]` No field names: extra bytes => error; ordering fixed by type.
- `[r.binary.roundtrip]` Primary validation: encode → decode round‑trip; overflow/underflow must error (no wrapping).
- `[r.binary.spans]` Spans as byte offsets/lengths; include field/variant name in diagnostics.
- `[r.binary.encoding]` Define endianness/varint and length‑prefix policy; keep it stable.

## Custom hooks
- `[r.custom.deserialize_with]` `deserialize_with` on fields (scalars and inside flatten); test success + failing parse.
- `[r.custom.serialize_with]` `serialize_with` on fields; test round-trip and skipped/failed cases.
- `[r.custom.skip_if]` `skip_serializing_if` predicates (e.g., `Option::is_none`) — ensure skipped when predicate true, present otherwise.
- `[r.custom.type_tag]` `type_tag` support (if your format uses explicit type tags); document/decide representation or mark unsupported.

## Round‑trip guarantees
- `[r.roundtrip.idempotent]` Serialize → parse → serialize idempotence for: basic structs, maps, options, flatten (struct + enum), interleaved fields. When order is undefined (maps/sets), assert presence not exact text.
