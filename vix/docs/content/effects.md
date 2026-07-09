+++
title = "Effects, and where they run"
weight = 32
+++

*Status: provisional — this page documents the language as designed; parts are
not implemented yet.*

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

A command is a **backtick tagged template**, tagged by a capability. Its arguments
are typed rather than concatenated, and `{expr}` interpolates a *value* into an
argv element. A path interpolated there is a dependency edge wearing an argv
costume: constructing the command forces nothing, and when the result is demanded,
the interpolation walk creates the edges.

**Coming from a shell**: backticks have meant command substitution for fifty
years, `"…"` interpolates and `'…'` does not. Vix keeps all three.

## `fetch` names bytes it has never seen

```vix
let tarball = fetch "https://static.crates.io/…" where { sha256: "9f3c…" };
```

A `fetch` is **pinned**. Its value identity is known *before* anything is
evaluated, because the checksum is right there in the source. The URL is not the
identity; it is a **provenance coordinate**, a hint about where the bytes might
live.

So `fetch` does not necessarily fetch. Demanding `tarball` resolves an identity:
the local store, a peer, a shared store, and only then the network. On a machine
that saw this blob an hour ago, nothing is transferred and nothing is downloaded.

Everything good about that follows from the pin, and the pin is what makes the
value **verifiable by a stranger**. A machine you do not administer can hand you
those bytes and you can check them.

**A read whose result identity is unknown until you perform it is a different
thing** — an *observation*, not a fetch. Observations must be made by somebody,
somewhere, and what they saw becomes the receipt's authority. They are not a
`fetch` with an argument left out.

> **An ambient read is an observation. An input is a pin.**

## Capabilities are named, not opened

```vix
let rustc = Rustc::acquire spec;
let out   = exec rustc`--edition 2024 {src / p"lib.rs"}`;
```

A **capability** is a toolchain, advertised by a machine, referenced by
**identity** — `rustc -vV`, and the hash behind it. `Rustc::acquire spec` does not
open a binary; nothing in a vix program evaluates, so it cannot. It *names* one.

That capability is the **tag** on the command. A command is not a free-floating
string that happens to name a program; it is an argv addressed to a toolchain you
have already pinned.

Which means a capability behaves exactly like a pinned blob: an identity that some
machine may be able to materialize. If no machine can, demanding it fails, and it
fails before anything has run.

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

## `place` is a strong boundary

Sometimes a demand should be evaluated somewhere else — because that machine has
the capability, or the bytes, or simply because there are more of them.

```vix
let out = place (exec rustc`-c {src} -o out`);
```

An island edge carries a *value* between two computations in one evaluator. A
`place` carries a *subgraph of demands* to a **different** evaluator. That is a
stronger boundary, and it is restricted:

> **A value may cross a `place` boundary only if its identity is known without
> evaluating it.**

A pinned blob crosses: its sha256 is in the source. A capability crosses: it is an
identity. A literal is its own identity. An observed input crosses, because the
demand root already pinned it. But `let x = expensive();` does **not** cross —
knowing what `x` *is* means computing it. Either compute it first, or draw the
`place` wider.

That single rule is what makes placement analyzable. Before anything is dispatched
you know exactly what crosses and what it weighs. No demand discovers, in flight,
that it needs something the boundary never accounted for.

### So where does the fetch happen?

```vix
let f   = fetch url where { sha256: "…" };
let out = place (exec rustc`-c {f} -o out`);
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
