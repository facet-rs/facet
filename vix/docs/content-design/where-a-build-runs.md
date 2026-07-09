+++
title = "Where a build runs"
+++

Status: PROPOSAL (round 9, from conversation). What placement may and may not
touch. The `Target::host()` finding in §2 is a **live bug** in
`vix/corpus-next/`.

Scheduling, policy, fleets and pricing are deliberately *not* here: the semantics
are open, the plural is the product.

## 1. Three sources, and each may touch exactly one thing

| source | says | may change the value? |
|---|---|---|
| the recipe (`.vix`) | what the value *is*; which capabilities, implicitly, via its commands | **yes — it is the only thing that can** |
| the capability | what a tool is and how it is identified (`rustc -vV`, `cc` identity) | no |
| the policy | constraints and preferences over where work runs | no |

A policy is not a recipe. It is a separate, committed, versioned artifact, and it
lives outside the language.

## 2. `Target::host()` is a plane smear, and it is in the corpus eight times

```vix
crate.vix:921    target: Target::host(),
crate.vix:588    let rustc = Rustc::acquire(unit.target);
rodin.vix:516    let rustc = Rustc::acquire(Target::host());
```

Three different machines are wearing one word:

- **target** — what the artifact is *for*. **Semantic.** It changes the value.
- **host** — what the compiler binary *runs on*. Cost-model, if hermetic.
- **executor** — which physical node. Cost-model.

`Target::host()` sets the *target* from the *executor*. Same recipe, a Mac and a
Linux box, two artifacts, two content hashes: **content addressing dies.** It is
the same disease as `machine.identity.canonical-memory` (ABI into identity) and
`map-order-independence` (user code into identity), and it is the most expensive
instance, because it reaches the artifact.

Cross-compilation makes it plain. A `rustc` on linux emitting darwin objects must
produce the same value as a `rustc` on darwin emitting darwin objects. If it does
not, that is rustc's reproducibility bug — measure it, don't paper over it by
pinning the host.

`Rustc::acquire(unit.target)` compounds it by parameterizing capability
acquisition on the *target*, as though the target selected the binary.

**The recipe never mentions the host.** Target is an ordinary argument. The
executor is never named. Then "build for the local arch but run it remotely, to
save battery" is trivially expressible, and nothing in the program knows.

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
