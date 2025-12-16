Run `cargo clippy` and fix all warnings and errors iteratively.

1. Run `cargo clippy --all-features --all-targets --message-format=short -- -D warnings` to treat warnings as errors
2. Parse the issues and create a todo list
3. For each clippy lint:
   - Read the relevant file to understand context
   - Apply the suggested fix or a better alternative
   - Mark the todo as complete
4. Re-run clippy to verify fixes and catch any new issues
5. Repeat until clippy passes cleanly

Prefer idiomatic Rust solutions. If a clippy suggestion would make code worse, use `#[allow(clippy::lint_name)]` with a comment explaining why - but this should be rare.
