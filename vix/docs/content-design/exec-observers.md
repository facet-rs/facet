+++
title = "exec observers: the zoo feature trees don't replace"
+++

> **STATUS (round 12): FINDINGS INTACT, MECHANISM SUPERSEDED.**
> Everything this note argues about *readiness being a protocol fact* is still true and
> now load-bearing. What is retired is the **observer as a surface construct**. `exec` and
> `place` are decoupled (`r[machine.primitive.exec-is-placement-agnostic]`): `exec` returns
> a struct with codata `stdout`/`stderr`, and you consume a process's output remotely by
> **placing the surrounding block**. The observer closure was never a feature of `exec` —
> it is the *lowering* of exactly that placed block, as
> `vix-language-design.md` §"What ships to executors" already said. Read this note for the
> argument; do not read it for the API. `exec cmd where { observer: … }` does not exist.
>
> **Two claims in the body are CORRECTED in round 12 and must not be repeated:**
>
> 1. *"rustc announces artifact availability as JSON messages on stdout — that is literally
>    how cargo does rmeta pipelining."* Cargo/rustc pipelining uses a readiness signal, but
>    identifying that signal with **stdout** is **unsourced**. The argument — readiness is a
>    protocol fact, not a filesystem fact — stands without it. Do not cite the mechanism.
> 2. *"vfsd close events remain a fallback readiness source for protocol-less tools."*
>    **A close event is not sound**: a process may close a file, reopen it, and mutate it.
>    For a protocol-less tool the safe authority is **process exit**. A close event becomes
>    admissible only when the command grammar *promises* monotonic or close-final outputs —
>    on the grammar's authority, never the filesystem's.

Status: design note from conversation (2026-07-09). The March-era exec
model shipped an **observer closure** with every exec; the current model
(exec returns a plain outcome record; progressive trees announce subfile
completion via vfsd close events) quietly dropped it. Amos's ruling-shaped
observation: **trees don't replace observers**, for three reasons that are
really one reason — *the tool's output streams carry semantics that the
filesystem cannot see.*

## Why "file written" is not "file ready"

A file appearing (or even closing) in the output tree is a filesystem
fact. Readiness is a **protocol fact**: rustc announces artifact
availability as JSON messages on stdout — that is literally how cargo does
rmeta pipelining; the `.rmeta` file's close event is neither necessary nor
sufficient. A tool may write a file incrementally and reopen it; it may
write ten files and declare a batch ready with one log line; it may never
touch the filesystem and stream its product on stdout. The observer
watches the JSONL stream — *that's how it knows*. vfsd close events remain
useful as a readiness source for tools with no protocol, but they are the
fallback, not the model.

## What observers are for

1. **Readiness authority.** The observer consumes the process's output
   streams as they arrive and emits per-subfile / per-product ready
   tokens. Progressive trees stay as the *transport* (subfile demands,
   mounts); the observer is the *authority* on when a subfile is
   semantically complete. Pipelining, check-builds, kill-when-satisfied
   all key off observer-announced readiness.
2. **Diagnostic transformation at the source.** A thousand-line log
   becomes a readable, structured error *before it ships anywhere* — vix
   code parsing JSON for tools that speak it, and **snark grammars for
   tools that don't**. This completes a symmetry the design already half
   has: command grammars type what goes INTO a tool (argv); output
   grammars/observers type what comes OUT (streams, logs). Both ride the
   capability.
3. **Deliberate subaction grouping.** The observer can demand follow-up
   work colocated with the product — objcopy-compress an object before it
   is ever sent anywhere; `ar` then `ranlib` back-to-back on the same
   host (the lua.vx TODO that invented this feature). Semantically these
   are ordinary demands; the colocation is a scheduling hint the as-if
   law permits and the cost model honors — the point is avoiding a round
   trip through store/network for bytes whose only consumer is standing
   right there.

## How it fits the current model (sketch)

- An observer is an ordinary vix closure — closures are values, and
  `requests-are-values` means it ships inside the exec request like
  everything else. It executes executor-side, next to the process.
- Its inputs are the process handle's streams — **journaled effect
  inputs** (external streams were already ruled: journaled, each read a
  pinned observation). Its outputs — ready tokens, structured
  diagnostics, transformed results — are part of the exec's receipted
  outcome. Determinism story unchanged: replaying the journal replays the
  observer.
- Observer failure is an effect failure (a result, not a crash), same as
  a nonzero exit.

## Open questions for the conversation

1. Does the observer's stream consumption appear in the read-set as one
   journaled stream observation, or per-message? (Granularity of replay
   and of early cutoff on logs.)
2. Is the observer surface part of `exec!` (an `observer:` field, as in
   the zoo) or a capability-level default with per-call override (the
   rustc capability ships its JSON observer; you rarely write one)?
   Lean: capability-level default — users get rmeta pipelining without
   writing observers, exactly like they get argv typing without writing
   grammars.
3. Colocated subaction demands: expressed as ordinary demands with a
   locality hint, or as observer-returned continuations the runner runs
   in-place? Lean: ordinary demands + hint (keeps the semantic plane
   clean; the hint is cost-model).
4. Ratchet impact: rung 128 (progressive trees) should be re-expressed as
   observer-announced readiness once this lands; a new rung should cover
   log-to-diagnostic transformation.
