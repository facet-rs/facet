+++
title = "Effects, and where they run"
weight = 32
+++

Running a compiler is not a side effect. It is an expensive way to compute a
value, on a machine that happens to have a compiler.

That sentence is the whole chapter. `rustc` is a function of its command, its
inputs, and its toolchain. Nothing it writes escapes; nothing it reads goes
unrecorded. So the thing every other build system calls "an action" is, here, an
ordinary demand — one whose evaluation requires something the machine has to go
and find.

## `exec` is boring

```vix
let objects = src.glob("*.c").map(|c| exec cc`-c {src / c} -o {c.with_ext('o')}`).collect();
```

`exec` is a primitive, alongside `fetch` and format parsing. It is not an
exception, it needs no special vocabulary, and nothing about the rest of the
language bends around it. Demanding an `exec` runs a process. Not demanding it
runs nothing.

## What `exec` gives back

```vix
struct ExecOutcome<A> {
    answer: A,
    tree: Tree,          // recursive: files, dirs, symlinks, exec bits
    stdout: ByteStream,  // drains to Blob; decode/framing is explicit
    stderr: ByteStream,
}
```

Four fields and **no exit status.**

`stdout` and `stderr` are **byte codata**: you can decode and read a line while the
process is still running, which is what a diagnostic renderer and a readiness check both
want. OS writes and network frames are not semantic boundaries. Immutable ranges are
addressed by byte offset; UTF-8 and line framing are explicit typed projections. Each
field's semantic content is the `Blob` it drains to, so
`ExecOutcome` has an identity as soon as the process is done, and a reader may consume
it long before.

`tree` is an ordinary value, and its **projections resolve at different times.**
Demanding `out.tree / p"early.txt"` does not demand the whole tree; it resolves when
that subfile is ready. That is not a special feature — it is partial dependency,
arriving at a subprocess.

## An exit code is a naked `Int` where a typed outcome belongs

So there isn't one.

A process termination is interpreted by the capability package's termination grammar.
It becomes either the typed `answer` field or a **failure**: the payload carries
the raw termination information, the failure identity carries the published
subject where one exists and the stable source site, and reporting supplies the
current span and demand chain.

```vix
let out = exec cc`-c {src} -o {obj}`;   // a bad compile poisons whatever demanded it
```

Anything the grammar does not recognise fails. A conventional command has answer type
`()` and maps exit zero to unit. A grep-shaped command has an answer enum and maps zero
to `Match`, one to `NoMatch`. `exec cmd?` gives
`Result<ExecOutcome<A>, Failure>` when you want to handle failure.

**Coming from a shell**: `$?` is gone, and so is the habit of comparing it to a
magic number nobody documented.

A command is a **backtick tagged template**, tagged by a capability. Its arguments
are typed rather than concatenated, and `{expr}` interpolates a *value* into an
argv element. A path interpolated there is a dependency edge wearing an argv
costume: constructing the command forces nothing, and when the result is demanded,
the interpolation walk creates the edges.

**Coming from a shell**: backticks have meant command substitution for fifty
years, `"…"` interpolates and `'…'` does not. Vix keeps all three.

## `fetch` names bytes it has never seen

```vix
let tarball = fetch "https://static.crates.io/…" where {
    blake3: "b1a4…",     // REQUIRED — vix's ContentHash: the name of these bytes
    sha256: "9f3c…",     // optional — what upstream published; checked on transfer
};
```

**`fetch` returns a `Blob`.** Bytes. Not a tree — an archive is a file, and unpacking it
is a separate demand.

A `fetch` is **pinned**: its value identity is known *before* anything is evaluated,
because it is written in the source. The URL is not the identity; it is a **provenance
coordinate**, a hint about where the bytes might live.

### One name, one receipt

The **`blake3` is required**. It is vix's content identity: the name of the value, in the
same identity space as every other value, resolvable from the local store, a peer, a
shared store, and only then the network.

The **`sha256` is optional transfer provenance**: what the CDN, the registry, or the
`Cargo.lock` published, checked against the bytes that actually arrive over the wire. It
never becomes the value's identity, because vix's identity space is blake3
(`r[machine.identity.blake3]`), and because a value should not be named in a hash family
chosen by whoever happened to host it. Both go in the receipt.

**There is no fetch without a `blake3`.** An operation whose result identity is unknown
until the bytes arrive is not a fetch — it is an *observation*, and `fetch` does not become
a different kind of thing depending on whether an optional field is present.

Computing the canonical blake3 for an upstream artifact is a **lock-time** act, not a
build-time one. When a dependency is added or bumped, the bytes are fetched once, their
blake3 is computed, and it is written into the recipe. Every build thereafter knows the
final `Blob` identity before evaluating anything — which is why every `fetch` satisfies
`machine.placement.identity-crosses` by construction rather than by care.

### An archive's digest is not its tree's digest

```vix
let tarball = fetch url where { blake3: "b1a4…" };   // Blob
let src     = extract tarball;                        // Tree — a different value
```

`extract` is an ordinary demand. Its result is a `Tree`, and a `Tree`'s identity is
computed from the tree's canonical encoding — names, entry kinds, file contents,
directories, symlinks, executable bits (`r[machine.identity.tree-model]`). **Nothing
about the tarball's bytes appears in it.** Two archives with different compression, or
different member order, that unpack to the same tree, have one tree identity and two
blob identities. That is not a coincidence to exploit; it is the point.

So `fetch` does not necessarily fetch. Demanding `tarball` resolves an identity. On a
machine that saw this blob an hour ago, nothing is transferred and nothing is downloaded.

Everything good about that follows from the pin, and the pin is what makes the
value **verifiable by a stranger**. A machine you do not administer can hand you
those bytes and you can check them.

**A read whose result identity is unknown until you perform it is a different
thing** — an *observation*, not a fetch. Observations must be made by somebody,
somewhere, and what they saw becomes the receipt's authority. They are not a
`fetch` with an argument left out.

> **An ambient read is an observation. An input is a pin.**

## A command is a tool projected out of a closure

```vix
fn compile(src: Tree) where { rust: Rust } -> Tree {
    let out = exec rust.rustc`--edition 2024 {src / p"lib.rs"}`;
    out.tree
}
```

Executables are seldom self-contained. `cc` is `cc1`, a specs file, a libc, a
linker, and a pile of headers — so naming one binary is a lie about what will run.

A command therefore names a **tool projected out of a closure**, and the projection
carries the closure with it. `rust.rustc` is not "the `rustc` binary"; it is `rustc`
*within* `rust`. You cannot pair `c.cc` with a different toolchain's `c.ar`, because
you cannot write it down.

A closure guarantees hermeticity in exactly one of two ways:

- **Materialized** — a complete, content-addressed description of every runtime
  dependency. Portable to a machine that has never seen it. `rustc` is this: an
  ordinary input, hashed like any other.
- **Ambient** — it lives on the local filesystem, and the daemon guarantees it does
  not shift underneath you: it advertises the toolchain, watches it, and **poisons**
  everything downstream the moment it changes. Xcode and MSVC are this, because they
  cannot legally be the first thing.

The demand root supplies `rust`, or a package/toolchain solve returns it as a value.
There is no ambient `Rust::acquire`: capability discovery and selection cannot read the
host into a Vix program. If no admissible machine can satisfy the selected closure, the
demand fails before anything has run.

## Reads are witnessed, and so are misses

Every byte a process reads is recorded. Not by politeness — by interposition:
store-backed bytes are obtainable only through an accessor that records the read.
If any path could read without recording, a receipt would be a hope.

Misses are recorded too. When `rustc` resolves `mod foo;` it probes `src/foo.rs`
and `src/foo/mod.rs`; one hits, one misses, and **both are receipt entries**. That
is what makes it sound to reuse a result when a file *appears*, and it is why
adding `src/foo/mod.rs` invalidates while editing your README does not.

The set of everything a demand read — including what it looked for and did not
find — is its **read-set**. A receipt is the observed read-set. It is not a claim
about what a build did; it is the record.

## Where things run

You do not say. A program that could steer placement could make its value depend
on where it ran, and then the same source would describe different artifacts on
different machines.

This is not asceticism, it is the same law as everywhere else: **nothing in a
program observes the world.** Forcing comes from outside. Ambient facts arrive as
inputs, supplied at the demand root:

```vix
fn build(src: Tree, target: Target) -> Tree { … }
```

`vx build --target aarch64-darwin` — and the CLI, which is outside the program,
defaults that flag to the host it is standing on. *"I want an artifact for my
Mac"* is an input. *"Whatever machine I happen to be on"* is an ambient read, and
a program may not make one.

**Coming from every build system you have used**: `Target::host()`, `uname`,
`process.platform` and `cfg!(target_os)` evaluated in the recipe are all the same
bug. They read the executor into the artifact.

## `exec` knows nothing about placement

These are two primitives and they do not mention each other.

`exec` runs a process and hands back a struct. Two of its fields are codata. That is the
whole of it: there is no observer parameter, no callback, no runner hook.

`place` evaluates a block somewhere else. It does not know what is in the block.

Put them together and the interesting thing happens by itself:

```vix
let diagnostics = place {
    let out = exec rustc`--error-format=json {src}`;
    out.stderr.decode(Utf8).lines().filter(is_error).map(render).collect()
};
```

The block consumes a stream. The block is placed. So the stream is consumed **where the
process runs**, and only `diagnostics` — a value — crosses back. Nobody designed that;
it is what "a `place` carries a subgraph of demands" and "`stdout` is a field" mean
together.

And note which way the identity rule points. *"A value may cross only if its identity is
known without evaluating it"* governs **dispatch** — what you capture and ship. It says
nothing about **results**: `diagnostics` is computed over there, so its identity is not
knowable before the block runs, and it acquires one where it is computed. In this example
stderr stays local and the finished value crosses back. A consumer on another evaluator
may instead subscribe to byte ranges or progressive tree projections directly: codata is
a remote demand edge with credit, cancellation, and replay. Placement is not a forced
materialization boundary.

Readiness works the same way. A file appearing in an output tree is a **filesystem** fact;
readiness is a **protocol** fact. A tool that announces artifact availability on a stream
it controls can be read for that announcement, and then the placed block reading
`out.stdout` is the readiness authority — `out.tree / p"foo.rmeta"` resolving early is the
*consequence*.

For a tool with no such protocol, the safe authority is **process exit**. A file closing is
not readiness: a process may close a file and reopen it and write more. A command grammar
may promise that its outputs are monotonic or close-final, and *then* a close event is
admissible — because the grammar promised it, not because the filesystem said so.

### Rustc pipelining across hosts

Suppose rustc A runs on executor X and produces `liba.rmeta` and `liba.rlib`, while
dependent rustc B is placed on Y. B demands only A's
`out.tree / p"liba.rmeta"` projection. X consumes the Rust capability package's product
protocol. When that protocol declares the metadata product immutable, X freezes the
Blob into its local value store, publishes the projection identity and receipt, and keeps
A running for the rlib. Y receives the completed projection plus grant, resolves the Blob
from X, a peer, or another admissible replica, and starts B.

The `.rmeta` is committed eagerly **with respect to that demanded projection**, not because
every file a process happened to write is automatically a memo result. The producer-local
store is the first required replica; replication and retention are policy. Bulk bytes move
on the store data plane. The demand/control lane carries identity, grant, completion,
receipt, credit, cancellation, and lease traffic.

With no product-readiness protocol, B waits for A's process exit. A VFS close becomes
authority only when the capability package promises close-final or monotonic output.

## `place` is a strong boundary

Sometimes a demand should be evaluated somewhere else — because that machine has
the capability, or the bytes, or simply because there are more of them.

```vix
let out = place { exec rustc`-c {src} -o out` };
```

An island edge carries a *value* between two computations in one evaluator. A
`place` carries a *subgraph of demands* to a **different** evaluator. That is a
stronger boundary, and it is restricted:

> **A value may cross a `place` boundary only if its identity is known without
> evaluating it.**

A pinned blob crosses: its `ContentHash` is in the source. A capability crosses: it is an
identity. A literal is its own identity. An observed input crosses, because the
demand root already pinned it. But `let x = expensive();` does **not** cross —
knowing what `x` *is* means computing it. Either compute it first, or draw the
`place` wider.

That single rule is what makes placement analyzable. Before anything is dispatched
you know exactly what crosses and what it weighs. No demand discovers, in flight,
that it needs something the boundary never accounted for.

### So where does the fetch happen?

```vix
let f   = fetch url where { blake3: "b1a4…" };
let out = place { exec rustc`-c {f} -o out` };
```

**On the executor.** Nothing outside the `place` demands `f`'s bytes — the only
demand for them is the `exec`, and the `exec` runs over there. What crosses the
boundary is thirty-two bytes of identity. The executor resolves them: its own
store, a peer, and only then the origin.

Your machine never downloads the tarball it is compiling.

### And a tree?

A tree crosses as an **identity plus a mount grant**: *you may read this prefix,
and here is where its blobs live.* Nothing is copied. As the process reads, each
blob is resolved by content hash, and only the files actually read ever move. A
workspace of ten thousand files, of which the compiler opens two hundred, moves
two hundred.

Then edit the README. The tree's content hash changes. **And nothing reruns** —
because the memo is indexed by *location*, which is content-free, and the entry it
finds carries a read-set that the README is not in.

### Killing a process early is not a scheduler feature

If you demanded a unit's `.rmeta` and never its `.rlib`, then once the `.rmeta` is
determined, the rest of that process's output is **undemanded**. Stopping it is not
an optimization the runner chose; it is the laziness law, arriving at a subprocess
boundary. And the value you demanded is bit-identical whether or not it stopped.

---

## What this buys, said once

- The same source, on any machine, describes the same artifact.
- A stranger's machine can compute your build, and you can check its work.
- Editing a file nobody read costs nothing, at every level: no transfer, no
  invalidation, no receipt entry.
- And a build system stops being a special kind of program. It is a program.
