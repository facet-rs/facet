# Canonical cross-format validation matrix for facet\*

This list is the shared contract every `facet-*` format crate (JSON, KDL, YAML, TOML, etc.) should satisfy. Port these scenarios into each crate’s tests to guarantee consistent behaviour.

## Shapes, naming, and enums
- Children, `children`, `argument`, `arguments`, `property` fields; `rename` and `rename_all` (snake ↔ kebab/Pascal); node-name-as-key map pattern; type annotations (where the format supports them) to disambiguate enums.
- Enum disambiguation by: property presence, child presence, explicit type annotation, value range/type (e.g., u8 vs u16, signed vs unsigned, int vs float vs string), and detection of truly ambiguous cases (identical fields) plus mixed-field error cases.

## Option and defaults
- `Option<T>` without `default` must be provided (or `null`), and omission is an error.
- `Option<T>` with `default` can be omitted; explicit `null` still works.
- `Option<flattened>`: absent → `None`; present → `Some`, partial fields fill defaults.
- Optional child nodes (`#[facet(child, default)]`) absent vs present.

## Flatten solver
- Flatten structs and enums; nested and multiple flatten layers.
- Interleaved properties (parent + flattened) still parse.
- Child-based disambiguation inside flattened enums/structs.
- Overlapping fields across variants: ambiguous → error; mixed fields from different variants → error.
- Flattened unit variants; flattened with `default` (absent uses Default, present parses normally).
- Duplicate-field detection when parent and flattened define the same name.

## Collections and maps
- Vec/sequence round-trips; set types (HashSet/BTreeSet) including deterministic ordering for BTreeSet.
- Maps with string keys; transparent/non-string keys via newtypes (e.g., `Utf8PathBuf`); node-name-as-key maps; ordering-insensitive assertions for HashMap/HashSet.

## Scalars and strings
- Escaping rules; raw/multiline strings (or format equivalent); booleans, null, and numeric boundary values.
- Special float values if the format allows; verify accepted/rejected cases match policy.

## Unknown data handling
- Default behaviour (unknown properties/children skipped) versus `deny_unknown_fields` rejection, including flattened cases. Ensure errors mention offending keys.

## Diagnostics and spans
- Parse errors surface meaningful messages.
- Spanned values propagate offsets; semantic validation example proves spans line up.

## Pointer/newtype transparency
- Box/Rc/Arc (or format-appropriate smart pointers) as arguments/properties/children.
- Transparent newtypes (e.g., `Utf8PathBuf`, path types) as keys and values.

## Custom hooks
- `deserialize_with` / custom converters on argument, property, and flattened fields; ensure error paths surface.

## Round-trip guarantees
- Serialize → parse → serialize idempotence for representative shapes: basic structs, maps, options, flatten (struct + enum), and interleaved property ordering. When ordering is undefined (maps/sets), assert presence rather than exact string.
