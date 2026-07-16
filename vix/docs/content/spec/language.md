+++
title = "Language specification"
weight = 1
+++

This is the normative source specification for Vix. The book explains why the
language has this shape; the runtime specification defines how demanded values
are evaluated. Historical design notes and old corpus spellings carry no
authority over this page.

The words MUST, MUST NOT, SHOULD, and MAY are normative. There are no open
language decisions in this specification. A missing case is a specification bug
to resolve here before an implementation or corpus program invents an answer.

## Evaluation boundary

> r[lang.demand.no-force]
>
> A Vix program describes immutable values and dependency wiring. It contains no
> operation that starts, forces, sequences, or observes evaluation. Evaluation
> begins only when an external holder of the program demands a root.

`let x = f y;` binds a wire. It does not call `f`. Selecting `x.field` denotes a
dependency on that field and not its siblings. The same rule applies to map
keys, tree paths, selected match arms, syntax nodes, process products, and
stream elements.

All language values have by-value semantics. Mutation, aliasing, residency,
placement, scheduling order, and island boundaries are unobservable. An
implementation may mutate unique private construction state or eagerly execute
a proven-strict island under the as-if rule in the runtime specification.

## Lexical and block syntax

> r[lang.syntax.blocks]
>
> Braces delimit blocks. A block contains `let` bindings and generator `yield`
> forms followed by an optional final expression. A `let` binding ends in `;`.
> Ordinary expression statements do not exist.

Parentheses group. They are not function-call punctuation. Double-quoted
strings interpolate `${expression}`; single-quoted strings are literal. Path
literals use `p"..."`. Command templates use capability-tagged backticks and
interpolate one typed argument fragment with `{expression}`.

Unary minus binds tighter than binary operators but is not a juxtaposition atom:
write `abs (-1)`, never `abs -1`. `%` begins an explicitly keyed collection
literal and is not modulo; use `rem` or `rem_euclid`.

Precedence from tightest to loosest is:

```text
field / method > juxtaposition > postfix ? > unary > binary > where { ... }
```

## Functions and arguments

> r[lang.call.juxtaposition]
>
> Function application is juxtaposition. A function has at most one positional
> parameter. All other parameters are fields of one named-argument record
> declared with `where`.

```vix
fn range where { from: Int, to: Int } -> [Int]
fn render(value: T) where { style: Style } -> String

let xs = range where { from: 0, to: 10 };
let text = render value where { style };
```

At-most-one positional parameter is a source rule, not merely an API guideline.
Tuples remain one positional value. Named arguments use `name: value`; a bare
name is field punning. A parameter with a default can only be supplied by name.
Adding a defaulted named parameter does not break existing calls.

`where` in a signature declares either an inline structural record or a named
record type. `where { ... }` at a call constructs that record. There is no
`partial` keyword or marker: pre-binding the positional subject is a closure,
and pre-binding named arguments is record construction and spread.

> r[lang.types.inference]
>
> Vix uses bidirectional type checking. Expected types flow into empty
> collections, `None`, enum constructors, decode expressions, closure
> parameters, structural records, and generic applications. A program is
> rejected when local expected-type propagation does not determine one type;
> the checker does not guess from runtime data.

Generics are monomorphized by concrete schema. Equality, structural order,
canonical encoding, hashing, and serialization are available for every Vix
value by language law; there are no user-visible dictionaries or trait bounds
for them.

> r[lang.types.generic-enum-monomorphized]
>
> A generic enum declaration is a type template, never a runtime type. Applying
> concrete type arguments substitutes every payload occurrence and produces a
> concrete nominal enum whose identity includes the fully applied name and
> substituted shape. Qualified constructor syntax names the declaration base;
> the expected type selects the concrete application when a variant payload
> does not mention every generic parameter. No erased type parameter or runtime
> generic dictionary survives lowering.

> r[lang.types.generic-function-monomorphized]
>
> A generic function declaration is a code template, never a runtime value.
> Each instantiation substitutes concrete types for the type parameters —
> inferred from the argument types — and lowers one body per distinct
> type-argument set. Type parameters are not part of the surface call syntax:
> an instantiation is written as an ordinary call and its type arguments are
> inferred. No erased type parameter or runtime generic dictionary survives
> lowering.

## Types, records, and enums

Named declarations are nominal. Anonymous `struct { ... }` types are
structural.

```vix
struct Point { x: Int, y: Int }
struct PkgId(Int);

let p = Point { x: 1, y: 2 };
let erased = struct { ..p };
```

Nominal identity includes the fully-qualified type name and shape. A tuple
struct with one field is the newtype form; construction uses its name and the
payload is projected as `.0`. A structural value may be used to construct a
nominal value only when every required field is present and there are no
unconsumed extra fields. Nominal-to-nominal spread requires the same type;
erasure through `struct { ..value }` is explicit.

Record fields MAY have defaults. A constructor fills omitted defaulted fields
and rejects omitted required fields, unknown fields, duplicate fields, and
silent extra-field loss.

Enums have unit, tuple, and record variants. Multi-field payloads SHOULD use a
record payload so roles remain named; tuple variants remain legal when the
payload is intrinsically positional.

> r[lang.pattern.record]
>
> A record-shaped pattern is one type path followed by named field patterns.
> The checker resolves that path against the scrutinee: it names either a
> nominal record type or a record enum variant. This distinction is semantic,
> not parser precedence, so qualified record names cannot be mistaken for enum
> variants. Field patterns recurse through the ordinary pattern algebra and
> source field order is irrelevant; projections follow declaration order.
> Omitted declared fields are accepted only when the pattern contains an
> explicit `..`. Without `..`, missing, unknown, and duplicate fields are
> diagnostics.

## Modules and methods

> r[lang.module.use]
>
> `use` is the only import spelling. Imports are lexical, explicit, and do not
> execute code. Public items use `pub`; unqualified collisions are compile
> errors.

```vix
use geometry::{Point, magnitude};
use caps::Rustc;
```

`namespace Type { ... }` declares inherent methods. `extend Type { ... }`
declares import-scoped extension methods. Lookup first considers an applicable
inherent method, then visible extensions. More than one applicable method in a
class is an ambiguity diagnostic. Prelude methods are ordinary lowest-scope
extensions; there is no hidden host-method table.

The receiver is implicit in method syntax; a method may additionally take one
explicit positional argument plus named arguments. Empty `()` is the dedicated
zero-explicit-argument method form (`rows.collect()`). Otherwise parentheses
still only group the one juxtaposed argument (`rows.map(f)` is `rows.map (f)`).

`<=>` is language-derived and cannot be declared in `namespace` or `extend`.
Adding a method never changes canonical identity or order.

## Structural order and identity-visible order

> r[lang.value.ordering-is-enum]
>
> `<=>` returns the ordinary enum `Ordering`, whose variants are `Less`,
> `Equal`, and `Greater` in that declaration order. `Ordering` uses the same
> value representation, pattern checking, and match lowering as every other
> enum; comparison does not introduce a hidden control channel.

> r[lang.value.structural-order]
>
> Every completed value has a total, equality-consistent structural order.
> The order is derived, never user-overridable, and is the only order used for
> canonical maps, sets, snapshots, and default sorting.

Base cases are numeric integer order; `false < true`; IEEE `totalOrder` with
canonicalized NaN and signed zero; Unicode scalar order for strings and paths;
and byte-lexicographic order for blobs. Structs compare fields in declaration
order. Enums compare variant declaration position then payload. Arrays compare
lexicographically by index. Maps compare canonical key-sorted rows. Functions
compare stable definition identity; closures then compare captures.

Alternative order is an ordinary `Order<T>` value passed explicitly. `by_key`
ties equal extracted keys by the structural order of the source value, so it
remains total. Semver precedence is modeled in `Version`'s field and variant
shape; it is not a user-defined `<=>`.

## Collections

`[T]` is a dense array whose positions are data. `Map<K,V>` is an immutable
canonical map. `Set<T>` is a distinct standard collection represented with the
same canonical map machinery, not a source-level alias. `Tree` is the recursive
artifact collection defined below.

```vix
[a, b, c]
%{ key => value }
%[a, b, c]
```

> r[lang.collection.array-positions-are-data]
>
> An array's positions are its fields, and its length is a property of each
> value rather than of its type. `[T]` names arrays of every length; there is no
> `[T; N]`. Two arrays with equal elements in equal positions are the same
> value, whatever built them.

> r[lang.collection.array-map]
>
> `xs.map(f)` on `xs: [T]` and `f: fn(T) -> U` has type `[U]` and the same
> length as `xs`. Result position `i` is `f(xs[i])`, and its origin is input
> position `i`. Result positions are independently demandable. The language
> promises no left-to-right execution order; the compiler selects an
> inspectable fused, looped, or fanned execution shape without changing the
> position-keyed semantic grain recipe.

> r[lang.collection.array-index]
>
> `a[i]` on `a: [T]` has type `T`. The index is an `Int` addressing positions
> `0..a.len()`. An index outside that range ends the current demand with a typed
> `IndexOutOfBounds` failure at the indexing expression's stable source site,
> carrying the demanded index and the array's length. Reporting resolves that
> site through the current source map to produce the current span. It is never a
> defaulted element, never a wrapped or saturated index, never an `Option<T>`,
> never a machine invariant, and never a process panic — an array read that
> succeeds has produced a `T`, so no caller unwraps one. Bounds are checked
> against the array value, never inferred from its type.

> r[lang.collection.map-canonical]
>
> Map construction order does not affect value identity. Rows are sorted by
> the structural order of `K`. A map literal containing duplicate keys is
> rejected. Dynamic field addition through `+` and map composition through
> `++` fail with a typed `DuplicateKey` when the key already exists in the left
> operand. Only `map.with (key, value)` deliberately inserts or replaces a row.

> r[lang.collection.field-addition]
>
> `+` adds one field to a materialized collection and `++` adds every field of
> another collection. Their exact types are `[T] + T -> [T]`,
> `[T] ++ [T] -> [T]`, `Set<T> + T -> Set<T>`,
> `Set<T> ++ Set<T> -> Set<T>`, `Map<K,V> + (K,V) -> Map<K,V>`, and
> `Map<K,V> ++ Map<K,V> -> Map<K,V>`. Array `+` appends and array `++`
> concatenates. Set addition and union are idempotent. Map addition rejects an
> existing key and map `++` requires disjoint key sets. Neither operator mutates
> either operand. Their results are `must_use`.

> r[lang.collection.map-with]
>
> `map.with (key, value)` returns a map containing that binding, replacing the
> previous value when the key was present. The original map is unchanged. The
> tuple is the method's one explicit positional argument. The result is
> `must_use`.

> r[lang.collection.map-get]
>
> `map.get(key)` has type `V`. A missing key ends the current demand with a typed
> `MissingKey { key }` failure at the get expression's stable source site. It is
> never `None`, a default value, or a machine invariant. `map.get(key)?`
> observes any failure of the projection as `Result<V, Failure>`.
> `map.has(key)` returns whether the key exists without demanding its value.
> `Set.has(value)` tests set membership.

`Map.keys()` and `Map.values()` return dense arrays in canonical key order.
`Map.stream()` returns `Stream<K,V>` with the map keys preserved. `Set.values()`
returns a dense array in structural element order; `Set.stream()` is keyed by
the elements. `Set.map` returns a set and therefore deduplicates equal images.

Array transformations preserve authored position. `map` transforms fields at
the same positions; `reversed` returns the reverse. Filtering belongs to streams,
where keys survive, and `.values()` is the explicit compaction back to a dense
array. `find_map` searches authored arrays left-to-right and `find_last_map`
searches right-to-left. `fold_until` is the explicit early-exit fold. There is
no mutation-shaped `push`, `pop`, or `insert`; use collection `+` / `++`,
`map.with`, array spread, or the `split_*` deconstructors.

A stream has no arrival order, so it has no `first`, `last`, or arrival-sensitive
fold. Deterministic stream selection names an `Order` and completes enough of the
stream to prove the result.

`String.lines()` returns a dense array in textual order. `strip_prefix` and
`strip_suffix` return `Option<String>`. Parse failures return typed errors.

## Streams and byte codata

> r[lang.codata.stream]
>
> `Stream<K,V>` is codata: a progressively available family of values with
> stable semantic keys and no arrival order. Each element is independently
> demandable. The completed semantic content is `Map<K,V>`.

`map` and `filter` preserve keys. `flat_map` composes keys as `(K,J)`.
`collect()` returns `Map<K,V>` and fails on duplicate keys. `.values()` is the
explicit compaction from a collected map to a dense array.

`Stream<V>` is shorthand for a compiler-keyed generator stream. Its keys are
stable yield-provenance paths derived from the yield site and any keyed dynamic
iteration, never delivery ordinals. A generator body declares its codata in the
return type and uses `yield expression;`. The compiler rejects a generator when
it cannot derive unique stable keys; callers needing public keys write
`Stream<K,V>` and yield keyed elements.

`ByteStream` is a separate progressive type whose completed value is a `Blob`.
OS write boundaries and transport frames are not semantic. Published ranges are
addressed by byte offset. UTF-8 decoding is explicit and fallible; `.lines()` on
decoded text yields line-number-keyed codata. A capability package MAY provide a
typed protocol decoder without removing access to raw bytes.

Codata may be a record field and may cross island and placement boundaries. The
record receives completed value identity only when the codata drains; consumers
may demand elements or ranges before then.

## Paths, blobs, and trees

`Path` is a relative, relocatable path value. Strings never coerce to paths.
`/` joins paths and tree projections. Reading a file as text requires an explicit
decode; a tree-file projection does not coerce to `String`.

```vix
Tree      = Map<Name, TreeEntry>
TreeEntry = File { content: Blob, executable: Bool }
          | Dir(Tree)
          | Symlink { target: String }
```

`Name` is one nonempty valid-UTF-8 segment. It excludes `.`, `..`, separators,
and NUL; it preserves spelling without Unicode normalization. Tree semantics are
case-sensitive on every platform. Empty directories round-trip.

`executable` is portable semantic intent and participates in identity on every
platform. Unix materialization maps it to a canonical mode. Windows preserves
the bit in Vix/Vixen metadata even though process creation does not consult a
POSIX execute mode.

mtime, uid/gid, mode bits other than executable, xattrs, resource forks,
hardlink identity, device/FIFO/socket nodes, ACLs, and host case-folding are not
Tree properties. A tool that observes them requires an explicit artifact type
and primitive.

Ordinary symlink targets are relative valid UTF-8 and are preserved without
normalization. Dangling targets and `..` are representable. Resolution is
against the containing directory and mount grant; escape is denied and
witnessed. Absolute links require an explicit non-relocatable artifact/import
policy.

`Tree.union` is a partial commutative, associative, idempotent structural join.
Directories recurse; identical leaves coalesce; any unequal leaf or kind
collision returns `TreeConflict` with the full path and both entries. A separate
`disjoint_union` rejects even identical duplicate ownership.

## Failure and recovery

> r[lang.failure.typed]
>
> `fail payload` ends the current demand with a typed `Failure`. `Failure` is a
> value whose identity includes the typed payload, an optional published subject
> identity, and the stable source-site identity of the failing operation. Raw
> byte spans and the reporting demand chain are observation context, resolved
> when the failure is reported, not stored in the value. No language operation
> returns `Result<_, String>`.

Postfix `?` is the only in-program observation of demand failure. For an
expression of type `T`, `expression?` has `Result<T,Failure>`. It does not force
the expression and does not turn failure into `Option`. `Option.unwrap()` fails
with a typed `UnwrapOnNone` payload at the call site. Indexing outside an
array's positions fails with a typed `IndexOutOfBounds` payload at the indexing
site (`lang.collection.array-index`). A missing map key fails with `MissingKey`
at the get site, and dynamic map addition with a duplicate key fails with
`DuplicateKey` at the operator site. `Result<T,E>` remains an ordinary domain
value for answers a caller is expected to branch on.

## Typed decoding

JSON, TOML, and other document formats decode directly into requested Vix
schemas through registered format primitives. The primitive parses a document
once; ordinary Vix code does not walk a stringly `Doc` on a hot path.

`#[decode { rename: "wire-name" }]` renames a field or variant.
`#[decode { untagged: true }]` selects an enum variant by disjoint input shape.
When shapes overlap, a variant may declare a required literal-field map with
`#[decode { require: %{ "workspace" => true } }]`. If zero or multiple variants
match, decoding returns a typed `DecodeError` containing a structured path,
expected shapes, and source range. A rendered message is presentation, not the
error's identity.

`Doc` remains available as an explicit dynamic-document type for genuinely
dynamic data. It is not the default decode surface.

## Effects, capabilities, and commands

Effects are demanded values implemented by registered runtime primitives. A
capability is a typed, identified executable closure supplied by the demand
root. Programs do not discover host tools or call `Target::host()`.

Capability values are ordinary parameters:

```vix
#[test]
fn check() where { rustc: Rustc, target: Target } -> Stream<Check> {
    let out = exec rustc`--target {target} -c {source}`;
    yield expect_present(out.tree / p"artifact.o");
}
```

There is no universal `Rustc::acquire`. Package/toolchain resolution may return
a `Rustc` capability, and a root may inject one, but acquisition is not ambient
language behavior. The exact capability identity and execution contract enter
the command recipe.

> r[lang.command.typed]
>
> A capability-tagged template produces `Command<A>`. Its versioned capability
> package owns four related schemas: the input command grammar, termination
> grammar, output protocol, and product protocol.

The command grammar parses argv roles and normalization. `Arg` is one argv
element made of typed fragments (`Text`, `Path`, `TreePath`, `Blob`, or a
capability-defined symbol); interpolation never stringifies a dependency.
Termination maps exits/signals to either an `A` constructor or a typed failure.
The output protocol frames stdout/stderr. The product protocol says when a
declared product becomes immutable and ready.

```vix
struct ExecOutcome<A> {
    answer: A,
    tree: Tree,
    stdout: ByteStream,
    stderr: ByteStream,
}
```

A conventional command uses `A = ()` and accepts exit zero. A grep-shaped
grammar can map zero to `Match` and one to `NoMatch`. Unmapped exits and signals
fail with raw termination data in a typed payload. No naked process-status
integer exists. `answer` resolves at termination; streams and product projections
may resolve earlier.

`fetch` requires Vix BLAKE3 content identity and returns `Blob`; an optional
upstream digest verifies transfer provenance. `extract` is a separate
`Blob -> Tree` demand. An unpinned network read is an observation primitive, not
an optional mode of `fetch`.

## Placement

`place { expression }` places a demand subgraph. It is not an effect and is not
coupled to `exec`. Captures crossing dispatch must already have identity without
evaluating the placed block. Results acquire identity remotely and cross back.
Codata and progressive projections cross as remote demand edges with credit,
cancellation, and replay.

Target, toolchain execution contract, and physical executor are distinct. Target
and selected toolchain identity are semantic inputs. The physical executor is an
unobservable policy choice constrained by the toolchain contract, capabilities,
grants, sovereignty, and trust.

## Tests and harness directives

Tests are ordinary demand roots returning `Stream<Check>`:

```vix
#[test]
fn arithmetic() -> Stream<Check> {
    yield expect_eq (2 + 2, 4);
}
```

> r[lang.diagnostic.must-use]
>
> `must_use` is a warning contract. When a value or operation marked `must_use`
> produces a result that is not consumed, the compiler warns; the program
> remains valid. Drivers and harnesses MAY promote that warning to an error.
> Collection `+`, collection `++`, `map.with`, and `Check` carry this marker.

Value checks are demanded during execution; trace checks are interpreted after
the run completes. A trace check receives a described expression wire and does
not consume its result. Trace-check constructors are harness intrinsics; there
is no user-visible `Demand<T>` or promise wrapper. The harness, not the
scheduler, partitions the phases.

Compile-fail and compile-warning expectations, fixture selection, second-source
variants, rerun mutations, chaos mode, and resource budgets are external harness
metadata. The ratchet currently carries that metadata in leading `//!`
directives; these are specified runner input, not hidden language statements.
An expected warning certificate does not promote that warning to a language
error. The `#[test]` attribute and test function are language syntax.

## Diagnostics

> r[lang.diagnostics.typed]
>
> Parser, name-resolution, type, lowering, and runtime diagnostics are typed
> records with a stable code, primary span, labeled related spans, structured
> payload, and optional fix. Rendered prose is not an API.

> r[lang.diagnostics.non-exhaustive-match]
>
> A non-exhaustive match emits `NonExhaustiveMatch`. Its primary span names the
> enclosing function declaration whose body is incomplete, a related label
> identifies the match expression, and its structured payload lists the missing
> unguarded constructors. Guarded arms do not contribute to exhaustiveness.

Lowering a legal program through a conservative path emits a reasoned
performance diagnostic; silently falling off a fast path is forbidden.

Diagnostics requested for multiple targets are keyed by target and diagnostic
identity. Arrival order and executor identity do not enter the result.
