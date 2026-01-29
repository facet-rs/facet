# Classic Benchmark Baselines

Captured: 2026-01-29

## JSON (facet-json)

| Benchmark | facet_json | serde_json | Ratio |
|-----------|------------|------------|-------|
| citm      | 12.7 ms    | 1.59 ms    | 8.0x  |
| twitter   | 5.32 ms    | 702 µs     | 7.6x  |
| canada    | 43.4 ms    | 4.41 ms    | 9.8x  |

## YAML (facet-yaml)

| Benchmark | facet_yaml | serde_yaml | Ratio |
|-----------|------------|------------|-------|
| citm      | 26.0 ms    | 14.6 ms    | 1.8x  |

## TOML (facet-toml)

| Benchmark | facet_toml | toml (serde) | Ratio |
|-----------|------------|--------------|-------|
| citm      | 20.6 ms    | 23.7 ms      | 0.87x |

## Postcard (facet-postcard)

| Benchmark | facet_postcard | postcard (serde) | Ratio |
|-----------|----------------|------------------|-------|
| citm      | 8.78 ms        | 247 µs           | 35.6x |
| canada    | 38.5 ms        | 986 µs           | 39.0x |

---

## Notes

- Ratios > 1.0 mean facet is slower than serde equivalent
- TOML is the only format where facet beats serde (uses deferred mode)
- JSON and Postcard show significant overhead from reflection machinery
- This is the baseline before Partial API refactor (ownership-transfer → &mut self)

## Purpose

These baselines will be used to measure the impact of refactoring `Partial` from
ownership-transfer pattern (`wip = wip.method()?.method()?.end()?`) to `&mut self`
pattern. The current pattern causes 6 moves of the 128-byte Partial struct per field.
