+++
title = "The machine manifest"
weight = 2
+++

What one machine can do, as declared data — and how a program that needs more
fails before anything runs.

The gap this closes: the runner currently *conjures* a capability value for
whatever type a root names (`publish_capability`, unconditionally). Nothing can
fail, because everything is granted. A program that cross-compiles a Windows
executable, run on a Linux host, deserves a typed refusal at binding time — not
a spawned process discovering the truth. The machine-side law already exists
(`machine.primitive.capabilities-by-identity`: "if no admissible executor can
satisfy its execution contract, the demand fails before an effect starts");
this page gives it something to check against.

Scope guard, stated first: **one machine, no solver, no daemon.** The
distributed-build shape (many machines advertising, a solve choosing) is the
same model with more candidates; nothing here needs rework to get there, and
none of it is built now.

> r[vixen.machine.manifest]
>
> [DESIGN] A machine's capability set is a declared, typed value — the
> manifest: the host `Target`, plus one capability value per offered
> capability type (`Sh`, `Rustc`, …). Each entry carries the tool closure's
> reference (for 0.1: a program path, host-trusting exactly as the exec
> backend is) and the capability's *facts* as ordinary typed fields —
> toolchain version, offered targets. The embedder loads the manifest as
> config; nothing is discovered ambiently, nothing is probed to mint identity
> (`vixen.capability.rustc-is-materializable` already rules 0.1 identity
> a-priori). The manifest is the single source of the machine's word, and the
> capability values it yields are what root capability parameters bind to.

> r[vixen.machine.binding-fails-before-effects]
>
> [DESIGN] Root capability parameters bind against the manifest by declared
> type. An unsatisfiable requirement — the type absent, or a required fact the
> offered value lacks — is a **typed failure raised before any effect
> starts**: no process spawns, no demand parks, and the diagnostic names both
> sides (what the program requires, what the machine offers). This is the
> vixen half of `machine.primitive.capabilities-by-identity`'s admissibility
> sentence, which until now had nothing to refuse with.

> r[vixen.machine.requirements-from-use]
>
> [DESIGN] A requirement finer than presence is never spelled beside the code
> that implies it — it is **extracted from use**. The capability package's
> command grammar captures the requirement-bearing roles of an invocation
> (a target-role flag, a target-role environment variable), normalizes the
> tool's dialect into the shared vocabulary (`Target` values via taxon — never
> the tool's strings), and the extracted requirement joins the binding check.
> A tool whose target is not in the invocation at all (a per-target gcc, a
> vcvars-bound `cl.exe`) states it as a *fact demanded of the capability
> itself*; both paths funnel into one comparison: required `Target` against
> offered targets. Literal captures check at lowering, so the mismatch is
> reportable statically; computed captures check at bind time — both are
> before any effect. `require(…)` raising a typed failure remains the explicit
> fallback for requirements no invocation expresses, and is expected to be
> rare.
>
> **Precision from implementation:** "checks at lowering" landed as
> *partition-time* extraction checked at bind — still static (the requirement
> is read from the plan without executing anything) and still pre-effect; a
> lowering-phase diagnostic with a source span is an upgrade, not a different
> mechanism. A *computed* capture is honestly reported and is not re-checked
> when the plan materializes at run time — that enforcement point needs a hook
> in the effect plane and is deferred, stated here so it cannot be mistaken
> for implemented.

> r[vixen.machine.facts-are-fields]
>
> [DESIGN] Machine-ness is not a set of booleans. "This is a Windows machine"
> is the manifest's `host: Target`; "can produce `.exe`" is the `targets`
> field of a toolchain capability — machines do not have abilities, toolchains
> do, and machines have toolchains. 0.1 does no constraint solving: presence
> is nominal (the parameter's type), facts are data (typed fields), and every
> check is equality or containment over typed values. Version *ranges*,
> alternative satisfaction, and cross-machine choice are solver work
> (`FV-E3`), deliberately deferred.

> r[vixen.machine.requirements-are-static]
>
> [DESIGN] A program's requirement set is readable without executing it: the
> root's capability parameter types, plus every literal role capture its plans
> contain. The runner (and eventually `vix check`) can therefore report "needs
> `Rustc` producing `x86_64-pc-windows-msvc`; this machine's `Rustc` offers
> `[x86_64-unknown-linux-gnu]`" as a static verdict. Computed captures
> degrade honestly to "needs `Rustc`, target decided at run time".

## What it looks like

(Illustrative syntax: `t"…"` target literals and the `| exists` projection are
the intended surface and do not exist yet — today's tests spell the check with
`.text()` and plain strings. `Target` itself is currently a typed newtype over
the canonical triple; the taxon-backed value of
`machine.primitive.target-value` upgrades its innards, not its call sites.)

The program — the requirement *is* the invocation:

```
#[test]
fn build(rustc: Rustc) -> Stream<Check> {
    let out = exec rustc`--target x86_64-pc-windows-msvc main.rs`;
    yield expect_eq(out.tree / "main.exe" | exists, true)
}
```

The manifest — this machine's word, as config:

```styx
machine {
  host t"x86_64-unknown-linux-gnu"
  capability Rustc {
    toolchain "1.89.0"
    targets [t"x86_64-unknown-linux-gnu"]
    program "~/.rustup/toolchains/1.89.0/bin/rustc"
  }
  capability Sh { program "/bin/sh" }
}
```

The refusal — typed, pre-effect, both sides named:

```
error[capability]: `build` demands Rustc producing x86_64-pc-windows-msvc
  --> build.vix:3   exec rustc`--target x86_64-pc-windows-msvc main.rs`
  machine offers: Rustc { toolchain 1.89.0, targets [x86_64-unknown-linux-gnu] }
  no effect was started
```

## Why this is not rustc-shaped

The mechanism's universal layer is the *role*, not the flag, and the per-tool
layer is exactly the part `machine.capability.no-argv-dialect` already forces
into per-tool data. The same target requirement is spelled, per dialect:

| tool | spelling | grammar's job |
|------|----------|---------------|
| rustc | `--target x86_64-pc-windows-msvc` | argv role, triple → `Target` |
| clang | `-target x86_64-pc-windows-msvc` | argv role, triple → `Target` |
| dotnet | `publish -r win-x64` | argv role, RID → `Target` |
| go | `GOOS=windows GOARCH=amd64` | **env** roles → `Target` |
| mingw gcc / `cl.exe` | none — the binary/environment *is* the target | capability fact, no capture |

The machine and the manifest never learn a dialect; they compare `Target`
values. A target-neutral invocation (`dotnet build`, `javac`) captures no
target role, imposes no target requirement, and runs wherever its SDK exists —
which is the correct semantics, not a special case. The honest stress test for
an implementation is therefore not rustc but the ugly ends of the table: an
env-role package and a fact-only package must exercise the same check.

## Acceptance

1. **Missing type.** A root requiring a capability type the manifest lacks
   fails typed, pre-effect (zero spawns, by counter), naming both sides.
2. **The exe case.** The program above, on a Linux-only manifest: typed
   refusal before any process exists. Same program, manifest offering the
   target: runs.
3. **The env-role case.** A go-shaped test package whose target rides
   environment roles hits the same refusal and the same pass — proving the
   mechanism is not flag-shaped.
4. **Neutral tools run anywhere.** No target capture ⇒ no target requirement.
5. **The static report.** Requirements (types + literal captures) are
   reported without executing the program.

## Explicitly out

The solver (ranges, alternatives, choice), the daemon
(advertise/watch/poison — ambient-toolchain territory, 0.1 has none),
package distribution, and any second machine. The manifest format is the
embedder's config concern (a typed facet value; the styx spelling above is
illustrative, not normative).
