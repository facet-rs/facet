Commit and push the current changes with a good commit message.

1. **Analyze changes**:
   - Run `git status` to see staged and unstaged files
   - Run `git diff --cached` to see staged changes
   - Run `git diff` to see unstaged changes
   - If nothing is staged, run `git add -A` to stage everything
2. **Craft a commit message**:
   - Summarize what changed and why
   - Use conventional format if appropriate (feat:, fix:, refactor:, etc.)
   - Keep the first line under 72 characters
3. **Commit**: `git commit -m "..."`
4. **If pre-commit hook fails**:
   - Read the hook output to understand what failed
   - Fix the issues (formatting, linting, etc.)
   - Stage the fixes and retry the commit
   - NEVER use `--no-verify`
5. **Push**: `git push`
6. **If pre-push hook fails**:
   - If it says to rebase: `git fetch origin main && git rebase origin/main`
   - Fix any other issues the hook reports
   - Retry the push
   - NEVER use `--no-verify`

Repeat until the push succeeds. Hooks exist for a reason — fix problems, don't skip them.
