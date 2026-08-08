+++
title = "The registry"
weight = 5
+++

How a package is published without anyone choosing a version number, and what
the registry checks before it accepts one.

The gap this closes: every other registry asks the publisher to do two things
no publisher can reliably do — pick a number, and classify their own change.
The number is unfalsifiable (nothing can contradict `1.2.4`) and the
classification is a guess made at release time about work done weeks earlier.
Measured semver compliance falls well short of total, and *semantic* breakage
runs several times *signature* breakage — the part a type system can check is
the small part. (The figures behind those two sentences, and their sources, are
in the design notes; they are evidence for this page, not claims of it.) This
page removes both asks and replaces them with facts the registry derives and
claims it can falsify.

Scope guard, stated first: **this is vix's own registry.** Consuming crates.io
is unchanged — version requirements, semver, the existing resolver, exactly as
cargo does it. Nothing here applies to a `Cargo.lock`. These rules bind packages
published *to vix*, whose publishers accepted them to get in.

Design notes, including the paths rejected and the reasoning behind each rule,
live at `vix-core/corpus-next/registry/README.md`. This page is normative; that
file is not.

## Identity

> r[vixen.registry.no-declared-versions]
>
> [DESIGN] A publication carries **no publisher-chosen version number and no
> publisher-chosen change classification**. It is identified by content. Where
> an ordering is needed, the registry derives it from **ancestry** — the
> publication's commit parents, read from the VCS — never from publish order.
> Publish order makes a backported fix a successor to the release it was
> backported from, which is wrong for precisely the query where being wrong is
> expensive: *what is the most recent combination that works*.

> r[vixen.registry.two-hashes]
>
> [DESIGN] Every publication carries **two identities over one tree**. The
> *code hash* covers everything and moves on any edit. The *spec hash* is a
> projection of the same tree that keeps only behavior notations and erases the
> rest, transitively. **Behavior changed** means *the spec hash moved* — a
> refactor moves the code hash alone and is therefore provably compatible, and
> returning a notation to a prior value returns the spec hash to its prior
> value, so "compatible with what it used to be" is a consequence of hashing
> rather than something anyone has to remember.

> r[vixen.registry.notations-are-requirements]
>
> [DESIGN] A behavior notation is a **reference to a spec requirement** — an
> `r[…]` identifier — not free text and not a number, and it sits **at the
> expression it describes**, not as an attribute on the enclosing function.
> Being an expression in the tree, it cannot drift away from the code it
> describes and needs no annotation syntax. It is the same identifier carried by
> `r[impl …]` in implementations and `r[verify …]` in tests, under the same
> fidelity authority. The discipline: **tag what you promise, not what you do** —
> a notation on an implementation detail leaks into the contract hash for no
> observable reason.

## Publication

> r[vixen.registry.tests-run-here]
>
> [DESIGN] Publication requires the package's tests to **execute on the
> registry**, hermetically. A publisher's assertion that they ran them is not
> evidence and is not accepted in place of running them.

> r[vixen.registry.coverage-or-dead]
>
> [DESIGN] Every branch of the package's **own vix code** must be exercised by
> its tests. **Used and covered, or dead and removed before publication** —
> there is **no third category and no exemption**. This is a dead-code mandate,
> not a unit-test mandate: integration tests exercise internals, and under
> demand-driven evaluation a node that was demanded is a node that contributed
> to a recorded output. Branch coverage, not path coverage: paths are infinite,
> branches are finite and structural.
>
> An earlier draft exempted branches "unreachable from any export". That
> exemption is struck: deciding reachability without running the program is a
> whole-program static analysis, which is the plan phase vix does not have and
> cannot have. A branch the tests never reached is *indistinguishable* from a
> branch that cannot be reached, so the exemption would have to be taken on the
> author's word — and this page exists to stop taking authors at their word.
> Delete it or cover it.

> r[vixen.registry.coverage-is-per-target]
>
> [DESIGN] Coverage and recorded behavior are **per target**. A package
> claiming two targets has two coverage sets and two corpora, and a
> target-specific branch is live only on the executor that ran it. **A target
> that was never run cannot be claimed.**

> r[vixen.registry.foreign-code-is-replayed-not-covered]
>
> [DESIGN] The coverage mandate **stops at the vix/foreign boundary**. Foreign
> code a package builds or wraps is constrained by replay, not coverage: replay
> needs only hermetic determinism at the observed boundary and therefore follows
> anything vix can execute, while coverage needs the evaluator and therefore
> does not. A package that builds a C library must fully cover its own vix code
> and must not be asked to cover the C. Foreign coverage may be *reported* as
> the foreign toolchain's self-report, presented distinctly, because it is that
> toolchain's word and does not carry the same authority.

## Claims

> r[vixen.registry.replay-against-predecessor]
>
> [DESIGN] **Spec hash unchanged ⇒ the predecessor's recorded behavior must
> reproduce exactly.** Spec hash moved ⇒ no such obligation. Replay uses the
> **predecessor's** tests, which are immutable and content-addressed — never the
> new publication's, or the escape is to change the behavior and quietly weaken
> the test. The predecessor's outputs are already memoized, so the old code is
> never re-run: the check is one execution of the new code against the old
> recorded inputs. A publication that contradicts its own unchanged claim is
> **rejected with a witness** — the input, the old output, the new output.

> r[vixen.registry.corpus-is-the-contract]
>
> [DESIGN] Coverage is a **liveness floor**; the recorded corpus is the
> **contract**. They are different obligations and both hold. Branch coverage
> does not imply pinned behavior — every branch can be hit with one input apiece
> while every boundary stays unconstrained — so what a consumer may rely on is
> what was *observed at the exports*, not what was merely executed.

> r[vixen.registry.stability-before-claims]
>
> [DESIGN] A corpus is checked for **stability before any claim is checked
> against it**. Nondeterminism in replay does not cause missed lies; it causes
> **false accusations**, which is the more expensive failure. A package whose
> recorded outputs disagree across lanes is not told it broke its contract — it
> is told its behavior is not stable, so no claim can be held against it, and
> its entry shows those behaviors unpinned.

> r[vixen.registry.contraction-is-visible]
>
> [DESIGN] Deleting code, and with it the tests that covered it, **shrinks the
> promised surface**. This is not a behavior change and not a violation, but it
> is a withdrawal of a promise, and it is surfaced on the entry as a distinct
> event naming what stopped being guaranteed. The publisher cannot retroactively
> un-promise: prior publications and their tests are immutable.

## What a rejection looks like

```
rejected: facet/1 #17 claims decode's behavior is unchanged since #16
  decode("1e-3")  #16 → Ok(0.001)   #17 → Err(BadExponent)
  either update the spec notation on decode, or this is a regression
```

A bug report, not a lint. The witness is what makes the claim falsifiable, and
falsifiability is the whole difference between this and a version number.

## What an entry looks like

```
facet · 47 exports · 312 branches, 312 covered
  x86_64-unknown-linux-gnu   289 behaviors pinned
  aarch64-apple-darwin       289 behaviors pinned
  foreign (libpng, gcov, self-reported)   71%
```

Everything unverified is **named**, never hidden. A consumer can see exactly how
much of what they are taking is backed by recorded behavior, on which targets,
and which part is somebody else's word.

## Acceptance

1. **A refactor publishes clean.** Code hash moves, spec hash does not, replay
   reproduces, publication accepted with no claim event.
2. **A silent behavior change is rejected**, with a witness naming the input and
   both outputs.
3. **A declared behavior change publishes**, and the changed requirement is what
   the entry cites as the reason.
4. **A revert restores compatibility.** Returning a notation to a prior value
   returns the spec hash to its prior value, with nothing remembering that it
   had changed.
5. **Weakening a test does not launder a change** — replay runs the
   predecessor's suite, so the contradiction still surfaces.
6. **Dead code blocks publication**, naming the uncovered branches.
7. **A flaky corpus is not an accusation** — it publishes with those behaviors
   marked unpinned, and no contract violation is reported.
8. **A foreign-code package** fully covers its vix code, is not asked to cover
   the foreign code, and still gets replay across the exec boundary.

## Open

These are decisions, not rules, and are recorded so they are not mistaken for
settled:

- **The `registry://` coordinate** — logical or absolute. It came from the
  package-manifest work, which is retracted; nothing carries it now, and a
  package currently has no addressable identity at all. This is the only
  genuinely undecided item in the mechanism and also the one everything else
  waits on.
- **Mutation testing.** Coverage plus recorded outputs says a branch ran and
  what it returned; mutation says whether *any* change to it would move a
  recorded output — whether the corpus pins behavior or merely observes it.
  Normally unaffordable; hermetic, memoized, and parallel over a pure IR it may
  not be. A quality signal, not adopted as a gate.
- **Consumer-side evidence.** If one package can depend on another, the
  registry runs dependents' tests too and can observe when a change breaks
  them. Whether that returns to the publisher as advice or as something
  stronger is undecided — and it is moot until packages can reference each
  other at all, which `r[vixen.package.*]` currently puts explicitly out. The
  same caveat applies to `two-hashes`' word *transitively*: within one
  package's tree it is exact, and across packages it describes a mechanism
  that does not yet exist.
- **Contraction presentation.** `contraction-is-visible` fixes that it is
  surfaced, not how.

## Deliberately not

Not deferred — refused. Each names the system that does the thing and the
reason we will not.

- **Elm's `elm bump` and `elm diff`.** The closest living prior art and the
  strongest objection to this page, because Elm already took the number away
  from the publisher. `elm diff` compares a package's API against a published
  version and classifies the change as MAJOR, MINOR, or PATCH; `elm bump`
  writes the resulting version into `elm.json`; `elm publish` rejects a
  version that does not match the diff, with an `INVALID VERSION` error. It
  works, it ships, and the instinct is right. It is also, precisely, the small
  part. Elm's diff sees types and nothing else, which Elm's own tracker
  documents: a UI package that "introduced a whole new way of managing the
  aesthetic / styles" of its components was classified as a minor change,
  because no signature moved (elm/compiler#2145). *Semantic* breakage runs
  several times *signature* breakage, and a derived number is read as a claim
  about behavior — being derived makes it more credible, not more true. We
  keep the derivation and drop the number: `r[vixen.registry.two-hashes]`
  diffs declared behavior rather than signatures, and
  `r[vixen.registry.replay-against-predecessor]` checks the half no diff can
  see.

- **CI as the registry.** The practice essentially every publisher uses, and
  which this page has so far declined to name: a tag push triggers a workflow
  that runs the tests and then `cargo publish` or `npm publish`, and the
  evidence a consumer actually sees is a green badge in a README.
  `r[vixen.registry.tests-run-here]` refuses it. A green workflow asserts that
  a run happened, on a machine the publisher configured, with whatever the
  runner had installed — it is the publisher's word with a check mark on it.
  npm's `--provenance` is the strongest form of this and still does not close
  the gap: it signs a Sigstore attestation binding the tarball to a source
  repository, commit, and workflow, logged in a public transparency ledger,
  and npm's own documentation says it "does not guarantee the package has no
  malicious code". Provenance answers *where this was built*. It says nothing
  about whether the tests pass, which is the only thing this registry accepts.

- **Go's major-version import paths.** Go writes the compatibility claim into
  the identifier: "Starting with major version 2, module paths must have a
  major version suffix like `/v2` that matches the major version", enforcing
  what the Go reference calls the import compatibility rule — "If an old
  package and a new package have the same import path, the new package must be
  backwards compatible with the old package." It is a rename convention, and
  it is unfalsifiable in exactly the way a version number is: an author who
  breaks behavior without renaming ships a module every consumer takes
  silently, and no part of the toolchain can contradict them. It also makes an
  incompatible change a *rename*, which we cannot do —
  `r[vixen.registry.no-declared-versions]` identifies a publication by content,
  and a name is not where claims live.

- **A resolver.** cargo unifies semver-compatible requirements so one build of
  a dependency serves everyone who asked for it; npm nests `node_modules` so
  two versions coexist in one program; pub and uv run PubGrub and report an
  unsatisfiable graph as a derivation tree. Every one of them is machinery for
  reconciling *publishers' compatibility claims*, and this page removes the
  claims — so there is nothing left to solve, and nothing to defer either.
  State the cost with the benefit, because a reader will find it either way:
  for a vix package there is no version requirement to write, no lockfile to
  commit (vixen adds none — `r[vixen.pins.come-from-the-ecosystem-lockfile]`),
  no diamond conflict, and **no diamond resolution**. Nobody unifies two
  requirements on your behalf, because nobody stated one. Consuming crates.io
  is untouched; a `Cargo.lock` keeps meaning exactly what cargo means by it.

- **A coverage percentage.** Codecov posts a commit status from
  `coverage.status.project.default.target` and `threshold`; with `target: auto`
  the comparison is against the base commit, and `threshold` "allows the
  coverage to drop by `<number>%`, and posting a `success` status". A number
  that may fall and still be green is a budget for dead code, denominated in
  percent. `r[vixen.registry.coverage-or-dead]` is not a number: every branch
  of a package's own vix code is exercised or the package is not published,
  there is no target to set and no threshold to tune, and the foreign half is
  presented as its toolchain's self-report rather than averaged into one
  figure (`r[vixen.registry.foreign-code-is-replayed-not-covered]`). Nothing
  here trends.

- **Deleting a publication.** npm allows unpublishing within 72 hours if
  nothing depends on the package, and after that only if it has no dependents,
  fewer than 300 downloads in the last week, and a single maintainer — and the
  tarball goes. crates.io never deletes: `cargo yank --version 1.0.7` "does
  not delete any data, and the crate will still be available for download via
  the registry's download link", it only stops resolution from selecting it.
  We are on the crates.io side of that line and go further than yanking
  reaches. A publication cannot be removed, because
  `r[vixen.registry.replay-against-predecessor]` runs the *predecessor's*
  tests against a successor: a later publication's admissibility is a fact
  about bytes that must still exist to be checked. Withdrawing a promise
  already has a mechanism that deletes nothing
  (`r[vixen.registry.contraction-is-visible]`). A softer signal is merely out
  for 0.1, below; removal is not on the road at all.

## Explicitly out

Version requirements, ranges, and the resolver that consumes them — for vix's
own packages. Consuming crates.io is untouched: a `Cargo.lock` keeps meaning
exactly what cargo means by it, and `r[vixen.pins.come-from-the-ecosystem-lockfile]`
is where that reading lives. Yanking, ownership, and transfer. Any mechanism by
which a publisher asserts compatibility rather than having it derived or
falsified.
