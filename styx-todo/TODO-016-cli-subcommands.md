# CLI: Subcommand disambiguation via path heuristic

GitHub issue: https://github.com/bearcove/styx/issues/7

## Decision

Replace the `@` prefix for subcommands with a simple heuristic:

**If the first argument contains `.` or `/` → file mode. Otherwise → subcommand.**

## Rationale

The `@` prefix was clever (mirrors Styx's tag syntax) but:
- Unfamiliar to users
- Shell completion requires special handling
- Inconsistent with every other CLI tool

The new heuristic works because:
- Config files have extensions (`.styx`, `.json`, etc.) or paths (`./`, `../`, `/`)
- Subcommands are bare words (`lsp`, `tree`, `publish`)
- Edge case of a file named `lsp` is solved by `./lsp`
- Stdin remains `-` (special-cased)

## Examples

```bash
# File mode (contains . or /)
styx config.styx                    # format to stdout
styx ./config                       # file with path
styx ../configs/app.styx            # relative path
styx /etc/myapp/config.styx         # absolute path
styx -                              # stdin (special case)

# Subcommand mode (bare word)
styx lsp                            # start language server
styx tree config.styx               # show parse tree
styx publish schema.styx            # publish to registry
styx cache --clear                  # cache management
```

## File mode options

```bash
styx config.styx                    # format to stdout
styx config.styx --in-place         # format in place (NO short form - destructive)
styx config.styx -o out.styx        # output to file
styx config.styx -o out.json        # output as JSON (infer from extension)
styx config.styx -o -               # explicit stdout
styx config.styx --json             # JSON to stdout
styx config.styx --compact          # compact/single-line format
styx config.styx --validate         # validate against @schema directive
styx config.styx --schema x.styx    # validate against specific schema
```

Note: `--in-place` intentionally has no `-i` short form. Destructive operations should require typing the full flag.

## Subcommands

```bash
styx lsp                            # start language server (stdio)
styx tree [--format sexp|debug] <file>   # show parse tree
styx cst <file>                     # show CST structure
styx extract <binary>               # extract embedded schemas
styx diff <schema> --crate <name>   # compare against published
styx package <schema> --name <n> --version <v>  # generate crate
styx publish <schema> [-y]          # publish to staging
styx cache [--open|--clear]         # cache management
styx skill                          # output Claude Code skill
```

## Implementation

1. Change `main.rs` dispatch logic:
   - Remove `@` prefix handling
   - Check if `args[0]` contains `.` or `/` or is `-` → file mode
   - Otherwise → subcommand mode

2. Update help text to reflect new syntax

3. Update any documentation/examples

## Migration

Users with scripts using `@` prefix will need to update:
```bash
# Old
styx @lsp
styx @tree config.styx

# New
styx lsp
styx tree config.styx
```
