+++
title = "Vixen (product)"
weight = 3
+++

Normative specification for the **product** half of the system: capability
packages, the executor manifest, what a package is, where pins come from,
artifact delivery, and the registry. (There is no pin *file* — `r[vixen.pins.come-from-the-ecosystem-lockfile]`
is emphatic that vixen maintains none.) The language spec
([/spec/language](/spec/language)) and the runtime spec
([/spec/machine](/spec/machine)) say what the evaluator must honor about these
things; this section says what the things *are*.

> r[vixen.spec.home]
>
> [SETTLED, 2026-07-26] The `vixen.*` namespace is real and its tree is hosted
> **in this repository**, at `/spec/vixen`. It sits beside `lang.*` (language
> semantics), `machine.*` (the abstract machine), and `solver.*` (rodin), and it
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
implementation pending). A third, **[STRUCK]**, marks a retired rule id kept as
a stub so stale references fail loudly rather than resolving to nothing.

There is no **[OPEN]** marker, and that is deliberate rather than an
achievement: **an undecided question never gets a rule id.** Open questions
live in prose `## Open` sections, so nothing can cite one, no id is spent on a
question, and a reader scanning rule ids is never reading a guess. Several
pages here carry substantial `## Open` sections; the absence of `[OPEN]` says
where undecided things live, not that there are none.

The pages correspond to the seams the language deliberately refuses to reach
through:

- [Capability packages](/spec/vixen/capability-packages) — what a `Rustc` value
  *is*, and where a tool's argv grammar comes from, given that the machine
  knows no tool's argv dialect (`machine.capability.no-argv-dialect`).
- [The executor manifest](/spec/vixen/executor) — what one box can do, as
  declared data, and how a program needing more fails before anything runs.
- [Pins](/spec/vixen/pins) — where a `fetch`'s required pin comes from, given
  that 0.1 never observes a live coordinate: the ecosystem's own lockfile, and
  no lockfile of vixen's own.
- [Delivery](/spec/vixen/delivery) — how a demanded value reaches a filesystem,
  given that no program may write one.
- [Packages](/spec/vixen/packages) — what a *package* is: an entry-point
  module that exports items, one of which is `_pkg`. The manifest-document
  genre that stood here is retracted entire, and the page records why so it
  is not rebuilt.
- [The registry](/spec/vixen/registry) — how a package is published without
  anyone choosing a version number, and what is checked before a publication
  is accepted.
