+++
title = "Commands, closures, and the thing Nix is actually good at"
+++

Status: DESIGN (round 11, from conversation). Amos: *"We don't get to gloat at Nix
without solving their bread and butter."* This note states what a command is, why we
do not need Nix's central mechanism, and what we owe instead.

## What a command is

Not a tag that resolves to an executable. **Executables are seldom self-contained** —
`cc` is `cc1`, a specs file, a libc, a linker, and a pile of headers — so naming one
binary is a lie about what will run.

> A **command** is a **tool projected out of a closure**, plus a typed argv. Where the
> closure came from is immaterial. What it must do is guarantee hermeticity.

Two discharges, and they are `r[machine.capability.two-classes]` restated as a duty
rather than a taxonomy:

1. **Materialized** — a complete, content-addressed description of every runtime
   dependency. Portable to a machine that has never seen it. (`rustc`, statically
   built. This is why `two-classes` says rustc-class is "not deeply a capability at
   all": it is an *input*.)
2. **Ambient** — it is on the local filesystem, and the daemon guarantees it does not
   shift underneath you: **advertise ⇒ watch ⇒ poison**. Xcode, MSVC, the platform's
   system libraries — things that cannot legally or technically be the first kind.

A projection carries its closure. `rust.rustc` is not "the `rustc` binary"; it is
`rustc` *within* `rust`. `c.cc` and a different toolchain's `c.ar` cannot be paired,
because you cannot write it down. That is what
`r[machine.primitive.exec-probed-toolchain]` means when it says the **toolchain's**
probe output — not the tool's hash — enters exec identity.

## We are not Nix, and the difference is upstream of everything

Nix packages *existing* software and plays by its rules. It may not change the
software, so its only lever is the filesystem: `/nix/store/<hash>-name`, byte-identical
on every machine on earth, so that a path baked into a binary resolves everywhere.
That global immutable path is not an aesthetic. It is Nix's entire mechanism, and it
is the price of not touching the source.

**We maintain patch sets.** Software is patched to build correctly, and the patch set
for GCC and Clang will not be empty. If a program `dlopen`s, it does so **from a path
we arranged.**

Which means the global path is not available to us and we do not need it:

- **Unprivileged installations.** There is no `/nix` to create.
- **The mount root differs** on Windows, Linux and macOS. Linux does not care about
  this. We have to.

So: **there is no stable global path.** An earlier draft of this note demanded one.
That was Nix's answer to Nix's constraint, imported without its premise. We produce
the binary, so we arrange its loader paths — and arranging them is not a hack, it is
the product.

## Nix is files-in, files-out, bash in the middle. We have types.

We know a compiler emits an object file. We know a linker emits an executable. We
know every input we handed over. And we are already in the business of parsing things
and making sense of them.

So the closure of a produced executable is not scanned out of its bytes by looking for
a magic prefix. It is **analyzed**:

```
ArtifactFacts {
    format:  Elf | MachO | Pe,
    needed:  [SoName],       // DT_NEEDED / LC_LOAD_DYLIB / import table
    rpath:   [Path],         // RUNPATH, @rpath, ...
    interp:  Option<Path>,
    exports: [Symbol],
    ...
}
```

A typed value. It has a content hash. It enters the command's identity, and therefore
the exec identity of anything that runs it.

This is not speculative: `vix/src/reloc_selection.rs` already parses Mach-O with the
`object` crate — sections, symbols, relocation walks — to derive test selection from
link inputs. Binary-snark, the Kaitai-shaped declarative dialect, is the same job said
better.

## Turning a produced executable into a command

> **A produced executable's command is a projection of the EXEC OUTCOME, not of the
> tree.**

`tree / p"build_script"` is a `Blob`. It is bytes, and bytes cannot promise anything.
The facts were gathered at the end, by the thing that produced it, so the command must
come from there:

```vix
let built = exec rust.rustc`--crate-type bin {src}`;
let cmd   = built.artifact p"build_script";   // carries ArtifactFacts + the closure
exec cmd`--out-dir {out}`;
```

Which answers the open question this note used to end on: **the closure obligation is a
type.** A `Command` is not constructible from a bare path. `crate.vix` tagging a
command with a `String` does not typecheck — not because we disapprove, but because a
`String` never knew what it depended on.

## The read-set is necessary and not sufficient — and the gap is the invariant

We observe reads (`r[machine.receipt.witness-reads]`), so the linker's read-set names
every shared object it opened **inside the VFS**.

That is not everything. On Windows the linker reads, and creates dependencies on,
system libraries that live outside the VFS entirely. Those reads cross no boundary we
control, so no read-set will ever name them.

The artifact analysis names them. So the two measurements check each other:

> **Every dynamic dependency of a produced artifact must be either (a) in the
> producing exec's read-set — materialized, ours, identity known — or (b) covered by
> an advertised ambient capability. Anything else is a hermeticity hole, and it is
> detected at production time.**

Nix detects that hole at *runtime*, on someone else's machine, as a missing shared
object. We detect it the moment the linker finishes, because we know what we gave it
and we can read what it produced.

That is the whole of the gloat, and it is narrow enough to be true.

## What analysis does not solve, and where it belongs instead

The **static** aspect is covered by the above. The **dynamic** aspect — plugins, a
`dlopen` of a path assembled at runtime — is not, and no byte scan solves it either.

It does not belong to analysis. It belongs to **the build files we provide per
platform**: the recipe declares where plugins live, the closure includes that
directory, and a `dlopen` outside it is an undeclared read — which is a **loud
failure**, not a silent success on the machine where `/usr/lib` happened to be visible.

That is the honest division. We do not infer the dynamic aspect; we *arrange* it,
because we are the ones patching the software.

## Open

1. **Per-exec prefixes bought two things**: cheap isolation, and per-exec
   `tracked_observations`. If loader paths are arranged by us rather than global, what
   exactly is the mount layout, and does attribution come from the sandbox's ACL rather
   than the namespace? (Guess: yes — the daemon knows which prefixes it granted this
   exec. Confirm against the `vx-vfsd` contract, don't assume.)
2. **Ambient closures may be the only thing that makes an exec unplaceable.** A
   materialized closure is blobs; it places anywhere. An ambient one runs only where the
   fingerprint is advertised. If that is the whole story,
   `r[machine.placement.capability-requirements-are-derived]` collapses into
   *placement is unconstrained except by ambient closures.*
3. **What does a forged capability mean for a test?** (Round 11: the harness supplies
   them.) A forged closure is a materialized closure over fixture blobs — so a forged
   capability may be nothing more special than a `Tree` and its facts.
4. **Patch-set provenance.** If we patch GCC, the patch is an input, the patched source
   has an identity, and the toolchain's identity descends from it. Where do patch sets
   live, and are they ordinary content-addressed inputs? (They should be. Say so.)
