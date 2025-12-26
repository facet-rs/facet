# Handoff: Remove tokio-macros/syn from dependency tree (Issue #109)

## Current State
**DONE:**
- Created `crates/rapace-axum-serve` - minimal `axum::serve` replacement using hyper-util's `TowerToHyperService`
- Updated all 8 demo Cargo.toml files to remove `tokio/macros` feature (was unused anyway)
- Updated axum dependencies to use `default-features = false` (excluding the `tokio` feature which pulls in tokio-macros)
- `cargo tree -i tokio-macros` now returns "package not found" - SUCCESS!
- Created skills: `~/.claude/skills/cargo-tree`, `~/.claude/skills/syn`, `~/.claude/skills/unsynn`
- Set up `~/.claude/hooks/block-sed.py` hook to prevent sed usage

**IN PROGRESS:**
- syn is still in tree from: serde_derive, clap_derive (xtask), futures-macro, tracing-attributes, wasm-bindgen-macro-support
- These need addressing per the syn skill (use facet, facet-args, etc.)

## Next Steps
1. Run tests to verify rapace-axum-serve works correctly (`cargo nextest run`)
2. Address remaining syn sources:
   - `clap_derive` in xtask → switch to `facet-args`
   - `serde_derive` → use `facet` (check if serde is actually needed)
   - `futures-macro` → use StreamExt combinators instead of select!/join!
   - `tracing-attributes` → disable with `default-features = false` on tracing (already done in workspace?)
3. Commit changes for issue #109
4. Update issue #109 with progress

## Key Files
- `crates/rapace-axum-serve/src/lib.rs` - The axum::serve replacement using TowerToHyperService
- `Cargo.toml` (workspace) - Updated axum to `default-features = false`
- `demos/dashboard/src/main.rs:864` - Changed `axum::serve` to `rapace_axum_serve::serve`
- `demos/http-tunnel/src/plugin.rs:233` - Changed `axum::serve` to `rapace_axum_serve::serve`
- `demos/http-tunnel/src/baseline.rs:76` - Changed `axum::serve` to `rapace_axum_serve::serve`

## Gotchas
- `axum::serve` requires hyper's Service trait, not tower's - must use `TowerToHyperService` adapter from hyper-util (enable `service` feature)
- The Builder must be bound to a variable before calling `serve_connection` (borrow checker)
- When checking dependencies, ALWAYS use `cargo tree -i <pkg> -e normal -e features` - the `-e features` is critical to see WHY something is pulled in
- User has strong opinions about syn avoidance - there's ALWAYS an alternative, never say "unavoidable"
- The sed-blocking hook triggers on "serde" because it contains "sed" - may need regex fix

## Bootstrap
```bash
# Verify tokio-macros is gone and check remaining syn sources
cargo tree -i tokio-macros -e normal -e features 2>&1 && cargo tree -i syn -e normal -e features --depth 2
```
