+++
title = "Code intelligence"
weight = 13
+++

Vix uses the same demand model for its own editor as it does for builds. There
is no second incremental-query engine hiding behind an LSP façade.

## One front end

Snark owns grammar packages, parse tables, incremental concrete syntax trees,
syntax queries, and syntax diagnostics. The generated Vix AST preserves every
source span. Vix owns names, types, capabilities, targets, VIR, and semantic
diagnostics. The intended query chain is:

```text
source snapshot
  -> Snark syntax tree
  -> generated structural AST
  -> names and types
  -> VIR and demand graph
  -> diagnostics / hover / definition / references / completion
```

Each arrow produces typed subvalues. An editor externally demands a result such
as `type_of(node)`, `references(symbol)`, or `diagnostics(module, target)`.
Ordinary value identity, projection, receipts, and memo validation provide
incrementality. This does not add an in-language force operation: the editor is
the graph holder and chooses roots just as `vx build` does.

The bootstrap implementation may be Rust, but the semantic query functions are
ordinary Vix-shaped functions and can move into Vix without changing the
protocol. The compiler service exposes typed values, not a privileged bag of
callbacks. Its own diagnostics pass through the same stable diagnostic schema
used by the command runner and runtime.

## Buffer snapshots without CAS churn

An unsaved buffer begins as a process-local source provider. Each revision has
content identity, but identity does not imply durable upload. Snark retains
incremental syntax structure; the Vix compiler retains values and memo claims
under the editor session's lease. Superseded bodies, syntax pages, and claims
may all be evicted.

When work must move, the editor and executor first negotiate known chunks.
Small edits transmit missing chunks or a delta against an acknowledged base;
large files may use CDC so unchanged regions keep their chunk identities. A
short-lived enterprise or executor-local shard may retain those chunks. The
orchestrator carries identities, leases, placement decisions, and completion
events; blob bytes travel directly over the selected store/data plane. Nothing
requires every keystroke to become a globally durable CAS object.

## Multi-target flycheck

Suppose one editor asks for Clippy diagnostics on six targets, only two of which
can execute locally. The requested value is conceptually:

```vix
Map<Target, Result<DiagnosticSet, CheckUnavailable>>
```

The editor may demand all entries or project one target at a time. For each
entry, placement derives the selected toolchain's execution contract and finds
an executor advertising both that capability and the required Weavy/primitive
ABIs. The target triple remains a semantic input; the physical executor does
not. Equal target results deduplicate by content identity even when different
executors produced them, while arrival order never enters the value.

Source and dependency trees cross as identities plus grants. Missing content is
pulled directly by the executor. A local target can keep the whole path local;
a remote target receives only the missing snapshot chunks and dependencies its
placed demand reads. Access control, sovereignty, tenant boundaries, latency,
and cost constrain the shard choice before transfer cost is optimized.

## rustc output and pipelining

A command package has four coordinated contracts, not one overloaded grammar:

- the command grammar types and normalizes arguments;
- the termination grammar maps exit states to typed answers or failures;
- the output protocol decodes stdout/stderr byte codata into typed values such
  as rustc JSON diagnostics;
- the product protocol says which files or projections become immutable and
  replayable, and when.

This is where rustc pipelining lives. A producer-local protocol may publish an
immutable `.rmeta` snapshot before the process exits when rustc's contract
proves that publication final. The snapshot is interned in the producer's local
store and receives value identity there. A consumer placed on the same executor
opens it locally; a consumer elsewhere receives its identity and grant and pulls
bytes over the store data plane. No mutable file, process handle, or molten
builder crosses hosts. Codata does cross: immutable diagnostic elements and
immutable byte ranges can be replayed by semantic key or offset.

For a protocol-less tool, process exit is the safe publication authority.
Filesystem close alone is not: a process can close, reopen, and mutate. A
command package may use an earlier close event only when its product protocol
explicitly promises close-final or monotonic output.

## What the editor can inspect

The compiler/runtime inspection service exposes source-mapped syntax, typed
AST, graph and partitioned VIR, lowered Weavy programs, demand and task states,
memo verdicts, receipts, placement decisions, counters, and causal events.
Hovering a diagnostic can therefore answer both “what is wrong?” and “why did
this target run or rebuild?” without forcing an undemanded value or cloning the
store. Native profiles map through Weavy and VIR back to Vix source, so Stax can
show editor latency in the same logical stacks as command-line builds.
