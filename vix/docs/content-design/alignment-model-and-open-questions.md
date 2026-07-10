+++
title = "Alignment model and open questions"
+++

Status: ALIGNMENT NOTE, 2026-07-10. This is not normative language or machine
specification. It records a source-grounded reading of the design at
`facet-cc@184305f1f` and `vixen@000797a4`, followed by recommendations for the
remaining design questions. A recommendation remains a recommendation until Amos
rules it and the corresponding normative rule is amended in place.

## The model in one sentence

Vix is a pure, typed language for describing demandable values; Snark supplies
syntax, Weavy executes lowered paths, Rust primitives mediate effects,
capabilities identify executable tool closures, placement moves demand subgraphs,
and `vx-store` persists values plus separately governed memo claims.

## Ownership boundaries

| layer | owns | deliberately does not own |
|---|---|---|
| Snark | grammar packages, parsing, incremental CSTs, queries, syntax diagnostics | Vix name resolution, typing, demand, placement |
| Vix / vixc | values, demand propagation, identity, memoization, receipts, lowering, language semantics | execution mode, physical executor policy, CAS residency |
| Weavy | frames, op execution, suspension/resume, interpreter/JIT equivalence | Vix values, memo keys, capabilities, placement, stores |
| Rust primitives | trusted effectful operations such as exec and fetch | pure collection/string/version operations, private result caches |
| vixd / Vixen | capability discovery, runners, VFS observation, placement policy, persistence and trust policy | Vix language semantics |
| `vx-store` | content-addressed chunks and nodes | Vix cache-validity decisions and semantic Tree identity |

The terms which are easiest to collapse must stay separate:

- a toolchain capability is an identified executable closure plus its command
  grammar;
- a Rust primitive is a registered in-machine effect service;
- a runner capability advertisement describes what a runner can execute;
- Weavy has no high-level `CapabilitySet`: its effective execution surface is its
  op vocabulary plus the host-function table supplied by the caller.

## Demand-driven evaluation

A Vix program denotes wiring. It performs no action and contains no forcing
operation. The CLI, IDE, test harness, audit UI, or another holder of the graph
demands a root; demand propagates through actual consumption. A record field, map
key, AST node, tree path, selected match arm, or process-output projection can be
demanded without demanding its siblings.

Three identities serve three different jobs:

1. **Location** says where a computation sits in the demand graph. It is
   content-free and nominates prior entries.
2. **Recipe** says what would be computed: closure identity plus argument value
   identities. Its digest is the `DemandKey`.
3. **Content** says what was produced: `(SchemaRef, ContentHash)`.

The memo is indexed by location and validates in this order: exact recipe,
read-set-verified projection, declared semantic comparator, then compute. Location
never proves reuse. A bad location heuristic can cause cold work, never a stale
answer.

Reads include misses, listings, path-resolution candidates, symlink decisions, and
mount-boundary decisions. Those observations form the receipt. A nested memo hit
contributes its original receipt to its caller, so evidence remains transitive.
Effects are ordinary demands and their results enter this memo rather than a
private exec/fetch cache.

## Tasks, islands, and Weavy

A demand is not a task. An island is a compile-time partition. A task is the
runtime traversal currently flowing through the island graph. It may cross many
ready edges inline and park only when it reaches genuinely pending work, joins an
existing path, waits on an effect, or deliberately splits. Not every edge becomes
a task boundary.

Weavy supplies the execution mechanism: declared raw-byte frames in a per-task
arena, typed ops over untagged operands, frame-direct calls, `Done` / `Yielded` /
`Parked`, and one lowered program shared by the interpreter and copy-patch JIT.
It does not know about Vix identity, memoization, receipts, toolchains, placement,
or `vx-store`.

Current Vix still constructs a fixed table of 64 indexed `HostFn`s containing
both real effects and pure operations. That is implementation lag, not the target
architecture: pure work becomes Weavy vocabulary, and effectful work goes through
registered primitives.

## Capabilities and primitives

A materializable toolchain is a content-addressed closure. An ambient toolchain is
daemon-advertised, fingerprinted, watched, and poisoned if it mutates. In both
cases the exact capability identity enters the recipe and receipt. Materialization
removes locality, not execution-platform compatibility.

The capability's command grammar owns argv roles, normalization, path-resolution
semantics, readiness promises, and command-specific answer semantics. The machine
must not grow rustc/clang-specific filename or argv sniffing.

The target primitive API is an open construction-time registry. Each primitive
provides a stable versioned `PrimitiveId`, request and response schemas, a memo
policy, and non-blocking `begin(request, EffectCtx) -> Ticket`. Requests and
responses are ordinary Vix values. `EffectCtx` permits witness-typed reads, result
interning, event emission, and mount-grant minting. It must not provide a path for
unwitnessed store input.

Pure array, map, string, option, comparison, version, and concrete-tree operations
are not primitives. The current numeric host IDs and bespoke fetch/exec paths are
the implementation shape to replace.

## Placement

Placement is cost-model plane. One demand with one pinned toolchain identity and
the same inputs must produce a bit-identical result on every admissible executor.

Keep three axes separate:

1. the target, which is semantic and says what the artifact is for;
2. the selected toolchain's execution contract, which is a pinned semantic
   property and an admissibility constraint;
3. the physical executor, which is unobservable cost-model choice.

Dispatch and return are asymmetric. A capture may enter a `place` only when its
identity is known without evaluating the placed block. The block's derived results
are computed remotely and acquire identity there. Finished values cross back by
identity; progressive projections and codata cross as remote demand edges as they
resolve.

A Tree crosses as an identity plus a mount grant. Files materialize per path on
read, and both hits and misses enter the receipt. `exec` and `place` remain
decoupled: placing a surrounding block naturally processes stdout/stderr next to
the process. The old observer closure is the lowering of that placed block, not a
surface feature of exec.

## Snark and self-hosted code intelligence

Snark owns the Tree-sitter compatibility boundary: grammar/scanner/query/corpus
ingestion, validated grammar facts, parse tables, incremental CST reuse, query
execution, provenance, and lowering the parser runtime into Weavy. It does not own
Vix binding or typing.

The present path is already one front end rather than an LSP rewrite:

```text
grammar.js -> Snark -> generated typed AST -> Rust binder -> LSP and evaluator
```

The self-hosted destination makes semantic editor operations ordinary Vix
functions:

```text
source -> ast(source) -> resolve(ast, scope) -> type_of(node)
       -> diagnostics(module) -> hover(node)
```

An editor request becomes an external demand for one result. Content identity on
syntax and semantic subvalues plus the ordinary memo/read-set machinery gives
incremental code intelligence without a separate query language. Today the Snark
parser, generated AST, Rust binder, LSP, and a small `ast()` probe exist. The
Vix-written resolver/typechecker does not; the hand-written AST probe is a proving
slice to replace with generated structural access.

## `vx-store`

`vx-store` is presently a validated file-backed CAS: raw BLAKE3 chunks, FastCDC
multi-chunk blobs, versioned PHON nodes, Merkle directories, cache-record nodes,
batched missing-object queries, and optional read-through upstream storage.

It has two conceptual planes:

- bytes and nodes are self-verifying by digest;
- claims mapping recipes or locations to results and receipts require tenant and
  trust policy.

The `CacheIndex` is the start of that claim plane. Current Vix values and memos
remain process-local; the open Vix persistence trait and the Vixen-backed
implementation are designed but not connected.

BLAKE3 agreement is necessary but does not collapse semantic and storage
identity. Vix identity is `(SchemaRef, ContentHash)`. `TreeHash` names the
recursive semantic Tree. Vixen `NodeHash` names serialized storage representation.
Chunk boundaries, `SmallFile` versus `LargeFile`, blob-node references, and
progress sizes may change without changing the Vix Tree.

## Difference from Bazel and Nix

Bazel is already a mature demand graph with action caching, CAS, remote execution,
toolchain resolution, and execution-platform constraints. Its unit is normally an
action generated after loading and analysis, with declared inputs and output names.
Vix's unit can be any typed value or projection, and its universal observed
read-set can prove reuse after an enclosing input changed. Bazel has discovered
inputs and depfile mechanisms; Vix makes mediated observation and misses the
general proof discipline rather than a tool-specific extension.

Nix's language is lazy and Nix realises only requested derivation outputs. Nix now
has both input-addressed and content-addressed output models. Its normal build unit
is still a derivation with an explicit input closure. Vix makes recipe identity,
typed result identity, content-free nomination, and read-set-verified reuse
separate first-class mechanisms at subvalue granularity.

The claim is not that Vix discovered graphs, laziness, CAS, or remote execution.
The distinction is the combination of language-enforced demand, typed subvalues,
observed dependencies, and placement that cannot enter semantics.

## Combined design-question docket

The questions below combine the last unresolved questions from the Vix-design
review agent with the alignment review above. Implementation obligations are
listed separately at the end.

### Q1. How does an accepted exit code become a typed answer?

**Recommendation:** command templates are typed as `Command<A>`, and exec returns
`ExecOutcome<A>`:

```vix
struct ExecOutcome<A> {
    answer: A,
    tree: Tree,
    stdout: ByteStream,
    stderr: ByteStream,
}
```

The command grammar carries a finite mapping from process termination to either an
`A` constructor or failure. A conventional command has `A = ()` and maps exit zero
to `()`. Grep can use `A = GrepAnswer` and map zero to `Match`, one to `NoMatch`.
Any unmapped exit or signal fails with the raw termination information in the
`Failure` payload.

The process status never becomes a naked language `Int`. `answer` resolves at
termination; independent tree or stream projections may resolve earlier. The
mapping is part of the versioned command grammar and therefore part of command
identity.

### Q2. Are stdout and stderr text streams or byte codata?

**Recommendation:** they are `ByteStream`s whose resolved semantic value is a
`Blob`. OS write boundaries and transport chunks are implementation details and
must not become stream keys. Text is an explicit deterministic projection:

```vix
out.stderr.decode(Utf8).lines()
```

That projection yields `Stream<Int, String>` keyed by semantic line number. Binary
stdout remains representable, invalid UTF-8 is typed, and the protocol's byte
surface agrees with the language. A command grammar may provide a higher typed
decoder, but exec itself does not guess encoding or record boundaries.

### Q3. What is the canonical Tree model across platforms?

The following five rulings travel together.

**Name and order recommendation:** `Name` is one nonempty valid-UTF-8 segment,
excluding `.` / `..`, separators, and NUL. It preserves spelling exactly: no
Unicode normalization. Tree semantics are case-sensitive on every platform. The
semantic order is Vix String structural order; storage may compare UTF-8 bytes
because UTF-8 preserves scalar order. A physical case-insensitive filesystem must
use the VFS representation or reject an unrepresentable materialization rather
than merge names.

**Executable recommendation:** `executable: Bool` is portable semantic intent, not
a POSIX mode fragment. It remains part of identity on every platform. Unix
materialization maps it to the chosen canonical executable/non-executable mode.
Windows preserves the bit even though its kernel does not enforce it as a mode;
setting it is not an error and importing a Tree must not silently discard it.

**Metadata recommendation:** mtime, uid/gid, POSIX mode beyond executable,
setuid/setgid/sticky, xattrs, resource forks, hardlink identity, device/FIFO/socket
nodes, ACLs, and host filesystem case-folding are not part of `Tree`. Inputs which
need those properties require an explicit typed artifact rather than silently
widening Tree. Immutable Trees make hardlink aliasing semantically irrelevant;
tools which inspect inode identity or link count are outside the hermetic Tree
contract.

**Symlink recommendation:** the v0 Tree admits relative UTF-8 targets only and
preserves their text without normalization. Dangling links and `..` are
representable. Resolution is against the containing directory and the mount
grant; escaping the grant is denied and witnessed rather than followed. Archive
extraction must not follow symlinks while constructing the Tree. Absolute links
are non-relocatable ambient dependencies and should require a different explicit
type or import policy rather than enter ordinary Tree silently.

**Empty-directory recommendation:** empty directories round-trip and participate
in Tree identity. This is required to represent an output which creates a
directory for a later process without writing a file into it.

### Q4. May `TreeHash` and `NodeHash` share a preimage?

**Recommendation:** no. Make the distinction intentional and domain-separated.

```text
TreeHash = blake3("vix/tree/v1" || semantic tree encoding)
NodeHash = blake3("vx/cas-node/v1" || versioned PHON storage node)
```

The current `DirectoryNode` contains representation fields which the semantic
Tree explicitly excludes, so it cannot supply the TreeHash preimage. A claim or
side index maps `TreeHash` to the current root `NodeHash`. Rechunking or migrating
the node encoding changes only the latter. Sharing a digest would save one mapping
while coupling the identity epoch to storage layout; that is the wrong trade.

The raw bytes of a Blob are a different case: Vix Blob content identity and
Vixen `ContentHash` may intentionally share `blake3(file_bytes)` while their Rust
types remain distinct.

### Q5. What are `Tree::union` collision semantics?

**Recommendation:** `union` is a partial, commutative, associative, idempotent
structural join returning `Result<Tree, TreeConflict>`.

- missing on one side: keep the present entry;
- directory plus directory: recurse;
- identical files, including the executable bit: coalesce;
- identical symlinks: coalesce;
- different files, different executable bits, different symlink targets, or any
  leaf/directory kind mismatch: return a typed conflict carrying the full path and
  both entries.

There is no left-wins or right-wins variant: argument order and scheduling must not
change a Tree value. If output ownership needs the stronger rule that even an
identical duplicate is an error, provide a separately named `disjoint_union` or
ownership validator. Do not overload semantic union with producer-ownership
policy.

### Q6. Does placement ship the AST or a lowered island?

**Recommendation:** transport the lowered, architecture-neutral Weavy artifact;
retain the canonical closure AST as semantic identity, source map, and audit
authority.

The lowered artifact is a cache indexed by `(closure identity, lowerer epoch,
primitive ABI epoch)`. Executors need Weavy and registered primitives, not vixc.
They JIT locally when available. An audit can re-lower the canonical AST and diff
the artifact. This keeps remote executors static and small without allowing the
lowered representation to become language identity.

### Q7. Does codata cross a `place` boundary?

**Corrected after review with Amos:** yes. A `place` boundary is not a forced
materialization boundary. Codata and progressive value projections may cross it as
remote demand edges.

Rustc pipelining is the forcing example. Suppose rustc A runs on executor X and
produces both `liba.rmeta` and `liba.rlib`, while dependent rustc B is admissible on
executor Y:

1. B demands A's `out.tree / p"liba.rmeta"` projection. It does not demand A's
   complete output tree or `liba.rlib`.
2. The demand routes to X, where A is running. X consumes whatever readiness
   protocol A's command grammar declares. That may be a tool-controlled message;
   for a protocol-less command it is process exit. A filesystem close is authority
   only when the grammar promises close-final output.
3. Once `liba.rmeta` is ready, X freezes it as a Blob, computes its content
   identity, publishes the projection's completion and receipt, and makes the Blob
   resolvable in the CAS. A continues producing `liba.rlib`.
4. Y receives the completed projection identity and mount grant, resolves the Blob
   from its store, a peer, or X, and resumes B. A and B now overlap on different
   executors.

The readiness stream need not itself leave X when the placed lowering turns it
into the progressive Tree projection, but the projection becoming ready before its
aggregate is complete is already the same distributed-demand protocol class. A
consumer which directly demands stdout/stderr elements across the boundary uses
that protocol without the Tree indirection.

The evaluator-to-evaluator protocol therefore needs demand/credit, ordered
delivery, cancellation, reconnection, and replay or spill. Timing and transport
chunk boundaries remain unobservable; stream elements or byte ranges are semantic
demands, and completion resumes the remote waiter exactly as a local pending edge
does.

Forbidding cross-host codata would let an executor expose a simpler
run-to-completion request/response surface in isolation. It does not simplify the
system we are building: remote joins, effect tickets, progressive Trees,
cancellation, executor loss, and rustc pipelining already require long-lived
duplex demand/completion traffic. The ban would preserve nearly all of that
machinery while preventing the placement policy from putting a pipelined consumer
on another worker. It buys the wrong simplification and is withdrawn.

### Q8. What exactly is the persistent store seam?

**Recommendation:** preserve two interfaces even if one implementation serves
both:

- a value CAS keyed by typed value identity;
- a claim index keyed by exact demand identity or content-free nomination
  coordinates, returning result identity plus receipt and claim provenance.

Open Vix owns claim verification. Vixen owns residency, tenant namespaces, trust
policy, signatures, sampling, and garbage-collection roots. A persistent exact
claim is trusted without read-set verification only for the proven class whose
inputs are exclusively content-addressed values; otherwise persistence never
weakens the live proof obligation.

### Q9. What replaces the hand-written `ast()` probe?

**Recommendation:** generated lazy structural access over the same typed AST the
Rust binder consumes. Field projection is demandable and preserves source spans;
generic Snark query results remain available for exploratory syntax work. Do not
stabilize the current summary-map vocabulary as the self-hosting ABI.

## Implementation obligations, not design questions

These are expected lag and should not keep reopening the architecture:

- replace the fixed 64-host table with Weavy-native pure ops plus registered
  primitives;
- connect Vix values, memo claims, receipts, and projection nomination to the
  persistence seam;
- bring fetch onto required BLAKE3 identity plus optional upstream digest;
- port exec to recursive Tree, byte codata, typed answers, and no naked status;
- converge the facet exec protocol and Vixen runner implementation over Vox RPC;
- implement cross-evaluator codata and progressive-projection demand, credit,
  cancellation, and replay;
- replace the AST probe and move resolver/type/hover queries into Vix;
- implement placed-subgraph transport and its static capability/admissibility
  analysis.

The implementation may be behind without making any of the ownership boundaries
above negotiable.
