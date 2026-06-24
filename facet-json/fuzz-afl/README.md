# facet-json AFL Harness

This harness fuzzes the opt-in Weavy deserializer against the default
`facet_json` deserializer over a fixed bank of Facet shapes.

Raw JSON bytes alone are not enough for useful coverage here: the interesting
input is `(target shape, JSON bytes, backend)`. The harness therefore selects a
target shape, then compares:

- `facet_json::from_slice::<T>`
- `JsonWeavyPlan::<T>::build().from_slice`
- `JsonWeavyPlan::<T>::build_jit().from_slice`

Successful decodes must produce the same value. If the default path rejects the
input, both Weavy paths must reject it too.

## Prerequisites

```bash
cargo install cargo-afl
```

## Build

```bash
cd facet-json/fuzz-afl
cargo afl build --bin weavy_shape_bank
```

## Fuzz

```bash
cargo afl fuzz -i in -o out target/debug/weavy_shape_bank
```

Seed files may optionally start with `<digit>\n` to select a shape explicitly.
Without that prefix, the first input byte selects the shape and the full input is
used as JSON bytes.
