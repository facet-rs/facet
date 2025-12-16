Fix a GitHub issue.

Arguments: $ARGUMENTS (required: GitHub issue number)

1. Fetch the issue details: `gh issue view $ARGUMENTS`
2. Summarize the issue: what's the problem, expected behavior, reproduction steps
3. Create a branch: `git checkout -b fix-$ARGUMENTS` (or a more descriptive name based on the issue)
4. Investigate the codebase to understand the problem
5. Create a todo list of steps to fix the issue
6. Implement the fix
7. Run `/check` and `/tests` to verify the fix doesn't break anything
8. When ready, offer to run `/pr` to create a pull request that references the issue

The PR should include "Fixes #$ARGUMENTS" in the description to auto-close the issue on merge.
