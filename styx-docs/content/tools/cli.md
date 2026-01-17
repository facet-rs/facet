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

Styx uses a **file-first** design:

```bash
styx <file> [options]         # Process a file
styx @<command> [args]        # Run a subcommand
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
| `--validate` | Validate against declared schema |
| `--override-schema <file>` | Use this schema instead of declared |

### Validation

Styx files can declare their schema with a `@` key:

```styx
@ ./schema.styx

host localhost
port 8080
```

Then validate:

```bash
styx config.styx --validate
```

Or override the schema:

```bash
styx config.styx --validate --override-schema ./other-schema.styx
```

## Subcommands

Subcommands use `@` prefix to distinguish from file paths.

### @tree

Show the parse tree:

```bash
styx @tree config.styx
styx @tree --format sexp config.styx   # S-expression format
styx @tree --format debug config.styx  # Debug format (default)
```

### @lsp

Start the language server (stdio transport):

```bash
styx @lsp
```

### @skill

Output Claude Code skill for AI assistance:

```bash
styx @skill
```

### @diff

Structural diff between two files (not yet implemented):

```bash
styx @diff old.styx new.styx
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
