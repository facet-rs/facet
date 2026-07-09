# The ratified surface — v2 (rounds 7–10)

**This supersedes v1 entirely. v1 described a different language.** Rounds 7–10
changed the calling convention, the collection kinds, the ordering model, the
type system, and the effect surface. If you are holding a v1 port, most of it is
now wrong.

When the surface below doesn't cover something, **DO NOT INVENT** — keep the old
shape and log it in your GAPS file. The gap list is the point of the exercise.

Reasoning: `/vix-design/spec-changelog` rounds 7–9,
`/vix-design/maps-all-the-way-down`, `/vix-design/where-a-build-runs`.

---

## 1. Calling convention

**Parentheses are grouping. They are never call syntax.** Application is
juxtaposition: a value followed by a value calls the first with the second.
`f (x)` is `f x`.

**At most one positional argument** — the subject, the thing acted on. Everything
else is named. (Lineage: Swift, Smalltalk.)

```vix
exec cc`-c input.c -o input.o` where { mounts: m }
range where { from: 0, to: n }
rows.sorted where { order: by_key(|r| r.weight) }
```

`f x y` parses (left-associative, `(f x) y`) and type-errors unless `f x` returns
something callable. At-most-one removes the ambiguity, not the grammar.

Precedence, tightest first:

```
.field / .method()   >   juxtaposition   >   unary -   >   binary ops   >   where { … }
```

**Unary minus is defended by rejection.** `abs -1` would parse as subtraction, so
a juxtaposed argument must be an **atom** — identifier, literal, string, path
literal, bracketed group. `-1` is not an atom: write `abs (-1)`. (Whitespace-
sensitive lexing was considered and rejected: invisible in a diff.)

**There is no `%` operator. No modulo.** Use `.rem()` / `.rem_euclid()` — the
`-7 % 3` sign convention was always an operator deciding silently. `%` is a
prefix collection sigil, nothing else.

**`partial` does not exist**, and isn't needed: named arguments are a record, so
pre-binding them is record construction; pre-binding the positional is a closure.

## 2. Named arguments are a record

`where` in a signature **declares** the named-argument record type. `where { }` at
a call site **constructs** it. `where` sits **left of `->`**: it names inputs, and
inputs belong left of the arrow.

```vix
fn exec(cmd: Command) where {
    mounts:   [Mount]  = [],
    writable: [Path]   = [],
} -> ExecOutcome

fn exec(cmd: Command) where ExecOpts -> ExecOutcome     // named option type
```

Inline `where { … }` declares a **structural** (anonymous) record type: this
function's one-off option set. `where ExecOpts` names a **nominal** one: a value —
storable, decodable, *spreadable* across a family of functions.

- **A parameter with a default must be passed by name.** Adding a defaulted
  parameter never breaks an existing call site.
- **`name: value` everywhere.** Never `name = value`. Attributes therefore take
  records: `#[test { budget_wall: 5s, budget_rss: 1GB }]`.
- **Punning**: bare names inside braces — `where { mounts, observer }`,
  `Guard { parent, dep, req }`. Not `:foo`: that spelling is reserved for symbols
  (the zoo's `:cc`, `:ar`, `:orb`).

Defaults dissolve the `X` / `X_target` twin-function family:
`target: Option<String> = None`.

## 3. Blocks, literals, lexical

**Braces are blocks.** (Parens-as-blocks is dead: the only argument for it was
that `{ }` was overloaded, and it no longer is.) `;` terminates `let` bindings; a
block's value is its final expression; **expression statements do not exist**.

- Unary minus. Array spread `[..a, ..b]`. Record spread `..base`.
- **Strings, bash lineage**: `"…"` **interpolates** with `${expr}`; `'…'` is
  **always literal**; `+` concatenation stays legal.
- **Backticks are COMMANDS**, not strings: a tagged template whose tag is a
  capability value. `rustc`-c {src} -o {out}`` — interpolation inside a command is
  `{expr}` (an argv element, typed; a path interpolated there is a dependency edge
  wearing an argv costume). See §8.
- **Suffix literals**: `5s` is a `Duration`, `1GB` a `ByteSize`. The suffix set is
  **closed and language-defined**; users never add suffixes.
- Paths: `p""` literals, `/` joins, String only as a joined segment.
- Booleans `||` `&&` `!`, `if`/`else` expressions, match guards.
- Destructuring: tuples/records in `let`, match arms, closure params.

## 4. Types: nominal and structural

`SchemaId` hashes the type's **name** alongside its shape
(`taxon/src/identity.rs:242`). Two kinds of type:

- **Nominal** — identity is name + shape. `Point{x,y}` ≠ `Vec2{x,y}`. This is what
  makes **newtypes** work.
- **Structural** — `struct { x: 1 }`, anonymous, identity is shape alone. Two
  identical anonymous structs in different modules are the same type.

**One keyword: `struct`.** The *name's presence* carries the distinction.

```vix
struct Point { x: Int, y: Int }   // declaration — nominal
Point  { x: 1, y: 2 }             // nominal literal, type-prefixed
struct { x: 1, y: 2 }             // anonymous literal — structural
```

**Spread. You may always drop a name; you must always earn one.**

```vix
struct { ..point }                // name erasure — always legal
Vec2 { ..struct { ..point } }     // legal: erase, then construct
Vec2 { ..point }                  // REJECTED — the fix-it suggests the line above
Point { ..point, x: 3 }           // same-type spread, legal
```

A structural spread into a nominal type may not carry **extra** fields the target
lacks: silently dropping data is the bug spread is otherwise perfect for hiding.

## 5. Order

**`<=>` is the structural comparison**: derived from a type's fields in
declaration order, total, and **not overridable**. There is no
`namespace T { fn <=> }`. `==`, `<`, `<=`, `>`, `>=` all derive from it.

**If a type's structural order is wrong, the type is wrong.** Reorder its fields,
or declare a field whose variant order carries the rule you meant. `Version` does
not define `<=>`:

```vix
enum PreIdent { Numeric(Int), Alpha(String) }         // semver: numeric < alphanumeric
enum PreTag   { Prerelease([PreIdent]), Release }     // semver: prerelease < release
struct Version { major: Int, minor: Int, patch: Int, pre: PreTag, build: Option<String> }
```

Its structural order **is** semver precedence, clause for clause.

**Anything else is an `Order<T>` you pass.** Orders are ordinary values.
`by_key(f)` ranks by the structural order of `f(x)`, ties broken by the structural
order of `x` — total **by construction**, consistent with `==` for free. A
comparison answering `Equal` for unequal values cannot be written.

Semver "equal precedence" is not value equality: `same_precedence(a, b)` is an
ordinary function.

## 6. Collections: one structure, four names

**`Map<K,V>` is the structure.** Rows are kept in key order — structural order on
`K`.

| name | keys | literal | is |
|---|---|---|---|
| `[T]` | positions, dense from `0` | `[a, b, c]` | a real type (density is an invariant) |
| `Set<T>` | the elements | `%[a, b]` | alias for `Map<T, ()>` |
| `Tree` | one path SEGMENT | `out.tree` from `exec` | recursive `Map<Name, TreeEntry>` — NOT `Map<Path,Blob>` |
| `Map<K,V>` | whatever you say | `%{k => v}` | itself |

`%` means *the keys are explicit*. Bare brackets mean *the keys are positions*.

**`Multiset<T>` DOES NOT EXIST.** Deleted. "A set that allows multiples because it
isn't hashed is an array." Every stored `Multiset` in the v1 ports was a `Set`
with a hand-maintained uniqueness invariant. If you truly want counts, that's
`Map<T, Int>`.

**`enumerate` and `Indexed<T>` do not exist.** The keys were always there.
**`fold_ascending` does not exist**: `stream.collect().values().fold(init, f)`.

**Arrays are not the default reflex.** An array is either **authored** (you wrote
the order, and the order is data — library link order, include search paths) or
**collected** (you named a rule that produced positions). *Positions have exactly
two provenances: you wrote them, or you sorted them.* Never the filesystem's,
never the scheduler's. Environment variables are a map. Directory listings are a
stream. Command-line arguments are a typed command.

## 7. Streams

**A stream is not ordered.** Its elements arrive as they become available, and
arrival order is a scheduling artifact, not a property of any value. A stream has
**recipe identity and no value identity**: its elements are ordinary demands
(memoized individually); the aggregate has no content hash until resolved. It cannot
be a map key, and cannot be sorted or compared.

**A stream CAN be a record field** (round 12 — `exec`'s output is one). The field's
semantic content is the value the stream drains to, so the record's identity exists
once the stream is done, while a reader may consume it long before. Same rule as
`r[machine.identity.streams-cross-island-edges]`: a field is an edge.

```
Stream<K, V>
    map(f: V -> U)                -> Stream<K, U>        key untouched
    filter(p: V -> Bool)          -> Stream<K, V>        nothing renumbers
    flat_map(f: V -> Stream<J,U>) -> Stream<(K,J), U>    keys compose into a path
    collect()                     -> Map<K, V>           the ONLY return type
Map<K,V>.stream()                 -> Stream<K, V>
Map<K,V>.values()                 -> [V]                 the one compaction
Set<T>.map(f)                     -> Map<T, U>           element -> image
```

- **`collect()` has exactly one return type.** No polymorphic return, no
  `FromIterator`, no turbofish. `.values()` reaches `[T]`, and it renumbers.
- **`collect()` fails on duplicate keys.** Since `map`/`filter` preserve keys and
  `flat_map` extends them, a duplicate is always attributable to one `rekey`.
- **Sorting an artifact by its content is unreachable**, not forbidden: a row's
  structural order compares the key first and never reaches the value.
- **`glob` returns a stream** — `readdir` order is not a position you justified.
- **Streams cross island edges as codata** (ruled). The edge's semantic content is
  the value the stream drains to; the incremental view is as-if.

**Generators do not yield in yield order.** `yield a; yield b;` may deliver `b`
first — what comes out is *availability* order. Top entry in the innovation-points
ledger. Write no code that depends on yield position.

## 8. Effects, capabilities, placement

- **`fail <payload>`** makes a demand have no answer. You supply a typed payload;
  the machine attaches the subject, the source span, and the demand chain. Failure
  poisons exactly what demanded it — 199 of 200 compiles keep their values.
- **`expr?`** (postfix, no space) is the ONLY way to observe a failure from inside a
  program: `T` -> `Result<T, Failure>`. It does NOT yield `Option` —
  `r[machine.error.option-not-channel]` says absence-as-failure erases the failure's
  address, and that erasure IS the governing incident. `expr?.ok()` if you mean it.
- **`Result<T,E>`** is for outcomes a caller branches on. `fail` is for absence.
  `o.unwrap()` is `match o { Some(v) => v, None => fail UnwrapOnNone {…} }`.
- **Tuples satisfy at-most-one-positional vacuously.** `f (a, b)` passes ONE
  argument: a struct whose fields are named by position. Right when the arguments
  together form a value (a pair, a span). Wrong when they have ROLES — a tuple has
  no room to name them, and the swap bug returns.
- **A test declares the capabilities it needs as parameters**; the harness, which is
  the demand root, supplies (or forges) them:
  `#[test] fn exec_echo(sh: Sh) -> Stream<Check>`. An undeclared capability is an
  unbound identifier, not a special error.
- **Commands are backtick tagged templates**, tagged by a capability VALUE:
  ```vix
  let rustc = Rustc::acquire spec;
  let out   = exec rustc`-c {src} -o out`;
  ```
  This is why a macro (`rustc!{ … }`) is wrong: a macro cannot refer to the
  capability you just bound. `name!{ … }` in the v1 corpus is the OLD shape.
- **`exec` is a boring effect**, like `fetch`. It is not an exception. It returns
  ```vix
  struct ExecOutcome { tree: Tree, stdout: Stream<Int,String>, stderr: Stream<Int,String> }
  ```
  **No exit status.** A nonzero exit is a `fail`; where a nonzero exit is a legitimate
  answer (`grep` -> 1), the command grammar declares it. `out.status == 0` does not exist.
  `stdout`/`stderr` are codata *fields*: consume lines while the process runs.
- **`exec` and `place` are decoupled and neither mentions the other.** There is no
  `observer:` parameter. To process a stream remotely, **place the surrounding block**.
  (The March observer closure is the *lowering* of that block, not a surface construct.)
- **A `Tree` is recursive**, mirroring the store's real entry model:
  ```vix
  Tree      = Map<Name, TreeEntry>              // Name is ONE segment
  TreeEntry = File { content: Blob, executable: Bool } | Dir (Tree) | Symlink { target: String }
  ```
  `Map<Path, Blob>` loses `mkdir -p`, every symlink, and the executable bit. Do not use it.
- **`fetch` returns a `Blob`, never a `Tree`.** `extract blob -> Tree` is a separate demand,
  and **an archive's digest is not its tree's digest**.
  **`blake3:` is REQUIRED** — vix's `ContentHash`, the name of the value. `sha256:` is
  OPTIONAL transfer provenance and never becomes an identity. **There is no SHA-only
  fetch**: computing the canonical blake3 is a lock-time act, so every `fetch` knows its
  final `Blob` identity before evaluation and crosses a `place` boundary by construction.
- **`fetch` is pinned.** `fetch(url) where { sha256 }` names a blob whose value
  identity is known *before* evaluation; the URL is a **provenance coordinate**,
  a hint about where bytes live. Materialization is cost-model: local store, peer,
  fleet store, and only then the origin.
- **Discovering a fact is a different primitive.** A read whose result identity is
  unknown until performed is an **observation**, not a fetch. Do not model it as
  `fetch` with an optional checksum. (Primitive TBD — log it.)
- **Capabilities are referenced by identity** (`primitive.md:9`).
  `Rustc::acquire(spec)` *names* a toolchain; nothing in-program evaluates, so it
  opens no binary. Acquiring **outside** a `place` is required, not merely
  allowed: the recipe pins one toolchain identity, and every node must materialize
  *that* one, or the same recipe yields different artifacts on different machines.
- **`place` is a strong boundary**, stronger than an island edge: an island edge
  carries a value between two computations in one evaluator; a `place` carries a
  subgraph of demands to a **different** evaluator.

  > **A value may cross a `place` boundary only if its identity is known without
  > evaluating it.** A pinned blob (the sha256 is in the source), a capability
  > identity, a literal, an observed input — all cross. `let x = expensive();`
  > does not: either compute it first, or draw `place` wider.

- **`Target::host()` is DEAD.** Three machines wore one word: *target* (semantic —
  changes the value), *host* (cost-model), *executor* (cost-model). The host is
  not a fact a program may read; it is an **input the demand root supplies**. The
  CLI defaults `--target` to the host. The recipe receives a `Target` value.
  An ambient read is an observation; an input is a pin.
- **A tree crosses as an identity plus a mount grant.** Nothing is copied. Only
  the blobs actually read ever move; every read *and every miss* is recorded
  (`r[machine.receipt.witness-reads]`, absence-is-an-observation). Changing a file
  nobody read invalidates nothing.

## 9. Methods, generators, tests

- Methods: `namespace Type { fn method(self, ...) ... }`, import-scoped
  `extend Type { }`. **No `fn <=>` override** (§5).
- Generators: ordinary functions returning `Stream<T>`, with `yield expr;` in the
  body. No `yields` keyword — the type is the whole declaration.
- Tests: `#[test] fn name() -> Stream<Check>`, yielding checks. Trace checks are
  ordinary calls (`never_demanded(expensive())`) — they work because passing an
  expression describes a value without demanding it.
- **`Check` is phase-partitioned.** A *value check* (`expect_eq`) is demanded
  during the run. A *trace check* (`never_demanded`, `overlapped`,
  `memo_hits_at_least`) is a claim about the completed run and is demanded after
  it. This is forced, not tidy: yield order is availability order, so there is no
  "last yield" to put a trace check in.
- `must_use` is a type marker: a bound value of a must-use type that is never used
  is a compile error.
- **`test NAME { } expecting { }` is DEAD**, and so is the `expecting` grammar.
- Runner directives are attributes: `#[test { budget_wall: 5s, budget_rss: 1GB }]`.
  `.reject.vix` files keep their `//!` headers (compile-fail can't live in-language).

## 10. NOT banked — do not use; keep old shapes and LOG

`.=` rebind sugar. `with` blocks. Pipes `|>`. Zero-arg `!`. Effect tags `#fs`.
`is` operator.
`#[key]` (dropped). `Keyed<K,V>` as a user-visible type (dropped — the key is a
stream parameter). A `Set` literal other than `%[…]`. Anything else the book
doesn't say.

When in doubt: old shape + a GAPS entry with a proposed form marked **PROPOSAL**.

## 11. v1 → v2, mechanically

| v1 | v2 |
|---|---|
| `f(a, b)` | `f a where { b: b }` — or take a record |
| `Multiset<T>` | `Set<T>` (unique) or `Stream<K,V>` (a pipeline) |
| `xs.values()` (array → multiset) | `xs.stream()` |
| `xs.enumerate()` | nothing — the key is already there |
| `ms.fold_ascending(init, f)` | `ms.collect().values().fold(init, f)` |
| `ms.sorted()` | `ms.collect().values()` |
| `sorted_by(cmp)` | `sorted where { order: by_key(f) }` |
| `namespace Version { fn <=> }` | declare `PreTag`; delete the override |
| `n % 2` | `n.rem(2)` |
| `{}` map literal | `%{}` |
| `test N { } expecting { }` | `#[test] fn N() -> Stream<Check>` + `yield` |
| `//! budget: 5s wall` | `#[test { budget_wall: 5s }]` |
| `[..ms.sorted(), x].values()` | `set.insert(x)` |
| `workspace.glob(p)` → `[String]` | → `Stream<Path, Path>` |
| `Target::host()` | a `target` parameter, supplied by the demand root |
| `Rustc::acquire(Target::host())` | `Rustc::acquire(target)` |
| `--stdout {p"cfg.stdout"}` | `out.stdout` — a codata field on `ExecOutcome` |
| `out.status == 0` | nothing: a nonzero exit is a `fail`; `exec cmd?` -> `Result` |
| `rustc! { … }` | `` rustc`…` `` — tagged by the capability value |
| `` `interp ${x}` `` | `"interp ${x}"` |
| `"literal"` | `'literal'` |
| anonymous record | `struct { x: 1 }` |

## 12. The GAPS file (the real deliverable)

Per port, `GAPS-<name>.md`: every awkwardness, every missing feature, every place
the surface was ambiguous about semantics you needed, every spot the port got
**longer or less clear** than the original, with `file:line` into your port and a
one-line proposed resolution marked **PROPOSAL** (Amos adjudicates). Log the wins:
measured line counts, old vs new.

Special attention, because these are the bets:

- Does `.values()` read as ceremony or as punctuation? It is on every pipeline.
- Does `where { … }` help or bury? Count the sites where you'd rather have a record.
- Is at-most-one-positional ever painful? Name the function and the call.
- Where does the absence of `Multiset` hurt, if anywhere?
- Does reading `out.stdout` as codata read naturally, or does every site drain it first?
