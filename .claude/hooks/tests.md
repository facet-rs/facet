Run tests and fix any failures. Uses `cargo nextest run` as required by this project.

Arguments: $ARGUMENTS (optional test filters, e.g., crate name or test name pattern)

1. Run tests:
   - If arguments provided: `cargo nextest run $ARGUMENTS`
   - If no arguments: `cargo nextest run`
2. If tests fail:
   - Parse the failure output to identify which tests failed
   - Create a todo list of failing tests
   - For each failure:
     - Read the test code and relevant implementation
     - Determine if the test or the implementation is wrong
     - Fix the appropriate code
     - Mark the todo as complete
   - Re-run the failing tests to verify fixes
3. Repeat until all tests pass

Remember: Fix the actual bug, don't just make the test pass. If a test is wrong, fix the test. If the implementation is wrong, fix the implementation.
