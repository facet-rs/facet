# The ratified surface — port authority

Ports in this directory use EXACTLY the language the book defines plus the
round-6 syntax rulings. This file is the porter's law. When the surface
below doesn't cover something, DO NOT INVENT — keep the old corpus shape
and log it in your GAPS file; the gap list is the point of the exercise.

## Ratified — use freely
- Booleans: `||`, `&&`, `!`, `if`/`else` expressions, match guards.
- Destructuring: tuples/records in `let`, match arms, closure params.
- Unary minus. Array spread `[..a, ..b]`. Record spread `..base`.
- Map literals `%{ k => v }`, empty `%{}`. Sets per book.
- String interpolation: backtick templates with `${expr}`; plain `""` is
  always literal. Concatenation `+` stays legal.
- Collections per the book chapter (/vix/std/collections): arrays are
  structs (field-wise `map`, `enumerate`, bare `fold` in field order,
  `any/all/contains`, `split_last`, `values()`); multisets (`filter`,
  `filter_map`, `flat_map`, `fold_ascending` — NO bare fold on multisets,
  `find_min/find_max`, `take_min/take_max`, `sorted/sorted_by`, `len`).
  `Indexed<T> = (Int, T)`; `enumerate` for carried positions. NO `pop`.
- Paths: `p""` literals, `/` joins, String only as a joined segment.
- Methods: `namespace Type { fn method(self, ...) ... }` + import-scoped
  `extend Type { }` — including `fn <=>(self, other) -> Ordering`.
- Generators: ordinary fns returning `Stream<T>`, `yield expr;` in the
  body. Streams are codata (head, rest).
- Tests: `#[test] fn name() -> Stream<Check>`, yielding checks
  (`assert_eq`, `expect`-family per /vix/testing — being rewritten to
  this shape); trace checks are ordinary calls (`never_demanded(expr)`).
- Typed decode: `json_decode`/`toml_decode` onto structs; `Option<T>`
  fields for absent; string-or-table enums (see ratchet 062–066).
- Attributes exist: `#[test(...)]`, decode annotations (shape per book).
- Blocks: `;` terminates bindings (`let`); block value = final
  expression; NO expression statements (generator `yield` lines are the
  one non-binding line form).

## NOT banked — do not use, keep old shapes and LOG
- `.=` rebind sugar (open). `with` blocks. Pipes `|>`. `partial`.
  Zero-arg `!`. Effect tags `#fs`. `fail` keyword. `is` operator.
  Parens-as-blocks (under discussion — use braces).
- Anything else the book doesn't say. When in doubt: old shape + GAPS
  entry with a proposed form marked PROPOSAL.

## The GAPS file (the real deliverable)
Per port, `GAPS-<name>.md`: every awkwardness, every missing feature,
every place the book was ambiguous about semantics you needed, every
spot the port got LONGER or less clear than the original, with file:line
into your port and a one-line proposed resolution (marked PROPOSAL —
Amos adjudicates). Also log the wins: measured line counts old vs new.
