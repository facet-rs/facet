Run `cargo check` and fix all compilation errors iteratively.

1. Run `cargo check --all-features --all-targets --message-format=short` to get a concise error list
2. Parse the errors and create a todo list of issues to fix
3. For each error:
   - Read the relevant file to understand context
   - Fix the error using the Edit tool
   - Mark the todo as complete
4. Re-run `cargo check` to verify fixes and catch any new errors
5. Repeat until the build succeeds

Focus on fixing actual errors, not warnings. If an error reveals a design problem, fix the design rather than working around it.
