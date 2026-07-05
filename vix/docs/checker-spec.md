# Vix Checker Spec

This is the normative contract for the Vix checker. There is exactly one
checker implementation, written in Vix. During bootstrap that implementation is
temporarily unchecked by design. There is no Rust oracle twin.

The checker contract is this spec plus the absolute corpus assertions in
`vix/tests/checker-corpus/`. Any future reimplementation is differential only
against typed facts, accept/reject outcome, and spans. Diagnostic prose is never
semantic.

## Rulings

These ten rulings are fixed:

1. local inference; MODULE boundaries fully annotated (public items explicit); omitted return type ILLEGAL, diagnostic suggests the inferred type
2. generics checked now, lowering rejected loudly, monomorphization is the intended ABI
3. fn(A)->B is sugar for a pending callable store value -- no distinct function kind
4. no subtyping/coercions; contextual LITERAL typing only (int literal in Float position); strings never coerce to paths
5. strict enum exhaustiveness, guards don't count, HARD ERROR; scalar/string matches keep the final-irrefutable rule
6. item/type/field/variant/type-param duplicates are errors; lexical let/param shadowing allowed; SEPARATE value and type namespaces
7. diagnostics ACCUMULATE (typed Error holes internally); machine lowering fail-fast projection; CLI reports all; lowering never emits dynamic checks
8. REAL MODULE SYSTEM FROM THE START -- no single-file simplification; vix:: and caps:: are ordinary auto-imported modules; multi-file programs are v1 scope
9. typed AbiClass facts; memo/query keys = CONTENT HASHES of structural types (V10's type hashing reused); schema strings are an output projection only
10. no oracle twin; the differential contract for any future re-implementation = typed facts + accept/reject + spans; diagnostic prose is never semantic

## Program Model

A checked program is a set of source files. Each file has a module path, source
text, generated AST, and spans in half-open byte offsets. Module identity is not
derived from a filename string after parsing; the build/load boundary supplies
the `(module_path, content_hash, source_text)` inputs.

Every program starts with two ordinary auto-imports:

- `vix::`, containing language/library types and primitives such as `Tree`,
  `Path`, `Target`, `Map`, `Blob`, `Doc`, `fetch`, `extract`, `toml`, `json`,
  and `elf`.
- `caps::`, containing capability types such as `Cc`, `Ar`, and `Rustc`.

Auto-imported modules are modules, not reserved syntax. A local item may shadow
a value name from an import in the value namespace. Type and value namespaces are
separate.

## Name Resolution

Resolution is query-based and namespace-aware.

Value namespace:

- functions
- parameters
- lexical `let` bindings
- closure parameters
- pattern bindings
- value imports
- primitive functions and constructors

Type namespace:

- structs
- enums
- type parameters
- type imports
- primitive types

Variant resolution is contextual. In expression and pattern contexts,
`Type::Variant` resolves through the type namespace first and then through the
variant table for that enum. A bare top-level identifier pattern is not a new
binding when the scrutinee is an enum; it must resolve as a variant or produce a
diagnostic. Payload-position identifiers bind values.

Lexical value shadowing is allowed for parameters and `let` bindings. Duplicate
items, duplicate types, duplicate fields, duplicate variants, duplicate type
parameters, duplicate call arguments, duplicate struct literal fields, and
duplicate pattern fields are errors.

## Types

The checker normalizes these type forms:

- paths: `Int`, `vix::Tree`, `caps::Cc`, local type names
- generic applications: `Map<String, String>`, `Pair<A, B>`
- arrays: `[T]`
- tuples: `(A, B, C)`
- pending callable sugar: `fn(A, B) -> C`

Every type is hashable, equatable, totally ordered, facet-shaped, and
serializable. Vix does not express trait bounds for these facts.

Function parameters and return types form module boundaries and must be
annotated. Omitted return type is illegal. The diagnostic carries the inferred
return type when inference reached one; inference failure still reports an
omitted return type with `Error` as the suggestion.

Inference is local. `let` annotations are optional. Literal typing may use an
expected type. The only contextual literal conversion is an integer literal in a
`Float` position. Strings never become paths; `p"..."` is the path literal form.
There is no subtyping and no implicit record, scalar, path, pending, or handle
coercion.

Generic declarations and generic function bodies are checked. Lowering any
generic function or generic instantiation is a hard lowering diagnostic until
monomorphization lands. The intended executable ABI is monomorphization, not
erasure or dynamic descriptor checks.

`fn(A) -> B` is not a separate runtime kind. It normalizes to a pending callable
store value with an argument signature and result type.

## Expressions

`type_of(expr, expected)` returns a type, ABI class, typed facts, and zero or
more diagnostics. On blocking errors it returns `Error` so dependent checks can
continue and accumulate diagnostics.

Identifiers resolve in the value namespace unless the syntactic context asks for
a type. Calls check positional arguments, named arguments, partial markers,
arity, argument types, and return type. Partial calls must bind a contiguous
argument prefix and produce a pending callable store value.

Field access on records uses field names. Tuple access uses tuple indices. Enum
constructors check payload shape. Struct literals check required fields,
defaults, update bases, unknown fields, and duplicate fields.

Binary operators are exact-type operators. Equality requires exact operand
types. Boolean operators require `Bool`. Arithmetic operators require the
declared scalar type; integer literals may be contextually typed as `Float`.

Command blocks, `fetch`, `extract`, `toml`, `json`, `elf`, capability
acquisition, tree projection, arrays, maps, and docs are typed by primitive
signatures in the auto-imported modules.

## Patterns And Exhaustiveness

`pattern_bindings(pattern, scrutinee_type, top_level)` returns the bindings
introduced by the pattern, constructor/field references, compatibility facts,
irrefutability, and diagnostics.

Enum matches are strictly exhaustive. Every variant must be covered by an
unguarded arm unless a final wildcard or binding arm exists. Guarded arms do not
count toward coverage. Non-exhaustive enum matches are hard errors.

Scalar and string matches keep the final-irrefutable rule: if a scalar/string
match contains refutable literal arms, the last arm must be an irrefutable
wildcard or binding. Irrefutable arms before the final arm are errors.

Match guards must have type `Bool`. Arm values must have one result type.

## ABI Classes

The checker computes typed ABI facts. Schema strings are a machine output
projection, not the key used by checker queries.

`AbiClass` values:

- `BitsWord`: inline frame word, including `Int`, `Float`, and `Bool`
- `Handle<T>`: store handle for non-scalar identity values such as `String`,
  `Path`, `Tree`, `Blob`, `Doc`, records, tuples, arrays, maps, and enums
- `PendingCallable<Args, Ret>`: pending callable store value, including
  `fn(Args) -> Ret` sugar and partial calls
- `Pending<T>`: pending store value for demanded computation
- `Realized<T>`: realized store value whose identity is still content-addressed
- `Error`: recovery placeholder; never crosses into lowering

Memo and query keys use content hashes of normalized structural types. V10 type
hashing is reused. Values contribute according to ABI class: bits hash by frame
word bytes; handles and store values hash by canonical content identity; pending
callables hash closure identity, remaining signature, bound argument ABI facts,
and bound argument identities.

Lowering consumes ABI facts and typed lowering plans. It never emits runtime
type checks or fallback dynamic checks.

## Diagnostics

Diagnostics accumulate for CLI and LSP. Machine lowering receives a fail-fast
projection over the same facts.

Diagnostic kinds in the v1 corpus:

- `OmittedReturnType`
- `UnresolvedValue`
- `UnresolvedType`
- `UnresolvedModule`
- `DuplicateItem`
- `DuplicateType`
- `DuplicateField`
- `DuplicateVariant`
- `DuplicateTypeParam`
- `DuplicateArgument`
- `UnknownArgument`
- `MissingArgument`
- `TooManyArguments`
- `DuplicatePartialMarker`
- `PartialCallNonPrefix`
- `TypeMismatch`
- `StringIsNotPath`
- `NonBoolGuard`
- `NonExhaustiveMatch`
- `GuardedArmDoesNotCover`
- `IrrefutableArmNotLast`
- `DuplicateStructField`
- `UnknownStructField`
- `MissingStructField`
- `PatternPayloadMismatch`
- `DuplicatePatternField`
- `GenericLoweringUnsupported`

Each diagnostic has `kind`, `primary_span`, and structured payload fields
specific to the kind. Prose is an output projection and not part of the
semantic contract.

## Query Vocabulary

All query inputs that include source or type structure use content hashes, not
mutable paths or schema strings, as memo keys.

### Source And Syntax

`source_text(file_id) -> Arc<str>`

Input: file id. Output: immutable source text.

`parse(file_id) -> Result<SourceFile, ParseDiagnostic>`

Input: source text hash. Output: generated AST or parse diagnostic.

`line_index(file_id) -> LineIndex`

Input: source text. Output: UTF-16-aware line index for LSP.

`syntax_highlights(file_id) -> Vec<Highlight>`

Input: source text. Output: syntax captures. This query remains syntax-based.

### Modules, Declarations, And Binding

`module_graph(program_id) -> ModuleGraph`

Input: set of `(module_path, content_hash)` files plus auto-import roots.
Output: module ids, import edges, and module diagnostics.

`decls(module_id) -> DeclTables`

Input: parsed files in one module. Output: functions, structs, enums, variants,
fields, generic params, imports, spans, duplicate diagnostics.

`bind(file_id) -> Bindings`

Input: AST and declaration prelude. Output: symbols, references, lexical scopes,
and unresolved occurrences.

`resolve_ref(ref_id, namespace, expected_type) -> ResolveResult`

Input: a binding occurrence, namespace, and optional expected type. Output:
resolved value/type/variant/prelude symbol or diagnostic.

`resolve_type(type_id, scope_id) -> TypeResult`

Input: AST type node and scope. Output: normalized type or `Error` plus
diagnostics.

`resolve_path(path_id, namespace, context) -> PathResolution`

Input: path expression, type path, or pattern path context. Output: definition,
variant, module, or diagnostic.

### Type And ABI Facts

`type_decl(def_id) -> TypeDecl`

Input: struct or enum definition. Output: kind, generic params, fields,
variant payloads, defaults, declaration order, and type hash.

`schema_of_type(type_id) -> SchemaName`

Input: normalized type. Output: current machine schema spelling. Projection
only.

`descriptor_of_type(type_id) -> Descriptor`

Input: normalized type. Output: `weavy::mem` layout descriptor facts.

`abi_class(type_id) -> AbiClass`

Input: normalized type. Output: typed ABI class.

`memo_key_part(type_id, value_occurrence) -> MemoKeyPart`

Input: type and typed value occurrence. Output: bits-vs-content hash discipline.

### Expressions And Patterns

`scope_of(node_id) -> ScopeId`

Input: AST node. Output: lexical scope.

`type_of(expr_id, expected_type) -> TypeResult`

Input: expression and optional expected type. Output: type, ABI class,
expression facts, and diagnostics.

`check_annotation(expr_id, annotated_type) -> CheckResult`

Input: expression and annotation. Output: typed expression facts or mismatch.

`call_signature(callee_id) -> CallableSig`

Input: callee expression/path. Output: params, defaults, return type, generic
params, and partial-call policy.

`check_call(call_id) -> CallFacts`

Input: call AST and callable signature. Output: ordered args, argument spans,
missing/duplicate/unknown diagnostics, partial facts, result type, and ABI
class.

`method_signature(receiver_type, method_name) -> MethodSig`

Input: receiver type and method name. Output: method shape or diagnostic.

`field_type(receiver_type, member) -> FieldFacts`

Input: receiver type and `.name` or `.index`. Output: field type, field
definition span, and diagnostics.

`pattern_bindings(pattern_id, scrutinee_type, top_level) -> PatternFacts`

Input: pattern and scrutinee type. Output: introduced bindings, constructor
refs, payload field refs, irrefutability, and diagnostics.

`exhaustive(match_id) -> ExhaustivenessResult`

Input: scrutinee type and arm pattern facts. Output: covered variants/literals,
missing cases, guarded non-coverage facts, and ordering diagnostics.

`type_of_match(match_id) -> TypeResult`

Input: match expression, arm value types, guards, and exhaustiveness facts.
Output: match result type and diagnostics.

### Lowering Inputs

`function_signature(fn_id) -> FnSig`

Input: function item. Output: typed params, typed return, generic params,
visibility, and boundary diagnostics.

`function_body_facts(fn_id) -> BodyFacts`

Input: function body. Output: typed body, local bindings, call sites, match
facts, lazy-let dependencies, and diagnostics.

`closure_hash_inputs(fn_id) -> ClosureHashFacts`

Input: bindings plus type refs. Output: function and type dependency graph
inputs for closure hash computation.

`lowering_plan(fn_id, mono_args) -> LoweringPlan`

Input: typed body facts, ABI facts, and optional monomorphization args. Output:
machine-ready typed operation plan or `GenericLoweringUnsupported`.

`diagnostics(program_id) -> Vec<Diagnostic>`

Input: parse, module, binding, type, pattern, call, ABI, and lowering queries.
Output: sorted diagnostics for CLI and LSP.

### LSP Projections

`symbol_at(file_id, offset) -> Option<SymbolId>`

Input: file id and byte offset. Output: resolved symbol when present.

`definition(symbol_id) -> Span`

Input: symbol. Output: definition span.

`references(symbol_id, include_decl) -> Vec<Span>`

Input: symbol. Output: reference spans.

`hover(offset) -> HoverInfo`

Input: file offset. Output: kind, name, type, schema projection, and ABI class
where known.

`semantic_tokens(file_id) -> Vec<Token>`

Input: file id. Output: syntax tokens plus checker refinements.

## Open Spec Ambiguities

None. The prior recon questions are closed by the ten rulings above.
