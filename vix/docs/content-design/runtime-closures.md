+++
title = "Commands, closures, and the thing Nix is actually good at"
+++

Status: OPEN PROBLEM (round 11, from conversation). Amos: *"We don't get to gloat
at Nix without solving their bread and butter."* This note states the problem, the
part we have already solved better, and the one concrete blocker.

## What a command is

Not a tag that resolves to an executable. **Executables are seldom self-contained**
— `cc` is `cc1`, a specs file, a libc, a linker, and a pile of headers — so naming
one binary is a lie about what will run.

> A **command** is a **tool projected out of a closure**, plus a typed argv. Where
> the closure came from is immaterial. What it must do is guarantee hermeticity.

Two discharges, and they are `r[machine.capability.two-classes]` restated as a duty
rather than a taxonomy:

1. **Materialized** — a complete CAS description of every runtime dependency.
   Portable to a machine that has never seen it. (`rustc` via a static rust-lang
   build. This is why `two-classes` says rustc-class is "not deeply a capability at
   all": it is an *input*.)
2. **Ambient** — it is on the local filesystem, and the daemon guarantees it does
   not shift underneath us: **advertise ⇒ watch ⇒ poison** (`vix-spec` V28,
   `r[machine.capability.poison-honored]`). For toolchains that are legally or
   technically un-materializable: Xcode, MSVC.

A projection carries its closure. `c.cc` is not "the `cc` binary"; it is `cc`
*within* `c`, and `c.cc` and `c.ar` cannot be paired across two `c`s. That is what
`r[machine.primitive.exec-probed-toolchain]` means when it says the **toolchain's**
probe output — not the tool's hash — enters exec identity.

## The executable you just built

`crate.vix` tags a command with `build_script`, a **`String`**. That is wrong, and
the shape it wants is the one already used for inputs:

```
Arg::Interpolation { tree, subpath }     // an input: a dependency edge wearing an argv costume
```

If an interpolated path in an argv is a dependency edge, then **the executable
position is the same edge, pointed at the program**. A produced executable is a
`Tree` subpath married to a command grammar. Strong types for inputs; strong types
for outputs.

What that does not give us is the built binary's **runtime closure**. That is the
open problem.

## What Nix does, and why

Given an output, what must exist at runtime for it to work?

Nix answers by **scanning the output's bytes for store paths**, because it cannot
observe what the process did. It is an over-approximation of a quantity it has no
way to measure. It works because `/nix/store/<hash>-name` is byte-identical on every
machine on earth — so a path baked into a binary resolves everywhere.

That global, content-addressed path is not an aesthetic. It is the entire mechanism.

## What we can do instead — measure it

We observe reads (`r[machine.receipt.witness-reads]`: bytes are obtainable only
through an accessor that records the read; absence is recorded too). So:

- **The link step's read-set** contains every shared object the linker opened. That
  is the static closure, *observed* rather than inferred.
- **The first run's read-set** contains every file the binary actually opened.

So the closure is discovered the way everything else here is: over-approximate at
first (mount the producing exec's closure), run, observe, narrow, pin. The two-step
dance, applied to a binary instead of to a build. And an undeclared read is a **loud
failure**, where Nix's failure mode is "works on my machine, because `/usr/lib`
happened to be visible."

**Honest residual.** A `dlopen` of a path assembled at runtime is in no read-set
until it happens, and in no byte scan either — Nix catches it only when the path is a
baked-in literal. Neither system catches `dlopen(prefix + name)`. Our first run
discovers it and fails loudly, which is better than silence and is not the same as
solving it.

## THE BLOCKER: produced executables are not relocatable under the current mount design

A binary carries an RPATH, an interpreter path, and often a data path **baked into
its bytes**. Those strings must resolve on every machine that runs it.

Today, `vx-vfsd` mounts inputs under **per-exec** prefixes —
`/Volumes/Vixen/vfs/vx/<prefix_id>/…`, `prefix_id` minted per execution. A binary
linked under that prefix has a path baked into it that is meaningless on the next
run, let alone the next machine.

> **The VFS must present a stable, CONTENT-ADDRESSED namespace.** A blob's path is a
> function of its identity, identical everywhere. Isolation comes from the
> **sandbox** — which prefixes a process may read — never from the **namespace** —
> which paths exist.

Half of this is already the design ("isolation enforced by the sandbox"). The other
half is not: the *naming* is per-exec, and it must not be.

Do that, and the runtime closure becomes: the set of content-addressed paths a
process is permitted to read, discovered by observation, recorded in the receipt, and
mounted by identity on the next machine. Nix's guarantee, derived from measurement
instead of from scanning.

## Open questions

1. **Is the closure obligation a type or a check?** Can a `Command` be *constructed*
   without a discharged closure, or is `Command` unconstructible until one of the two
   discharges is supplied? (The first is a lint. The second is the language doing the
   work — and it is the same instrument as "you may always drop a name; you must
   always earn one.")
2. **Per-exec prefixes bought two things**: cheap isolation and per-exec
   `tracked_observations`. If the namespace becomes global and content-addressed, how
   are observations attributed to an exec? (Probably: the sandbox's ACL *is* the
   attribution. Confirm against the `vx-vfsd` contract.)
3. **Ambient closures are not portable by construction.** An `exec` whose closure is
   ambient can only be placed on a machine advertising that fingerprint. That is a
   capability requirement in the sense of `r[machine.placement.capability-requirements-are-derived]`
   — and it is the *only* thing that makes an exec unplaceable. Materialized closures
   place anywhere. Is that the whole of the placement constraint?
4. **`dlopen` of a computed path.** Accept (loud failure on first run), or require a
   declaration for tools known to do it?
5. What does a **fake capability** mean for a test (round 11: the harness forges
   one)? A forged closure is a materialized closure over fixture blobs — so a forged
   capability may be nothing more special than a `Tree`.
