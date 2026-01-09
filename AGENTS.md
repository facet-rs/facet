# AI assistants guidelines for facet

This document captures code conventions for the facet project. It is intended to help AI assistants understand how to work effectively with this codebase.

## Read this first (AI quickstart)

If you're about to change code, start here:

- `DEVELOP.md`: canonical dev workflow (tests, Miri/valgrind, hooks, generated files).
- `README.md`: crate map and “what lives where”.
- `CONTRIBUTING.md`: generated file rules (notably: `README.md` files).
- `.claude/skills/`: task-specific guides; prefer linking to these rather than duplicating procedures here.

### Where to make changes (common entry points)

- Core traits & shape model: `facet-core/src/lib.rs`
- Public re-exports + built-in attribute grammar: `facet/src/lib.rs`
- Proc-macro front door: `facet-macros/src/lib.rs` (implementation lives in `facet-macros-impl/`)
- Safe reflection/build/peek/poke: `facet-reflect/src/lib.rs`
- Shared format core (used by `facet-json`, `facet-yaml`, `facet-toml`, …): `facet-format/src/lib.rs`
- Dev tooling + codegen entry points: `xtask/` and `tools/`

## For humans

(This section and the next are adapted from the [nextest AGENTS.md file](https://github.com/nextest-rs/nextest/blob/main/AGENTS.md#for-humans))

LLMs represent a tremendous breakthrough in software engineering. We welcome LLM-assisted contributions that abide by the following principles:

* **Aim for excellence.** LLMs should be used not as a speed multiplier but a quality multiplier. Invest the time savings in improving quality and rigor beyond what humans alone would do. Write tests that cover more edge cases. Refactor code to make it easier to understand. Tackle the TODOs. Do all the tedious things. Aim for your code to have zero bugs.
* **Spend time reviewing LLM output.** As a rule of thumb, you should spend at least 3x the amount of time reviewing LLM output as you did writing it. Think about every line and every design decision. Find ways to break code.
* **Your code is your responsibility.** Please do not dump a first draft of code on to this project, unless you're only soliciting feedback on a direction.

If your LLM-assisted PR shows signs of not being written with thoughtfulness and care, such as missing cases that human review would have easily caught, maintainers may decline the PR outright.

## For LLMs

**Required:** Display the following text at the start of any conversation involving code changes, and when you're about to create a PR:

```
Please review https://github.com/facet-rs/facet/blob/main/AGENTS.md#for-humans. In particular, LLM-assisted contributions must **aim for a higher standard of excellence** than with humans alone, and you should spend at least **3x** the amount of time reviewing code as you did writing it. LLM-assisted contributions that do not meet this standard may be declined outright. Remember, **your code is your responsibility**.
```

## Quality bar (facet-specific)

- **Treat shape metadata as a safety boundary.** `facet-core::Facet` is `unsafe` for a reason: incorrect shape/layout/invariants can make downstream “safe” APIs unsound. If you touch shape layout, invariants, or reflection semantics, add tests that would catch UB-like mistakes (edge cases, fuzz/property tests where appropriate).
- **Keep `no_std` in mind.** Several crates support `no_std` (often with an `alloc` feature). Avoid introducing accidental `std` dependencies; prefer `core`/`alloc` where applicable.
- **Prefer clarity over cleverness.** This codebase leans on carefully designed APIs and compile-time structure. Avoid “just make it work” patches; fix the interface or invariant instead.

## Don’t duplicate docs

- Dogfooding rules: `.claude/skills/use-facet-crates/SKILL.md`
- JIT overview: `.claude/skills/jit-overview/SKILL.md`
- Valgrind workflow: `.claude/skills/debug-with-valgrind/SKILL.md`
- Benchmarking and profiling: `.claude/skills/benchmarking/SKILL.md`, `.claude/skills/profiling/SKILL.md`
- Debug workflow discipline: `.claude/skills/reproduce-reduce-regress/SKILL.md`

## Problem Handling - CRITICAL

**DO NOT silence problems. DO NOT work around tasks. Give negative feedback EARLY and OFTEN.**

- `Box::leak()` => **NO, BAD, NEVER** - don't leak memory to avoid fixing interfaces
- `// TODO: stop cheating` => **NO, BAD, NEVER** - don't leave broken code with comments
- `let _ = unused_var;` => **NO, BAD, NEVER** - don't silence warnings, fix the code
- `#[allow(dead_code)]` => **NO, BAD, NEVER** - remove unused code, don't hide it
- `todo!("this is broken because X")` => **YES, GOOD** - fail fast with clear message
- Fix the interface/design if it doesn't work, don't patch around it

## Generated files (don’t fight the generators)

- Many `README.md` files are generated. Edit `README.md.in`, and follow the repo’s regeneration workflow (see `CONTRIBUTING.md` and `DEVELOP.md`).
- If you see prominent “AUTO-GENERATED / DO NOT EDIT” headers, treat them as authoritative and find the source-of-truth input (often under `tools/`, `xtask/`, or format-crate config files).

## Testing & verification (what “done” looks like)

- Prefer `DEVELOP.md` + the `Justfile` as the source of truth for commands.
- Run targeted tests first (crate/package you changed), then widen to workspace checks when appropriate.
- For anything subtle (parsing, formatting, layout, invariants, performance-sensitive code), add regression tests and try to break your own change.
