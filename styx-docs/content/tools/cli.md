+++
title = "CLI"
weight = 2
+++

Process, format, and validate Styx files from the command line.

## Installation

```bash
cargo install styx-cli
```

## Usage

Styx uses a simple disambiguation rule:

- If the first argument contains `.` or `/`, or is `-` → **file mode**
- Otherwise → **subcommand mode**

```bash
styx config.styx [options]    # File mode (has '.')
styx ./config [options]       # File mode (has '/')
styx -                        # Stdin (file mode)
styx lsp                      # Subcommand (bare word)
styx tree config.styx         # Subcommand with file arg
```

## File Mode

Process a Styx file — format, convert, or validate.

```bash
# Format and print to stdout
styx config.styx

# Format in place
styx config.styx --in-place

# Convert to JSON
styx config.styx --json-out -
styx config.styx --json-out output.json

# Single-line compact format
styx config.styx --compact

# Read from stdin
styx - < input.styx
cat input.styx | styx -
```

### Options

| Option | Description |
|--------|-------------|
| `-o <file>` | Output to file (styx format) |
| `--json-out <file>` | Output as JSON (`-` for stdout) |
| `--in-place` | Modify input file in place |
| `--compact` | Single-line formatting |
| `--validate` | Validate against declared schema (no output) |
| `--schema <file>` | Use this schema instead of declared |

Note: `--in-place` intentionally has no short form — destructive operations should require the full flag.

### Validation

Styx files can declare their schema with a `@schema` key:

```styx
@schema ./schema.styx

host localhost
port 8080
```

Then validate:

```bash
styx config.styx --validate
```

This validates and exits with code 0 (success) or 2 (validation error). No output is printed on success — use exit codes in scripts.

To validate and also output:

```bash
styx config.styx --validate -o -
```

Override the schema:

```bash
styx config.styx --validate --schema ./other-schema.styx
```

## Subcommands

### tree

Show the parse tree:

```bash
styx tree config.styx
styx tree --format sexp config.styx   # S-expression format
styx tree --format debug config.styx  # Debug format (default)
```

### cst

Show the concrete syntax tree (CST) structure:

```bash
styx cst config.styx
```

### lsp

Start the language server (stdio transport):

```bash
styx lsp
```

### extract

Extract embedded schemas from a binary:

```bash
styx extract ./my-binary
```

### diff

Compare a local schema against a published version:

```bash
styx diff schema.styx --crate my-schema
styx diff schema.styx --crate my-schema --baseline 0.1.0
```

### package

Generate a publishable crate from a schema:

```bash
styx package schema.styx --name my-schema --version 0.1.0
styx package schema.styx --name my-schema --version 0.1.0 --output ./out
```

### publish

Publish a schema to staging.crates.io:

```bash
styx publish schema.styx
styx publish schema.styx -y  # Skip confirmation
```

Requires `STYX_STAGING_TOKEN` environment variable.

### cache

Manage the schema cache:

```bash
styx cache              # Show cache info
styx cache --open       # Open cache directory
styx cache --clear      # Clear all cached schemas
```

### skill

Output Claude Code skill for AI assistance:

```bash
styx skill
```

### completions

Generate shell completions. Add one of these to your shell config:

**Bash** (`~/.bashrc`):
```bash
eval "$(styx completions bash)"
```

**Zsh** (`~/.zshrc`):
```zsh
eval "$(styx completions zsh)"
```

**Fish** (`~/.config/fish/config.fish`):
```fish
styx completions fish | source
```

Or write to a file for faster shell startup:

```bash
styx completions bash > ~/.local/share/bash-completion/completions/styx
styx completions zsh > ~/.zfunc/_styx
styx completions fish > ~/.config/fish/completions/styx.fish
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Syntax error |
| 2 | Validation error |
| 3 | I/O error |

## CI Integration

```yaml
# GitHub Actions
- name: Validate config
  run: styx config.styx --validate

# Check formatting
- name: Check format
  run: |
    styx config.styx > /tmp/formatted.styx
    diff config.styx /tmp/formatted.styx
```
