# AI Assistant Rules for This Repository

## Absolute Rules - NEVER VIOLATE

### Pre-push Hooks and Safety Checks
- **NEVER use `--no-verify`** unless explicitly instructed by the human
- **NEVER bypass pre-push hooks** or any safety mechanisms
- **ALWAYS fix the actual problems** that cause hook failures, don't work around them
- **RESPECT all git hooks, cargo checkers, linters, and other safety tools**

### When Pre-push Hooks Fail
1. **Fix the actual issues** that are causing failures
2. **Ask the human for guidance** if you're unsure how to fix something
3. **If stuck on a hook issue for more than 10 minutes**, ask the human for help
4. **Never use workarounds or bypasses**

### Examples of What NOT to Do
- ❌ `git push --no-verify`
- ❌ `git commit --no-verify`  
- ❌ Ignoring clippy warnings, formatting errors, or test failures
- ❌ Taking shortcuts that bypass quality checks

### Examples of What to Do Instead
- ✅ Fix clippy warnings: `cargo clippy --fix`
- ✅ Fix formatting: `cargo fmt`
- ✅ Fix failing tests: understand why they fail and fix the code
- ✅ Ask for help when stuck: "I'm getting this error from pre-push hooks, how should I handle it?"

## Repository-Specific Guidelines

### Testing
- Use `cargo nextest run` instead of `cargo test` (required by facet-testhelpers)
- Run `just nostd-ci` or check nostd target manually for no_std compatibility

### Code Quality
- Never leave `// TODO: stop cheating` comments - fix the actual issue
- Never use `let _ = unused_var;` - fix the warning by using the variable or removing it
- Never use `#[allow(dead_code)]` - remove unused code
- Never use `#[allow(warnings)]` - fix the warnings instead
- Never use `Box::leak()` - fix the interface instead of leaking memory

### Git Workflow
- **NEVER merge to main directly** - always create a branch, push, and open a PR
- Let the human merge PRs, don't use `gh pr merge`

## Enforcement

**This file is read FIRST before any git operations**. If I violate these rules, the human should immediately stop me and reference this file.

**Remember: Safety checks exist for a reason. Respect them.**