+++
title = "The exec rail"
weight = 34
+++

How a capability-carrying, non-hermetic effect becomes a registered primitive —
the design [issue #2597](https://github.com/facet-rs/facet/issues/2597) gates
`Op::Exec`'s exit from the scheduler on.

The gate question, as the issue put it: what does an authority-crossing effect
look like when the scheduler holds none of its logic? `Op::InvokePrimitive` and
`Op::InvokeCodataPrimitive` are the rails that exist, and neither models an
effect whose demand preimage must include a capability's identity. This
document answers that question. It is a mechanism design: the *semantics* of
exec are already settled by the spec (`machine.primitive.exec-outcome`,
`machine.primitive.exit-status-is-not-a-value`,
`machine.primitive.command-package`, `machine.primitive.exec-identity`,
`machine.capability.no-argv-dialect`) and nothing here revisits them. What is
designed here is how the registered-primitive rail carries them, so that the
scheduler ends up holding only what `machine.primitive.registered` permits it
to hold: keying, parking, admission, and receipts.

## What the scheduler holds today, and which parts are already generic

The inventory, from `vix-core/src/runtime/scheduler.rs`:

- **Keying.** `submit_exec` builds
  `DemandPreimage { closure: exec_plan_recipe(argv), arguments: [capability.identity] }`
  and derives the demand key from it. This is the ordinary preimage shape used
  by every demand in the machine — the tier-1 exec identity
  (`machine.primitive.exec-two-tier-key`) is *already* "closure × argument
  identities". Nothing about the keying is exec-specific except that it is
  hand-rolled at one call site instead of derived from a declaration.
- **Parking.** The spawned process is waited on by an isolated worker thread;
  the raw termination crosses the unified completion inbox; the parked frame
  resumes when the scheduler drains it (`machine.scheduler.block-on-event`).
  This is exactly the demand-owned-ticket discipline registered primitives
  already use — exec merely has a private copy (`ExecPending`,
  `apply_exec_completion`) beside the generic one.
- **Progressive products.** `read_exec_stdout` parses `vix-ready\t<path>`
  protocol lines; `submit_exec_projection` gives each announced product its own
  demand identity and memo location, served from protocol readiness or, as the
  safe fallback, process exit — never from filesystem polling. This is the
  mechanism that lets a consumer demand `out.tree / p"lib.rmeta"` while codegen
  is still running. It is sound, and it is trapped: only exec can use it.
- **Domain residue.** The scheduler also holds what it must not: it sniffs the
  capability's *record name* (`ProgressiveSh`) to select the output protocol —
  a per-tool match arm in machine code, exactly what
  `machine.capability.no-argv-dialect` bans; it owns the process boundary
  (`std::process::Command`), the workspace, the archive encoder, and the
  trivial termination mapping. `Op::Exec` itself is a per-primitive op with a
  per-primitive ABI ordinal, which `machine.primitive.registered` names as the
  disease.

The design consequence falls out of the inventory: **nothing about exec needs a
new kind of machinery — it needs three existing mechanisms generalized from
private copies into declared contracts**, and one seam (the process boundary)
pushed behind a service object.

## The rail, in four moves

### 1. Capability-role arguments key the demand

`ArgRoleDecl` today has exactly one variant, `Value`. It gains `Capability`.
A registered declaration marks which request arguments are capabilities, and
the rail derives the effect demand preimage generically:
`closure = the normalized request recipe`,
`arguments = the capability arguments' identities, in declaration order`.
That is byte-for-byte the preimage `submit_exec` builds by hand today, so
**moving exec onto the rail re-keys nothing**.

The role split is what makes the two halves of a capability separable: the
*identity* enters the preimage (it is what the demand means), while the *value*
is redeemed on the host side by the backend (it is what the demand runs).
`machine.primitive.capability-role` pins this.

### 2. The process boundary is a service, and hermeticity is an identity claim

Registered primitives already receive external authority as explicit
`PrimitiveServices` trait objects (`OriginAdapter`, `ValuePersistence`). Exec's
backend joins them: an `ExecBackend` owning spawn, stream, and wait. The
initial backend is the current behavior verbatim — `std::process::Command`,
fresh workspace, host-trusting — and is *declared* as such: per
`machine.primitive.memo-policy` a backend performing ambient reads it cannot
witness supports no `Hermetic` claim, so its receipts carry the capability
witness as `Unverifiable`, exactly as `submit_exec` records today.

The load-bearing property is that the demand key never mentions the backend.
Identity is "this plan under this capability" now and forever; when a confining
backend (VFS interposition, mount declarations, materialized toolchains —
`machine.primitive.exec-hermetic-traps`) replaces `Command::new`, what improves
is the *trust* in the witness, not the key. Enforcement is an upgrade to the
service, not a migration of the memo.

### 3. Progressive response projections become rail vocabulary

`EffectCtx` already lists progressive projection/codata publication as part of
a primitive's machine window (`machine.primitive.effectctx-witness-only`); the
scheduler already serves per-product projection demands for exec. The move is
to make the publication path *declaration-driven*: an in-flight registered
effect may publish

- **byte-stream extensions** — stdout/stderr chunks, addressed by byte offset
  (`machine.primitive.exec-outcome`: OS writes and transport frames are not
  keys; line framing is an explicit projection, a stdlib combinator over the
  bytes, not machine vocabulary), and
- **product readiness** — an immutable product at a declared path, on the
  authority of the output protocol (or effect completion as the fallback).

Each served projection is its own demand with its own memo location — the
generalization of `submit_exec_projection`, with the `ExecTreeText` special
case replaced by the projection vocabulary the value's schema already has. The
settled tail of the response — `answer` and the completed `tree` — is published
once, at completion, through the ordinary ticket.

This is what makes the two pipeline laws testable facts rather than intentions:
demanding `out.answer` parks until termination (the answer is a fact about a
finished process); demanding `out.tree / p"lib.rmeta"` resolves the moment the
protocol publishes it, while the process runs. Neither demands the other.

### 4. Protocols come from the capability package, not from core

The output protocol, termination grammar, command grammar, and product protocol
are the capability package's four contracts (`machine.primitive.command-package`,
SETTLED). The `ProgressiveSh` name-sniff dies with `Op::Exec`; the exec
primitive reads the protocol from the capability value's typed content. The
termination grammar likewise rides the capability: today's trivial grammar
(exit zero ⇒ unit answer; any other termination ⇒ typed `ProcessFailure`
carrying the raw termination data) is the degenerate case of
`machine.primitive.exit-status-is-not-a-value`, and it stays data — versioned
command semantics entering command identity — not machine code.

## What deletes from core

When exec rides the rail, `vix-core` loses: `Op::Exec` and its ABI ordinal,
`lower_exec`, `submit_exec` / `evaluate_exec` / `submit_exec_projection` /
`apply_exec_completion`, `ExecPending` / `ExecProjectionPending` /
`ExecWorkspace` / `ExecProtocol` / `read_exec_stdout` /
`validate_exec_product_path` / `archive_directory`, and the `ProgressiveSh`
name match. What it keeps is what every registered effect uses: preimage
derivation from declarations, the demand-owned ticket and completion inbox,
projection demand service, receipts. The exec primitive itself lands in
`vixen-primitives` beside `untar`, bound through the injected-surface rail so
core spells no name.

## Honest deltas against the settled spec

Stated so the implementation cannot silently claim more than it ships:

- Today's runtime `ExecOutcome` is `{ tree, stdout, stderr }` with stdout and
  stderr as lossy UTF-8 line-maps settled at exit, and `answer` implicit.
  `machine.primitive.exec-outcome` wants byte codata and an explicit `A`. The
  rail move carries the *mechanism* for byte-stream projections; upgrading the
  surface shape to the settled one is part of the move, but replay of a stream
  from a memoized outcome must be indistinguishable from a live one, and the
  witness records published extensions to make that so.
- Tier-2 reuse (read-set-verified, the anti-Nix event) is specified
  (`machine.primitive.exec-identity`) and remains future work; the rail carries
  tier 1 exactly as today.
- Fixtures do not ride this rail. `FixtureStore` is an `OriginAdapter` —
  origin-shaped, not exec-shaped — and exits with the fetch/origin work.

## Acceptance

The implementation is done when, beyond behavior parity:

1. **Demanding the answer synchronizes.** A test demands `out.answer` (for
   anything) and observes that evaluation parks until process termination.
2. **Not demanding it doesn't.** The rustc simulation: a producer announces an
   early product (the `.rmeta` shape) through the output protocol; a dependent
   demand consumes it and completes — observably, by event order — before the
   producer terminates; the test then demands the full outcome and observes
   completion. Together with (1) this pins that streams and products progress
   independently of termination and that the answer alone is the
   synchronization point.
3. **Nothing re-keys.** The same program under the same capability produces
   the same demand key before and after the move (the untar precedent:
   identity survives the rail).
4. **Core is clean.** No `Op::Exec`, no exec-named scheduler state, no
   capability record-name matches anywhere in `vix-core`.

## Staging

Two implementation steps, each gateable alone:

1. **Rail extension** — `ArgRoleDecl::Capability` + declaration-derived effect
   preimages; `ExecBackend` as a `PrimitiveServices` entry; progressive
   publication generalized. Exec still runs on `Op::Exec`; the rail is proven
   by its own tests.
2. **The move** — exec becomes a registered primitive in `vixen-primitives` on
   the extended rail; the core machinery listed above deletes; the acceptance
   tests land.
