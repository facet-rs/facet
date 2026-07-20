# Vix

Vix is a typed, demand-driven language whose evaluation is a build, an IDE
query, a solve, or any other requested value. Programs describe immutable
values and their dependencies; evaluation begins only when a holder of the
program demands a root.

The project has five authoritative surfaces:

- the [book](docs/content/_index.md), which explains the model;
- the [language specification](docs/content/spec/language.md), which defines
  source semantics;
- the [runtime specification](docs/content/spec/machine/_index.md), which
  defines demand, identity, memoization, scheduling, effects, placement, and
  observability;
- the [Rodin solver specification](../rodin/docs/content/_index.md), which
  defines dependency solving independently of runtime machinery;
- the [ratchet](tests/ratchet/README.md) and the real-program corpus, which are
  the executable acceptance surface.

Snark parses Vix and provides incremental syntax trees. Vix checks and lowers a
typed, demand-aware IR. Weavy executes the resulting islands through one
interpreter/JIT vocabulary. Rust primitives mediate effects. Vixen supplies
capabilities, placement policy, runners, and persistent storage over Vox.

Historical design notes are deliberately not authorities. A question that
changes source semantics belongs in the language specification; a question
about evaluation belongs in the runtime specification; a solver decision
belongs in the Rodin specification. Rungs and corpus programs may expose a
missing ruling, but they do not create a second language by accident.

Implementation follows the book's [ratchet-climbing
strategy](docs/content/implementation-strategy.md): one authoritative VIR and
runtime lane, specification work at the point it becomes load-bearing, a frozen
old evaluator, and deliberate parallelism only across stable ownership
boundaries.
