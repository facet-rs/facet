# facet-json Weavy parity grid

This file tracks the work needed before the Weavy deserializer can become the
main `facet-json` deserialization backend.

Scope: deserialization only. Serialization has its own path today.

## Legend

- **Covered**: Weavy lowers and runs this surface, with direct tests or fuzz
  oracle coverage.
- **Partial**: part of the surface works, but coverage or semantics are not yet
  enough for a default switch.
- **Gap**: the current Weavy lowerer rejects this shape or there is no Weavy
  entrypoint yet.

## Promotion gates

We should not switch `from_str` / `from_slice` to Weavy by default until:

1. Every **Gap** row in the deserialization grid is either implemented or
   deliberately kept on an explicit compatibility adapter.
2. Every **Partial** row has a direct Weavy test and an oracle/fuzz story when
   the surface is broad enough.
3. Unsupported shapes fail predictably at plan build time, not halfway through
   partially initialized output.
4. Native JIT fallback is observable through `jit_fallback_report()` whenever
   `build_jit()` cannot stay native.
5. Error parity requirements are written down per surface. Success/failure
   parity is required broadly; exact diagnostic parity can stay narrower.

## Deserialization grid

| Surface | Status | Current evidence | Next action |
|---|---|---|---|
| Owned `from_str` / `from_slice` replacement | Partial | Public default APIs still use `facet-format`; opt-in APIs are `from_str_weavy`, `from_slice_weavy`, and reusable `JsonWeavyPlan`. | Switch defaults only after this grid is green or explicitly adapter-backed. |
| Borrowed output APIs | Gap | Weavy APIs require `T: Facet<'static>`; default APIs include `from_str_borrowed` and `from_slice_borrowed`. | Decide whether borrowed output is a Weavy lifetime mode or a compatibility adapter. |
| JSONC APIs | Gap | Default APIs have `from_str_jsonc`, `from_slice_jsonc`, and borrowed JSONC variants; Weavy has no JSONC entrypoint. | Add JSONC parser mode to Weavy plans or leave JSONC on a named adapter. |
| Scalar roots | Partial | The lowerer has a scalar root path, but direct tests mainly exercise scalars inside structs, options, lists, and maps. | Add direct root scalar parity tests for bool, integer, float, string, char, and unit/null. |
| Named structs with scalar fields | Covered | `weavy_deserializes_named_struct_scalars`, wide scalar tests, ordered/out-of-order tests, and fuzz oracle shapes. | Keep extending generated oracle shapes as scalar policies change. |
| Named structs with nested fields | Covered | `Person`, `PointList`, `MapHolder`, and recursive `Node` tests cover nested options, lists, maps, structs, and pointers. | Add larger mixed nesting to the fuzz shape bank. |
| Field matching: rename, alias, escaped keys | Partial | Lowering uses `effective_name()` and `alias`; direct tests cover alias and escaped field names. | Add direct Weavy tests for `rename`, `rename_all`, and rename-vs-alias precedence. |
| Unknown fields and strict skip | Covered | Tests cover skipped unknown containers, raw-key UTF-8 validation, and invalid skipped values from fuzz replay. | Expand fuzz input around skipped strings, nested arrays, and malformed numeric tokens. |
| `deny_unknown_fields` | Covered | `weavy_reports_unknown_field_after_raw_key_matching` exercises the strict path. | Add nested strict-struct coverage. |
| Duplicate fields | Covered | Tests cover duplicate fields after ordered matching and duplicate defaulted fields. | Keep duplicate cases in the fuzz shape bank once more shapes are added. |
| Missing fields and defaults | Partial | Default trait fields, absent options, absent required vec/map fields, and null scalar defaults are covered. | Add direct custom default function coverage. |
| Scalar coercions and range checks | Covered | Tests cover numeric strings, null scalar defaults, float-as-string, and out-of-range narrowing parity. | Add root-scalar coercion cases with the scalar-root tests. |
| `Option<T>` | Covered | Top-level null option, absent option field, `Option<String>`, and `Vec<Option<u16>>` are covered. | Add `Option<struct>` and `Option<map>` oracle shapes. |
| Lists / `Vec<T>` | Covered | Scalar lists, struct lists, nullable scalar lists, direct-list adoption, and drop-on-error tests are covered. | Add nested list-of-list and list-of-map oracle shapes. |
| String-key maps | Covered | `HashMap<String, String>`, map values as vectors/structs, duplicate keys, replacement drops, and value-error drops are covered. | Add `BTreeMap` / `IndexMap` parity rows if those vtables differ materially. |
| Non-string scalar map keys | Partial | Weavy supports exact-width signed/unsigned integer keys with direct default-path parity tests. Float-like keys and enum unit-variant keys are still default-only. | Add float-like and enum map-key parity or split those into dedicated rows. |
| Sets | Covered | Weavy lowers set shapes through `SetDef` init/insert vtables; direct tests and oracle-bank shapes cover `BTreeSet`, `HashSet`, nullable scalar elements, nested struct elements, duplicates, and drop-on-error. | Consider a bulk `from_slice` optimization once raw-builder ownership is explicit. |
| Smart pointers | Partial | Pointer lowering is generic when `new_into` exists; recursive `Box<Node>` is covered. | Add `Box<T>`, `Rc<T>`, `Arc<T>`, `Box<str>`, `Rc<str>`, `Arc<str>`, and `Arc<[T]>` parity tests. |
| Recursive shapes | Covered | `Node { child: Option<Box<Node>> }` and stats tests cover recursive block calls. | Add recursive list/map payloads when those shapes enter the oracle bank. |
| Tuple structs, tuple values, newtypes | Gap | The lowerer rejects non-named structs. | Decide JSON shape for tuple/newtype lowering and add format-suite parity cases. |
| Enums: externally tagged | Partial | Weavy lowers `repr(int)` external unit, renamed unit, newtype, multi-field tuple, struct, and simple `#[facet(other)]` fallback variants, with payload cleanup coverage. Cow-like enums, tag/content-capturing fallbacks, and non-`repr(int)` enums still stay default-only. | Add cow/fallback metadata handling after the enum guard machinery is broader. |
| Enums: internally tagged | Gap | Default path covers internal tags, nested internal tags, rename rules, and flatten inside variants. | Implement after basic enum dispatch exists; preserve tag-order-independent matching. |
| Enums: adjacently tagged | Gap | Default path covers `tag` + `content` layouts and rename rules. | Implement after internal/external dispatch machinery is reusable. |
| Enums: untagged and mixed tagged/untagged | Gap | Default path has untagged and mixed tagged/untagged tests. | Needs replayable candidate decoding and rollback semantics in Weavy. |
| `#[facet(other)]` fallback variants | Partial | Simple single-field external fallbacks decode unknown object tags and bare payloads through Weavy. Tag/content-capturing fallback metadata remains default-only. | Add metadata-bearing fallback variants when the Weavy path can preserve variant tags as values. |
| Flattened fields and flattened maps | Gap | The lowerer rejects flattened fields; default path has struct flatten, optional flatten, nested flatten maps, and flattened enum cases. | Implement object-field routing before or alongside enum work. |
| Skipped deserialize fields | Gap | The lowerer rejects skipped fields with flattened fields. | Add missing-field synthesis that respects skip-deserializing policy. |
| Proxies and format-specific proxies | Gap | The lowerer rejects container and format proxies. Default path has field/container/json-specific proxy tests. | Decide whether proxy lowering goes through Weavy child programs or an adapter call. |
| Transparent wrappers | Gap | Transparent/newtype-like wrappers are not handled because non-named structs/proxies are rejected. | Fold into newtype/transparent lowering design. |
| Raw JSON capture | Gap | Default path has `RawJson` tests for scalar, array, object, nested, and optional capture. | Add parser span capture as a Weavy intrinsic or adapter operation. |
| Parsed/custom scalar-like types | Partial | Scalar writers can use parse hooks, but there is no broad Weavy test row for third-party scalar-like types. | Add representative `FromStr`/parse-hook types from the format suite. |
| Third-party reflected types | Partial | Default path covers chrono, time, uuid, camino, compact strings, tendril, bytes, decimal, and more. | Split into scalar-like, collection-like, and proxy-like rows as each enters Weavy. |
| Error diagnostics and paths | Partial | Weavy tests mostly assert success/failure parity or selected error kinds, not exact message/path parity. | Define which diagnostics must match before the default switch. |
| Fuzz oracle shape bank | Partial | The current bank covers points, wide scalars, defaults, person/options/lists, point lists, string maps, and map holders. | Add enums, flatten, non-string maps, pointers, raw JSON, and proxy shapes as each surface lands. |

## Native JIT grid

Native JIT support is narrower than Weavy interpreter support. `build_jit()` may
still run through the interpreter, and that fallback must remain visible.

| Surface | Status | Current evidence | Next action |
|---|---|---|---|
| Native availability | Covered | Tests expect native JIT on `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`; other targets report fallback. | Keep platform checks in CI as new native backends appear. |
| Root scalar struct, ordered fields | Covered | `weavy_jit_plan_uses_native_for_root_scalar_struct_when_available`, wide scalar JIT tests. | Keep stax/CodSpeed tracking against serde and interpreter. |
| Root list of scalar structs, ordered fields | Covered | `weavy_jit_plan_uses_native_for_root_scalar_struct_list_when_available`, float/wide list JIT tests. | Add larger list shapes to reduce microbench bias. |
| Out-of-order fields and unknown fields | Partial | JIT-requested plans replay/fall back to interpreter for non-ordered objects and skipped unknowns. | Move field dispatch and unknown skip into native stencils. |
| Defaults, missing fields, duplicates | Partial | JIT-requested plans fall back for defaulted structs and duplicate detection. | Add native default/duplicate handling or make fallback costs explicit in benches. |
| Nested structs, options, maps, pointers | Gap | Native root support is scalar structs or scalar-struct lists only. | Share native field/write stencils before adding nested continuation support. |
| Enums, flatten, proxies, raw JSON | Gap | Interpreter support is also missing today. | Do not JIT these until the interpreter parity rows exist. |
| Scanner work inside JIT | Partial | Native path uses JSON stencils but still relies on host/parser behavior for many decisions. | Move punctuation, field-key matching, scalar validation/parsing, and skip primitives into native stencils. |

## Existing performance coverage

| Benchmark file | What it covers | Notes |
|---|---|---|
| `benches/typeplan_reuse.rs` | Reused default type plans, Weavy interpreter, Weavy JIT-requested plans, serde baselines, scalar structs, defaults, lists, skipped unknowns, maps, and batch reuse. | Main microbench suite; report Weavy-vs-serde ratios. |
| `benches/citm.rs` | Large nested object/list workload with rename-all field names. | Good for parser/object traversal and JIT fallback pressure. |
| `benches/twitter.rs` | Larger real-ish object graph with defaults, renames, strings, and lists. | Good for owned-string and defaults pressure. |
| `benches/canada.rs` | GeoJSON-like nested arrays and renamed `type` fields. | Good for list-heavy object graphs. |

## Suggested next grid rows to make green

1. Add explicit Weavy tests for root scalars, `rename` / `rename_all`, custom
   defaults, and representative parsed scalar-like types. These are likely
   already close to supported and reduce uncertainty cheaply.
2. Extend non-string map keys beyond integers. This keeps map-key scalar
   parsing moving before enum dispatch lands.
3. Continue enum lowering by using the same guard machinery for internally and
   adjacently tagged variants.
