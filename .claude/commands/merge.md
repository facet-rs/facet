Watch PR checks and merge when they pass.

1. **Watch checks**: `gh pr checks --watch --fail-fast`
2. **If checks fail**:
   - Parse the failure output to understand what failed
   - Fix the issue (clippy, tests, build errors, etc.)
   - Commit and push the fix: run `/push`
   - Go back to step 1 (watch checks again)
3. **When checks pass**: `gh pr merge --squash`

The gh CLI handles checkout back to main after merge.
