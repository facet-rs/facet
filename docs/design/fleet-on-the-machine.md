# Fleet on the Machine

Design pass for moving `vix-wire` off the frozen oracle and onto `vix::machine`.
No migration is implemented here.

## Current Blocker

`vix-wire` still depends on the frozen oracle at the exact places the funeral
cannot carry forward:

- `vix-wire/src/lib.rs` imports `vix::oracle::{Oracle, PathDemand,
  PathMissing, PathPending, Value}`.
- `FleetBackend` implements `vix::oracle::ExecBackend`, and `FleetRun`
  implements `vix::oracle::PendingRun`.
- executor-side observer evaluation does `Oracle::load(module)`,
  `vix::oracle::receive(observer)`, `oracle.invoke(...)`, and
  `vix::oracle::ship(&result)`.
- the wire tests build observer closures and decode observer results through
  `Oracle`, `Value`, `ship`, and `receive`.

The promotion slice in `vix/src/machine/driver.rs` is the right prior art:
`PendingInvocation` is already a store value with content-addressed identity
`closure_hash x canonical args x remaining arity`. That confirms the shipped
observer spine should be a machine store value, not an AST closure. It does
not, by itself, fully discharge O12 for fleet use yet: the spine is private
driver state today, the wire envelope is not public, and code availability is
not modeled in the public API.

## Proposed Public API

Names below are proposed, not an implementation prescription. The important
shape is that public machine APIs exchange typed store values and host-exec
progress, never `vix::oracle::Value`.

```rust
pub mod vix::machine {
    #[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
    pub struct StoreValue {
        pub schema: String,
        pub bytes: Vec<u8>,
        pub content_hash: Vec<u8>, // exactly 32 bytes
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct StoreHandle(i64);

    #[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
    pub struct CodeRef {
        pub module_hash: Vec<u8>,
        pub closure_hash: u64,
    }

    #[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
    pub struct CodeBundle {
        pub module_hash: Vec<u8>,
        pub bytes: Vec<u8>,
    }

    #[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
    pub struct ValueBundle {
        pub root: StoreValue,
        pub dependencies: Vec<StoreValue>,
        pub code: Vec<CodeRef>,
    }

    pub enum MachineArg {
        Word { schema: String, word: i64 },
        Value(StoreHandle),
        Imported(StoreValue),
        Int(i64),
        Float(f64),
        Bool(bool),
        String(String),
        Path(String),
        Flag(String),
        Tree(crate::exec::Tree),
    }

    pub struct NamedArg {
        pub name: String,
        pub value: MachineArg,
    }
}
```

`StoreValue` is phon-clean because it is just schema plus opaque bytes plus
the content hash already maintained by the value store. `PendingInvocation`
does not need to become a public Rust enum to cross the wire; it should cross
as a `StoreValue` whose schema is `Pending<T>`. The receiver validates the
content hash and only decodes the pending bytes when the value is demanded.
The existing `vix::value::Value` also derives `Facet` and serializes with
phon, but it is the wrong fleet surface: `ship()` deep-forces pending trees
and closure values carry legacy AST bodies.

## The Five Gaps

| Gap | Current vix-wire shape | Minimal machine replacement |
| --- | --- | --- |
| `Oracle::load` + `invoke` for shipped observer closures | Executor receives `module: String` plus `observer: Vec<u8>`, loads an oracle, receives an AST closure, builds a legacy `Run` value, invokes, then ships the forced result. | `Machine::import_value(ValueBundle) -> StoreHandle`, `Machine::intern_run_value(ok, output_tree) -> StoreHandle`, `Machine::invoke_pending(observer, &[run]) -> StoreHandle`, then `Machine::export_value(result) -> ValueBundle`. Loading source on the executor disappears from the observer path; demand resolves the closure hash against loaded or fetched machine code. |
| Ship/receive wire format | `vix::oracle::ship/receive` over `vix::value::Value`. | `phon` over `ValueBundle`, rooted at `StoreValue`. This includes `Pending<T>` values such as `PendingInvocation` without exposing their internal layout. Result values cross as store values too. |
| `ExecBackend` fleet hook | `FleetBackend: vix::oracle::ExecBackend`, returning `Arc<dyn vix::oracle::PendingRun>`. | `Machine::with_exec_backend(Arc<dyn MachineExecBackend>)`, where `MachineExecBackend::spawn(MachineExecRequest) -> Arc<dyn MachinePendingRun>`. This is the one public machine capability the fleet needs at the execution boundary. |
| `PendingRun` exec-progress adapter | `FleetRun: vix::oracle::PendingRun` waits for `PathReady`, serves `fetch_path`, and fetches the final tree on `flush`. | `FleetRun: MachinePendingRun` with the same progressive semantics but returning `MachinePathDemand` plus `crate::exec::ExecEvent`, not oracle types. The driver maps first demand to `DriveEvent::RunStarted` and flush to `DriveEvent::RunCompleted`. |
| `Oracle::call` generic `Value` args | Tests and callers pass dynamically shaped `Value` trees, structs, paths, bools, and capabilities into `Oracle::call`. | `Machine::call(name, &[NamedArg]) -> StoreHandle` and `Machine::intern_arg(expected_schema, MachineArg) -> StoreHandle`. The machine already knows entry parameter schemas; public calls should be schema-directed and return store handles/rendered values, not dynamic `Value`. |

The proposed execution hook is:

```rust
pub trait MachineExecBackend: Send + Sync {
    fn spawn(&self, request: MachineExecRequest)
        -> Result<std::sync::Arc<dyn MachinePendingRun>, String>;
}

#[derive(Clone, Debug)]
pub struct MachineExecRequest {
    pub command: String,
    pub plan: crate::exec::ExecPlan,
    pub capability: u64,
    pub mounts: Vec<crate::exec::Mount>,
    pub output: String,
    pub span: Option<(u32, u32)>,
    pub observer: Option<ValueBundle>,
}

pub trait MachinePendingRun: Send + Sync {
    fn demand_path(&self, path: &str) -> Result<MachinePathDemand, String>;
    fn flush(&self) -> Result<(crate::exec::Tree, crate::exec::ExecEvent), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MachinePathDemand {
    File(String),
    FinishRequired { path: String },
    Missing { path: String },
}
```

The observer field is optional because the machine can support both shapes:
the language may produce a pending run and observe it later, or the fleet may
send a run observer as part of the remote exec request so the executor returns
only the observer result. The important rule is that the observer is a
`Pending<T>` store value rooted in a `ValueBundle`.

## Code Availability

The pragmatic shape is the store's own spine/flesh model applied to code.
The shipped observer value carries the spine: closure hash plus canonical
arguments as the shipped identity. When the remote machine demands it, the
remote first checks whether the same module/code is already loaded; this is the
usual fast path. If the closure hash is unknown, the remote fetches the
content-addressed code flesh (`CodeBundle`) on demand through the same store
transfer path used for values. The closure hash remains the stable identity;
the code bundle is merely the fetched body needed to run that identity.

## Migration Shape

The migration is mechanically straightforward once the public surfaces above
exist:

1. Add the machine public value surface: `StoreHandle`, `StoreValue`,
   `ValueBundle`, import/export, schema-directed argument interning, pending
   creation/invocation, and result rendering.
2. Add the machine exec backend hook and move the current driver-local exec
   scheduling through it. The default backend remains the existing local
   `ExecCache` path, so machine tests should keep their current behavior.
3. Re-plumb `vix-wire`:
   - change `WireExecRequest.observer` from `Option<Vec<u8>>` plus `module:
     String` to `Option<ValueBundle>`, with code refs/fetch handled by the
     machine store;
   - change `ObserverResult { value: Vec<u8> }` to carry a `ValueBundle`;
   - remove every `vix::oracle` import from `vix-wire`;
   - implement `MachineExecBackend` for `FleetBackend`;
   - implement `MachinePendingRun` for `FleetRun`;
   - replace `evaluate_observer` with machine import/intern-run/invoke/export.
4. Update tests to use `vix::machine::Machine` as the entry point and decode
   results through store rendering or typed store inspection.

`FleetBackend` remains responsible for placement, tree location accounting,
`put_tree`, `pull_from`, and progressive path availability. `FleetRun` remains
the adapter from wire events to the machine's demand API. The language-level
contract does not change: a path projection waits only for that path; a flush
fetches the final tree; live identical runs join; completed runs tier-1 hit;
read-set verified runs tier-2 cut off.

Post-migration, `lua_builds_across_two_machines` should assert the same user
contract it asserts today:

- `lua.vix::lua(linux)` returns a tree containing `lua`;
- the cold call creates/requests five runs and completes the same three
  flushed `cc` runs;
- all completed fleet execs are `Ran` on the cold path;
- round-robin placement still produces at least one executor-to-executor
  gravity pull;
- the warm rebuild is a root machine memo hit and does not consult the fleet.

The wire-specific tests should keep their current behavioral contracts:

- `rmeta` is fetchable before `rlib` finishes;
- identical concurrent demands join one process;
- one run can serve many distinct observers without observer-result aliasing;
- unread mount changes tier-2 cut off over the wire;
- observer results are values projected on the executor, not output worlds
  shipped back to the orchestrator.

## Scope

This should be several PRs, not one migration commit:

1. **Machine public value surface.** Export/import store values, expose
   schema-directed call/arg/result APIs, and make `Pending<T>` values ship as
   opaque `StoreValue`s.
2. **Machine exec backend seam.** Introduce `MachineExecBackend` and
   `MachinePendingRun`, with the current local `ExecCache` as the default
   backend. This is the only semantically new public machine capability.
3. **Machine code flesh store.** Add code bundle export/import/fetch by
   content hash. Same-module-loaded is the fast path; remote fetch is the
   fallback.
4. **vix-wire migration.** Replace oracle APIs with machine APIs and update
   fleet/wire tests.
5. **Frozen evaluator deletion.** After `vix-wire` no longer imports
   `vix::oracle`, the funeral can remove oracle/engine evaluator tests per the
   parity ledger.

The value and pending APIs are mostly public surface over existing internals:
`ValueStore`, `StoreEntry`, `PendingInvocation`, `TreeEntry::Exec`, and
`DriveEvent` already exist. The fleet hook is a real public capability because
the current machine always schedules through its local `ExecCache`. The code
flesh store is also new public infrastructure, but not a new evaluator
semantic: it is the existing closure-hash identity plus a content-addressed way
to obtain the executable body when a remote machine does not already have it.
