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

## Notes
Both of these make the "offensively nice tooling" promise real — config that knows about your actual environment, not just abstract types.
