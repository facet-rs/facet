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

*(Verified. `notes/nix-verification.md`. An earlier draft claimed Nix "may not change the
software." That is false and the correction is more useful than the claim was.)*

**nixpkgs patches constantly.** `patches = [...]` in `patchPhase`, shebang rewriting, and
— for ELF outputs — `patchelf` rewriting `RPATH` and the dynamic interpreter in
`fixupPhase`. Patching is a standard build phase, not an exception. So "Nix can't touch
the source" is not why it needs `/nix/store`.

It needs `/nix/store` for a narrower and more mundane reason. The store path is a hash of
the derivation's inputs, and that path gets **baked into the output's bytes** — via
`RPATH`, via `PT_INTERP`, via shebangs. Two things follow:

1. **Binary substitution.** If the same derivation hashes to the same path everywhere, a
   substituter (`cache.nixos.org`) can hand you a prebuilt output instead of rebuilding it.
   That is the entire cache story.
2. **The path must be absolute.** The Linux kernel does not honour `$ORIGIN` in `PT_INTERP`
   or in a shebang line. There is no portable way to make the *loader path itself* relative.

Neither is the price of not touching the source. They are the price of choosing global,
content-hashed, substitutable paths as the caching mechanism.

(And it is byte-identical *per platform and architecture*, not "on every machine on earth.")

### What we inherit anyway, and what we don't

We do **not** inherit the caching mechanism: a materialized closure travels as
content-addressed blobs, keyed by identity, not as a path that must resolve identically on
every machine that ever runs it. So the two constraints Nix accepted — no `/nix` to create
without privilege, a mount root that differs across Windows, Linux and macOS — are
constraints we never adopted, because we never adopted their cause.

We **do** inherit the kernel constraint, and pretending otherwise would be the same error
in a different direction. If *we* bake an interpreter path into a binary we produced, that
path must be absolute and must resolve on whatever machine later runs it. `$ORIGIN` does not
save us either.

Two answers, and we should be honest that they are answers rather than an escape:

- **Static linking**, where we control the build. A statically linked `rustc` has no
  `PT_INTERP` problem because it has no interpreter. This is why "rustc via a static
  rust-lang build" is the shape of a materialized toolchain.
- **Relocation at materialization**, where we don't. We produce the binary and we already
  analyze it (below), so we can rewrite `PT_INTERP` and `RPATH` when it is mounted on a
  node. That rewrite changes the bytes, so **the relocated copy is not the value**: the
  value is the canonical artifact, and the relocation is a materialization detail under the
  as-if law. The receipt attests the value, not the copy.

The cost of the second is real and worth stating: **an artifact you carry off the fleet is
canonical, not runnable.** Nix's answer to that is "the store path exists on the target."
Ours has to be "the closure travels with it, and materializing it relocates it." Whether
that is acceptable is a product question, not a language one.

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

That is not everything. On Windows the linker reads, and creates dependencies on, system
libraries that live outside the VFS entirely. Those reads cross no boundary we control, so
no read-set of ours will name them. (The comparator here is **Bazel**, not Nix — Nix has no
native Windows target and never fields this problem.)

**And it may be solvable, by prior art we should read rather than reinvent.** Microsoft's
BuildXL observes Windows system-DLL reads by **API hooking with Detours** — a non-VFS
observation mechanism. If reads outside a filesystem boundary can be witnessed, the gap
below narrows from "unobservable" to "observed by a second mechanism."; the invariant does
not change, only how much of it the read-set alone can carry.

The artifact analysis names them. So the two measurements check each other:

> **Every dynamic dependency of a produced artifact must be either (a) in the
> producing exec's read-set — materialized, ours, identity known — or (b) covered by
> an advertised ambient capability. Anything else is a hermeticity hole, and it is
> detected at production time.**

Nix detects that hole at *runtime*, on someone else's machine, as a missing shared object:
its sandbox **restricts visibility** rather than recording reads, and its reference detection
is a scan of the output for the hash parts of input store paths — an after-the-fact scan
cannot notice a dependency whose path was never embedded.

One carve-out, so a maintainer cannot land it on us: `autoPatchelfHook` **does** fail at
build time when a `DT_NEEDED` entry cannot be matched. It is scoped to packaging prebuilt
binaries, not to ordinary source builds — but it is a genuine build-time check and we should
say so before someone else does.

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

## Where patch sets live (Amos): the package manager. As vix packages.

Not a mechanism. A **package**.

Which means applying a patch set is not a build-system feature at all — it is
**resolution**. The overlay system already designed for the registry is the patch
mechanism: *"an internal overlay so it can patch any package, at will, invisibly to
build logic — first-class, not `[patch]`-section folklore,"* and *"the overlay is a
resolver concern: rodin resolves against the composed view."* Receipts already name
the overlay that supplied each package, so patch provenance costs nothing extra.

And the package brief already wrote the other half without connecting it here: *"the
full resolved input graph by content hash + fetch coordinates, toolchain included —
the whole pancake stack is just more content-addressed inputs. rustc, linker, sysroot:
hashes in the graph."*

So:

> **A materialized toolchain is a resolved package graph.** Its identity is that
> graph's identity. `Rust::acquire spec` is a version requirement, resolved by rodin,
> exactly like any dependency. A patched GCC is `upstream@hash` + `patchset@version`,
> composed by an overlay, resolved.

Two consequences, and both are simplifications.

**There is no capability system for materialized toolchains.** They are packages. Only
*ambient* toolchains — Xcode, MSVC, the platform's system libraries — need capabilities
at all, which is `r[machine.capability.two-classes]` saying "rustc-class is not deeply a
capability" and finally meaning something operational.

**Placement is unconstrained except by ambient closures.** A materialized closure is
blobs; any node that can fetch can run it. An ambient one runs only where the
fingerprint is advertised. That is the whole of the placement constraint, and it
collapses `r[machine.placement.capability-requirements-are-derived]` into one sentence.

## The seed — and the people who already solved it are allies

*(Verified. `notes/bootstrap-verification.md`, every claim with a source URL.)*

A binary embeds its recipe; the recipe names its toolchain; the toolchain is a package;
that package was built by a toolchain. The chain bottoms out at a binary that has no
recipe — a **seed**. Content addressing does not answer this. Every byte of the seed is
content-addressed and equally unexplained.

It is also not ours to solve. The **Bootstrappable Builds** community has done it:

- `hex0` → `hex1` → `hex2` → `M0` → `cc_x86` → **M2-Planet** → **GNU Mes** (`mescc`) →
  `tcc` → and then a **GCC ladder** (2.95.3 → 4.7.4 → 10.x → …), with `catm` and
  `mescc-tools` along the way.
- **`hex0` is 181 bytes on x86 today** — small enough to audit by eye. (The widely quoted
  *357 bytes* was x86-only and already nine months stale when Guix's own 2023 blog post
  used it. Cite the current figure, per architecture, or none.)
- Principals: **Jeremiah Orians** (stage0), **Jan "Janneke" Nieuwenhuizen** (GNU Mes, Guix's
  bootstrap).

**Guix** shipped the full-source bootstrap in 2023, on an earlier and distinct "Reduced
Binary Seed" milestone. LWN's headline said *"(almost)"* for a reason: a ~25 MiB statically
linked Guile driver binary still ships.

**And Nix moved.** nixpkgs PR #479322, merged **2026-01-28**: `x86_64-linux` and
`i686-linux` stdenv now bootstrap from the minimal `hex0` seed, with `stdenv.bootstrapTools`
**removed** on those platforms. `aarch64-linux` and Darwin still use the older binary seed.
Guix is furthest along — *not exclusively*, and the gap is narrower than it was six months
ago.

> Twice in this document I described Nix from memory, and twice I was unfair to it. It
> patches liberally, and it has largely eliminated its binary seed on the platforms that
> matter most. Both errors ran the same direction. Verify before comparing.

### What reuse costs, exactly

`stage0`, `stage0-posix`, `bootstrap-seeds`, `M2-Planet` and `GNU Mes` are **GPLv3-or-later**,
with **no CLA**. `live-bootstrap` is REUSE/SPDX-annotated per file across a dozen licenses,
because it vendors dozens of upstream projects.

Invoking them as an external toolchain — a pinned `fetch` that a vix recipe runs as a
separate build step — is the relationship every package manager already has with the GPL'd
compilers it shells out to, and creates no copyleft obligation on vix. **Vendoring or
linking their code into a proprietary binary would.** That is a sentence for counsel, not
for me.

### What we hand back, and it is genuinely new

**Nothing in this ecosystem records, at exec granularity, what a build step actually read.**
`reproducible-builds.org` establishes reproducibility by *determinism plus independent
rebuild-and-diff*; `guix challenge` compares binaries built by different parties, after the
fact. Both are rebuild-and-compare. Neither witnesses a read.

So "their chain becomes reproducible by observation" is not an incremental improvement on
something half-built. It is a capability that does not exist:

> Every step of the bootstrap becomes an exec with a **witnessed read-set** and a receipt.
> "This `gcc` descends from that 181-byte seed" stops being a claim about a build script and
> becomes a **receipt chain** a stranger can re-verify.

And **`snark` is ours, and bootstrap work really does hand-roll its parsers** — confirmed in
source, not inferred: `mescc-tools` writes ELF directly (`blood-elf.c`, `hex2_linker.c`,
`elf_headers/`); `mescc-tools-extra` hand-rolls `untar`, `ungz`, `unbz2`, `unxz`, each its own
small C program; `M2-Planet` hand-writes a C-subset reader (`cc_reader.c`, `cc_types.c`).
(Not `ar` — no hand-rolled `ar` exists at this layer, and the earlier draft claiming one was
guessing.) A declarative binary-format dialect is exactly what that work wants.

### Prior art to name, so a Guix reader doesn't have to ask

**Software Heritage × Guix** (2025): SWHIDs, an ISO/IEC standard, bidirectionally linked with
Guix, so a pinned revision's dependency graph can fall back to the archive when upstream
source vanishes — measured, ~8% of what Guix packaged five years ago is already unreachable
from its original location. That is content-addressed **source archival**. It is not a
transparency log of build attestations, and it does not witness reads. It is adjacent, real,
and should be named.

### And the honest ceiling

Nobody has a zero-assumptions bootstrap end to end. Guix still ships a 25 MiB Guile driver.
`live-bootstrap`'s own README says: *"there is no way to perform the bootstrap without
external preparations! This is a currently unsolved problem."*

So the recipe-embedded artifact's leaves would be: pinned source tarballs, a 181-byte
human-auditable seed, **and whatever driver the chain still needs**. That is far short of
"no unexplained binary anywhere in the graph," and it is further than anyone has got.

Brothers in arms, not rivals.

## Open

1. **The seed.** What is it, how big, and what is the reduction plan? (Above.)
2. **Per-exec prefixes bought two things**: cheap isolation, and per-exec
   `tracked_observations`. If loader paths are arranged by us rather than global, what
   is the mount layout, and does attribution come from the sandbox's ACL rather than the
   namespace? (Guess: yes — the daemon knows which prefixes it granted this exec.
   Confirm against the `vx-vfsd` contract; do not assume.)
   **Read `BuildXL`'s Detours-based observation first.** It witnesses reads that never touch
   a VFS, which is exactly the Windows system-library case, and it is shipped prior art.
3. **What does a forged capability mean for a test?** (Round 11: the harness supplies
   them.) A forged closure is a materialized closure over fixture blobs — so a forged
   capability may be nothing more special than a `Tree` and its facts.
4. **Does `Rust::acquire` survive?** If a materialized toolchain is a package, depending
   on it is depending on a package. `acquire` may be a word for something that no longer
   exists.
