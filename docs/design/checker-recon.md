# Vix Checker Recon

Recon and design-question pass for the next arc: the checker is the query
engine. No implementation is proposed here beyond the shape of the query plane.

The ruled direction is one Rust program-analysis implementation serving both
diagnostics and `vix-lsp` behind `LanguageQueries`; later, a vix-written checker
is checked against this Rust checker and eventually checks itself.

## Existing Type Knowledge

### Grammar Surface

The checked surface is already wider than the machine subset.

- The grammar says all AST-relevant children are derived from `field()` and
  cardinality, so checker facts should key off the generated AST rather than a
  parallel front end (`playgrounds/snark/src/bundled/vix/grammar.js:36`).
- Items include `use`, `fn`, `struct`, and `enum`
  (`playgrounds/snark/src/bundled/vix/grammar.js:42`).
- Function items have visibility, a name, optional generic params, typed params,
  an optional return type, and a block body
  (`playgrounds/snark/src/bundled/vix/grammar.js:52`).
- Structs are record, tuple, or unit, with optional generic params and record
  field defaults (`playgrounds/snark/src/bundled/vix/grammar.js:63`,
  `playgrounds/snark/src/bundled/vix/grammar.js:66`).
- Enums are named, optionally generic, and contain unit, tuple, or record
  variants; declaration order is semantic
  (`playgrounds/snark/src/bundled/vix/grammar.js:75`,
  `playgrounds/snark/src/bundled/vix/grammar.js:86`).
- Type params have no lifetimes and no explicit hash/eq/ord bounds because every
  vix value has those properties by construction
  (`playgrounds/snark/src/bundled/vix/grammar.js:104`).
- Types are arrays, function types, tuple types, generic applications, or type
  paths (`playgrounds/snark/src/bundled/vix/grammar.js:111`).
- `let` supports optional annotations
  (`playgrounds/snark/src/bundled/vix/grammar.js:136`).
- Expression grammar includes calls, method calls, field and tuple-index access,
  matches with guards, closures, command blocks, struct literals, maps, tuples,
  arrays, scoped identifiers, literals, and booleans
  (`playgrounds/snark/src/bundled/vix/grammar.js:148`).
- Calls can mix positional args, kwargs, and a trailing partial marker `..`
  (`playgrounds/snark/src/bundled/vix/grammar.js:237`).
- Pattern grammar includes wildcard, variant, struct, tuple, scoped identifier,
  bare identifier, string, and number patterns
  (`playgrounds/snark/src/bundled/vix/grammar.js:266`).
- Flags are only array elements, path literals are distinct from strings, and
  numbers are either integer-looking or float-looking tokens
  (`playgrounds/snark/src/bundled/vix/grammar.js:300`,
  `playgrounds/snark/src/bundled/vix/grammar.js:350`,
  `playgrounds/snark/src/bundled/vix/grammar.js:358`).

### Binder

The binder is already the analysis front half for the editor and engine. It is
generated-AST-facing on purpose, so grammar drift should break Rust compilation
rather than create a second front end
(`vix/src/binder.rs:1`).

What it knows:

- File scope has order-independent functions and imports; function scope has
  params; block scope has sequential lets; closure scope has closure params;
  shadowing is allowed (`vix/src/binder.rs:8`).
- Command names are value references, so `cc!` renames with the binding
  (`vix/src/binder.rs:15`).
- Today, unresolved references are not diagnostics. They are future type/prelude
  facts: primitives, constructor-like patterns, and unimported types
  (`vix/src/binder.rs:18`, `vix/src/binder.rs:65`).
- Symbols distinguish functions, params, lets, closure params, imports, type
  declarations, type params, and pattern bindings
  (`vix/src/binder.rs:32`).
- Built-in scalar-ish types resolve silently: `Int`, `Float`, `String`, `Bool`,
  `Blob`, `Doc`, `Tree` (`vix/src/binder.rs:47`).
- Pass 1 defines imports, functions, structs, and enums in file scope
  (`vix/src/binder.rs:135`).
- Pass 2 defines function generic params, param names, param types, return types,
  and bodies; struct/enum bodies bind generic params and type uses
  (`vix/src/binder.rs:161`, `vix/src/binder.rs:236`,
  `vix/src/binder.rs:248`).
- Type binding resolves generic bases, generic args, tuple/fn members, and the
  head of type paths; qualified type paths still wait for module/type-directed
  work (`vix/src/binder.rs:288`, `vix/src/binder.rs:312`).
- Call kwargs names are not value references, and struct literal field names are
  not value references (`vix/src/binder.rs:431`, `vix/src/binder.rs:373`).
- Match pattern bindings scope over guards and arm values
  (`vix/src/binder.rs:360`).
- A top-level bare identifier pattern is intentionally unresolved and
  constructor-like; payload-position identifiers bind. This prevents a typoed
  variant from silently becoming a catch-all
  (`vix/src/binder.rs:443`).
- Known editor limitation: shorthand field pattern rename would need to expand
  `{ name }` into `{ name: new_name }`
  (`vix/src/binder.rs:449`).

The binder tests pin the same behavior: unresolved is a value list, not an
error list (`vix/tests/binder.rs:87`), and let shadowing is sequential
(`vix/tests/binder.rs:108`).

### LSP Query Seam

`vix-lsp` is already shaped to swap query engines without a protocol rewrite:

- The module comment says Rust parser/binder queries are current, and
  vix/fable-hosted query engines can later swap in behind `LanguageQueries`
  (`vix-lsp/src/lib.rs:4`).
- `LanguageQueries` currently exposes `analyze(source) -> Analysis` and
  `highlights(source) -> Vec<Highlight>` (`vix-lsp/src/lib.rs:144`).
- `RustLanguageQueries::analyze` parses and then binds; no checker facts are
  available yet (`vix-lsp/src/lib.rs:172`).
- Diagnostics currently publish parse/analyze failure only, and successful
  analysis yields no diagnostics (`vix-lsp/src/lib.rs:549`).

### Type Tour

`types.vix` is the user-facing type promise.

- Every declared type is hashable, equatable, totally ordered, facet-shaped, and
  serializable; the language cannot express a non-memo-key type
  (`playgrounds/snark/src/bundled/vix/samples/types.vix:1`).
- Imports bring in `Tree`, `Path`, `Target`, `Map`, `Cc`, and `Ar`
  (`playgrounds/snark/src/bundled/vix/samples/types.vix:7`).
- Record structs support field defaults, tuple structs, unit structs, and enums
  with declaration-order semantics
  (`playgrounds/snark/src/bundled/vix/samples/types.vix:10`,
  `playgrounds/snark/src/bundled/vix/samples/types.vix:18`,
  `playgrounds/snark/src/bundled/vix/samples/types.vix:22`).
- Variants can be tuple, record, or unit
  (`playgrounds/snark/src/bundled/vix/samples/types.vix:24`).
- Generic structs exist at the surface
  (`playgrounds/snark/src/bundled/vix/samples/types.vix:31`).
- Matches support guarded tuple variants, wildcard tuple payloads, record
  shorthand, rest patterns, and unit variants
  (`playgrounds/snark/src/bundled/vix/samples/types.vix:37`).
- Generic function bodies and function-typed params are in the tour via `swap`
  and `apply` (`playgrounds/snark/src/bundled/vix/samples/types.vix:46`,
  `playgrounds/snark/src/bundled/vix/samples/types.vix:50`).
- Partial application and tuple indexing are in the tour
  (`playgrounds/snark/src/bundled/vix/samples/types.vix:58`,
  `playgrounds/snark/src/bundled/vix/samples/types.vix:65`).
- Struct construction, record update, enum match over `target.os`, maps, tuple
  construction, and tuple-indexed spread bases are in the tour
  (`playgrounds/snark/src/bundled/vix/samples/types.vix:72`).

The type tour test pins parse/bind shape rather than full checking: it asserts
zero unresolved names (`vix/tests/types.rs:17`), declaration counts and shapes
(`vix/tests/types.rs:30`), generic params as `TypeParam`
(`vix/tests/types.rs:69`), match pattern binding and shorthand behavior
(`vix/tests/types.rs:86`), `fn` type syntax (`vix/tests/types.rs:111`),
struct literal/update/map/tuple indexing (`vix/tests/types.rs:115`), partial
call syntax (`vix/tests/types.rs:156`), and recursive self-hosting type shape in
`eval.vix` (`vix/tests/types.rs:184`).

### Module Tables And Schema Names

`vix/src/module.rs` is the type declaration table and closure-hash input layer:

- `EnumInfo` records variant names plus unit/tuple/record shapes, and
  `StructInfo` records field names, defaults, and unit-ness
  (`vix/src/module.rs:11`, `vix/src/module.rs:23`).
- `load_module_tables` parses once, collects functions, enums, structs, type
  declaration hashes, spans, descriptors, and closure hashes
  (`vix/src/module.rs:38`).
- Schema names are stringified from `Type`: paths, generic applications,
  arrays, tuples, and `Fn`; qualified type paths are rejected for the current
  machine subset (`vix/src/module.rs:112`, `vix/src/module.rs:137`).
- Declared descriptors are built for user structs/enums over `weavy::mem`
  (`vix/src/module.rs:147`).
- Descriptor fields are inline for `Int`, `Float`, and `Bool`; `String` and
  other non-scalars become 8-byte handles to a target schema
  (`vix/src/module.rs:209`).
- Closure hashes include referenced functions and referenced types, with type
  references separated from value references using binder symbol kinds
  (`vix/src/module.rs:230`).

### Machine Lowering As De-Facto Checker

The machine lowering currently enforces many static rules as load-time lowering
errors. It should become a consumer of checker facts rather than the first place
these facts are discovered.

Schema and ABI facts:

- `Machine` stores entry param schemas, param names, return schemas, render
  names, source, and module hash (`vix/src/machine/lower.rs:45`).
- `MachineArg` already splits raw words, handles, scalar values, strings,
  paths, flags, trees, and target values (`vix/src/machine/lower.rs:61`).
- Load/reload compute fn return schemas and parameter schemas from declared
  annotations (`vix/src/machine/lower.rs:98`, `vix/src/machine/lower.rs:111`,
  `vix/src/machine/lower.rs:284`, `vix/src/machine/lower.rs:297`).
- Schema refs and literal handles are deterministic and pre-interned
  (`vix/src/machine/lower.rs:139`, `vix/src/machine/lower.rs:153`,
  `vix/src/machine/lower.rs:216`).
- `intern_arg` enforces schema-directed external calls: raw words can cross
  unchecked, scalar handles are accepted as words, non-scalar handles must carry
  the expected store schema, and typed scalars/strings/paths/flags/trees/targets
  are interned by expected schema (`vix/src/machine/lower.rs:446`).
- Public metadata exposes entry param and return schemas
  (`vix/src/machine/lower.rs:594`).

Pre-registration inference:

- `schema_names_for` starts from descriptors and built-ins, adds params/returns,
  descriptor closures, block annotations, and expression-derived schemas
  (`vix/src/machine/lower.rs:657`).
- Expression schema collection is local and opportunistic: literals, tuples,
  function call returns, variant constructors, selected method results, tuple
  indexes, match arm agreement, struct literals, map first key/value schema, and
  unit structs/variants (`vix/src/machine/lower.rs:1351`).

Lowering-time checks:

- Function params become typed slots, return type defaults to `Int`, and the
  tail is coerced to the return schema (`vix/src/machine/lower.rs:1854`,
  `vix/src/machine/lower.rs:1886`).
- Let bindings are lazy cells with optional expected schemas; expression
  statements and missing tails are rejected
  (`vix/src/machine/lower.rs:1925`).
- Literals choose schemas contextually for `Float` and structurally for string,
  path, bool, tuple, map, array, and command values
  (`vix/src/machine/lower.rs:1962`).
- Binary operators enforce schema-specific operation sets, with exact-schema
  equality for `==` and `!=` (`vix/src/machine/lower.rs:2050`,
  `vix/src/machine/lower.rs:2115`, `vix/src/machine/lower.rs:4438`).
- Match lowering enforces arm pattern shape, variant field counts, guard `Bool`,
  arm result schema agreement, wildcard/binding last, and scalar/string
  irrefutable tail until the checker owns exhaustiveness
  (`vix/src/machine/lower.rs:2210`, `vix/src/machine/lower.rs:2364`,
  `vix/src/machine/lower.rs:2397`).
- Calls enforce known functions, builtins, named/positional arity, duplicate
  kwargs, partial-call prefix discipline, and pending invocation shape
  (`vix/src/machine/lower.rs:2449`, `vix/src/machine/lower.rs:2583`).
- Methods enforce receiver schemas and method-specific arities for `Path`,
  `Tree`, arrays, maps, options, and docs (`vix/src/machine/lower.rs:2690`).
- Struct literals enforce known structs, duplicate fields, one record update
  spread, required fields/defaults/base update, and enum-record variant fields
  (`vix/src/machine/lower.rs:2926`).
- Variant constructor calls enforce tuple-variant shape and positional-only args
  (`vix/src/machine/lower.rs:3005`).
- Store allocation requires a schema ref and records values as frame words
  (`vix/src/machine/lower.rs:3311`).
- Fetch, document parsers, ELF, tree projection, and command blocks all enforce
  small static argument/receiver subsets (`vix/src/machine/lower.rs:4086`,
  `vix/src/machine/lower.rs:4120`, `vix/src/machine/lower.rs:4213`,
  `vix/src/machine/lower.rs:4262`, `vix/src/machine/lower.rs:4308`).

The machine constitution says this static knowledge is not optional: typed
instructions are chosen at lowering time, runtime never asks what a value is,
and lowered-program validation is the safety net if one is wanted
(`vix/src/machine/mod.rs:12`). The same file says vix/fable-authored layouts
come from the language checker emitting `weavy::mem::Descriptor`s
(`vix/src/machine/mod.rs:21`). `vix/src/machine/value.rs` repeats that
`weavy::mem` is the single layout vocabulary (`vix/src/machine/value.rs:1`).

The current bits-vs-handles rule is spread across layers:

- `LoweredFn` says scalar arg words hash by word bytes while handles hash the
  canonical content of the target store entry (`vix/src/machine/driver.rs:160`).
- `write_alloc_fields` asserts slice-2 store fields are word-sized
  (`vix/src/machine/driver.rs:4990`).
- Pending invocation identity hashes closure hash, remaining arity, each arg
  schema, and each canonical word/hash (`vix/src/machine/driver.rs:4286`).
- The fleet design already says `StoreValue` should be schema plus opaque bytes
  and content hash, and `Pending<T>` crosses as opaque store value
  (`docs/design/fleet-on-the-machine.md:86`).

## Parked Checker-Era Requirements Floor

These are not optional nice-to-haves; they are the current floor implied by
`PARITY.md` plus code comments:

1. Checker-backed exhaustiveness.
   Lowering requires final irrefutable scalar/string arms only until
   exhaustiveness checking arrives (`vix/src/machine/lower.rs:2397`). The match
   lowering comment explicitly says the checker owns exhaustiveness
   (`vix/src/machine/lower.rs:2210`). The checker must handle enum variant
   coverage, wildcard/binding irrefutability, guards, and the existing typoed
   bare-identifier rule from the binder.

2. Inline-vs-handle ABI discipline.
   The machine requires typed untagged operands and static layout knowledge
   (`vix/src/machine/mod.rs:12`), `weavy::mem` descriptor authority
   (`vix/src/machine/mod.rs:21`), scalar-inline vs non-scalar-handle
   descriptors (`vix/src/module.rs:209`), word-sized stored fields
   (`vix/src/machine/driver.rs:4990`), and pending/store value identity across
   memo and fleet boundaries (`vix/src/machine/driver.rs:4286`,
   `docs/design/fleet-on-the-machine.md:86`). The checker must make this a
   source-level contract: bits-strict values may cross memo boundaries as frame
   words; identity-lazy values cross as store handles or pending store values.

3. Generic function bodies.
   `Pair<A, B>`, `swap<A, B>`, and `apply(fn(Int)->Int, Int)` are in the tour
   (`playgrounds/snark/src/bundled/vix/samples/types.vix:31`,
   `playgrounds/snark/src/bundled/vix/samples/types.vix:46`,
   `playgrounds/snark/src/bundled/vix/samples/types.vix:50`). The parser and
   binder pin generic params and fn types (`vix/tests/types.rs:69`,
   `vix/tests/types.rs:111`). Lowering currently parks any generic fn or
   fn-typed signature as a zero-returning inert stub
   (`vix/src/machine/lower.rs:1663`). The checker must define what these bodies
   mean before lowering can stop stubbing them.

4. Named-argument diagnostics and arity.
   The parity ledger pins duplicate named arguments as evaluator behavior
   (`vix/src/machine/PARITY.md:84`) and missing named args for `scaled(k: 2)`
   (`vix/src/machine/PARITY.md:99`). The exact current test is
   `types_vix_named_argument_diagnostics_are_pinned`
   (`vix/src/machine/lower.rs:5447`). Lowering already detects unknown args,
   duplicate args, missing args, duplicate partial markers, and non-prefix
   partials (`vix/src/machine/lower.rs:2583`).

5. Duplicate detection beyond named args.
   Struct literals reject duplicate fields (`vix/src/machine/lower.rs:2940`).
   The binder intentionally allows lexical shadowing (`vix/src/binder.rs:8`),
   but nothing today states whether duplicate item names, type params, record
   fields, variant names, or duplicate pattern fields are allowed. The checker
   should own the diagnostic policy for declaration-space duplicates while
   preserving allowed value-shadowing unless Amos rules otherwise.

6. Parser/editor contracts survive the evaluator funeral.
   The parity ledger marks highlight and typed-AST parser contracts as class 4,
   not evaluator behavior, but says they must move to surviving parser/editor
   tests before doomed files die (`vix/src/machine/PARITY.md:27`,
   `vix/src/machine/PARITY.md:104`, `vix/src/machine/PARITY.md:106`). The
   checker query engine should expose enough typed spans for diagnostics,
   hover, references, and semantic tokens without stealing responsibilities
   from the syntax highlighter.

## V1 Checker Scope Proposal

V1 should formalize what the grammar, binder, type tour, and machine schemas
already imply. It should not invent traits, subtyping, implicit conversions, new
effect systems, or new type features.

Checks in scope:

- Parse-to-analysis plumbing: one `Analysis` object that includes syntax,
  bindings, checker facts, and diagnostics.
- Name resolution finalization: unresolved binder entries become typed outcomes:
  known prelude primitive, known import/type, known enum variant under a
  scrutinee type, or diagnostic.
- Declaration tables: functions, structs, enums, variants, fields, generic
  params, and prelude/builtin symbols.
- Duplicate diagnostics for declaration spaces that are not intentionally
  shadowable.
- Type schema normalization for all grammar-level types currently present:
  path, generic application, array, tuple, fn type.
- Local expression typing: literals, identifiers, calls, method calls, binary
  ops, struct literals/updates, enum constructors, tuples, arrays, maps, field
  access, command blocks, fetch/docs/elf primitives, closures only in existing
  supported method contexts unless ruled broader.
- Annotation checking for params, returns, and `let` annotations.
- Call checking: positional/kwarg mixing, unknown names, duplicate names,
  missing args, too many args, partial marker validity, partial prefix
  discipline, pending invocation result type.
- Match checking: scrutinee typing, pattern compatibility, payload bindings,
  guard `Bool`, arm result type agreement, and enum exhaustiveness.
- ABI classification: for each type, compute whether it is frame-word scalar,
  frame-word handle, `Pending<T>`, `Realized<T>`, map/array store value, or
  other store-backed identity.
- Memo-boundary discipline: every function call, partial, pending value, map
  insertion, command observer/fleet crossing, and public `Machine::call`
  crossing must use the checker-computed ABI class.
- LSP diagnostics: publish checker diagnostics, not just parse failures.
- LSP query data: definition/references/hover should use the same `Analysis`
  facts as machine lowering.

Deferred from V1 unless ruled in:

- General higher-rank or first-class function values beyond the existing
  `fn(...) -> ...` surface and the current partial/pending call machinery.
- Full generic lowering strategy. V1 can check generic declarations and reject
  unsupported instantiations loudly while Amos rules monomorphization vs erasure.
- Cross-file/modules beyond the current single-source-file analysis and imports
  as named leaves.
- Type-directed command grammars beyond the current `cc`/`ar`/`rustc` command
  block shape.
- Arbitrary structural subtyping, record width subtyping, or implicit string to
  path conversion. The grammar explicitly says strings never coerce to paths
  (`playgrounds/snark/src/bundled/vix/grammar.js:5`).
- Runtime-style recovery semantics. The machine contract says runtime does not
  ask what values are; recovery belongs in diagnostics, not generated code
  (`vix/src/machine/mod.rs:12`).

## Query Decomposition

These query names are the proposed checker/query-plane vocabulary. They are
meant to be narrow enough to become projection-narrowed memo units later.

### Source And Syntax

- `source_text(file_id) -> Arc<str>`
  Input: file id. Output: immutable source text.
- `parse(file_id) -> Result<SourceFile, ParseDiagnostic>`
  Input: source text. Output: generated AST or parse diagnostic.
- `line_index(file_id) -> LineIndex`
  Input: source text. Output: UTF-16-aware line index for LSP.
- `syntax_highlights(file_id) -> Vec<Highlight>`
  Input: source text. Output: query captures; this remains syntax-based.

### Declarations And Binding

- `decls(file_id) -> DeclTables`
  Input: AST. Output: functions, structs, enums, variants, fields, generic
  params, imports, prelude entries, spans.
- `bind(file_id) -> Bindings`
  Input: AST and decl prelude. Output: existing binder facts: symbols, refs,
  unresolved occurrences.
- `resolve_ref(ref_id) -> ResolveResult`
  Input: binding occurrence, expected namespace if known. Output: value/type/
  variant/prelude resolution or unresolved diagnostic.
- `resolve_type(type_id, scope_id) -> Type`
  Input: AST type node and scope. Output: normalized checker type.
- `resolve_path(path_id, namespace, context) -> DefId | VariantId | Diagnostic`
  Input: path expression/type/pattern context. Output: resolved item.

### Type And ABI Facts

- `type_decl(def_id) -> TypeDecl`
  Input: struct/enum def. Output: declared shape, generic params, field types,
  variant payload types.
- `schema_of_type(type) -> SchemaName`
  Input: normalized type. Output: current machine schema spelling.
- `descriptor_of_type(type) -> weavy::mem::Descriptor<SchemaName>`
  Input: normalized type. Output: layout descriptor.
- `abi_class(type) -> AbiClass`
  Input: type. Output: `BitsWord`, `Handle { target }`,
  `Pending { value }`, `Realized { value }`, `StoreValue`, or `Fn`.
- `memo_key_part(type, expr_id) -> MemoKeyPart`
  Input: type and value occurrence. Output: scalar word hash vs handle content
  hash discipline.

### Expressions And Patterns

- `scope_of(node_id) -> ScopeId`
  Input: AST node. Output: lexical scope.
- `type_of(expr_id) -> TypeResult`
  Input: expression id plus expected type if context supplies one. Output: type,
  ABI class, coercions/barriers needed, and diagnostics.
- `check_annotation(expr_id, annotated_type) -> CheckResult`
  Input: expression and annotation. Output: typed expression or mismatch.
- `call_signature(callee_id) -> FnSig | PrimitiveSig | PendingSig`
  Input: callee expression/path. Output: parameter names/types/defaults,
  return type, partial-policy.
- `check_call(call_id) -> CallFacts`
  Input: call AST, callee signature, expected result. Output: ordered args,
  missing/duplicate/unknown diagnostics, partial facts, return type.
- `method_signature(receiver_type, method_name) -> MethodSig`
  Input: receiver type and method name. Output: method shape or diagnostic.
- `field_type(receiver_type, member) -> TypeResult`
  Input: receiver type and `.name` or `.index`. Output: field type and access
  facts.
- `pattern_bindings(pattern_id, scrutinee_type, top_level) -> PatternFacts`
  Input: pattern and scrutinee type. Output: bindings, constructor refs,
  payload field refs, irrefutability.
- `exhaustive(match_id) -> ExhaustivenessResult`
  Input: scrutinee type and arm pattern facts. Output: covered variants/
  literals, missing cases, unreachable/irrefutable-before-last diagnostics.
- `type_of_match(match_id) -> TypeResult`
  Input: arm value types, guards, exhaustiveness facts. Output: match result
  type and diagnostics.

### Lowering Inputs

- `function_signature(fn_id) -> FnSig`
  Input: fn item. Output: typed params, return type, generic params.
- `function_body_facts(fn_id) -> BodyFacts`
  Input: fn body. Output: typed body, local bindings, call sites, lazy let
  dependencies.
- `closure_hash_inputs(fn_id) -> ClosureHashFacts`
  Input: bindings plus type refs. Output: function and type dependency graph
  inputs currently computed in `module.rs`.
- `lowering_plan(fn_id) -> LoweringPlan`
  Input: typed body facts and ABI facts. Output: enough information for machine
  lowering to emit typed weavy ops without rechecking semantics.
- `diagnostics(file_id) -> Vec<Diagnostic>`
  Input: parse, binding, type, pattern, call, ABI queries. Output: sorted
  diagnostics for LSP and CLI.

### LSP Projections

- `symbol_at(file_id, offset) -> Option<SymbolId>`
  Existing binder query, backed by `Analysis`.
- `definition(symbol_id) -> Span`
  Existing definition provider.
- `references(symbol_id, include_decl) -> Vec<Span>`
  Existing references/rename provider.
- `hover(offset) -> HoverInfo`
  Input: symbol/type/expression at offset. Output: kind, name, type/schema/ABI
  where known.
- `semantic_tokens(file_id) -> Vec<Token>`
  Syntax highlights plus optional checker refinements later.

## Design Questions For Amos

1. Inference depth: full HM vs local checking.

   Options:
   - Full Hindley-Milner over expressions and functions.
   - Local inference only, with function boundaries annotated and `let`
     annotations optional.
   - Annotation-first: infer literals/constructors only when an expected type is
     present.

   Recommendation: local inference with required function boundary annotations.
   Existing code already requires param annotations in the grammar, defaults
   omitted returns to `Int` in lowering, and opportunistically infers local
   literals/tuples/maps for schema registration (`vix/src/machine/lower.rs:1886`,
   `vix/src/machine/lower.rs:1351`). This keeps query outputs stable and makes
   memo-boundary ABI explicit.

   Ruling needed: should omitted return type remain `Int`, become inferred, or
   become illegal after the checker lands?

2. Generic story.

   Options:
   - Monomorphize at call sites, producing concrete schemas and lowering plans.
   - Erase generics to store-value handles with dynamic descriptor checks.
   - Check generic declarations but reject lowering/invocation until a later
     arc.

   Recommendation: check generic declarations and uses now, reject unsupported
   generic lowering loudly, then rule monomorphization before implementing
   generic execution. Erasure conflicts with typed untagged operands and static
   descriptor authority. The grammar and type tour already promise generics,
   but lowering currently stubs generic functions (`vix/src/machine/lower.rs:1663`).

   Ruling needed: are generic vix functions compiled by monomorphization, or is
   there a different static ABI story?

3. Function types and partials.

   Options:
   - Treat `fn(A)->B` as a first-class value type backed by `Pending<B>`/closure
     identity.
   - Keep only named function calls plus current partial-call pending values;
     accept `fn` types syntactically but reject general values.
   - Defer all `fn`-typed params, including `apply`, as checked-but-not-lowered.

   Recommendation: keep the existing partial/pending path as the executable
   subset, and check general `fn` types without lowering them until the closure
   ABI is ruled. The fleet note already points toward pending store values
   rather than legacy AST closures (`docs/design/fleet-on-the-machine.md:86`).

   Ruling needed: is `fn(Int)->Int` just sugar for a pending callable store
   value, or a distinct function value type?

4. Coercions and subtyping.

   Options:
   - No subtyping; only explicit, enumerated coercions already in machine
     lowering.
   - Add structural subtyping for records/maps/options.
   - Add broader scalar or literal coercions.

   Recommendation: no subtyping. Keep only existing explicit coercion barriers:
   numeric contextual float literals, `Pending<T>`/`Realized<T>` to `T`,
   `Realized<Doc>`/`Doc` to concrete doc projections, and scalar arg
   normalization (`vix/src/machine/lower.rs:3138`,
   `vix/src/machine/lower.rs:3153`). Strings must not coerce to paths
   (`playgrounds/snark/src/bundled/vix/grammar.js:5`).

   Ruling needed: should int literals be contextually accepted as `Float`, as
   today, or should numeric literal typing be stricter?

5. Exhaustiveness semantics.

   Options:
   - Strict enum exhaustiveness: all enum variants covered unless final
     wildcard/binding exists; guarded arms do not count as covering.
   - Lowering-compatible minimal rule: non-final refutable arms allowed only if
     final irrefutable arm exists.
   - Warning-only exhaustiveness for V1.

   Recommendation: strict enum exhaustiveness with guards not counted for
   coverage, while preserving the current scalar/string final-irrefutable rule.
   This matches the lowerer comment that checker owns exhaustiveness and keeps
   typoed variants diagnostic-producing rather than catch-all bindings.

   Ruling needed: should non-exhaustive matches be hard errors immediately, or
   LSP diagnostics with machine lowering still requiring a final fallback during
   migration?

6. Duplicate declaration policy.

   Options:
   - Only duplicate call args and struct literal fields are errors; declaration
     shadowing remains broad.
   - Item/type/field/variant/type-param duplicates are errors; local value
     shadowing remains allowed.
   - No duplicate declaration diagnostics in V1 except existing lowerer cases.

   Recommendation: item/type/field/variant/type-param duplicates should be
   checker errors; local lets and params should keep the existing lexical
   shadowing rule unless Amos rules a narrower policy. Existing binder docs
   explicitly allow lexical shadowing (`vix/src/binder.rs:8`), while lowerer
   already treats duplicate named args and duplicate struct literal fields as
   errors (`vix/src/machine/lower.rs:2630`, `vix/src/machine/lower.rs:2940`).

   Ruling needed: do function names, type names, and imports share one top-level
   namespace or separate value/type namespaces?

7. Error philosophy and recovery.

   Options:
   - Fail-fast checker: first blocking error stops dependent queries.
   - Accumulating checker: produce multiple diagnostics with typed-error holes.
   - LSP accumulates, machine fail-fast: same query engine, different projection.

   Recommendation: accumulating diagnostics for LSP, fail-fast projection for
   machine lowering. The checker can represent `Type::Error`/`ExprType::Error`
   internally for recovery, but lowering must not emit dynamic checks or
   fallback ops. That preserves the machine rule that runtime never asks what a
   value is (`vix/src/machine/mod.rs:12`).

   Ruling needed: should CLI/machine load expose all diagnostics or the first
   blocking diagnostic?

8. Prelude and imports.

   Options:
   - Hard-code current primitives and builtins in Rust checker.
   - Model a prelude module with typed primitive signatures.
   - Leave unresolved primitives as binder unresolved until lowering.

   Recommendation: model a checker prelude table with spans synthesized as
   builtins. Binder already treats unresolved primitive-like names as future
   type/prelude facts (`vix/src/binder.rs:18`), while lowering has concrete
   primitive call rules for `fetch`, `extract`, `toml`, `json`, and `elf`
   (`vix/src/machine/lower.rs:2466`).

   Ruling needed: should prelude names be importable/shadowable like values, or
   reserved builtins?

9. ABI classification vocabulary.

   Options:
   - Keep schema strings as the checker ABI boundary.
   - Introduce typed `TypeId`/`SchemaId`/`AbiClass` facts, render schema strings
     only at machine boundary.
   - Let lowering keep recomputing ABI from schema strings.

   Recommendation: typed `AbiClass` facts in the checker, with schema strings as
   an output projection. The current string helpers (`Pending<T>`,
   `Realized<T>`, `Map<K,V>`, `Tuple<...>`) are enough to map current behavior
   but not enough as the long-term query key.

   Ruling needed: should query memo keys use structural type ids, content hashes,
   or current schema strings for V1?

10. Rust checker oracle shape.

   Options:
   - Rust checker is the normative implementation until the vix checker reaches
     differential parity.
   - Rust checker is only a differential oracle against written semantics, with
     no special authority.
   - Rust checker and vix checker both compare against machine-lowering
     outcomes.

   Recommendation: Rust checker is the normative query engine for diagnostics
   and LSP in V1, then becomes the differential oracle for the vix-written
   checker. Machine lowering should consume checker facts, not remain the
   oracle, because many current errors are accidental lowerer text rather than a
   language-level diagnostic surface.

   Ruling needed: which outputs are part of the differential contract: only
   accept/reject and typed facts, or exact diagnostic text/spans too?
