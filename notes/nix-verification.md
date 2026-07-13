+++
title = "Verification: Nix claims in runtime-closures.md"
+++

Status: verification pass against `vix/docs/content-design/runtime-closures.md`, sections
"We are not Nix, and the difference is upstream of everything" and "The read-set is
necessary and not sufficient". Outcome up front: **claim 1 as written is false and must be
struck.** Nix patches extensively. The document's real argument survives, but only in a
narrower form — the global store path is not "the price of not touching the source," it is
the price of a specific caching/substitution design that Nix chose and vix doesn't need.
Claims 2, 3, 4 (with a carve-out), 5, and 6 check out. Section 7 below flags one more
overreach the task didn't ask about: "byte-identical on every machine on earth."

## Table

| # | CLAIM | VERDICT | SOURCE | Corrected text |
|---|-------|---------|--------|-----------------|
| 1a | "Nix ... may not change the software" | **FALSE, corrected** | [nixpkgs stdenv chapter](https://github.com/NixOS/nixpkgs/blob/master/doc/stdenv/stdenv.chapter.md), [patchelf](https://github.com/NixOS/patchelf), [autoPatchelfHook](https://ryantm.github.io/nixpkgs/hooks/autopatchelf/) | nixpkgs applies a `patches` list in `patchPhase`, rewrites shebangs, and runs `patchelf` on RPATH/interpreter in `fixupPhase` for essentially every Linux package. Patching is the default, not the exception. |
| 1b | Global store path is "the price of not touching the source" | **FALSE premise, corrected** | [Nix manual: Local Store](https://nix.dev/manual/nix/2.34/store/types/local-store), [fzakaria: Nix needs relocatable binaries](https://fzakaria.com/2026/06/21/nix-needs-relocatable-binaries), [nix#9549](https://github.com/NixOS/nix/issues/9549) | The store path is fixed and machine-identical so that (a) the same derivation hashes to the same path everywhere, letting a binary cache (`cache.nixos.org`) substitute pre-built outputs instead of rebuilding, and (b) `PT_INTERP` and shebang lines can be absolute paths, which is what the Linux kernel currently requires — the kernel does not support `$ORIGIN` in either. This is a kernel/caching constraint, not a "don't touch upstream" constraint. |
| 2 | Nix "scans the output's bytes for store paths" | **CONFIRMED** | [Nix manual: Building — "Calculate the references"](https://nix.dev/manual/nix/2.34/store/building.html), [Nix Pills 9: Automatic Runtime Dependencies](https://nixos.org/guides/nix-pills/09-automatic-runtime-dependencies.html) | Manual, verbatim: "Nix scans each output path for references to input paths by looking for the hash parts of the input paths." This is the `scanForReferences` mechanism; it treats a bare 32-char hash occurrence as a reference, which is known to produce false positives (Nix issue [#4396](https://github.com/NixOS/nix/issues/4396)). |
| 3 | Content-addressed derivations (`ca-derivations`) status/effect | **CONFIRMED, with nuance** | [RFC 0062](https://github.com/NixOS/rfcs/blob/master/rfcs/0062-content-addressed-paths.md), [NixOS/nix milestone #35](https://github.com/NixOS/nix/milestone/35) | Still experimental (behind `--extra-experimental-features ca-derivations`), active stabilization work as of Jan 2025. It moves the store-path hash from being a function of *inputs* to a function of *output content*, enabling "early cutoff." It does **not** remove the global `/nix/store` prefix and does **not** remove hash-scanning — the RFC's own "hash rewriting" trick for self-references depends on scanning. |
| 4 | Nix "detects [a hermeticity hole] at runtime ... as a missing shared object" | **CONFIRMED for source builds, with one carve-out** | [Nix manual: Building](https://nix.dev/manual/nix/2.34/store/building.html), [autoPatchelfHook source](https://github.com/NixOS/nixpkgs/blob/master/pkgs/build-support/setup-hooks/auto-patchelf.sh), example failures: [nix-community/dream2nix#556](https://github.com/nix-community/dream2nix/issues/556) | For ordinary `stdenv.mkDerivation` source builds, there is no build-time check that a produced binary's `DT_NEEDED`/`dlopen` targets are actually satisfiable — the reference scan only records what *did* get embedded in the output; it can't notice an entry that's silently absent. Failure surfaces later as `error while loading shared libraries` on some other machine. **Carve-out:** `autoPatchelfHook`, used to repackage pre-built binaries, *does* fail the build if a `DT_NEEDED` entry can't be matched against `buildInputs` ("could not satisfy dependency X") — this is real build-time detection, but scoped to that hook's use case, not general to all Nix builds. |
| 5 | Nix sandbox "infers," doesn't "measure," reads | **CONFIRMED** | [NixOS Discourse: What is sandboxing?](https://discourse.nixos.org/t/what-is-sandboxing-and-what-does-it-entail/15533), [Nix manual: sandboxing](https://nix.dev/manual/nix/2.23/command-ref/conf-file.html?highlight=sandbox) | On Linux, the sandbox uses mount/PID/network/IPC/UTS namespaces and bind-mounts to restrict *what's visible* (whitelist of store paths + declared inputs); it does not log or witness actual `open()`/`read()` calls during the build. Post-hoc dependency inference is entirely the byte-scan in claim 2, run after the build finishes. **On macOS**, sandboxing is off by default and uses the deprecated `sandbox-exec`/Seatbelt facility, which is weaker and can't do bind-mounts the way Linux does — the macOS story is closer to vix's own "ambient capability" framing than to Linux's. |
| 6 | "On Windows the linker reads ... system libraries that live outside the VFS entirely" | **CONFIRMED as a general fact about hermetic builders on Windows** | [Bazel: Sandboxing](https://bazel.build/docs/sandboxing), [bazelbuild/bazel#5136](https://github.com/bazelbuild/bazel/issues/5136), [Bazel Windows guide](https://bazel.build/configure/windows) | Bazel has no real Windows sandbox — "most actions are executed using local strategy" (no namespace/bind-mount isolation exists on Windows the way it does on Linux/macOS). The MSVC linker resolves default libs (`kernel32.lib`, UCRT, etc.) via `LIB`/`INCLUDE` pointing at the Windows SDK on the host, outside any tool's control. **Caveat:** Nix itself has no native Windows story to compare against — it only runs via WSL (i.e., as Linux). So this isn't really a fact "about Nix"; it's a fact about hermetic build tooling on Windows generally (Bazel is the closest real comparator), and the document's phrasing ("on Windows the linker...") should not imply Nix has and clears this bar — it never faces it. |
| 7 | (unprompted) "byte-identical on every machine on earth" | **OVERSTATED, minor** | same as 1b | True only for the same derivation on the same architecture/platform with the same `system` attribute — a `x86_64-linux` output and an `aarch64-darwin` output of "the same" package are different store paths. Not false, but "on every machine on earth" reads as a stronger claim than intended; "every machine that builds or substitutes the same derivation" is accurate. |

## Replacement text for "We are not Nix, and the difference is upstream of everything"

Written so each sentence survives a hostile Nix-maintainer read.

> Nix packages existing software, and nixpkgs patches it constantly: `patches = [...]`,
> shebang rewriting, and — for ELF outputs — `patchelf` rewriting RPATH and the dynamic
> interpreter in `fixupPhase`. Patching is not the exception in nixpkgs; it is a standard
> build phase. So "Nix can't touch the source" is not why it needs `/nix/store`.
>
> It needs `/nix/store` for a narrower and more mundane reason: the store path is derived
> from a hash of the derivation's inputs, and that path gets baked — via RPATH, via
> `PT_INTERP`, via shebangs — into the bytes of what gets built. Two things follow from
> that choice. First, if the same derivation hashes to the same path everywhere, a binary
> substituter (`cache.nixos.org`) can hand you a pre-built output instead of rebuilding it
> — that's the entire cache story. Second, the path has to be absolute, because the Linux
> kernel does not support `$ORIGIN` in `PT_INTERP` or in a shebang line, so there is no
> portable way to make the *loader path itself* relative. Neither of those is "the price of
> not touching the source." They're the price of choosing global, content-hashed,
> substitutable paths as the caching mechanism.
>
> We don't need that mechanism, because we're not building a shared, cross-user binary
> cache keyed by filesystem path — we arrange loader paths per exec, and a materialized
> closure travels as content-addressed blobs, not as a path that has to resolve
> identically on every machine that ever runs it. So the same two constraints Nix
> accepted — no `/nix` to create without privilege, and a mount root that differs on
> Windows, Linux and macOS — are constraints we simply don't inherit, because we never
> adopted the mechanism that created them in the first place.

The "read-set is necessary and not sufficient" section's Nix comparison (claim 4/5) is
accurate as originally written and needs no correction — Nix's sandbox restricts
visibility rather than recording reads, and its scan-after-the-fact mechanism can't notice
an entry that never got embedded. Only add, if space allows, the one-line carve-out that
`autoPatchelfHook` is a genuine build-time counterexample scoped to prebuilt-binary
packaging — worth keeping only if the document wants to preempt a maintainer citing it.

The Windows claim (claim 6) should drop the implicit "Nix would face this too" framing —
Nix has no native Windows target, so the honest comparator is Bazel, not Nix. Suggested
edit: replace "On Windows the linker reads..." framing with something that doesn't imply
a Nix/vix contrast on this specific point, since Nix never fields it.

## What I could not verify

- Eelco Dolstra's ICFP 2008 paper / PhD thesis: fetched the PDF but WebFetch could not
  extract text from it (FlateDecode streams weren't decoded by the fetch tool). I did not
  find an HTML mirror with quotable passages in the time available. The claims that needed
  primary-source backing (global path rationale, scanning mechanism, patching) were all
  independently confirmed via the current Nix manual, nixpkgs source, and an active GitHub
  issue instead, so this doesn't leave any claim unconfirmed — it just means I'm citing the
  contemporary manual/source rather than the 2006 thesis for the historical "why."
- Exact current adoption rate of `ca-derivations` in production (e.g. whether
  cache.nixos.org substitutes CA outputs at scale) — found status (experimental,
  stabilization milestone open Jan 2025) but not deployment scale. Doesn't affect the
  claims above, which only needed the feature's mechanism and status.
- Whether any other hermetic build system (beyond Bazel) has cracked Windows sandboxing
  more thoroughly than Bazel has. I did find one: **Microsoft BuildXL** observes Windows
  file access, including reads of system DLLs, via [Detours](https://github.com/microsoft/BuildXL/blob/main/Public/Src/Sandbox/Windows/DetoursServices/DetouredFunctions.cpp)
  — API-level binary patching/hooking, not namespace or bind-mount isolation. This doesn't
  contradict claim 6: the document's argument is that reads crossing no VFS boundary can't
  be named by a *VFS-mediated* read-set, and BuildXL's mechanism is a different kind of
  observation (API interception) than the VFS-crossing witness the document describes for
  vix. But it's a real existence proof that "observe, don't infer" is achievable on Windows
  by *some* mechanism — worth citing in the doc's own Open-question #2 (attribution via
  sandbox ACL vs. something else) as prior art if vix ever needs a non-VFS way to witness
  Windows system-library reads, rather than treating them as permanently un-witnessable.
