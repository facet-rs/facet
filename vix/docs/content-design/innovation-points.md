+++
title = "the innovation-points ledger: where looking like Rust lies"
+++

Status: design note from conversation (2026-07-09). Deferred, not
forgotten — Amos: syntax weirdness can wait, but the principle must not
be lost. **TL;DR: looks-like-Rust-but-misbehaves is worse than looking
weird.**

## The principle

V31 ("syntax spends the fewest innovation points") produced the current
surface: Rust minus mutation. That was right as a default and wrong as an
absolute, because familiarity is a claim: syntax that reads as Rust
*promises Rust behavior*, and every place vix quietly keeps the look while
changing the meaning is a trap that costs more trust than an unfamiliar
sigil would have. The uncanny valley applies to languages. Distance can be
honest signaling.

## The known trap inventory (grows as found)

Places where a Rust reader's intuition is wrong, ranked by how well the
current design already defends them:

- **`insert`/`push` return the new value** — DEFENDED: no expression
  statements, so the fire-and-forget shape is a compile error, not a
  silent no-op. The rejection is the teaching. (This defense is the model
  for the rest: where we can't look different, we must *reject* the
  Rust-shaped misuse loudly.)
- **`pop` doesn't exist; `split_last` returns both pieces** — DEFENDED by
  absence + a reject rung.
- **Multiset fold order** — RULED (and the ruling predated this note:
  names carry semantics; the book initially ignored it). Defended by
  distance: multisets have `fold_ascending`, no bare `fold`; arrays keep
  bare `fold` in field order where the look matches the behavior.
- **Arguments are wires, not eager computations** — mostly benign
  (pure code can't observe it) but becomes visible the day someone
  reasons about cost by Rust intuition.
- **`==`/`<=>`/hash ambient on everything** — benign surprise (things
  work that a Rust reader expects to fail), the good direction.
- **Struct-update `..base` on a *record you also renamed fields of*,
  reordering fields changes sort order** (declaration-ordered total
  order) — a semantic meaning Rust never gave field order. Needs at
  least a book callout; possibly a lint-shaped diagnostic.
- **No statements at all** — blocks are wiring. Rust readers will write
  `log(x);` shapes forever. Defense: the expression-statement rejection
  must have a GREAT diagnostic (it's rung 013; the diagnostics band
  111–122 should include this one's message quality specifically).

## The ledger discipline

Every time a trap is found: it gets an entry here, a decision (defend by
rejection / defend by distance / defend by documentation / accept), and —
where the defense is observable — a ratchet rung. Innovation points may
then be spent deliberately, latest possible, where the misbehavior is
worst: the fold-order trap is currently the strongest candidate for
spending one.
