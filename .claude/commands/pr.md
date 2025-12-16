Create a pull request for the current work, or update an existing one.

Arguments: $ARGUMENTS (optional: branch name and/or PR title)

1. Check `git status` to see what changes exist
2. Check current branch: `git branch --show-current`
3. **If on a non-main branch**, check for existing PR:
   - Run `gh pr view --json state,url` to see if a PR already exists
   - If PR exists and is open: just run `/push` (commit and push to update the PR)
   - If no PR exists: continue to step 5
4. **If on `main`**, create a new branch:
   - Use the argument as branch name if provided
   - Otherwise, generate a descriptive branch name from the changes
5. Stage and commit changes:
   - Run `git add -A`
   - Create a commit with a clear message describing the changes
6. Push the branch: `git push -u origin <branch>`
7. Create the PR: `gh pr create --fill` (or with custom title if provided)
8. Report the PR URL

Remember: NEVER push directly to main. Always create a branch and PR.
