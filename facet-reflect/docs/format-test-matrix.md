# Canonical cross-format validation matrix for facet\*

How to use: for each bullet, pick or adapt one skeleton test, replace `FORMAT` with your crate’s API, and map facet concepts into your wire format. Example mappings:
- Text/tree formats (JSON/KDL/YAML/TOML/XML): `property` → attribute/key/value, `child` → child element/object/node, node-name-as-key → element name/object key. Decide whether ordering matters (XML may; JSON/YAML typically not) and assert accordingly.
- Binary formats (bincode/postcard): there is no external field name to skip, so “unknown fields” translates to extra data → error; “node-name-as-key” doesn’t apply; ordering is fixed by the type definition—encode/decode round-trips are the main guardrail.
- Streaming: currently out of scope; assume full-buffer APIs.

## Quick start / API surface
- Provide `from_str<T: Facet>(input: &str) -> Result<T, Error>` (or `from_slice` for binary). A reader-based API is optional; if implemented, note solver expectations (it buffers anyway today).
- Provide `to_string` / `to_vec` / `to_writer` as appropriate; deterministic output is preferred for testability.
- Error type: format-specific but should implement `std::error::Error`; if possible also `miette::Diagnostic`. For binary formats, spans can be byte offsets/lengths.
- Hook points: implement your format’s `Serializer`/`Deserializer` that satisfy the `facet-reflect` visitor contracts.
- Serialization contract: define whether `None` fields are omitted or emitted as explicit nulls; define key/attribute ordering (stable/deterministic recommended); note indentation/pretty defaults if applicable.

## Shapes, naming, and enums
- Children, `children`, `argument`, `arguments`, `property` fields; `rename` and `rename_all` (snake ↔ kebab/Pascal); node-name-as-key map pattern; type annotations (where the format supports them) to disambiguate enums.
- Enum disambiguation by: property presence, child presence, explicit type annotation, value range/type (e.g., u8 vs u16, signed vs unsigned, int vs float vs string), and detection of truly ambiguous cases (identical fields) plus mixed-field error cases.

**Skeleton:** *(replace `FORMAT` with kdl/json/yaml/toml helpers)*
```rust
#[derive(Facet, PartialEq, Debug)]
enum Kind { Small { #[facet(property)] v: u8 }, Large { #[facet(property)] v: u16 } }

#[test]
fn enum_value_disambiguation() {
    let small: Root = facet_FORMAT::from_str(r#"v=10"#).unwrap();
    let big: Root  = facet_FORMAT::from_str(r#"v=1000"#).unwrap();
    assert!(matches!(small.kind, Kind::Small { .. }));
    assert!(matches!(big.kind,   Kind::Large { .. }));
}
```

## Option and defaults
- `Option<T>` without `default` must be provided (or `null`), and omission is an error.
- `Option<T>` with `default` can be omitted; explicit `null` still works.
- `Option<flattened>`: absent → `None`; present → `Some`, partial fields fill defaults.
- Optional child nodes (`#[facet(child, default)]`) absent vs present.
 - Null vs absent: for Option fields, `null` and “missing” both map to `None`; for non-Option fields, `null` is an error.

**Skeleton:**
```rust
#[derive(Facet, Debug, PartialEq)]
struct Server {
    #[facet(argument)] host: String,
    #[facet(property)]               // no default
    port: Option<u16>,
    #[facet(property, default)]      // optional
    timeout: Option<u32>,
}
```

## Flatten solver
- Flatten structs and enums; nested and multiple flatten layers.
- Interleaved properties (parent + flattened) still parse.
- Child-based disambiguation inside flattened enums/structs.
- Overlapping fields across variants: ambiguous → error; mixed fields from different variants → error.
- Flattened unit variants; flattened with `default` (absent uses Default, present parses normally).
- Duplicate-field detection when parent and flattened define the same name.

**Skeletons to port (replace `FORMAT`):**

1) **Struct flatten + interleaved props**
```rust
#[derive(Facet, PartialEq, Debug)]
struct Root { #[facet(child)] svc: Svc }
#[derive(Facet, PartialEq, Debug)]
struct Svc {
    #[facet(argument)] name: String,
    #[facet(property)] enabled: bool,
    #[facet(flatten)] conn: Conn,
}
#[derive(Facet, PartialEq, Debug)]
struct Conn { #[facet(property)] host: String, #[facet(property)] port: u16 }

let r: Root = facet_FORMAT::from_str(r#"svc "api" host="h" enabled=#true port=80"#).unwrap();
assert_eq!(r.svc.conn.port, 80);
```
*XML hint:* element `<svc name="api" host="h" enabled="true" port="80"/>`; test both grouped and interleaved attribute orders if your parser allows.

2) **Flatten enum by property presence**
```rust
#[derive(Facet, PartialEq, Debug)]
enum Backend {
    File { #[facet(property)] path: String },
    Db   { #[facet(property)] url: String, #[facet(property)] branch: String },
}
// path only -> File; url+branch -> Db; url without branch → error (incomplete Db)
```
*Binary (postcard/bincode) hint:* variants are distinct; “incomplete” translates to decode error due to missing required field bytes.

3) **Flatten enum child disambiguation**
```rust
#[derive(Facet, PartialEq, Debug)]
enum Mode {
    Tuned { #[facet(child)] tuning: Tuning, #[facet(property)] gain: u8 },
    Simple { #[facet(property)] level: u8 },
}
// presence of tuning child picks Tuned; absence picks Simple; gain without tuning → error.
```
*XML hint:* `tuning` child element is mandatory for Tuned; validate that gain alone does not pick Tuned.

4) **Nested flatten (struct inside struct, enum inside)**
```rust
struct Server {
    #[facet(argument)] name: String,
    #[facet(flatten)] settings: Settings,
}
struct Settings {
    #[facet(property)] enabled: bool,
    #[facet(flatten)] backend: Backend,
}
enum Backend { Http { #[facet(property)] url: String },
               Grpc { #[facet(property)] addr: String, #[facet(property)] tls: bool } }
```
*Binary hint:* round-trip equality; tamper with payload (drop last byte) should fail decode.

5) **Multiple flattens side-by-side**
```rust
struct Connection {
    #[facet(argument)] name: String,
    #[facet(flatten)] auth: Auth,       // Basic | Token
    #[facet(flatten)] transport: Trans, // Tcp | Unix
}
```
*Expectation:* all 2×2 combos parse; missing required field in either flatten → error.

6) **Unit variant + flattened siblings**
```rust
enum Output { Stdout, File { #[facet(property)] path: String } }
// No props -> Stdout; path -> File; path+extra unknown -> error with deny_unknown_fields.
```
*XML hint:* `<output path="..."/>` → File; `<output/>` → Stdout; unknown attr with `deny_unknown_fields` → error.

7) **Ambiguity & mixed fields**
```rust
enum Kind { A { #[facet(property)] x: u8 }, B { #[facet(property)] x: u8 } }
// x only -> should error (truly ambiguous).

enum Mode { Simple { #[facet(property)] level: u8 },
            Tuned  { #[facet(property)] level: u8, #[facet(child)] tuning: T } }
// level+gain (missing tuning) → error (mixed fields).
```

8) **Value-type disambiguation**
```rust
enum Ints { Small { #[facet(property)] v: u8 }, Large { #[facet(property)] v: u16 } }
// v=255 -> Small; v=1000 -> Large.
enum Signed { Signed { #[facet(property)] n: i8 }, Unsigned { #[facet(property)] n: u8 } }
// n=-5 -> Signed; n=200 -> Unsigned.
```
*Binary hint:* ensure overflow rejects rather than wraps (decode error).

9) **Option<flatten>**
```rust
struct Server {
    #[facet(argument)] name: String,
    #[facet(flatten)] tuning: Option<Tuning>,
}
struct Tuning { #[facet(property)] ttl: u32, #[facet(property)] strategy: Option<String> }
// absent -> None; ttl only -> Some { strategy: None }; ttl+strategy -> Some { ... }.
```
*Binary hint:* absent = zero-length/flag; present fills fields; partial present uses defaults/None.

10) **Flatten with Default**
```rust
struct Server {
    #[facet(argument)] name: String,
    #[facet(flatten, default)] limits: Limits, // Default fills when absent
}
#[derive(Default)]
struct Limits { #[facet(property, default)] max: u32, #[facet(property, default)] burst: u32 }
```

11) **Tuple structs / variants**
```rust
#[derive(Facet, PartialEq, Debug)]
struct Point(#[facet(argument)] i32, #[facet(argument)] i32);

#[derive(Facet, PartialEq, Debug)]
enum Pairish {
    Pair(#[facet(argument)] u8, #[facet(argument)] u8),
    Unit,
}
```

12) **Unit structs / markers**
```rust
#[derive(Facet, PartialEq, Debug)]
struct Marker;
```

13) **Recursive**
```rust
#[derive(Facet, PartialEq, Debug)]
struct Node {
    #[facet(argument)] name: String,
    #[facet(children)] kids: Vec<Node>,
}
```

14) **Result<T, E> (if supported by your format)**
```rust
#[derive(Facet, PartialEq, Debug)]
struct Wrapper { #[facet(child)] outcome: Result<u8, String> }
```

## Collections and maps
- Vec/sequence round-trips; set types (HashSet/BTreeSet) including deterministic ordering for BTreeSet.
- Maps with string keys; transparent/non-string keys via newtypes (e.g., `Utf8PathBuf`); node-name-as-key maps; ordering-insensitive assertions for HashMap/HashSet.

**Skeleton:**
```rust
let cfg: Root = facet_FORMAT::from_str(INPUT).unwrap();
assert_eq!(cfg.map.get("key"), Some(&"val".into()));
// BTreeSet -> assert sorted iteration; HashMap -> assert contains, not order.
```

## Scalars and strings
- Escaping rules; raw/multiline strings (or format equivalent); booleans, null, and numeric boundary values.
- Special float values if the format allows; verify accepted/rejected cases match policy.

**Skeleton:** test string with `\n`, `\"`, and a raw/multiline form; assert parsed value and round-trip escaped output.

## Unknown data handling
- Default behaviour (unknown properties/children skipped) versus `deny_unknown_fields` rejection, including flattened cases. Ensure errors mention offending keys.

**Skeleton:** two tests: one without `deny_unknown_fields` (parses, ignores), one with (fails and mentions key).
*XML hint:* treat unknown attributes/elements the same way; if order matters, keep fixtures minimal.  
*Binary hint:* “unknown” == trailing/extra bytes → decoding must fail.

## Diagnostics and spans
- Parse errors surface meaningful messages.
- Spanned values propagate offsets; semantic validation example proves spans line up.

**Skeleton:** parse into `Spanned<u32>`, validate range, return error with recorded span; assert rendered message highlights offending slice.
*Binary hint:* use byte offset/length for spans; if you cannot map to line/col, still surface offsets and the field/variant name in the message.

## Pointer/newtype transparency
- Box/Rc/Arc (or format-appropriate smart pointers) as arguments/properties/children.
- Transparent newtypes (e.g., `Utf8PathBuf`, path types) as keys and values.

**Skeleton:** `#[facet(argument)] path: Utf8PathBuf` round-trip; `#[facet(argument)] value: Box<u64>` deserializes.

## Custom hooks
- `deserialize_with` / custom converters on argument, property, and flattened fields; ensure error paths surface.

**Skeleton:** hex-string parser via `deserialize_with`, test good/errored input.

## Round-trip guarantees
- Serialize → parse → serialize idempotence for representative shapes: basic structs, maps, options, flatten (struct + enum), and interleaved property ordering. When ordering is undefined (maps/sets), assert presence rather than exact string.

**Skeleton:** create value with options + flatten + maps → `let text = to_string(&value); let reparsed = from_str(&text); assert_eq!(reparsed, value);`.
