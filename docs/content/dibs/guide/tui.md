+++
title = "The dibs TUI/CLI"
description = "Tour of the terminal interface and CLI commands"
weight = 2
+++

dibs provides both a **<abbr title="Text User Interface">TUI</abbr>** (terminal UI) and a **<abbr title="Command Line Interface">CLI</abbr>** for working with your schema.

## The TUI

Running `dibs` without arguments launches the interactive terminal UI:

```bash
dibs
```

The TUI lets you:

- Browse your schema (tables, columns, constraints)
- View migration status and detect drift
- Inspect individual migrations
- See what would change if you ran `dibs diff`

**Screenshots coming soon.**

## CLI Commands

### Schema inspection

```bash
dibs schema           # Pretty-print the schema
dibs schema --plain   # Plain text output
dibs schema --sql     # Show as SQL DDL
```

### Migrations

```bash
dibs diff                          # Compare schema against live database
dibs generate-from-diff <name>    # Generate a migration from the diff
dibs generate <name>               # Create a blank migration skeleton
dibs migrate                       # Apply pending migrations
dibs status                        # Show migration status
```

### How the CLI spawns your schema

When you run CLI commands, dibs:

1. Reads `.config/dibs.styx` to find your db crate
2. Spawns `cargo run -p my-app-db --` (or uses a prebuilt binary)
3. The child process connects back to the CLI via a local TCP port
4. The CLI sends schema requests and the db binary responds

This keeps the dibs CLI lightweight — your schema stays in your own crate.
