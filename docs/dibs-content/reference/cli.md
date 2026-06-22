+++
title = "CLI"
description = "Commands and environment variables"
+++

## Synopsis

```
dibs [OPTIONS] [COMMAND]
```

## Options

```
-V, --version    Show version information
-h, --help       Show help
```

## Environment variables

```
DATABASE_URL    Database connection URL (required for diff/migrate/status/generate-from-diff)
EDITOR          Editor used by the TUI to open files
```

## Commands

### (default) TUI

```bash
dibs
```

### `migrate`

Run pending migrations.

```bash
dibs migrate
```

### `status`

Show applied/pending migration status.

```bash
dibs status
```

### `diff`

Compare the Rust schema to the live database schema.

```bash
dibs diff
```

### `generate NAME`

Create an empty migration skeleton.

```bash
dibs generate add-users-table
```

### `generate-from-diff NAME`

Generate a migration from the current schema diff.

```bash
dibs generate-from-diff add-users-table
```

### `schema`

Browse/print the current Rust schema.

```bash
dibs schema
dibs schema --plain
dibs schema --sql
```

### `lsp-extension`

Run as an LSP extension (invoked by the Styx LSP).
