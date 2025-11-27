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

## Quick API surface
- `[r.api.from]` `from_str` / `from_slice` -> `Result<T, Error>`.
- `[r.api.to]` `to_string` / `to_vec` / `to_writer`.
- `[r.api.deterministic]` Output should be deterministic: stable struct field order; stable map key order (sort for text formats; preserve declaration order for binary).
- `[r.api.errors]` Errors: implement `std::error::Error`; `miette::Diagnostic` if possible. For binary, spans = byte offsets/lengths.

## Implementation tiers
- `[r.tier.m1]` **Tier 1 (MVP):** scalars, structs, `Option<T>`, `Vec<T>`, round-trip equality.
- `[r.tier.m2]` **Tier 2 (Complete):** enums (unit/newtype/struct/tuple), flatten (struct + enum), maps with string keys, deny‑unknown‑fields, tuple structs/variants, unit structs.
- `[r.tier.m3]` **Tier 3 (Advanced):** spans/diagnostics, `deserialize_with`, non‑string map keys, value‑based disambiguation, `Option<flatten>`, flatten defaults, skip serialize/deserialize, 128‑bit ints, recursive types.

## Option & defaults
- `[r.option.missing]` Option without default: missing = error; explicit null = None. (Matches facet-json behaviour.)
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
9. `[r.flatten.option]` Option<flatten>:
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
11. `[r.flatten.tuple]` Tuple structs / variants:
```rust
struct Point(i32, i32);
enum Pairish { Pair(u8, u8), Unit }
// Choose array representation for text formats; ordered fields for binary.
```
12. `[r.flatten.unit-struct]` Unit structs / markers:
```rust
struct Marker;
```
13. `[r.flatten.recursive]` Recursive:
```rust
struct Node { name: String, kids: Vec<Node> }
```
14. `[r.flatten.result]` Result<T, E> (if supported):
```rust
struct Wrapper { outcome: Result<u8, String> }
```
*Suggested representation:* tagged form `{ "Ok": <value> }` / `{ "Err": <error> }` for text formats; for binary, use enum variant index + payload.
15. `[r.flatten.skip]` Skip serialize / skip deserialize:
```rust
struct Hidden { visible: u8, #[facet(skip_serializing)] transient: u8, #[facet(skip_deserializing)] cache: u8 }
```
*Expectation:* skipped-on-deserialize fields must be initializable (e.g., Default); input values for them are ignored if present.

16. `[r.flatten.rename]` Rename / rename_all:
```rust
#[facet(rename_all = "camelCase")]
struct Config {
    #[facet(rename = "serverHost")]
    server_host: String,
}
```

17. `[r.flatten.transparent]` Transparent newtypes:
```rust
#[facet(transparent)]
struct PathBufLike(String); // serializes/deserializes as inner String directly
```

## Collections & maps
- `[r.collections.vec]` Vec/sequence round‑trips.
- `[r.collections.set]` Sets (HashSet/BTreeSet) — BTreeSet ordering deterministic.
- `[r.collections.map]` Maps with string keys; non‑string keys via newtypes (e.g., path types).
- `[r.collections.node-name-key]` Node‑name‑as‑key: hierarchical formats only (element name carries data). Flat formats: ignore; use normal key/field name.
- `[r.collections.tuple-repr]` Tuple structs/variants: prefer arrays for text formats; document choice.

## Scalars & strings
- `[r.scalars.strings]` Escaping rules; raw/multiline where supported.
- `[r.scalars.numbers]` Booleans, null, numeric boundaries; special floats policy (NaN/∞) explicit.

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

## Round‑trip guarantees
- `[r.roundtrip.idempotent]` Serialize → parse → serialize idempotence for: basic structs, maps, options, flatten (struct + enum), interleaved fields. When order is undefined (maps/sets), assert presence not exact text.
