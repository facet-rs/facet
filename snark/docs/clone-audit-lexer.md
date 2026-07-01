# Lexer/scanner hot-path allocation audit

Scope: `snark/src/lexical.rs`, lexing-related code in `snark/src/lower/weavy.rs`
(the `lex()` family and `runtime_weavy_input_point_at`/`runtime_weavy_input_ranges`),
and the `CompiledLexMode`/pattern-matching machinery in `snark/src/parser.rs` that
`lex()` calls into. GLR branch/stack machinery (`RuntimeWeavyBranch`, reduce/tree
plumbing) is out of scope — another agent owns it.

Static audit only; nothing here has been measured against a live profile run in
this pass. Ranked by expected impact given call frequency.

## Current Status

- **Resolved:** byte-to-point lookup now uses a per-parse
  `RuntimeWeavyInputPoints` line-start index instead of rescanning from byte
  zero for every range.
- **Resolved:** direct-pattern matching now reuses
  `RuntimeWeavyStepper::direct_pattern_ends` instead of allocating a fresh
  `Vec<Option<LexMatch>>` per lex call.
- **Open:** `RegexSet::matches` still returns a per-call match result. Removing
  that allocation likely means moving this direct-pattern path to a
  `regex-automata` API with caller-owned cache/match storage.

The findings below are kept as historical provenance. Resolved entries are not
current work items.

## 1. [RESOLVED] `runtime_weavy_input_point_at` rescanned from byte 0 every call — O(n) per call, called ~once per token/reduction → O(n²) total

- **File**: `snark/src/lower/weavy.rs:4401-4413`, called from `runtime_weavy_input_ranges` (`weavy.rs:4385-4399`).
- **Call sites** (all in the lex-adjacent hot path, `weavy.rs`):
  - `SnarkIntrinsic::Shift` handler, `weavy.rs:2703` — once per shifted token (start point + end point).
  - `SnarkIntrinsic::ShiftExtra` handler (same pattern shortly after `weavy.rs:2746`).
  - `runtime_reduce_fragment`, `weavy.rs:3554` (alias byte range) and `weavy.rs:3594` (public-node byte range) — once or twice per grammar reduction.
  - `runtime_extra_fragment`, `weavy.rs:3671` — once per extra (whitespace/comment) fragment closed.
  - `recover_runtime_weavy_no_token`, `weavy.rs:2470` — per error-recovery step.
- **What's wasteful**: `runtime_weavy_input_point_at(input, byte)` does `input[..byte].chars()` and walks every character from the *start of the file* to `byte` to recompute `(row, column)`, every single time it's called. `RuntimeWeavyFragment` (`weavy.rs:3959-3975`) only stores `start_byte`/`end_byte` (plain `usize`), never `PointBytes`, so there is no cached row/column anywhere — every reduce, every shift, every extra-fragment close pays for a full rescan from byte 0.
- **Why it matters**: this isn't a `malloc`, it's pure compute, so it wouldn't show up under the 95% malloc/free/memmove hot spots from the current profile — but it's algorithmically O(n) per call with O(n) calls (roughly one shift + one reduce per token), giving O(n²) total work. For a 181KB file this is bounded today by the malloc costs dominating, but once the bigger `Vec`/`String` clones are fixed elsewhere, this becomes the next wall, and it gets quadratically worse as input size grows (a 1MB file would do ~30x more work here per byte than a 181KB one, not a linear scale-up).
- **Fix**: track an incremental cursor instead of recomputing from scratch:
  - Maintain a `(byte, row, column)` cursor on `RuntimeWeavyStepper`/`RuntimeWeavyBranch`, advanced only over the bytes newly consumed since the last shift (i.e. scan `input[old_byte..new_byte]`, not `input[..new_byte]`). Parsing is monotonic in byte position per branch, so this turns the per-shift cost from O(byte_position) into O(token_length).
  - For reduce/extra-fragment points: store the already-computed `PointBytes` (not just `usize` bytes) on `RuntimeWeavyFragment` when a fragment is created (at shift time, where the point is cheap to get from the cursor above), and reuse those cached points when building the reduced node's `bytes`/`points` instead of re-deriving them from raw byte offsets.
- **Confidence**: high on the algorithmic complexity/mechanism; medium on present-day real-world weight in the 181KB benchmark specifically (may be masked by the malloc costs today), high on it being worth fixing regardless since the fix is cheap and removes a standing quadratic-time landmine.

## 2. [RESOLVED] `match_compiled_direct_pattern_set` allocated a fresh `Vec<Option<LexMatch>>` on every `lex()` call

- **File**: `snark/src/lower/weavy.rs:3287-3306`, specifically `weavy.rs:3295`:
  ```rust
  let mut ends = vec![None; pattern_set.terminal_indices.len()];
  ```
- **Call frequency**: `lex()` (`weavy.rs:2925`) calls this once per token attempt via `weavy.rs:2954-2955`, and `lex()` itself runs once per `SnarkIntrinsic::Lex` step, i.e. **once per token per active GLR branch**. This is squarely per-token, the same granularity as the `RuntimeWeavyBranch.clone()` already found in `step_runtime_weavy_branch`.
- **Why it's wasteful**: `pattern_set.terminal_indices.len()` is a small, fixed-per-mode number known at grammar-compile time (`compile_direct_pattern_set`, `parser.rs:4149-4176`). Allocating a new `Vec` for it on every token is pure per-token malloc/free churn for a buffer whose size never changes for a given lex mode.
- **Fix**: hoist the buffer out of the per-call path. Since `lex()`/`match_compiled_direct_pattern_set` take `&self` (not `&mut self`), either:
  - add a `scratch: RefCell<Vec<Option<parser_ir::LexMatch>>>` (or per-mode `Vec<RefCell<Vec<_>>>`) field to `RuntimeWeavyStepper`, sized once and `clear()`/refilled with `None` each call instead of reallocated, or
  - thread a `&mut Vec<Option<LexMatch>>` scratch buffer down from the `SnarkIntrinsic::Lex` call site (`weavy.rs:2607-2615`), owned by the branch/stepper and reused across all `lex()` calls for that branch.
  Either way the Vec's backing allocation is made once (or once per lex-mode width) and reused, not malloc'd per token.
- **Confidence**: high — the allocation is unconditional whenever a lex mode has a direct-pattern set (which JSON's number/string-body patterns are likely to hit), and the fix is a straightforward buffer-reuse.

## 3. [OPEN] `RegexSet::matches(haystack)` allocates an internal match-set buffer on every `lex()` call

- **File**: `snark/src/lower/weavy.rs:3299` inside the same `match_compiled_direct_pattern_set`:
  ```rust
  let matches = pattern_set.regex_set.matches(haystack);
  ```
- **Call frequency**: same as finding #2 — once per token per branch, whenever the current lex mode has a `direct_pattern_set` (`CompiledLexPatternSet`, `parser.rs:3760-3763`, built once at grammar-compile time in `compile_direct_pattern_set`).
- **Why it's wasteful**: `regex::RegexSet::matches` builds and returns a fresh `SetMatches` result (an internal bitset sized to the number of patterns in the set) on every call rather than writing into a caller-provided buffer. For a grammar with a handful of direct patterns (e.g. JSON's number/whitespace/comment-ish patterns), this is a small-but-nonzero heap allocation on every single token, same frequency as finding #2.
- **Fix**: keep a single reusable match-buffer alive across calls and use the buffer-reuse entry point of the `regex` crate (check whether `regex` 1.12's `RegexSet` still only exposes `matches()` — if the reusable-buffer API isn't available at the `RegexSet` level, this is more invasive, e.g. dropping to `regex_automata`'s meta engine which does support caller-owned `Cache`/match buffers). At minimum, this should be fixed in the same change as #2 since both live in the same function and both scale as "once per token".
- **Confidence**: high that the allocation exists and recurs per-token; medium on the exact reuse API without checking the installed `regex` crate's docs directly (flagged for verification before implementing, since I did not build/run anything for this audit).

## Lower priority / not worth chasing right now

- `mode_token_spellings` (`weavy.rs:3241-3255`) builds a `Vec<String>` via `.to_owned()` on every terminal/external name — but it's only called on the `RuntimeWeavyError::NoToken` **error path** (`weavy.rs:3050-3054`), not the success path. Not hot for a well-formed 181KB JSON parse.
- `auto_close_specs()` / `update_auto_close_stack` (`weavy.rs:3108-3219`) do a `flat_map` scan over all compiled lex modes' terminals per shifted token, plus occasional `spec.tag.clone()` `String` clones on stack push/pop. This only matters for grammars using `AutoClose` (XML/HTML/JSX-style tag matching) — JSON's grammar has no auto-close specs, so this is dead weight for the profiled workload specifically, but worth a look if/when an HTML-like grammar is profiled.
- `ExternalScannerToken::from_fact` (`lexical.rs:117-125`) and everything in `lexical.rs`/`compile_lex_modes`/`compile_terminal_matcher` (`parser.rs:4123-4330`) run once per grammar compile, not per parse — not part of the per-parse hot path regardless of allocation shape.

## Summary (ranked)

1. **Resolved:** byte-to-point lookup uses `RuntimeWeavyInputPoints`, not a
   full rescan from byte zero.
2. **Resolved:** direct-pattern result storage uses
   `RuntimeWeavyStepper::direct_pattern_ends`, not a per-token allocation.
3. **Open:** `RegexSet::matches()` still allocates/returns a fresh match result
   per call; fix needs a reusable-buffer API path, likely via `regex-automata`.
