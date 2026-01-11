# Phase 001: Assess Swift Baseline

**Status**: TODO

## Objective

Thoroughly audit the current Swift implementation to understand:
1. What's already implemented and working
2. What's missing compared to Rust/TypeScript
3. What's broken or incomplete
4. What the existing `roam-codegen` Swift target produces

## Background

The Swift implementation exists in `swift/roam-runtime/` and appears functional for basic
wire protocol operations. However, tracey shows 0% spec coverage, suggesting either:
- No `// [impl ...]` annotations exist
- The implementation is incomplete
- Tests aren't being run against the spec suite

We need a clear picture before proceeding with further work.

## Tasks

### 1. Audit Swift Runtime Files

Review each file in `swift/roam-runtime/Sources/RoamRuntime/`:

| File | Purpose | Questions |
|------|---------|-----------|
| `Postcard.swift` | Primitive encode/decode | Complete? Matches Rust? |
| `Varint.swift` | Variable-length integers | Correct zigzag? Overflow handling? |
| `Wire.swift` | Wire message types | All variants? Correct discriminants? |
| `COBS.swift` | Framing | Correct algorithm? |
| `Channel.swift` | Tx/Rx types | How does binding work? |
| `Binding.swift` | Channel binding | Uses Mirror? Limitations? |
| `Schema.swift` | Schema types | What's defined? |
| `Driver.swift` | Connection driver | Complete message loop? |
| `Transport.swift` | SwiftNIO integration | Working? |
| `RoamRuntime.swift` | Public API | What's exported? |

### 2. Run Existing Tests

```bash
cd swift/roam-runtime && swift test
```

Document:
- How many tests exist?
- Do they all pass?
- What do they cover?
- What's missing?

### 3. Check Golden Vector Compatibility

The Swift tests use golden vectors from `test-fixtures/golden-vectors/`.
Verify:
- Which vectors are tested?
- Do wire encoding tests pass?
- Are there any format mismatches?

### 4. Audit roam-codegen Swift Target

Review `rust/roam-codegen/src/targets/swift/`:

| File | Purpose | Status |
|------|---------|--------|
| `mod.rs` | Entry point | What does `generate_service()` produce? |
| `types.rs` | Type generation | What types can it generate? |
| `schema.rs` | Schema generation | Does it exist? What format? |
| `encode.rs` | Encode generation | How does it work? |
| `decode.rs` | Decode generation | How does it work? |
| `client.rs` | Client stub generation | Complete? |
| `server.rs` | Server dispatcher generation | Complete? |

### 5. Try Running Codegen

```bash
cargo xtask codegen --swift
```

- Does this command exist?
- What does it produce?
- Where does output go?
- Does it compile?

### 6. Run Swift Subject Against Spec Tests

```bash
# Build subject
cd swift/subject && swift build

# Run spec tests
SUBJECT_CMD='swift/subject/subject-swift.sh' cargo nextest run -p spec-tests
```

- Does it build?
- How many tests pass/fail?
- What errors occur?

### 7. Compare with TypeScript Implementation

For each component, note differences:

| Component | TypeScript | Swift | Gap |
|-----------|------------|-------|-----|
| Schema types | `schema.ts` | `Schema.swift` | ? |
| Schema encode | `schema_codec.ts` | ? | ? |
| Schema decode | `schema_codec.ts` | ? | ? |
| Channel binding | `binding.ts` | `Binding.swift` | ? |
| Wire codec | `codec.ts` | `Wire.swift` | ? |

## Deliverables

1. **Gap analysis document** — What's missing, what's broken
2. **Updated overview.md** — Revise phases based on findings
3. **Test results** — Current test pass/fail status
4. **Codegen output** — Sample of what Swift codegen produces

## Success Criteria

1. Clear understanding of current Swift implementation state
2. Identified all gaps compared to Rust/TypeScript
3. Verified or updated the phase plan based on findings
4. Documented any blockers or surprises

## Notes

- The Swift implementation may be more complete than tracey suggests
- Lack of `// [impl ...]` annotations doesn't mean the code is wrong
- The `roam-codegen` Swift target may already do most of what we need
- SwiftNIO integration appears to be working based on the subject code

## Questions to Answer

1. Does Swift have schema-driven encode/decode, or only manual?
2. How does Swift channel binding currently work? Mirror-based?
3. Is the `roam-codegen` Swift target functional and up-to-date?
4. What's blocking the spec tests from passing?
5. Are there Swift-specific challenges (async/await patterns, actor isolation)?
