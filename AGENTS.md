# AI assistants guidelines for facet

This document captures code conventions for the facet project. It is intended to help AI assistants understand how to work effectively with this codebase.

## Read this first (repo orientation)

If you're about to change code, start here:

- `README.md`: high-level overview and crate map.
- `DEVELOP.md`: project workflow, MSRV notes, CI expectations, and why `cargo nextest` is preferred.
- `Justfile`: canonical local/CI commands (tests, clippy, docs, no_std checks, etc.).
- `CONTRIBUTING.md`: small but important rules (notably: generated `README.md` files).
- `.config/nextest.toml`: nextest profiles (notably `valgrind`, `valgrind-lean`, `lldb`).
- `.config/captain/config.kdl` + `hooks/`: pre-commit/pre-push automation (Captain) and hook installation for worktrees.
- `.claude/skills/`: task-specific “how to …” docs (benchmarking, profiling, valgrind, and dogfooding). Keep `AGENTS.md` high-level and link out instead of duplicating.

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

## Dogfooding & dependencies

- Prefer facet crates over serde/clap ecosystem crates whenever feasible; see `.claude/skills/use-facet-crates/SKILL.md` for the full mapping and exceptions.
- Prefer `unsynn` over `syn` for proc-macro parsing (this repo optimizes heavily for compile times).

## Serialization/format architecture (high-level)

- `facet-format` is the shared core for format crates; changes here tend to have wide blast radius across `facet-json`, `facet-yaml`, `facet-toml`, etc.
- JIT deserialization is implemented in `facet-format` behind the `jit` feature and uses Cranelift; see `.claude/skills/jit-overview/SKILL.md` for the mental model + entry points, and `.claude/skills/windbg-jit.md` for Windows crash debugging notes.

## Problem Handling - CRITICAL

**DO NOT silence problems. DO NOT work around tasks. Give negative feedback EARLY and OFTEN.**

- `Box::leak()` => **NO, BAD, NEVER** - don't leak memory to avoid fixing interfaces
- `// TODO: stop cheating` => **NO, BAD, NEVER** - don't leave broken code with comments
- `let _ = unused_var;` => **NO, BAD, NEVER** - don't silence warnings, fix the code
- `#[allow(dead_code)]` => **NO, BAD, NEVER** - remove unused code, don't hide it
- `todo!("this is broken because X")` => **YES, GOOD** - fail fast with clear message
- Fix the interface/design if it doesn't work, don't patch around it

## Fast path (common local commands)

Prefer `just …` recipes over inventing ad-hoc command lines:

- `just test -p <crate>` (wraps `cargo nextest run`)
- `just clippy` / `just clippy-all`
- `just doc-tests`
- `just miri` (runs a strict UB/provenance check suite; use for unsafe-boundary changes)
- `just valgrind …` (wraps `cargo nextest run --profile valgrind …`)
- `just gen` (regenerates docs like `README.md`)
- `just nostd` (catch accidental `std` usage in core crates)

## Generated files (don’t fight the generators)

- Many `README.md` files are generated. Edit `README.md.in` instead, then regenerate via `just gen` (or the relevant `facet-dev` workflow) — see `CONTRIBUTING.md` and `DEVELOP.md`.
- If you see prominent “AUTO-GENERATED / DO NOT EDIT” headers, treat them as authoritative and find the source-of-truth input (often under `tools/`, `xtask/`, or format-crate KDL/config files).

## Testing & verification (what “done” looks like)

- Prefer the `Justfile` as the source of truth for commands.
- Run targeted tests first (crate/package you changed), then widen to workspace checks when appropriate.
- For anything subtle (parsing, formatting, layout, invariants, performance-sensitive code), add regression tests and try to break your own change (we’d rather reject a patch than ship a footgun).

## Git hooks, CI hygiene, and GitHub workflows

- This repo uses [Captain](https://github.com/bearcove/captain) for pre-commit/pre-push tasks; configuration lives in `.config/captain/config.kdl`, and the hook scripts in `hooks/` invoke `captain` / `captain pre-push`.
- If you use git worktrees, install hooks into all worktrees via `hooks/install.sh`.
