+++
title = "Vixen (product)"
weight = 3
+++

Normative specification for the **product** half of the system: capability
packages, the pin file, and artifact delivery. The language spec
([/spec/language](/spec/language)) and the runtime spec
([/spec/machine](/spec/machine)) say what the evaluator must honor about these
things; this section says what the things *are*.

> r[vixen.spec.home]
>
> [SETTLED, 2026-07-26] The `vixen.*` namespace is real and its tree is hosted
> **in this repository**, at `/spec/vixen`. It is the fourth namespace beside
> `vix.*` (semantics), `vixc.*` (the compiler), and `solver.*` (rodin), and it
> is the only one whose subject is a deployable product rather than a language.
>
> This closes reconciliation Decision 2. The case for a separate tree (the
> daemon, runner, and registry are a separately released product with their own
> privacy boundary) is real but premature: there is no daemon yet, and a
> cross-repo seam that nothing can traverse costs more today than co-location
> does. Hosting is not ownership — moving this directory out later is a
> directory move, which is why the namespace keeps the `vixen` name now rather
> than being folded into `vixc.*` and renamed twice.

Rules here carry the same confidence markers as the runtime spec: **[SETTLED]**
(decreed; changing it is a project decision) and **[DESIGN]** (agreed, with
implementation pending). There are no live **[OPEN]** rules.

The three pages correspond to the three seams the language deliberately refuses
to reach through:

- [Capability packages](/spec/vixen/capability-packages) — what a `Rustc` value
  *is*, and where a tool's argv grammar comes from, given that the machine may
  not know one (`machine.capability.no-argv-dialect`).
- [Pins](/spec/vixen/pins) — where a `fetch`'s required BLAKE3 comes from, given
  that minting one is a lock-time act and 0.1 never observes a live coordinate.
- [Delivery](/spec/vixen/delivery) — how a demanded value reaches a filesystem,
  given that no program may write one.
