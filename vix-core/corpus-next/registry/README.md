# A registry with no version numbers

Design notes, not a spec. `/spec/vixen/registry` carries the normative rules;
this file carries the reasoning that produced them, the paths that were tried
and rejected, and the questions still open — so none of it has to be
re-litigated from memory.

There is no code fiction here yet. Writing one (a package that publishes, a
package that consumes it, a publication that gets rejected) is the natural next
exercise and would pressure this the way `corpus-next/libpng` pressured
templates.

## Scope, stated first

**This is vix's own registry only.** Consuming crates.io stays exactly as cargo
does it — version requirements, semver, the existing resolver. Nothing here
proposes changing how vix reads a `Cargo.lock`. The design applies to packages
published *to vix*, by people who accepted vix's rules to get in.

**One removal from the usual design:** nobody picks a version number, and
nobody classifies their own change. The semantics show what happened.

## Why versions were dropped

The starting question was whether a versionless registry can work at all. Two
rounds of adversarial research (blue team / red team) produced these findings.

**The non-transitivity trap is real but avoidable.** "Compatible with" as a
*pairwise* relation is not transitive — A works with B, B works with C, A fails
with C — and a partition computed from a pairwise relation is therefore
ill-defined. This is what sank OSGi's version ranges as a solver input. The
escape is that semver's own compat-class is not pairwise: it is
**connected-components over a chain**, where the edges are "adds only". A chain
needs an ordering, and the ordering is **ancestry**.

**Ancestry is not a new thing to declare — it is already in the VCS.** The
registry reads the commit's parents. It is *not* publish order: publish order
says a backported 1.x security fix is a successor to 2.0, which is wrong for
precisely the query where being wrong is expensive ("what is the most recent
combination that works").

**Multiple majors coexisting is not a Rust law of nature.** Alex's objection —
"in Rust we can have several hundred 1.x serdes if we want" and "my nixconfig
has seven nixpkgs inputs and that's useful" — is correct and was the right
challenge. Amos's read is that the "only across majors, not across minors"
restriction is a cargo-specific artifact, not something the design must inherit.

**Bazel's `compatibility_level` is not the precedent it looks like.** It was
cited in round one as evidence that you need an explicit compatibility marker.
Round two found it was **deprecated and made a no-op in Bazel 8.6.0 / 9.1.0**.
The citation was withdrawn.

**Semver compliance numbers.** Ochoa et al.'s replication found **83.4%**
compliance, not the weaker "wrong a third of the time" figure that circulates.
Sembid found semantic breakage at **2–4× the rate** of signature breakage —
which is the actual argument: the part of a contract a type system can check is
the small part.

**What versions actually bought us, in the end:** ranges, which limit dependency
churn. That is a real service and its replacement is the reproducible bisect —
"I've been away six months, find the most recent combination of packages where
nothing breaks" — which is a query the registry can answer directly because it
can run everything hermetically. Hyrum's Law cuts in our favor here: if it
builds and passes on every platform we care about, nothing has broken.

## The mechanism

### Two hashes over one tree

The code hash covers everything and moves on every refactor. That over-
approximates behavior change badly, so a second identity is projected from the
same tree: the **spec hash**, which keeps only behavior notations and erases
the rest, transitively.

- refactor → code moves, spec doesn't → **provably compatible**
- behavior change → the notation changes → spec moves
- revert to a prior behavior → the notation reverts → **the hash reverts**, and
  "declared compatible with the old one" falls out of hashing rather than being
  remembered

### Notations are requirement references, at the expression

Amos's constraint was that this be *economic*: not a number slapped on a
function as an attribute. The resolution:

- **expression level, not function level** — you annotate the thing that
  changed, while you are looking at it
- **not a number, not a classification** — a reference to a **spec requirement**
- these are the `r[…]` identifiers this project already runs, with `r[impl …]`
  in implementations, `r[verify …]` in tests, and `ddc coverage` as the fidelity
  authority. It ties into the requirement-tracking work with Tracy.
- "they might be functions, mostly identity functions" — making it an expression
  in the tree rather than a comment means it cannot drift away from the code it
  describes, and needs no new attribute syntax

The discipline that keeps the closure from over-firing: **tag what you promise,
not what you do.** A notation on an internal implementation detail ("uses binary
search") leaks into the contract hash for no observable reason.

### The lie is checkable

A version number's claim is unfalsifiable. A spec-hash-unchanged claim is not:

> **spec hash unchanged ⇒ the predecessor's recorded behavior must reproduce
> exactly.** Spec hash changed ⇒ no such obligation.

You are never asked to prove you broke nothing (impossible). You are asked not
to **contradict your own recorded behavior while claiming it is unchanged**.

**Replay uses the predecessor's tests, not the new ones.** Otherwise the escape
is trivial: change the behavior, quietly weaken the test, regenerate the corpus,
no contradiction. The predecessor's suite is immutable and content-addressed;
run *that* against the new code.

**It is nearly free.** The predecessor's outputs are already memoized, so the
old version is never re-run — you run the new code once against the old recorded
inputs and compare.

**Rejection carries a witness**, which is what makes it a bug report rather than
a lint:

```
rejected: facet/1 #17 claims decode's behavior is unchanged since #16
  decode("1e-3")  #16 → Ok(0.001)   #17 → Err(BadExponent)
  either update the spec on decode, or this is a regression
```

### Tests run on the registry

A publisher's word that they ran the tests is not evidence. This is the
"we're not doing a cargo here" requirement: the tests execute registry-side,
hermetically, or the publication does not happen.

### Coverage, and what it is for

The insight that makes 100% coverage meaningful here and vanity everywhere else:
**coverage plus recorded outputs means the test author does not have to write
assertions.** Elsewhere, covering a branch only proves it executed — a test that
calls a function and ignores the result scores 100% and pins nothing. Here the
registry records `(input → output)` for every demanded node, so **the replay is
the assertion** and the recorded value becomes the golden automatically.

Branch, not path. Paths are infinite; branches are finite and structural.

**Amos's rule: used and covered, or dead and removed before publication.** This
is a dead-code mandate, not a unit-test mandate — nobody writes a unit test per
private helper, integration tests exercise internals, and demand-driven
evaluation means "this node was demanded" is very close to "this node
contributed to a recorded output". That equivalence is why the mandate is
reasonable here and unreasonable in an imperative language.

Error branches are the mandate's **best** case: the arms nobody tests are the
failure paths, and failure paths are where the bugs are.

**Coverage and the corpus are two different jobs**, and both are needed:

| | what it does |
|---|---|
| coverage | a **liveness floor** — everything shipped is exercised, nothing more |
| recorded corpus | the **contract** — what was observed at the exports is what is promised |

They are not the same, because branch coverage does not imply pinned behavior:
you can hit every branch with one input apiece and leave every boundary
unconstrained.

### Stability gates the claim

Nondeterminism in the replay check does not cause missed lies — it causes
**false accusations**, which is the far more expensive direction. So the corpus
is checked for stability (the existing plain/chaos lanes) *before* any claim is
checked against it. A flaky package is not told it broke its contract; it is
told its recorded behavior is not stable, so no claim can be held against it,
and its entry shows unpinned.

## Scope of the coverage mandate — the thing that was almost misread

The mandate covers **a vix package's own vix code**. It does not extend to
foreign code a vix package builds or wraps. Two mechanisms, different
requirements:

| mechanism | what it needs | scope |
|---|---|---|
| **differential replay** | hermetic determinism at the observed boundary | anything vix can execute — C, Rust, Python, rustc |
| **coverage** | owning the evaluator | vix code only |

A package that builds libpng records behavior at the exec boundary — inputs in,
tree and exit and bytes out — and nothing about that cares what language is
inside. Its *own* vix code must be fully covered. libpng's C must not.

Foreign coverage may be reported on the entry as the toolchain's self-report
(gcov, coverage.py), visibly distinct from vix coverage, because it is the
toolchain's word and does not carry the same authority.

## Why hermeticity is a detection problem, not a sealing problem

Alex asked whether the syscall behind `SystemTime::now` can be hooked. It
cannot, and the reason generalizes:

- `clock_gettime(CLOCK_REALTIME)` is served by the **vDSO** — a userspace page,
  no syscall entry. seccomp, ptrace, and syscall-user-dispatch never see it.
- **time namespaces** (`CLONE_NEWTIME`, Linux 5.6+) do virtualize the vDSO path,
  but `time_namespaces(7)` states that `CLOCK_REALTIME` is **not** virtualized.
  Monotonic and boottime, yes. The one Rust's `SystemTime` uses, no.
- `RDTSC` is an instruction (trappable via `PR_SET_TSC`, at ruinous cost);
  `RDRAND` is an instruction trappable only from a hypervisor; thread
  interleaving is not a call at all and is where real build nondeterminism
  comes from.

Real sealing is available (gVisor ships and maps its own vDSO; a microVM works)
and is not free. It is not needed, because the strategy is **normalize the cheap
sources, then detect by differential** — which is reproducible-builds.org's
methodology and Nix's `--check`, and which the plain/chaos lanes already provide.

## Rejected

**Function-level attributes carrying a number.** Rejected as not economic: it
asks the publisher to classify their own change, which is the thing being
removed.

**"Untested behavior is undefined behavior, and we shrug."** Alex's proposal,
and it was a real improvement in one respect — it turns a coverage gap from a
confession into a definition, and it dissolves the zero-test policy question.
Rejected as the *whole* answer because Amos does not want the compromise: dead
code should not ship. But its core survives as
`vixen.registry.corpus-is-the-contract` — coverage guarantees liveness, the
corpus defines the promise.

It also had a perverse incentive that the dead-code mandate closes: if tests
define obligations, the way to buy freedom is to delete tests. Under the
mandate that fails the publish instead. The only way to shed an obligation is to
**delete the code**, which is an honest removal of surface.

**A pairwise compatibility relation.** See the non-transitivity trap above.
Join is a *check* (congruence closure, near-linear), never a solver input.

## Open

1. **The `registry://` coordinate** — logical or absolute. Carried over from the
   package manifest work; still the one genuinely undecided item.
2. **Mutation testing.** Coverage-with-recorded-outputs says a branch ran and
   what it returned; mutation says whether *any* change to that branch would
   move a recorded output — i.e. whether the corpus pins behavior or merely
   observes it. Normally unaffordable; hermetic, memoized, parallel, over a pure
   IR, it may not be. Flagged as a quality signal, not adopted as a gate.
3. **Contraction events.** Deleting code shrinks the promised surface. The rule
   says that is visible on the entry; the exact presentation is unspecified.
4. **Consumer-side evidence.** The registry runs downstream tests too, so it can
   observe when a change breaks dependents. Whether that feeds back as advice
   ("this moved and broke 3 suites — want to pin it?") or as anything stronger
   is not decided.
