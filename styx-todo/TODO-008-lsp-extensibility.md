# TODO-008: LSP Extensibility & Smart Completions

## Status
TODO

## Description
Make the LSP extensible and smarter about context-aware completions.

## Ideas

### External validator hooks
Allow the LSP to call into external binaries to determine what values are valid *right now*. For example:
- A schema could declare that a field accepts "one of the running Docker containers"
- The LSP shells out to `docker ps` (or a user-specified command) to get the list
- Autocomplete offers actual container names

This makes configs that reference dynamic resources (containers, k8s pods, database names, etc.) much nicer to write.

### Path autocompletion
If the schema declares a field must be a path (and optionally specifies it's relative to some base), the LSP can:
- Autocomplete actual file/directory names from the filesystem
- Validate that the path exists (as a warning or error depending on schema)
- Handle `~` expansion, env vars, etc.

## Security

We can't just run arbitrary binaries without consent. The UX flow:

1. **Diagnostic appears**: "This field could offer smarter completions if we run `docker ps --format '{{.Names}}'`. Allow?"
2. **Two code actions**:
   - **"Allow (add to whitelist)"** — adds the command to the whitelist, never asks again
   - **"Never ask for this command"** — adds to a deny list, suppresses the diagnostic

The whitelist/denylist lives in `styx.config` (or `~/.config/styx/config.styx` for global). Something like:

```styx
lsp {
  external-commands {
    allow (
      "docker ps --format '{{.Names}}'"
      "kubectl get pods -o name"
    )
    deny (
      "rm -rf /"  // nice try
    )
  }
}
```

This way users explicitly opt in to each command pattern, and the decision persists across sessions.

## Notes
Both of these make the "offensively nice tooling" promise real — config that knows about your actual environment, not just abstract types.
