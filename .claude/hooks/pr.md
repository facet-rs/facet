Create a pull request for the current work, or update an existing one.

Arguments: $ARGUMENTS (optional: branch name and/or PR title)

1. Check `git status` to see what changes exist
2. Check current branch: `git branch --show-current`
3. **If on a non-main branch**, check for existing PR:
   - Run `gh pr view --json state,url,number` to see if a PR already exists
   - If PR exists and is open: go to **Update existing PR** flow below
   - If no PR exists: continue to step 4
4. **If on `main`**, create a new branch:
   - Use the argument as branch name if provided
   - Otherwise, generate a descriptive branch name from the changes
5. Stage and commit changes:
   - Run `git add -A`
   - Create a commit with a clear message describing the changes
6. Push the branch: `git push -u origin <branch>`
7. Create the PR: `gh pr create --fill` (or with custom title if provided)
8. Report the PR URL

## Update existing PR flow

When a PR already exists and is open:

1. First, commit and push any pending changes (run `/push`)
2. Analyze all commits in the PR:
   - Run `gh pr view --json commits` to get all commits
   - Run `git log main..HEAD --oneline` for a quick summary
   - Run `git diff main...HEAD --stat` to see all files changed
3. Generate an updated title and description:
   - Title should summarize the overall purpose of all changes
   - Description should include:
     - **Summary**: 2-3 sentences explaining what the PR accomplishes
     - **Changes**: Bullet points of key changes (group related commits)
     - **Testing**: Any relevant testing notes
4. Update the PR:
   - Run `gh pr edit <number> --title "New title" --body "New description"`
5. Report the updated PR URL

Remember: NEVER push directly to main. Always create a branch and PR.
