+++
title = "The package manifest"
weight = 4
+++

What one package *is*, as declared data — its pinned inputs, its runnable
commands, its buildable artifacts, and the programs it exposes — and how
`vx r cargo build` becomes a no-config cargo build without the machine, the
language, or the manifest learning anything about cargo.

The gap this closes: the system can build, but nothing can *name a package*.
Capability packages are a const table in `vixen-primitives`
(`vixen.capability.packages-ship-in-vixen-primitives` defers distribution);
the machine manifest says what a box offers but not what any piece of
software is; and there is no verb that takes a directory of `.vix` and makes
its entry points invocable. The manifest is the missing document, and its
genre decision is the load-bearing one.

Scope guard, stated first: **no registry, no daemon, no cross-package
imports.** The resolution rungs below name where a registry will stand; none
of it is built now.

> r[vixen.package.manifest-is-data]
>
> [DESIGN, MVP] A package is described by one styx document, `package.styx`,
> at the package root. The manifest is **dead data**: every entry is a
> literal (a name, a version, a pin, a fn path, a path into a tree), it is
> read by `facet_styx::from_str` with zero evaluation, and anything computed
> lives in a `.vix` fn the manifest merely names.
>
> This is the flake architecture — literal inputs plus outputs as pure
> functions of inputs — with the data/code boundary moved to a **file
> boundary**. Nix polices the same split *inside* flake.nix with a restricted
> evaluator mode, because the inputs must be readable before anything can be
> evaluated and the language cannot otherwise promise that; vix has two
> layers natively (styx is the data layer, `.vix` is the logic layer), so the
> parser is the police and no restricted mode exists to maintain. The same
> bootstrap-order argument, resolved structurally instead of procedurally.
>
> The document shape has **one source of truth**: the `vix-package-schema`
> crate. Its Facet-derived types are the schema; their facet `Shape` is the
> seam that carries the shape to every consumer (Rust deserialization, styx
> schema generation, the vix rail via `Type::from_facet`). The vix-side
> mirror (`cargo-vix/package.vix`) restates the same shapes in the language,
> and CI pins all three to the same corpus documents
> (`vix-core/corpus-next/package/`). Two spelling rules keep the mirror
> honest with no rename surface in vix: enum values wear kebab-case tags
> matched against the kebab spelling of the variant identifier (`@exit-only`
> ⇔ `ExitOnly`, stated once at the decoder; `rename_all = "kebab-case"` on
> the Rust side), and field names are single words, spellable identically in
> styx, Rust, and vix.
>
> Sections are maps keyed by public name (`command { build {…} }`) because
> styx admits nothing else — an entry is at most two atoms, and a repeated
> key is a duplicate-key error, so there is no `command build {…}` form and
> no accumulation. The constraint turned out to be the right shape anyway:
> public names are unique by construction, and enumeration order is the
> map's canonical order, not authoring order.

### Why a file, and not frontmatter in the entry point

The obvious alternative is to put the manifest at the top of the package's
entry-point `.vix` file, the way PEP 723 puts script metadata in a comment
block and cargo-script puts a manifest in a doc comment. For a one-file
package that is plainly nicer, and it is not ruled out here. The reason to
start with a file is tooling, not doctrine:

- A standalone `.styx` document is *already* readable by everything that
  reads styx — `styx fmt`, the LSP's live schema validation and completion,
  a registry indexer, a bot bumping a pin — with no frontmatter convention
  to teach any of them first.
- Frontmatter has to be legal in **both** languages at once, and it is legal
  in neither: a `---` fence is not vix, and a manifest embedded in `//`
  comments is not a styx document. One of the two parsers grows a mode, and
  every tool downstream of it grows the same mode.
- A pin change stays a diff in a small data file rather than arriving inside
  a code diff, which is where a reviewer is looking for it.

None of that is permanent. The data model is carrier-independent — the
schema types decode a styx *document*, not a *file* — so admitting
frontmatter later is a READER change (find the bytes, decode the same
shape), not a format change. What it would cost is fixing the entry point's
name, since a manifest that says which module to load cannot itself be found
by loading that module: today the fixed name is `package.styx`, and with
frontmatter it would become `main.vix`. The convention does not disappear,
it moves onto the code.

> r[vixen.package.inputs-are-provider-pins]
>
> [DESIGN] The `input` section is a map of **provider pins**, and explicitly
> not a requirement list. Requirements stay where they always are — the
> capability parameters of the fns the other sections name
> (`vixen.machine.requirements-from-use`); an input answers the *other*
> question: when those fns demand `Rustc`, which package satisfies it, at
> which identity. A fn demanding a capability no declared input provides
> fails at binding, before any effect
> (`vixen.machine.binding-fails-before-effects`).
>
> An input is a discriminated union, one arm per way a value can enter a
> package from outside:
>
> - `@package { version, hash }` — another vix package, pinned by its
>   **source-tree identity**. That tree contains the provider's own
>   `package.styx`, so the provider's pins are covered transitively: one
>   hash pins the closure, and no lock file exists to generate, refresh, or
>   drift (`vixen.pins.come-from-the-ecosystem-lockfile` already covers the
>   many-pins case; the manifest carries only the few).
> - `@fetch { origin, hash, unpack }` — a pinned blob: an origin coordinate
>   whose scheme names the origin adapter, an upstream digest in the
>   `vixen.pins.format` spelling, and the blob→tree step. It decodes toward
>   the existing wire shape `PinnedBlobRef`; the resulting tree's identity
>   is the input's identity contribution
>   (`vixen.capability.rustc-is-materializable`), the archive bytes are
>   transport.
>
>   The coordinate's scheme is not magic and not new: it is the routing key
>   origin adapters already declare (`OriginAdapterDecl.schemes`, one
>   claimant per scheme, unclaimed ⇒ typed refusal, no default backend).
>   What IS undecided is which spelling the manifest genre blesses: an
>   **absolute** coordinate (`https://static.rust-lang.org/dist/…` — fetches
>   today through the http adapter, self-contained, hash-verified either
>   way) or a **logical** one (`registry://…` — the machine's installed
>   adapter decides transport, which is the mirror/vendor/air-gap seam but
>   makes the manifest depend on machine cooperation; today only the test
>   harness claims that scheme). `PinnedBlobRef.origins` is a list on the
>   wire, so logical-first-absolute-fallback is expressible without a new
>   shape. The corpus examples spell `registry://` to keep the logical seam
>   visible; the decision is open and this sentence is its marker.
>
> There is deliberately **no `@path` arm**. A local directory is an unpinned
> word in a document whose whole job is to be pinned — the flake `path:`
> input is the impurity this rule refuses to inherit. Local overrides are
> policy (`vx r --override-input cargo=../cargo`), which may change where
> work happens but never what a demand means, and an override produces a
> visibly different demand instead of a manifest that lies.

> r[vixen.package.commands-and-artifacts-are-fn-refs]
>
> [DESIGN, MVP] A command and an artifact are both **bare fn refs**: a
> public name (the map key) bound to a `module::fn` path into the package's
> own code. The fn is the whole recipe — its parameters declare its
> requirements, its return type decides its presentation — and the manifest
> repeats neither. There is no build-rule vocabulary, no `mkDerivation`, no
> scripts block: the language is the build rule, and purity makes an
> artifact nothing but its fn's memoized value (`vx build` twice is a store
> hit).
>
> The two sections stay distinct because they carry different claims.
> `artifact { bins { fn facet::build } }` claims the fn returns `Tree` or
> `Blob` — the materializability claim `vixen.delivery.result` requires —
> and the claim is checked at binding, so a record-returning fn named as an
> artifact fails before any effect, not at delivery. A command's return
> type is looser: whatever `vx r` knows how to present. The artifact name
> is committed API; the fn path behind it is private and renameable.

> r[vixen.package.programs-are-paths-into-values]
>
> [DESIGN] The `program` section is the on-disk spelling of a capability
> package (`vixen.capability.package-is-data`) — the distribution format
> that rule defers. A program is a **path into a tree, plus the four
> contracts** (for now: output protocol and target discipline; command and
> termination grammars when they land). Where the tree comes from is a
> two-arm union — `@input { name, path }` for a fetched distribution,
> `@artifact { name, path }` for a value this package builds — and the arm
> is **invisible to consumers**: a capability resolved from a toolchain
> archive and one carved from a just-built artifact bind identically. That
> invisibility is the composability seam; it is also why a build script or
> proc-macro binary needs no special story (the same rule already closes
> that case for capability values generally).
>
> Every program source must name a declared input or artifact of the same
> manifest; a dangling source is a manifest-coherence failure raised at
> binding, before any effect. The program key provides the capability type
> (`rustc` ⇒ `Rustc` — the implicit Pascal-casing is an acknowledged open
> question; an explicit `capability` field is the alternative if a real
> collision arrives).

> r[vixen.package.run]
>
> [DESIGN] `vx r <package> <command> [args…]` (alias `vx run`) resolves
> `<package>` through three rungs, each a stated word and never a probe:
>
> 1. the manifest of the package the working directory is inside, when it
>    declares `<package>` as an input (or is it);
> 2. otherwise the **machine manifest's default registry pin** — the
>    no-config rung: a bare cargo workspace with no `package.styx` anywhere
>    reaches the cargo package here, and that package's `build` command
>    reads `Cargo.toml`/`Cargo.lock` as data;
> 3. `vx r cargo@1.89 build` overrides explicitly.
>
> Once a package has a manifest, rung 2 is off for that package's own fns:
> writing a manifest means stating your toolchain, and a silent fall-through
> would unpin what the author just pinned. `<command>` then resolves in the
> named package's `command` map — an unknown command is a typed refusal that
> lists the commands the manifest does declare — and the fn runs as a root:
> capability parameters bind against the package's inputs and the machine
> manifest, and the working directory enters through a `workspace()`
> constant surface, the only door to "here".

> r[vixen.package.run-mvp]
>
> [DESIGN, MVP] The MVP is resolution rung 1 in its smallest honest form,
> chosen so the seam runs end-to-end today and every deferral is a typed
> refusal rather than a silent gap:
>
> - `vx r <package-dir> <command>` reads `<package-dir>/package.styx`
>   through `vix-package-schema`, resolves `<command>` in the command map
>   (unknown ⇒ refusal listing the declared commands), and follows the fn
>   ref into the package's `.vix` files — the named module becomes the root
>   file, its directory siblings become the module set, exactly the shape
>   `module_graph` already gives a directory.
> - The command fn must have **check-stream root standing** (today: a
>   `#[test]`-attributed fn). Only the named root runs. Root standing for
>   value-returning command fns — `fn build(…) -> Tree` demanded as a root
>   and presented — is the named next rung, not an MVP behavior; a command
>   naming a fn without standing is refused with exactly that sentence.
> - Inputs are read and reported but **not resolved**: capabilities come
>   from the machine manifest alone, and a command demanding more fails at
>   binding as always. `@package` input resolution, the registry rung, the
>   `workspace()` surface, artifacts (`vx build <pkg>#<name>`), program
>   exposure, and command argument tails are all deferred — each is listed
>   here so its absence cannot be mistaken for a decision.

## What it looks like

The manifest of a source package — every line a literal
(`corpus-next/package/facet/package.styx` is the CI-pinned original):

```styx
package {
  name facet
  version "0.1.0"
}

input {
  cargo @package{
    version "0.1.0"
    hash "blake3:…"        // cargo's source-tree identity — pins its closure
  }
}

command {
  build { fn facet::build }
  test { fn facet::test }
}

artifact {
  bins { fn facet::build }  // claims: facet::build returns Tree|Blob
}

program {
  fx {
    source @artifact{
      name bins
      path "bin/fx"
    }
    protocol @exit-only
    target @neutral
  }
}
```

The code half — the fns the manifest names, requirements as parameters:

```
use cargo::{build_workspace, test_workspace};

fn build(rustc: Rustc, sh: Sh) -> Tree {
    build_workspace(workspace(), rustc, sh)
}
```

The refusal — an unknown command names what exists:

```
vx: no command `bulid` in package `facet`
  package.styx declares: build, test
```

## Acceptance

1. **One seam, three artifacts.** The corpus manifests decode through the
   Rust schema (`vix-package-schema/tests/decode_corpus.rs`) and through the
   vix reader (`vixen-runtime/tests/package_manifest.rs`) — same bytes, and
   a drift on any side breaks the suite that names it.
2. **The MVP run.** A directory holding `package.styx` and a `.vix` file:
   `vx r <dir> <command>` runs exactly the named command fn through the
   production path and reports its checks. An unknown command, a missing
   manifest, a dangling fn ref, and a fn without root standing each produce
   their own typed refusal, with zero effects started.
3. **Coherence is checked, not trusted.** A program sourcing a declared-
   nowhere artifact is reported by name (`dangling_program_sources` in the
   vix reader; the binder's check when binding lands).
4. **No second lockfile.** Nothing in the MVP or the design writes a
   generated pin file; the manifest's own hashes are the few, the ecosystem
   lockfile's the many.

## Explicitly out

The registry (resolution rung 2's pin source, `@version` overrides,
distribution); cross-package `use` (an input's name as an importable
namespace — new module doctrine, today's graph is directory-local); root
standing for value-returning fns and `vx build <pkg>#<name>` delivery of
artifacts; command argument tails (a trailing `[String]` first, the
parsed-but-unlowered `command` grammar items as the typed surface later);
program exposure feeding the capability binder; schema publication via
`GenerateSchema`/`styx-embed` for editor validation. Each has a named home
above; none is 0.1.
