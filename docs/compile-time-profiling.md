
## Benchmarking Compile Times

### Feature Combinations

The `facet-bloatbench` crate has separate features for fair comparisons:

```bash
# facet only (no json)
cargo build -p facet-bloatbench --features facet

# serde only (no json)  
cargo build -p facet-bloatbench --features serde

# facet + facet-json (no serde)
cargo build -p facet-bloatbench --features facet_json

# serde + serde_json (no facet)
cargo build -p facet-bloatbench --features serde_json

# Both together (for comparison with real-world usage)
cargo build -p facet-bloatbench --features json
```

### Collecting Self-Profile Data

```bash
# Clean and profile facet_json
rm -rf profile && mkdir -p profile
cargo clean -p facet-bloatbench
RUSTFLAGS="-Zself-profile=profile -Zself-profile-events=default,args" \
    cargo +nightly build -p facet-bloatbench --features facet_json

# Convert to chrome trace format
crox profile/facet_bloatbench-*.mm_profdata
gzip -f chrome_profiler.json

# Open in https://ui.perfetto.dev/
```

### Querying Profile Data with DuckDB

```bash
zcat chrome_profiler.json.gz | duckdb -c "
SELECT name, count(*) as cnt, sum(dur)/1000 as total_ms
FROM read_json_auto(/dev/stdin)
GROUP BY name
ORDER BY total_ms DESC
LIMIT 20
"
```

### Current Results (as of $(date +%Y-%m-%d))

| Feature | Debug | Release |
|---------|-------|---------|
| facet_json only | 5.24s | 2.12s |
| serde_json only | 5.95s | 3.57s |
| **facet improvement** | **12% faster** | **40% faster** |

