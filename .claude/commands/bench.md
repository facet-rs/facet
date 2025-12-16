Run benchmarks and analyze results.

Arguments: $ARGUMENTS (optional: specific benchmark filter or crate)

1. If arguments provided, run specific benchmarks:
   - `cargo bench -p <crate> $ARGUMENTS` if a crate is specified
   - Otherwise filter by benchmark name pattern
2. If no arguments, list available benchmarks:
   - Check for `benches/` directories and `[[bench]]` sections in Cargo.toml files
   - Ask which benchmarks to run

For benchmark comparisons or analysis, check if the project has benchmark tooling in place (like `benchmark-analyzer` or similar).

Common benchmark crates in this project:
- Look for crates with `-bench` suffix or `benches/` directories
