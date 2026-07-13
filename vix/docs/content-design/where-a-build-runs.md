+++
title = "Where a build runs"
+++

> **STATUS (round 12): ADOPTED, and AMENDED. The normative text is
> `r[machine.placement.capability-requirements-are-derived]` in
> `/vix/spec/machine/placement` — read that, not this, when the two differ.**
>
> Two things changed since this was written:
>
> 1. **§2's three-way split was itself a plane smear.** "Host — cost-model, if
>    hermetic" is **wrong**. The selected toolchain's host / execution ABI (Cargo's
>    `HOST` among them) is a **pinned semantic property of the toolchain** *and* a
>    scheduler **admissibility** constraint. It is not cost-model. Only the
>    **physical executor** is. Corrected in place below.
> 2. **§2's corpus finding is RESOLVED.** `Target::host()` occurs **zero** times in
>    `vix/corpus-next/*.vix`; the ports thread a supplied `Target` instead
>    (`GAPS-rodin.md:50`, `GAPS-crate.md:29`). The line numbers this note once cited
>    (`crate.vix:921`, `rodin.vix:516`) no longer point at what it claimed. Kept as
>    the argument that motivated the removal, not as a bug report.

Status: PROPOSAL (round 9, from conversation), adopted round 12 with the amendments
above. What placement may and may not touch.

Scheduling, policy, fleets and pricing are deliberately *not* here: the semantics
are open, the plural is the product.

## 1. Three sources, and each may touch exactly one thing

| source | says | may change the value? |
|---|---|---|
| the recipe (`.vix`) | what the value *is*; which capabilities, implicitly, via its commands | **yes** |
| the capability **instance** | which tool, exactly (`rustc -vV`, `cc` identity, execution ABI) | **yes** — and it is the only other thing that may |
| the policy | constraints and preferences over where work runs | **no** |

Two sources, not one. The recipe names a capability; the capability *instance* that
name resolves to is negotiated at acquisition, is pinned, and enters exec identity
(`r[machine.primitive.exec-probed-toolchain]`). Swapping rustc 1.83 for 1.84 changes
the artifact and must change the receipt. What may **never** change the value is the
policy — and the physical executor it selects.

A policy is not a recipe. It is a separate, committed, versioned artifact, and it
lives outside the language.

## 2. `Target::host()` was a plane smear (RESOLVED in the corpus)

It was written eight times. It is now written **nowhere**:

```vix
// round 9, as found:                        // round 12, as ported:
crate.vix:921    target: Target::host(),     // gone — target is a parameter
crate.vix:588    Rustc::acquire(unit.target) // crate.vix:630  Rustc::acquire(unit.toolchain)
rodin.vix:516    Rustc::acquire(Target::host()) // rodin.vix:495 Rustc::acquire(target)
```

**Three** different machines were wearing one word — and an earlier draft of this
section mislabelled the second of them:

- **target** — what the artifact is *for*. **Semantic.** It changes the value.
- **the selected toolchain's host / execution ABI** — what the compiler binary
  *runs on*, including Cargo's `HOST`. **Semantic *and* an admissibility
  constraint**, ~~cost-model, if hermetic~~: it is part of what `Rustc::acquire`
  names, it enters exec identity via
  `r[machine.primitive.exec-probed-toolchain]`, and it decides which nodes may run
  the tool at all.
- **executor** — which admissible physical node actually ran it. **Cost-model**,
  unobservable, absent from the semantic receipt.

`Target::host()` set the *target* from the *executor*, collapsing the first plane
into the third. Same recipe, a Mac and a Linux box, two artifacts, two content
hashes: **content addressing dies.** It is the same disease as
`machine.identity.canonical-memory` (ABI into identity) and `map-order-independence`
(user code into identity), and it is the most expensive instance, because it reaches
the artifact.

Note that `Rustc::acquire(unit.target)` was wrong for a *second* reason, and the
port fixed it accordingly: it parameterized capability acquisition on the **target**,
as though the target selected the binary. A toolchain is named by its own identity —
`unit.toolchain` — whose execution ABI is a property of the thing named, not of what
it emits.

Cross-compilation makes it plain — and it is worth being exact about what vix
actually promises, because it is *weaker* than it first looks.

A linux `rustc` emitting darwin objects and a darwin `rustc` emitting darwin objects
are **two different toolchains**: two capability identities, therefore two *recipe*
identities, therefore **two different demands**. Vix does not require them to emit
the same value, and nothing is broken if they don't. Two demands, two answers, and
content addressing files them separately without complaint.

What vix requires is placement-invariance of **one** demand
(`r[machine.placement.value-irrelevant]`):

> The **same** demand, with the **same** pinned toolchain identity, evaluated on any
> **admissible** executor, yields a bit-identical result.

That is the invariant the scheduler leans on, and it is the whole reason placement
needs no consensus. It says nothing whatever about two *different* toolchains.

Cross-toolchain agreement is a different thing, and a good thing: it is
**reproducibility evidence**, worth measuring and worth publishing. When the outputs
do coincide, content identity deduplicates them for free, and the audit is stronger
for it. When they don't, you have learned a fact about `rustc` — not found an
inconsistency in vix. The error `Target::host()` committed was never "two toolchains
disagreed"; it was letting the **executor** reach into a **single** demand's value.
That is the thing that must never happen, and it is the only thing this section is
about.

**The recipe never mentions the executor.** The target is an ordinary argument; the
toolchain is named, and its execution ABI comes along as a property of the thing
named. Nothing in the program can ask which machine it got. Then "build for the local
arch but run it remotely, to save battery" is trivially expressible, and nothing in
the program knows — while "build with *this* rustc" stays a thing the recipe can say,
because that is a semantic choice and not a placement one.

## 3. A program cannot steer placement. An operator can.

The islands doctrine shuts the door on a *program* steering placement, because a
program that steers placement can make its value depend on where it ran. It says
nothing about the operator.

> **"No programmer control" was never "no operator control."**

Placement steering lives in policy, and policy may be as explicit as it likes,
**because policy cannot change the value.** The language keeps its side of the
bargain: observable, never steerable. What an operator may say, and how, is not
the language's business.

Capabilities are the exception that proves it. They are **derived from the
recipe** — a command *is* a capability requirement, and the set of commands
reachable in a block is syntactic. A program never declares where it runs; it
declares what it needs, by using it.

## 4. The trust surface: a cachet must not see placement

An attestation over a build splits in two:

- **Semantic receipt** — read-set, input identities, toolchain identity.
  Placement-independent. **This is what gets sealed.**
- **Operational record** — which machine, how long, how much. Placement-dependent.
  Never sealed.

> **If placement leaks into the sealed object, two identical builds attest
> differently.** Two runs of the same recipe, in different places, must produce
> the same attestation.

This belongs in the open documentation because it is checkable by anyone: it is
the property that makes a receipt worth reading. It is also a tripwire — for any
field someone proposes to seal, ask whether a different machine would have
changed it.

## Open

- Does the **AST** travel to where a demand is computed, or the **lowered
  island**? Lowering artifacts are already epoch-scoped and as-if, so both are
  sound; it decides what a runner must contain.
- Is **capability disjointness** a mandatory island cut criterion, alongside
  effects and unprovable demand? If so, an island can never discover mid-run that
  it needs a capability it lacks — which is the restriction that keeps a
  distributed demand graph statically analysable.
- Capability **class** is syntactic (a static over-approximation: costs perf,
  never correctness). Capability **instance** — which `rustc` — is negotiated at
  acquisition and *is* value-affecting, so toolchain identity belongs in the
  semantic receipt while the machine that hosted it does not.
