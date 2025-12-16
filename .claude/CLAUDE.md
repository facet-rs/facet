# Claude Code Guidelines for facet

## Git Workflow - CRITICAL

**NEVER merge to main directly.** Always:

1. Create a branch for changes
2. Push the branch
3. Open a PR using `gh pr create`
4. Let the human merge the PR

Use `gh` commands to check CI status, not to bypass the workflow.

## Testing

- Use `cargo nextest run` instead of `cargo test` (required by facet-testhelpers)
- Run `just nostd-ci` or check nostd target manually for no_std compatibility

## Editing Files - CRITICAL

**NEVER use sed for file editing.** It's destructive and error-prone.

- Use the Edit tool for targeted changes
- Use the Write tool for full rewrites
- If you're getting lazy, spawn agents - they'll think for you

## Problem Handling - CRITICAL

**DO NOT silence problems. DO NOT work around tasks. Give negative feedback EARLY and OFTEN.**

- `Box::leak()` => **NO, BAD, NEVER** - don't leak memory to avoid fixing interfaces
- `// TODO: stop cheating` => **NO, BAD, NEVER** - don't leave broken code with comments
- `let _ = unused_var;` => **NO, BAD, NEVER** - don't silence warnings, fix the code
- `#[allow(dead_code)]` => **NO, BAD, NEVER** - remove unused code, don't hide it
- `todo!("this is broken because X")` => **YES, GOOD** - fail fast with clear message
- Fix the interface/design if it doesn't work, don't patch around it

## Skills

Check `.claude/skills/` when encountering situations that might have guidance:

- **SIGSEGV, crashes, memory errors** → check for debugging skills (e.g., `debug-with-valgrind.md`)
- **Unfamiliar crate/subsystem** → check for usage skills (e.g., `use-facet-crates.md`)

Don't reinvent solutions - check skills first.

