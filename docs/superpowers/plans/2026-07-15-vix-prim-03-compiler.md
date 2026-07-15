# Vix Typed Primitives — Phase 03: Compiler Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a registered primitive callable from vix source — `probe_version where { text: ... }` type-checks its named fields against the primitive's registered Request record and lowers to a new `Op::EffectRequest` node typed as the Response — with typed diagnostics for every mis-call, and zero lowering/scheduler wiring (phases 04–05).

**Architecture:** The compiler is handed a **`PrimitiveManifest`** — a name-keyed projection of registered `PrimitiveDescriptor`s carrying only vir types + an effect id, no handlers. It threads into `ModuleContext`. `lower_where_call` (today hardcoded to `range`) generalizes: on a manifest hit it checks the `where { ... }` fields against the Request `RecordType` (reusing the field-diagnostics idiom from `lower_named_record_values`), builds an `Op::Record` request node, then one `Op::EffectRequest { primitive }` node typed as the Response. VIR gains a `vir`-local `EffectId` (so `vir` keeps zero `runtime` dependency) and an `EffectKind::Effect`; the runtime converts `PrimitiveId → EffectId` and builds the manifest (`runtime → compiler`, the natural downstream direction).

**Tech Stack:** Rust (edition 2024), the phase-02 `vix::runtime::primitive` module, `vix::vir`, `vix::compiler`, `vix::diagnostic`.

## Global Constraints

- Branch: `vix-prim-03-compiler`, created with `git town append vix-prim-03-compiler` from `vix-prim-02-core`. All commits `git commit --no-verify` (owner instruction: the facet-dev hook is skipped; never add AI attribution).
- **Layering is load-bearing.** `vir` MUST NOT import anything from `crate::runtime` (`vir.rs` today imports only `crate::diagnostic`/`crate::support` — keep it that way; r[machine.ir.vix-level]). Therefore `Op::EffectRequest` carries a `vir`-local `EffectId([u8; 32])`, never `runtime::primitive::PrimitiveId`. Likewise `compiler.rs` MUST NOT import `crate::runtime`: it takes a `PrimitiveManifest` built from vir types only; the `runtime → compiler` conversion lives in `runtime/primitive`.
- **No per-primitive arms/fields/variants anywhere** (`machine.primitive.registered`): one `Op::EffectRequest`, one `EffectKind::Effect`, one manifest lookup — everything keyed by descriptor data.
- No `Result<_, String>`. All call diagnostics are typed `Diagnostic`s reusing EXISTING `DiagnosticCode` variants (`UnknownName`, `UnknownField`, `MissingField`, `DuplicateField`, `TypeMismatch`) — do NOT add a new `DiagnosticCode`.
- **Phase 03 is compile-only.** The compiler EMITS `Op::EffectRequest`; lowering and the scheduler do NOT handle it yet (phases 04/05). Their `match &node.op` sites already end in `_ =>` wildcards (`lowering.rs:3324`, `ratchet.rs`), and `runtime/scheduler.rs` matches *weavy* ops, not `vir::Op` — so adding the variant still compiles and the full suite stays green. Do NOT add lowering/scheduler logic in this phase. Tests compile source and inspect the `Module`/diagnostics; they never lower or run a primitive call.
- Canonical op tag for `Op::EffectRequest` is **85** (free tags: 66–79 and 85+; note the pre-existing `84` collision between `AwaitWire` and `IntToString` — do not touch it).
- Test runner: `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix` (the system toolchain is rustc 1.96.1; the `nix develop` shell pins 1.91 which is too old — do NOT use it). Clippy gate: `nix shell nixpkgs#clippy nixpkgs#cargo-nextest --command cargo clippy -p vix --all-targets -- -D warnings` (nixpkgs clippy is 1.96.1, matching).

## File Structure

- Modify `vix/src/vir.rs` — add `EffectId`, `EffectKind::Effect`, `Op::EffectRequest { primitive: EffectId }`, two new `canonical_node` arms. Tests inline.
- Modify `vix/src/compiler.rs` — add `PrimitiveManifest` + `PrimitiveSignature` (vir-only), `ModuleContext.primitives`, thread through `lower_module` + `Compiler`, generalize `lower_where_call`, add `lower_where_record_values`, extract `lower_range_where`.
- Modify `vix/src/runtime/primitive/descriptor.rs` — `PrimitiveId::effect_id()`.
- Modify `vix/src/runtime/primitive/register.rs` — `PrimitiveSet::compiler_manifest()`.
- Create `vix/tests/primitive_compiler.rs` — compiler integration tests (name resolution, where-call type errors, precedence, e2e manifest build).

---

### Task 1: VIR — `EffectId`, `EffectKind::Effect`, `Op::EffectRequest`, canonical form

**Files:**
- Modify: `vix/src/vir.rs` (add `EffectId` near `EffectKind` ~`vir.rs:736`; add `EffectKind::Effect` at `vir.rs:738-741`; add `Op::EffectRequest` in the `Op` enum ~`vir.rs:764-925`; add two arms in `canonical_node` at `vir.rs:2787-2790` and `vir.rs:2796-2955`)
- Test: `#[cfg(test)]` in `vir.rs`

**Interfaces:**
- Consumes: `facet::Facet`, `crate::support::Span`, existing `Node`/`EffectFacts`/`Op`/`canonical_node`.
- Produces (later tasks rely on these exact names):
  - `pub struct EffectId(pub [u8; 32])` — derives `facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash`.
  - `EffectKind::Effect` (new variant, appended after `Codata`).
  - `Op::EffectRequest { primitive: EffectId }` (new struct variant).

- [ ] **Step 1: Write failing tests** in `vir.rs` `#[cfg(test)]` (add to the existing test module, or create one; `canonical_node` is private but in-module):

```rust
#[test]
fn effect_request_canonical_form_tracks_the_primitive_id() {
    let make = |primitive: EffectId| Node {
        id: NodeId(0),
        span: crate::support::Span { start: 0, end: 0 },
        ty: Type::Int,
        effect: EffectFacts {
            kind: EffectKind::Effect,
            fallible: true,
            placed: false,
        },
        inputs: vec![NodeId(1)],
        op: Op::EffectRequest { primitive },
    };
    let ids = std::collections::BTreeMap::new();
    let a = canonical_node(&make(EffectId([1u8; 32])), &ids);
    let b = canonical_node(&make(EffectId([2u8; 32])), &ids);
    assert_ne!(a, b, "different primitive ids must canonicalize differently");
    assert_eq!(a, canonical_node(&make(EffectId([1u8; 32])), &ids), "stable");
}

#[test]
fn effect_kind_effect_canonicalizes_distinctly() {
    let base = Node {
        id: NodeId(0),
        span: crate::support::Span { start: 0, end: 0 },
        ty: Type::Int,
        effect: EffectFacts::PURE,
        inputs: Vec::new(),
        op: Op::Int(0),
    };
    let mut effectful = base.clone();
    effectful.effect = EffectFacts {
        kind: EffectKind::Effect,
        fallible: false,
        placed: false,
    };
    let ids = std::collections::BTreeMap::new();
    assert_ne!(canonical_node(&base, &ids), canonical_node(&effectful, &ids));
}
```

- [ ] **Step 2: Run to verify failure** — `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix effect_request` → compile error (`EffectId`, `Op::EffectRequest`, `EffectKind::Effect` missing). Expected.
- [ ] **Step 3: Implement.**

Add `EffectId` immediately above `EffectKind` (~`vir.rs:735`):

```rust
/// A registered primitive's identity as embedded in VIR: the 32 content bytes of
/// a `runtime::primitive::PrimitiveId`. VIR-local on purpose so `vir` keeps zero
/// dependency on `runtime` (r[machine.ir.vix-level]); the runtime converts at the
/// compile/schedule boundary via `PrimitiveId::effect_id`.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectId(pub [u8; 32]);
```

Extend `EffectKind` (`vir.rs:738-741`) — append the variant so existing ordinals are unchanged:

```rust
pub enum EffectKind {
    Pure,
    Codata,
    /// A registered effect primitive request (r[machine.primitive.registered]).
    Effect,
}
```

Add the `Op` variant inside the `Op` enum (place it next to `AwaitWire`, its closest semantic analog — park/resume-with-typed-result):

```rust
    /// Request a registered effect primitive. Consumes one input — the request
    /// record node — and is typed as the primitive's Response. The scheduler
    /// resolves it at the demand layer (phases 04/05); phase 03 only emits it.
    EffectRequest {
        primitive: EffectId,
    },
```

In `canonical_node`, extend the effect-kind match (`vir.rs:2787-2790`):

```rust
            match node.effect.kind {
                EffectKind::Pure => 0,
                EffectKind::Codata => 1,
                EffectKind::Effect => 2,
            },
```

And add a new op arm in the `match &node.op` block (anywhere before the closing brace at `vir.rs:2955`), using free tag `85`:

```rust
        Op::EffectRequest { primitive } => {
            op.push(85);
            op.extend_from_slice(&primitive.0);
        }
```

The compiler may report other non-exhaustive `match`es on `Op`/`EffectKind`; per research the only exhaustive-without-wildcard sites are these two in `canonical_node`. If the build flags another, add a mirroring arm there (do NOT introduce a wildcard).

- [ ] **Step 4: Run** `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix` (full crate — the new variant must not break any existing `Op` match). Expected: PASS.
- [ ] **Step 5: Commit** — `git add -A && git commit --no-verify -m "vix: VIR EffectId, EffectKind::Effect, Op::EffectRequest"`

---

### Task 2: Compiler manifest + threading (empty-manifest no-op)

**Files:**
- Modify: `vix/src/compiler.rs` — add `PrimitiveManifest`/`PrimitiveSignature` (top of file near other pub types, ~`compiler.rs:37`); add `primitives` field to `ModuleContext` (`compiler.rs:138-143`) and populate it at the construction site (`compiler.rs:804-814`); extend `fn lower_module` (`compiler.rs:766`) and `Compiler` (`compiler.rs:20-74`).
- Test: `#[cfg(test)]` in `compiler.rs` (or extend Task 3's integration file — but a minimal inline test keeps this task self-contained).

**Interfaces:**
- Consumes: `crate::vir::{EffectId, Type}`.
- Produces (later tasks rely on these exact names):
  - `pub struct PrimitiveManifest { entries: BTreeMap<String, PrimitiveSignature> }` — `Clone, Debug, Default`; `pub fn new() -> Self`, `pub fn insert(&mut self, name: impl Into<String>, signature: PrimitiveSignature)`, and `fn get(&self, name: &str) -> Option<&PrimitiveSignature>` (module-private).
  - `pub struct PrimitiveSignature { pub effect: crate::vir::EffectId, pub request: crate::vir::Type, pub response: crate::vir::Type }` — `Clone, Debug`.
  - `Compiler::with_primitives(self, primitives: PrimitiveManifest) -> Self` (builder).

- [ ] **Step 1: Write failing test** in `compiler.rs` `#[cfg(test)]`:

```rust
#[test]
fn range_still_compiles_with_a_primitive_manifest_present() {
    let source = "#[test]\nfn t() -> Stream<Check> {\n    let xs = range where { from: 0, to: 3 };\n    yield expect_eq(xs, range where { from: 0, to: 3 });\n}\n";
    let manifest = PrimitiveManifest::new();
    let compiler = Compiler::new().with_primitives(manifest);
    assert!(compiler.compile(source).is_ok(), "empty manifest is a no-op for builtins");
}
```

- [ ] **Step 2: Run to verify failure** — `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix range_still_compiles` → compile error (`PrimitiveManifest`/`with_primitives` missing). Expected. (If the exact `source` string does not type-check for reasons unrelated to primitives, simplify it against `vix/tests/ratchet/002-arithmetic.vix` — the assertion under test is only that an empty manifest changes nothing.)
- [ ] **Step 3: Implement.**

Add the manifest types near the top of `compiler.rs` (after `Compilation`, ~`compiler.rs:48`):

```rust
/// A compiler-facing projection of the registered primitives: exactly what
/// `lower_where_call` needs to type-check `name where { ... }`, and nothing about
/// handlers or the runtime. Built by the runtime from `PrimitiveDescriptor`s
/// (r[machine.primitive.registered]); vir-only, so `compiler` keeps zero
/// dependency on `runtime`.
#[derive(Clone, Debug, Default)]
pub struct PrimitiveManifest {
    entries: std::collections::BTreeMap<String, PrimitiveSignature>,
}

#[derive(Clone, Debug)]
pub struct PrimitiveSignature {
    pub effect: crate::vir::EffectId,
    pub request: Type,
    pub response: Type,
}

impl PrimitiveManifest {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, signature: PrimitiveSignature) {
        self.entries.insert(name.into(), signature);
    }

    fn get(&self, name: &str) -> Option<&PrimitiveSignature> {
        self.entries.get(name)
    }
}
```

Add the field to `ModuleContext` (`compiler.rs:138-143`):

```rust
struct ModuleContext<'a> {
    signatures: &'a BTreeMap<String, FunctionSignature>,
    types: &'a BTreeMap<String, Type>,
    primitives: &'a PrimitiveManifest,
    closures: RefCell<ClosureState>,
    config: CompilerConfig,
}
```

Thread it through `fn lower_module`. Change its signature to accept the manifest and set the field at the construction site (`compiler.rs:804`):

```rust
fn lower_module(
    ast: &ast::Module,
    config: CompilerConfig,
    primitives: &PrimitiveManifest,
) -> Result<Module, Diagnostics> {
    // ... existing body unchanged up to the ModuleContext literal ...
    let context = ModuleContext {
        signatures: &signatures,
        types: &types,
        primitives,
        closures: RefCell::new(ClosureState { /* unchanged */ }),
        config,
    };
    // ... rest unchanged ...
}
```

(Use the real `ast::Module` type name shown at the current `lower_module` definition; keep the rest of the body verbatim.)

Extend `Compiler` (`compiler.rs:20-23`) with an owned manifest, default-empty, and a builder. Update `Compiler::new`/`with_config` to initialize `primitives: PrimitiveManifest::default()`, and `Compiler::compile` (`compiler.rs:69-74`) to pass `&self.primitives`:

```rust
pub struct Compiler {
    parser: SurfaceParser,
    config: CompilerConfig,
    primitives: PrimitiveManifest,
}

impl Compiler {
    // in new()/with_config(): add `primitives: PrimitiveManifest::default(),`

    #[must_use]
    pub fn with_primitives(mut self, primitives: PrimitiveManifest) -> Self {
        self.primitives = primitives;
        self
    }

    pub fn compile(&self, source: &str) -> Result<Compilation, Diagnostics> {
        let ast = self.parser.parse(source)?;
        let module = lower_module(&ast, self.config, &self.primitives)?;
        let warnings = lint_module(&module);
        Ok(Compilation { module, warnings })
    }
}
```

Any other caller of `lower_module` (e.g. in `#[cfg(test)]` blocks or `ratchet.rs`) must pass `&PrimitiveManifest::default()`. Grep `lower_module(` and fix each call site.

- [ ] **Step 4: Run** `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix` → PASS (no behavior change; every existing caller uses an empty manifest).
- [ ] **Step 5: Commit** — `git commit --no-verify -am "vix: thread a PrimitiveManifest into the compiler"`

---

### Task 3: Generalize `lower_where_call` to registered primitives + field diagnostics

**Files:**
- Modify: `vix/src/compiler.rs` — rewrite `lower_where_call` (`compiler.rs:6877-6917`); extract the current `range` body into `lower_range_where`; add `lower_where_record_values` (model on `lower_named_record_values` at `compiler.rs:5785-5862`).
- Create: `vix/tests/primitive_compiler.rs`

**Interfaces:**
- Consumes: `context.primitives` (Task 2), `PrimitiveSignature` (Task 2), `Op::EffectRequest`/`EffectId`/`EffectKind::Effect` (Task 1), existing `push_node`, `lower_value_expected`, `require_type`, `lookup_binding`, `field_diagnostic`, `unknown_name`, `Diagnostic::unsupported`, `ast::WhereCall`/`ast::WhereArgs`/`ast::NamedValue`, `crate::vir::{Op, Type, RecordField, RecordType, EffectFacts}`.
- Produces: a where-call to a registered name lowers to `[Op::Record request] -> Op::EffectRequest { primitive }`, the result node typed as the Response `Type`.

- [ ] **Step 1: Write failing tests** in `vix/tests/primitive_compiler.rs`:

```rust
use vix::compiler::{Compiler, PrimitiveManifest, PrimitiveSignature};
use vix::diagnostic::{DiagnosticCode, Diagnostics};
use vix::vir::{EffectId, Op, RecordField, RecordType, Type};

/// Manifest with one primitive: `probe_version where { text: String, deep: Bool } -> Version { major: Int }`.
fn probe_manifest() -> PrimitiveManifest {
    let mut manifest = PrimitiveManifest::new();
    manifest.insert(
        "probe_version",
        PrimitiveSignature {
            effect: EffectId([7u8; 32]),
            request: Type::Record(RecordType {
                name: "ProbeRequest@0000000000000001".into(),
                fields: vec![
                    RecordField { name: "text".into(), ty: Type::String },
                    RecordField { name: "deep".into(), ty: Type::Bool },
                ],
            }),
            response: Type::Record(RecordType {
                name: "Version@0000000000000002".into(),
                fields: vec![RecordField { name: "major".into(), ty: Type::Int }],
            }),
        },
    );
    manifest
}

fn compile(source: &str) -> Result<vix::compiler::Compilation, Diagnostics> {
    Compiler::new().with_primitives(probe_manifest()).compile(source)
}

/// Wrap a where-call expression in a minimal compilable test function whose body
/// binds the call and yields a trivial check. Lowering the binding reaches the
/// where-call; on error, compile returns before the yield is checked.
fn program(call: &str) -> String {
    format!("#[test]\nfn t() -> Stream<Check> {{\n    let v = {call};\n    yield expect_eq(v.major, 1);\n}}\n")
}

fn sole_code(source: &str) -> DiagnosticCode {
    match compile(source) {
        Err(diagnostics) => {
            assert_eq!(diagnostics.entries.len(), 1, "expected one diagnostic, got {:?}", diagnostics.entries);
            diagnostics.entries[0].code
        }
        Ok(_) => panic!("expected a diagnostic, got a successful compile"),
    }
}

#[test]
fn registered_primitive_lowers_to_an_effect_request() {
    let source = program("probe_version where { text: \"1.2.3\", deep: true }");
    let compilation = compile(&source).expect("registered primitive compiles");
    let found = compilation
        .module
        .functions
        .iter()
        .flat_map(|function| function.nodes.iter())
        .any(|node| matches!(node.op, Op::EffectRequest { primitive } if primitive == EffectId([7u8; 32])));
    assert!(found, "the call lowers to an Op::EffectRequest for the registered primitive");
}

#[test]
fn wrong_field_type_is_a_type_mismatch() {
    // text expects String, given Int.
    let source = program("probe_version where { text: 5, deep: true }");
    assert_eq!(sole_code(&source), DiagnosticCode::TypeMismatch);
}

#[test]
fn missing_field_is_a_missing_field() {
    let source = program("probe_version where { text: \"x\" }");
    assert_eq!(sole_code(&source), DiagnosticCode::MissingField);
}

#[test]
fn extra_field_is_an_unknown_field() {
    let source = program("probe_version where { text: \"x\", deep: true, extra: 1 }");
    assert_eq!(sole_code(&source), DiagnosticCode::UnknownField);
}

#[test]
fn duplicate_field_is_a_duplicate_field() {
    let source = program("probe_version where { text: \"x\", text: \"y\", deep: true }");
    assert_eq!(sole_code(&source), DiagnosticCode::DuplicateField);
}

#[test]
fn unregistered_name_is_an_unknown_name() {
    let source = program("not_a_primitive where { text: \"x\" }");
    assert_eq!(sole_code(&source), DiagnosticCode::UnknownName);
}
```

- [ ] **Step 2: Run to verify failure** — `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix --test primitive_compiler` → the registered-primitive tests FAIL (`lower_where_call` still rejects every non-`range` callee with `UnknownName`, so the happy-path and field-diagnostic tests fail). Expected.

  Note: if `program(...)`'s scaffolding (`-> Stream<Check>`, `expect_eq`, `.major` projection) does not type-check independently of the primitive, adjust it against `vix/tests/ratchet/001-harness.vix` and `vix/tests/solver_value_lane.rs` — the invariant to preserve is that the where-call is the only thing under test.

- [ ] **Step 3: Implement.**

Extract the current `range` body into a helper (verbatim move of `compiler.rs:6886-6916`):

```rust
fn lower_range_where(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::WhereCall,
) -> Result<LoweredValue, Diagnostics> {
    let from = named_field_value(&call.named_args, "from")?;
    let to = named_field_value(&call.named_args, "to")?;
    if call.named_args.fields.len() != 2 {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.named_args.span,
            "range accepts exactly the named bounds `from` and `to`",
        )));
    }
    let from = lower_value_expected(nodes, bindings, context, from, Some(&Type::Int))?;
    require_type(&from, &Type::Int, expr_span_of_named(&call.named_args, "from"))?;
    let to = lower_value_expected(nodes, bindings, context, to, Some(&Type::Int))?;
    require_type(&to, &Type::Int, expr_span_of_named(&call.named_args, "to"))?;
    let ty = Type::array(Type::Int);
    Ok(LoweredValue {
        node: push_node(
            nodes,
            call.span,
            ty.clone(),
            EffectFacts { fallible: true, ..EffectFacts::PURE },
            vec![from.node, to.node],
            Op::Range,
        ),
        ty,
    })
}
```

Add the WhereArgs field checker (parallel to `lower_named_record_values`, no spread, punning via `lookup_binding`):

```rust
/// Type-check `where { ... }` named fields against a declared record's fields,
/// returning the field value nodes in declared order. Mirrors
/// `lower_named_record_values` for the `ast::WhereArgs` shape.
fn lower_where_record_values(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    owner: &str,
    declared_fields: &[RecordField],
    supplied: &ast::WhereArgs,
) -> Result<Vec<NodeId>, Diagnostics> {
    let mut provided = BTreeMap::new();
    for field in &supplied.fields {
        if provided.insert(field.name.value.clone(), field).is_some() {
            return Err(field_diagnostic(
                DiagnosticCode::DuplicateField,
                field.name.span,
                owner,
                &field.name.value,
            ));
        }
    }
    let mut inputs = Vec::with_capacity(declared_fields.len());
    for declared in declared_fields {
        let field = provided.remove(&declared.name).ok_or_else(|| {
            field_diagnostic(DiagnosticCode::MissingField, supplied.span, owner, &declared.name)
        })?;
        let value = if let Some(expression) = &field.value {
            lower_value_expected(nodes, bindings, context, expression, Some(&declared.ty))?
        } else {
            lookup_binding(bindings, &field.name.value, field.name.span)?
        };
        require_type(&value, &declared.ty, field.span)?;
        inputs.push(value.node);
    }
    if let Some((name, field)) = provided.into_iter().next() {
        return Err(field_diagnostic(
            DiagnosticCode::UnknownField,
            field.name.span,
            owner,
            &name,
        ));
    }
    Ok(inputs)
}
```

Rewrite `lower_where_call` to dispatch `range` → the extracted helper, else a manifest lookup:

```rust
fn lower_where_call(
    nodes: &mut Vec<Node>,
    bindings: &BTreeMap<String, LoweredValue>,
    context: &ModuleContext<'_>,
    call: &ast::WhereCall,
) -> Result<LoweredValue, Diagnostics> {
    if call.callee.value == "range" {
        return lower_range_where(nodes, bindings, context, call);
    }
    let signature = context
        .primitives
        .get(&call.callee.value)
        .ok_or_else(|| unknown_name(call.callee.span, &call.callee.value))?
        .clone();
    let Type::Record(request_record) = &signature.request else {
        return Err(Diagnostics::one(Diagnostic::unsupported(
            call.callee.span,
            "primitive request type is not a record",
        )));
    };
    let inputs = lower_where_record_values(
        nodes,
        bindings,
        context,
        &request_record.name,
        &request_record.fields,
        &call.named_args,
    )?;
    let request_node = push_node(
        nodes,
        call.span,
        signature.request.clone(),
        EffectFacts::PURE,
        inputs,
        Op::Record,
    );
    let response_ty = signature.response.clone();
    let node = push_node(
        nodes,
        call.span,
        response_ty.clone(),
        EffectFacts { kind: EffectKind::Effect, fallible: true, placed: false },
        vec![request_node],
        Op::EffectRequest { primitive: signature.effect },
    );
    Ok(LoweredValue { node, ty: response_ty })
}
```

(`.clone()` the signature to avoid holding a borrow of `context.primitives` across the `&mut nodes` calls.)

- [ ] **Step 4: Run** `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix --test primitive_compiler` → PASS, then the full `... run -p vix` → PASS (range and all existing behavior unchanged).
- [ ] **Step 5: Commit** — `git add -A && git commit --no-verify -m "vix: registered primitives lower through generalized where-call"`

---

### Task 4: Runtime→compiler manifest bridge + effect id + precedence

**Files:**
- Modify: `vix/src/runtime/primitive/descriptor.rs` — `impl PrimitiveId { pub fn effect_id(&self) -> crate::vir::EffectId }`
- Modify: `vix/src/runtime/primitive/register.rs` — `impl PrimitiveSet { pub fn compiler_manifest(&self) -> crate::compiler::PrimitiveManifest }`
- Modify: `vix/tests/primitive_compiler.rs` — add the realistic end-to-end + precedence tests.

**Interfaces:**
- Consumes: `crate::vir::EffectId` (Task 1), `crate::compiler::{PrimitiveManifest, PrimitiveSignature}` (Task 2), `PrimitiveSet::descriptors()` (phase 02), `PrimitiveName`/`RESERVED_NAMES` (phase 02).
- Produces: `PrimitiveId::effect_id`, `PrimitiveSet::compiler_manifest` — the real path from a registered `PrimitiveSet` to a compiler manifest.

- [ ] **Step 1: Write failing tests** — append to `vix/tests/primitive_compiler.rs`:

```rust
use vix::runtime::primitive::{MemoPolicy, PrimitiveName, PrimitiveSet, RegistrationError};

#[derive(facet::Facet)]
struct AddRequest {
    left: i64,
    right: i64,
}
#[derive(facet::Facet)]
struct AddResponse {
    sum: i64,
}

#[test]
fn a_registered_primitive_set_compiles_a_real_call() {
    let mut set = PrimitiveSet::new();
    set.register_function::<AddResponse, AddRequest, _>(
        "add_numbers",
        MemoPolicy::Hermetic,
        |req: AddRequest| Ok(AddResponse { sum: req.left + req.right }),
    )
    .unwrap();
    let manifest = set.compiler_manifest();
    let source = "#[test]\nfn t() -> Stream<Check> {\n    let r = add_numbers where { left: 40, right: 2 };\n    yield expect_eq(r.sum, 42);\n}\n";
    let compilation = Compiler::new()
        .with_primitives(manifest)
        .compile(source)
        .expect("registered primitive call compiles");
    assert!(
        compilation
            .module
            .functions
            .iter()
            .flat_map(|function| function.nodes.iter())
            .any(|node| matches!(node.op, Op::EffectRequest { .. })),
        "the call lowers to an effect request",
    );
}

#[test]
fn a_primitive_cannot_take_a_reserved_builtin_name() {
    // Precedence guard: a registered name can never shadow `range` (or any builtin).
    assert!(matches!(
        PrimitiveName::new("range"),
        Err(RegistrationError::ReservedName { .. })
    ));
}
```

- [ ] **Step 2: Run to verify failure** — `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix --test primitive_compiler` → the e2e test fails (`compiler_manifest` missing). Expected.
- [ ] **Step 3: Implement.**

In `descriptor.rs`, add to `impl PrimitiveId`:

```rust
impl PrimitiveId {
    /// The VIR-embeddable form of this id: its 32 content bytes. `vir` cannot
    /// name `PrimitiveId` (layering), so the compiler carries this instead and
    /// the scheduler converts back in phase 05.
    #[must_use]
    pub fn effect_id(&self) -> crate::vir::EffectId {
        crate::vir::EffectId(self.0.0)
    }
}
```

(`self.0` is the `Digest`; `self.0.0` is its `[u8; 32]`.)

In `register.rs`, add to `impl PrimitiveSet`:

```rust
    /// Project the registered descriptors into a compiler manifest — vir types
    /// and effect ids only, no handlers (r[machine.primitive.registered]). This
    /// is the `runtime -> compiler` boundary; `compiler` never imports `runtime`.
    #[must_use]
    pub fn compiler_manifest(&self) -> crate::compiler::PrimitiveManifest {
        let mut manifest = crate::compiler::PrimitiveManifest::new();
        for descriptor in self.descriptors() {
            manifest.insert(
                descriptor.name.as_str(),
                crate::compiler::PrimitiveSignature {
                    effect: descriptor.id.effect_id(),
                    request: descriptor.request.vix_type.clone(),
                    response: descriptor.response.vix_type.clone(),
                },
            );
        }
        manifest
    }
```

If `MemoPolicy`/`PrimitiveName`/`RegistrationError` are not already re-exported from `vix::runtime::primitive`, confirm `runtime/primitive/mod.rs` `pub use`s them (phase 02 re-exports `descriptor::*` and `register::*`, so they are). `facet` must be a dev-usable path in the integration test — add `use facet::Facet;` if the derive needs it, matching how other `vix/tests/*.rs` derive Facet.

- [ ] **Step 4: Run** `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix --test primitive_compiler` → PASS, then full `... run -p vix` → PASS.
- [ ] **Step 5: Commit** — `git add -A && git commit --no-verify -m "vix: build the compiler manifest from a registered PrimitiveSet"`

---

### Task 5: Phase gate

- [ ] Full suite: `nix shell nixpkgs#cargo-nextest --command cargo nextest run -p vix` → all green (nothing outside the new surface may regress; `range` and every existing lane unchanged).
- [ ] Clippy: `nix shell nixpkgs#clippy nixpkgs#cargo-nextest --command cargo clippy -p vix --all-targets -- -D warnings` → clean.
- [ ] Re-read the diff against the Global Constraints: `vir` still imports no `runtime`; `compiler` still imports no `runtime`; exactly one `Op::EffectRequest` / one `EffectKind::Effect` / one manifest lookup (no per-primitive arms); all call diagnostics reuse existing `DiagnosticCode` variants; no lowering/scheduler handling of `Op::EffectRequest` was added; `r[machine.primitive.*]` / `r[machine.ir.vix-level]` comments present on the new items.
- [ ] Update this plan's checkboxes to `[x]`, append a short landing-notes section (deviations, as phase 02 did), commit, then stop — phase 04 (lowering: partition effect edges, `Island::effect_inputs`, `LoweringArtifact` effect bindings) plans against this landed state.

## Self-review notes (already applied)

- **Spec coverage** (design §Component 5, testing §Compiler 03): manifest into `ModuleContext` (Task 2); where-call generalization (Task 3); `Op::EffectRequest` + `EffectKind::Effect` + `canonical_node` (Task 1); diagnostics for wrong/missing/extra field + type mismatch + unknown name (Task 3); precedence vs builtins / reserved names (Task 4). The spec's "seed one synthetic `FunctionSignature` per primitive" is intentionally NOT done: primitives are called through `ast::WhereCall`, which dispatches to `lower_where_call` and never touches the `FunctionSignature`/`lower_call` path — a synthetic signature would be dead machinery. Threading the manifest into the where-call path achieves the same name-resolution/type-checking with no unused state.
- **Layering deviation from the spec's prose:** the spec shows `Op::EffectRequest { primitive: PrimitiveId }`. Because `vir` must not depend on `runtime`, the variant carries a `vir`-local `EffectId([u8; 32])`; `PrimitiveId::effect_id()` converts. Same 32 content bytes, so identity is preserved; the scheduler reverses it in phase 05.
- **Diagnostics:** no new `DiagnosticCode` — `UnknownField` covers "extra field" (an extra field is definitionally unknown), exactly as `lower_named_record_values` already treats leftover supplied fields.
- **Out of scope (phases 04–06):** VIR partitioning / `Island::effect_inputs` / lowering bindings (04); scheduler effect resolution, memo policy, receipts, events, failure generalization (05); first real primitive + corpus e2e + perf gate + docs (06). Phase 03 emits `Op::EffectRequest` and stops; lowering/scheduler reach it through existing wildcard arms.
- **Type consistency:** `PrimitiveSignature { effect: EffectId, request: Type, response: Type }` is produced by `compiler_manifest` (Task 4) and consumed by `lower_where_call` (Task 3); `Op::EffectRequest { primitive: EffectId }` matches Task 1's definition and Task 3's construction; `effect_id()` returns the same `EffectId` the manifest carries.
- **Known unknowns for the executor:** the exact minimal vix source that type-checks around a where-call (`program(...)` scaffolding) — validate against `tests/ratchet/001-harness.vix` and `tests/solver_value_lane.rs`; the precise set of other caller sites of `lower_module` needing an empty-manifest argument — grep before editing.
