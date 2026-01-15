# Phase 008: styx-cli (Command-Line Tool)

## Design Philosophy

- File-first: `styx <file>` is the common case, flags modify behavior
- No dangerous defaults: `--in-place` required for modification
- Composable: stdin/stdout via `-`, works with pipes
- `@` for subcommands: consistent with styx language (tags are special)

## Command Structure

```bash
styx <file> [options]           # file-first (common case)
styx @<cmd> [args] [options]    # subcommand via @ tag

# file can be a path or `-` for stdin
# output goes to stdout unless -o/--*-out specified
# if first arg starts with @, it's a subcommand
```

## Examples

```bash
styx config.styx                              # formatted styx to stdout
styx config.styx --in-place                   # fmt in place
styx config.styx --compact --in-place         # fmt compact in place
styx config.styx --json-out config.json       # convert to JSON file
styx config.styx -o out.styx                  # explicit output file
styx config.styx --validate                   # validate against declared schema
styx config.styx --validate --override-schema app.schema  # override declared schema
styx - < config.styx                          # read from stdin
styx config.styx -o -                         # explicit stdout (same as default)
cat config.styx | styx -                      # pipe through

# Subcommands via @
styx @diff old.styx new.styx                  # structural diff
styx @tree config.styx                        # debug parse tree
styx @lsp                                     # start language server
```

## Options (file-first mode)

| Option | Description |
|--------|-------------|
| `-o <file>` | Output file (styx format) |
| `--json-out <file>` | Output as JSON |
| `--in-place` | Modify input file in place |
| `--compact` | Single-line formatting |
| `--validate` | Validate against schema declared in document (via `@` key) |
| `--override-schema <file>` | Use this schema instead of declared one (requires --validate) |

## Schema Declaration (per spec r[schema.declaration])

Documents declare their schema via the `@` key at root:

```styx
// External reference
@ https://example.com/schemas/server.styx
server {host localhost, port 8080}

// Inline schema
@ { schema { @ @object{server @object{host @string}} } }
server {host localhost}
```

`--validate` looks for this `@` key and loads/parses accordingly.

## Safety

```bash
styx config.styx -o config.styx
# error: input and output are the same file
# hint: use --in-place to modify in place
```

## stdin/stdout

- `-` in file position = stdin
- `-o -` = stdout (explicit, same as default)
- Pipe-friendly: `cat foo.styx | styx - --json-out bar.json`

## Exit Codes

- 0 = success
- 1 = syntax error
- 2 = validation error
- 3 = I/O error

## Validation Output

When validation fails: diagnostics to stderr, reliable exit code, no other output.

## Technical Decisions

- **CLI parser**: facet-args (dogfooding)
- **No YAML initially**: Just styx ↔ json, can add --yaml-out later
- **Subcommand detection**: first arg starts with `@` → subcommand mode

## Deliverables

- `crates/styx-cli/src/main.rs` - Rewrite to file-first design

## Dependencies

```toml
[dependencies]
styx-parse = { path = "../styx-parse" }
styx-tree = { path = "../styx-tree" }
styx-format = { path = "../styx-format" }
styx-schema = { path = "../styx-schema" }
facet = "0.42"
facet-args = "0.42"
serde_json = "1"
```

## Verification

```bash
# Basic formatting
echo "foo 1 bar 2" | styx -
styx config.styx

# In-place
styx config.styx --in-place
styx config.styx --compact --in-place

# Conversion
styx config.styx --json-out config.json

# Validation
styx config.styx --validate
styx config.styx --validate --override-schema schema.styx

# Safety check
styx config.styx -o config.styx  # should error

# Pipes
cat config.styx | styx - --json-out - | jq .
```
