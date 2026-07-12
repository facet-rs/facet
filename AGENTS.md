# Agent workflow for this repository

## Verify every delegated worktree base before starting an agent

Paseo may resolve a moving branch name from its published remote ref rather
than from the caller's newer local ref. A stale agent base wastes work and can
produce convincing but obsolete architecture conclusions.

For every delegated worktree:

1. Start from a clean integration worktree. Commit and push the current
   integration checkpoint before delegation.
2. Record the intended immutable base with `git rev-parse HEAD`, and verify the
   branch's upstream resolves to the same commit.
3. Create the Paseo worktree as a separate operation, using the exact commit
   SHA as its base. Never combine worktree creation and agent creation.
4. Before attaching an agent, run `git rev-parse HEAD` in the returned worktree
   and require it to equal the recorded base SHA exactly.
5. Only after that equality check succeeds, create the agent in the existing
   Paseo workspace. Include the expected base SHA in its prompt.
6. If the check fails, do not let an agent edit the worktree. Recreate the
   still-empty worktree from the exact SHA. Never repair a work-bearing branch
   with reset, rebase, checkout, or discarded changes.
7. Push every clean, gated integration checkpoint so the published branch does
   not lag behind the base future agents are expected to use.

Agents working in isolated worktrees commit forward and leave pushing to the
integration agent unless Amos explicitly assigns publication ownership.
