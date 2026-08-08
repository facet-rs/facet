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

> r[vixen.package.three-things]
>
> [SETTLED] A **capability**, a **command grammar**, and an **exported fn** are
> three different things. They were previously wearing one word, and every
> retraction on this page follows from separating them.
>
> | | what it is |
> |---|---|
> | **capability** | *live* — acquired, served, scheduled against, carrying the four contracts |
> | **command grammar** | how an invocation *against* a capability is spelled; belongs to a capability, is not one |
> | **an exported fn** | a runnable item of this package; needs no genre at all |
>
> A capability is therefore **not a straight export**, and the distinction is
> not pedantry. It has to be **acquired** — fetched and served, or ambient —
> work is **scheduled against** it, and it carries contracts. A value of a
> capability type is not a `pub const` a module happens to hand back, and the
> machinery that acquires and schedules is not something an export list
> subsumes.
>
> This rule exists because the distinction spent its first draft as prose.
> Nothing could cite it, and `r[vixen.capability.*]` demonstrably did not
> absorb it.

> r[vixen.package.two-toolchain-classes]
>
> [SETTLED] A toolchain a package did not build is **acquirable** or
> **ambient**, and the two are not variations of one thing.
>
> An **acquirable** toolchain is a pinned fetch **served at a VFS prefix**. Its
> identity is known *a priori from the pin*, before any byte moves — no read
> tracking is needed to establish it, and no probe mints it.
>
> An **ambient** toolchain **cannot go through the VFS and therefore cannot be
> observed**. There is no read-set to record, so the only sound assumption is
> that **100% of it is used**: watch the whole image with filesystem change
> events and **poison everything on any change**. This is *why* poison exists.
> Nothing may be written that implies an ambient toolchain is observed.
>
> The word **materializable** is struck, and so is its verb: an acquirable
> toolchain is not "untarred into a `Tree` and mounted", and its identity is
> not that tree's identity computed after the fact.

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
- **The `std` namespace itself.** `r[vixen.package.metadata-export]` names
  `std::PackageMetadata` and nothing in this spec defines `std`. The decree is
  the export and its type; where the standard library is specified, and what
  else is in it, is unwritten.
- **Package identity.** With no declared name and the coordinate undecided
  (`r[vixen.registry.*]`), a package currently has no way to be addressed at
  all. Nothing downstream — provider resolution, cross-package reference,
  publication — can be finished before this is.
- **Provider resolution.** Binding a `Rustc` parameter must find *which
  package's* capability. Named above; undesigned.
- **How a package provides a capability at all**, given that a capability is
  acquired and scheduled against rather than exported as a value.
- **Running a package.** `vx r` is gone with the manifest it read. Demanding
  a named export is the obvious replacement and has no spelling yet.

## Deliberately not

Not deferred — refused. Each names the system that does the thing and the
reason we will not.

- **A field that lists the package's runnable items.** cargo's `[[bin]]`,
  npm's `"exports"`, Python's `[project.scripts]`. npm's is the strictest:
  "When the `exports` field is defined, all subpaths of the package are
  encapsulated and no longer available to importers", and `require('pkg/x.js')`
  for an unlisted subpath throws `ERR_PACKAGE_PATH_NOT_EXPORTED`. Python's is
  the loosest, and shows the failure mode: a `[project.scripts]` entry naming
  `broken:does_not_exist` installs with exit 0, generates a wrapper whose body
  is `from broken import does_not_exist`, and raises `ImportError` the first
  time a user runs the command. All three are a second copy of the export list
  in a format the checker does not check, so the two copies can disagree —
  strictly (npm), silently (Python), or by drifting (cargo's `path`). Under
  `r[vixen.package.is-a-module]` there is no second copy to keep in sync.

- **Bazel's `BUILD` files.** A rule names its `srcs`, `hdrs`, and `deps` as
  literal lists, so loading can parse "all the necessary BUILD files for the
  initial targets, and their transitive closure of dependencies" and analysis
  can perform "the construction of a build dependency graph, and the
  determination of exactly what work is to be done in each step of the build"
  — all of it before one action runs. `bazel query` then answers questions
  about that graph "over the results of Bazel's loading phase", without
  building anything. This is the sharpest contrast in the system and nothing
  in vix corresponds: **a package's requirements are not readable without
  executing it.** Plans do not exist until evaluation, and a statically typed
  but interpreted language cannot enumerate its dependencies without running
  (`r[vixen.executor.root-surface-is-static]` fixes how narrow the honest
  static claim is). We will not add a declaration file to recover the graph,
  because a file a tool reads and code that does something else are two
  descriptions of one thing, and the tool believes the wrong one.

- **cargo's `[features]`.** "When a dependency is used by multiple packages,
  Cargo will use the union of all features enabled on that dependency when
  building it" — so a feature name changes what a package exports, from
  outside, and `default-features = false` in your manifest "may not ensure the
  default features are disabled" if anything else in the graph asks for them.
  The surface a consumer receives is a property of the whole graph rather than
  of the package. A manifest field that rewrites the export list is a surface
  that is not the export list, and the export list is the only surface
  `r[vixen.package.is-a-module]` admits. Compiling differently under different
  configuration is a language question; it is not a metadata field.

- **npm's `"scripts"`.** `npm run build` executes a string from the
  `"scripts"` object in `/bin/sh` (`cmd.exe` on Windows, `script-shell` to
  change it), with `node_modules/.bin` prepended to `PATH`. A package's
  most-used entry points are therefore shell strings: no parameters the
  checker knows about, no return type, and the result is whatever they left on
  the filesystem. In vix a runnable thing is an exported fn — its parameters
  are its arguments, its return type is its result — and the shell does not
  get to be a second language living in a metadata field.

- **A tier of the package that is read without running the package.**
  `flake.nix` splits into `inputs` and `outputs`, and `outputs` is a *function
  of* the fetched inputs, so `inputs` has to be readable and resolvable before
  `outputs` can be called at all. Nix pays for that fence with pure evaluation
  mode, which is the default and is where a flake lives: `builtins.getEnv
  "HOME"` returns `""`, `builtins.currentSystem` is `error: attribute
  'currentSystem' missing`, and an unlocked fetch is `error: in pure
  evaluation mode, 'fetchGit' will not fetch unlocked input`. Every one of
  those checks is correct for Nix and none of them are free — they exist
  because one language has to serve two tiers with different rules. vix has no
  bootstrap ordering problem (see **Retracted**, "Readable without an
  evaluator"), so it has no second tier, no fence around one, and no `--impure`
  to leave it by.

- **A catalog of other people's software.** nixpkgs describes zlib at
  `pkgs/development/libraries/zlib/default.nix`; zlib's own tree has never
  heard of nixpkgs. Guix does the same in `gnu/packages/*.scm`. The
  consequences are structural: the recipe's author is not the software's
  author, the unit of versioning is a revision of the monorepo rather than of
  the package, and fixing one package means a pull request into a repository
  that holds every other one. vix packages foreign software — a package that
  builds a C library is exactly that — but as **a package**: published on its
  own, covering its own vix code (`r[vixen.registry.coverage-or-dead]`),
  pinned to its own recorded behavior. There is no central tree of recipes and
  nobody merging into it.

## Explicitly out

Cross-package `use`; the registry's resolution half
(`r[vixen.registry.*]` covers publication, not resolution); any document, in
any format, that restates what an export list already says.
