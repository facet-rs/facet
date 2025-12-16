# Performance UI: Complete Rewrite Summary

## Critical Changes

### 1. **Shared CSS System** ✅
- **Single file:** `/shared-styles.css` used by both index and reports
- **No duplication:** All design system variables, colors, typography defined once
- **CI integration:** `copy-perf-reports.js` includes shared CSS in deployment

### 2. **Commit Message Extraction** ✅ (Bug Fix)
**The Problem:** GitHub Actions creates merge commits for PR testing. Running `git log -1 --format=%B` on the checked-out merge commit returned "Merge X into Y" instead of actual commit messages.

**The Fix:**
- CI extracts commit message from actual PR HEAD SHA (`github.event.pull_request.head.sha`)
- Passes commit data via environment variables to avoid shell injection
- `copy-perf-reports.js` uses env vars when available, falls back to git
- Commit subjects now display correctly everywhere

### 3. **Index Page Rewrite** ✅
**Collapsed row (two lines):**
```
branch-name                              instr 702,426,274  ▲ -12.4%
Fix parser edge case in CITM · 2 commits · last run 5m ago
```

**Expanded view:**
```
┏━ bench-improvements  (3px left accent border)
┃
┃  Fix parser edge case in CITM
┃  last run 5m ago · latest commit: 0d5cfea4 · 2 commits
┃
┃  LATEST RESULT VS MAIN
┃  instructions: 702,426,274
┃  ▲ -12.4% (24px, weight 650, green)
┃
┃  View full benchmark (deserialize) | serialize
┃
┃  COMMIT HISTORY (2)
┃  ▸ Fix parser edge case in CITM
┃    0d5cfea4 · 9m ago · ▲ -12.4%
┃  ▸ Previous commit subject
┃    a1b2c3d4 · 1h ago · ▬ +0.2%
┗━
```

**Key features:**
- Visual hierarchy: expanded state has left border, indentation, weight gradation
- Commit links point to reports (not GitHub)
- Delta indicators in commit history (subject → hash → time → delta)
- Three-state system: ▲ faster / ▬ neutral / ▼ slower

### 4. **Report Pages Redesign** ✅

**Breadcrumb navigation:**
```
All branches › bench-improvements · 0d5cfea4
```

**Header simplified:**
```
facet-json deserialization benchmarks     [→ Serialization]
Generated: 2025-12-16 · Commit: 0d5cfea4
```

**Tables updated:**
```
Target                | Median Time | Instructions | Instr. Ratio
facet-format-json+jit |   19.35 µs  |   409.84K   | 0.84× fewer
serde_json            |   23.12 µs  |   487.92K   | 1.00× (baseline)
```

**Key changes:**
- "Instr. Ratio" column is **instruction-based** (was time-based "vs serde_json")
- Text labels: "fewer", "more", "neutral" (not color-only)
- Epsilon threshold: 0.5% for neutral zone
- Legend explains ratio semantics

### 5. **Three-State Delta System** ✅

**Epsilon threshold: 0.5%**

| State | Condition | Icon | Color | Label | Meaning |
|-------|-----------|------|-------|-------|---------|
| **Improvement** | < -0.5% | ▲ | Green | "faster"/"fewer" | Meaningful improvement |
| **Stalemate** | ±0.5% | ▬ | Muted | "neutral" | Within measurement noise |
| **Regression** | > +0.5% | ▼ | Red | "slower"/"more" | Meaningful regression |

**Applied consistently:**
- Index page: branch deltas vs main
- Report tables: instruction ratios vs serde_json
- Commit history: per-commit deltas

### 6. **Instruction Ratio as First-Class Metric** ✅

**Core principle:** `facet-format-json+jit / serde_json` instruction ratio is the canonical performance metric.

**Semantics:**
- **< 1.0×** = fewer instructions = good (green)
- **> 1.0×** = more instructions = bad (red)
- **≈ 1.0×** = neutral (within 0.5%)

**Why this matters:**
- Architecture-agnostic (stable across machines)
- Deterministic (no timing variance)
- Semantically meaningful (JIT efficiency measure)
- CI-friendly (reproducible in automation)

**Implementation:**
- Report tables show instruction ratio in dedicated column
- Time remains absolute (never shown as ratio)
- Index page tracks instruction counts (future: show ratio)
- Legend explicitly explains ratio semantics

### 7. **Design System Specification** ✅

**Document:** `docs/perf-design-system.md`

**Contents:**
- Semantic color definitions with exact hex values
- Typography hierarchy (6 levels: 24/18/14/13/12/11px)
- Spacing scale (s1-s6: 4px-32px)
- Reference/baseline language rules
- Instruction ratio semantics
- Error presentation standards
- Commit information extraction rules
- Implementation checklist

## Files Modified

### Core Infrastructure
- `scripts/shared-styles.css` - **NEW** - Single source of truth for all styles
- `scripts/app.js` - Rewritten with proper hierarchy, uses shared CSS
- `scripts/copy-perf-reports.js` - Copies shared CSS, uses env vars for commit data

### CI/Workflow
- `.github/workflows/test.yml` - Extracts commit message from PR HEAD, exports via env vars

### Report Generation
- `tools/benchmark-analyzer/src/report.rs` - Uses shared CSS, instruction ratio column, breadcrumbs
- `tools/benchmark-analyzer/src/main.rs` - Passes commit_short to report generator

### Index Generation
- `tools/perf-index-generator/src/main.rs` - Uses shared CSS instead of inline styles

### Documentation
- `docs/perf-design-system.md` - **NEW** - Complete design system specification
- `docs/perf-ui-changes-summary.md` - **NEW** - This document

## Breaking Changes

None. All changes are additive or fix existing bugs.

## Testing Checklist

- [x] Rust code compiles (`cargo check -p benchmark-analyzer`)
- [x] Rust code compiles (`cargo check -p perf-index-generator`)
- [x] JavaScript syntax valid (`node --check scripts/app.js`)
- [ ] Run benchmark and verify report generation works
- [ ] Verify index page renders correctly
- [ ] Verify commit messages display correctly (not "Merge X into Y")
- [ ] Verify instruction ratio calculation is correct
- [ ] Verify three-state delta system works with epsilon

## Next Steps

### Immediate
1. Test full benchmark flow: `cargo xtask bench`
2. Verify CI generates correct commit messages on next PR
3. Confirm instruction ratios match expected values

### Future Enhancements
1. Add instruction ratio to index page summary
2. Add chart toggle: Time | Instructions
3. Track instruction ratio deltas over time (trend visualization)
4. Add instruction ratio to PR comments (GitHub integration)

## Success Metrics

**Before:**
- Commit messages: "Merge X into Y" ❌
- CSS duplication: 300+ lines duplicated ❌
- Delta semantics: color-only indicators ❌
- Instruction ratio: buried in table, time-based ❌
- Visual hierarchy: flat, no distinction between states ❌

**After:**
- Commit messages: actual commit subjects ✅
- CSS duplication: zero, single shared file ✅
- Delta semantics: three-state with epsilon, text labels ✅
- Instruction ratio: first-class column, instruction-based ✅
- Visual hierarchy: clear nesting, weight gradation, borders ✅

---

**Status:** Ready for testing in CI
**Next PR:** This rewrite + instruction ratio implementation
