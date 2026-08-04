+++
title = "Packages"
weight = 4
+++

What one package *is*: an entry point that exports items.

This page is short because most of what stood here was wrong and has been
struck. What survives is a shape and a decree; the mechanisms hanging off them
are undesigned and are named as such rather than sketched.

> r[vixen.package.is-a-module]
>
> [DESIGN] A package is an **entry-point module that exports items**. The
> exports are ordinary vix items — functions, types, values — and their
> **types are their claims**. There is no manifest document, no sidecar file,
> no second format, and no genre vocabulary layered over the module system: a
> package's surface is its export list, and the checker already enforces every
> claim that list makes.
>
> The consequence worth stating, because a whole page used to say otherwise:
> a fn that returns `Tree` needs nothing to declare that it returns `Tree`.
> Its signature is the declaration.

> r[vixen.package.metadata-export]
>
> [SETTLED] A package MUST export **`_pkg`**, of type
> **`std::PackageMetadata`**. That is the entire metadata story — one export,
> one type, no document.
>
> `_pkg` carries only what no signature can: facts that are inert by nature
> rather than derivable from the code. It does **not** carry a version
> (`r[vixen.registry.no-declared-versions]` — nobody picks one) and it does
> not restate anything the export list already says. Its exact fields are open
> (see below).

## What a capability is not

A capability is **not a straight export**, and the distinction is not
pedantry. A capability is *live*: it has to be **acquired** (fetched and
served, or ambient), work is **scheduled against** it, and it carries the four
contracts a capability package declares. A value of a capability type is
therefore not a `pub const` that a module happens to hand back, and the
machinery that acquires and schedules is not something the export list
subsumes.

Three things that were previously wearing one word, kept apart here on
purpose:

| | what it is |
|---|---|
| **capability** | live — acquired, served, scheduled against, four contracts |
| **command grammar** | how an invocation *against* a capability is spelled; belongs to a capability, is not one |
| **an exported fn** | a runnable item of this package; needs no genre at all |

## Retracted

Recorded so the reasoning is not repeated and the rule IDs are not reused.

**The manifest document genre, entire.** `manifest-is-data`, `frontmatter`,
`frontmatter-is-not-code`, `frontmatter-reads-without-vix`,
`commands-and-artifacts-are-fn-refs`, `programs-are-paths-into-values`, `run`,
and `run-mvp` are all struck, along with the `vix-package-schema` crate, the
`package.styx` carrier, the frontmatter carrier that replaced it, the vix-side
reader, and the `vx r` subcommand.

Each died for its own reason and they compound:

- **`artifact` named nothing.** It meant "an export whose return type is
  `Tree` or `Blob`" — a query over the export list, not a category. The
  signature already carries the claim.
- **`command` was the wrong word for a visibility fact.** It meant "an
  exported fn you can run", which is not a capability (a capability is live)
  and not a command grammar (a grammar is how you spell an invocation against
  a capability). Three things, one section.
- **`program` had no referent.** A vix module does not export programs. What
  certain packages provide is a *capability*, which is its own machinery.
- **"Readable without an evaluator" was solving nobody's problem.** It was
  inherited from `flake.nix`, whose inputs must be readable *before*
  evaluation because of bootstrap order — the constraint that forces Nix's
  restricted evaluation mode. vix has no such ordering problem, and the
  constituency is empty: the registry runs your tests, so it has an evaluator;
  the editor has the language server; the pin-bumping bot has no pins to bump.
  The property existed only to justify the styx fence.
- **Styx was answering a question vix does not have.** The fence, the
  data/code split, the parser-as-police — all of it exists in Nix because one
  language cannot otherwise promise a value is inert. vix has constants. A
  constant *is* dead data by being a constant.

**`inputs-are-provider-pins`, struck earlier and separately.** It declared an
`input` section of pinned provenance (`@package{version,hash}`,
`@fetch{origin,hash,unpack}`). Wrong at the root: arbitrary vix code can fetch
anything at any time as an ordinary demand, and a list of sources fixed up
front is a Nix reflex. It also cited `rustc-is-materializable`, whose verb was
separately wrong — a toolchain is *acquirable* (pinned, served at a VFS
prefix, identity known a priori from the pin), not untarred into a tree and
mounted.

What that retraction did **not** dispose of, and this one does not either:
**source inputs and capability-provider pins are different questions.** The
first is discharged by code. The second — when a fn demands `Rustc`, *which
package's* capability satisfies it, at which identity — is configuration, is
real, and has no home.

## Open

- **The fields of `std::PackageMetadata`.** The type is decreed; its contents
  are not. The test for admission: a fact no signature can carry and the code
  cannot state about itself. Version is excluded by rule. Name is doubtful —
  the registry coordinate is where a package is published, not something the
  package asserts about itself.
- **Provider resolution.** Binding a `Rustc` parameter must find *which
  package's* capability. Named above; undesigned.
- **How a package provides a capability at all**, given that a capability is
  acquired and scheduled against rather than exported as a value.
- **Running a package.** `vx r` is gone with the manifest it read. Demanding
  a named export is the obvious replacement and has no spelling yet.

## Explicitly out

Cross-package `use`; the registry's resolution half
(`r[vixen.registry.*]` covers publication, not resolution); any document, in
any format, that restates what an export list already says.
