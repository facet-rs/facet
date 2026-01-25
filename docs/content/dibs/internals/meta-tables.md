+++
title = "Meta tables"
description = "How dibs tracks schema provenance"
weight = 2
+++

dibs maintains metadata tables (prefixed with `__dibs_`) to track schema provenance and history.

## Philosophy

- **Source traceability**: Every table and column knows where it came from in Rust code
- **Migration history**: Track which migration created/modified each schema element
- **Rich context**: Doc comments, types, constraints - all preserved
- **<abbr title="Text User Interface">TUI</abbr> integration**: Click a column, see its source file, jump to editor

## Tables

### `__dibs_tables`

Metadata about tables defined in code.

```sql
CREATE TABLE __dibs_tables (
    table_name TEXT PRIMARY KEY,
    
    -- Source location (from Rust proc macro)
    source_file TEXT NOT NULL,
    source_line INTEGER NOT NULL,
    source_column INTEGER,
    
    -- Documentation
    doc_comment TEXT,  -- /// comments from Rust
    
    -- History
    created_by_migration TEXT,
    modified_by_migration TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    modified_at TIMESTAMPTZ DEFAULT now()
);
```

### `__dibs_columns`

Metadata about columns.

```sql
CREATE TABLE __dibs_columns (
    table_name TEXT NOT NULL,
    column_name TEXT NOT NULL,
    
    -- Source location
    source_file TEXT NOT NULL,
    source_line INTEGER NOT NULL,
    source_column INTEGER,
    
    -- Documentation
    doc_comment TEXT,
    
    -- Type info (for reference, authoritative source is code)
    rust_type TEXT,
    sql_type TEXT,
    is_nullable BOOLEAN,
    default_value TEXT,
    
    -- Constraints
    is_primary_key BOOLEAN DEFAULT FALSE,
    is_unique BOOLEAN DEFAULT FALSE,
    is_indexed BOOLEAN DEFAULT FALSE,
    
    -- Foreign key (if any)
    fk_references_table TEXT,
    fk_references_column TEXT,
    
    -- History
    created_by_migration TEXT,
    modified_by_migration TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    modified_at TIMESTAMPTZ DEFAULT now(),
    
    PRIMARY KEY (table_name, column_name),
    FOREIGN KEY (table_name) REFERENCES __dibs_tables(table_name)
);
```

### `__dibs_indices`

Metadata about indices.

```sql
CREATE TABLE __dibs_indices (
    table_name TEXT NOT NULL,
    index_name TEXT NOT NULL,
    
    -- Source location
    source_file TEXT,
    source_line INTEGER,
    source_column INTEGER,
    
    -- Index info
    columns TEXT[] NOT NULL,  -- ordered list of columns
    is_unique BOOLEAN DEFAULT FALSE,
    
    -- History
    created_by_migration TEXT,
    modified_by_migration TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    modified_at TIMESTAMPTZ DEFAULT now(),
    
    PRIMARY KEY (table_name, index_name),
    FOREIGN KEY (table_name) REFERENCES __dibs_tables(table_name)
);
```

### `__dibs_migrations`

Applied migrations history.

```sql
CREATE TABLE __dibs_migrations (
    name TEXT PRIMARY KEY,           -- "20260117234801_add-users-table"
    applied_at TIMESTAMPTZ DEFAULT now(),
    checksum TEXT,                   -- SHA256 of migration content
    execution_time_ms INTEGER,
    
    -- What changed
    tables_created TEXT[],
    tables_modified TEXT[],
    tables_dropped TEXT[],
    
    -- Source info (if generated from TUI)
    generated_from_diff BOOLEAN DEFAULT FALSE
);
```

## How it works

### Schema collection (compile time)

The proc macro captures source location via `Span`:

```rust
#[derive(Facet)]
#[facet(dibs::table = "users")]
/// User accounts in the system
struct User {
    #[facet(dibs::pk)]
    id: i64,
    
    /// User's email address, must be unique
    #[facet(dibs::unique)]
    email: String,
}
```

### Migration generation

When you generate a migration, dibs includes meta table updates alongside schema changes:

```sql
-- Schema changes
ALTER TABLE users ADD COLUMN email_verified BOOLEAN NOT NULL DEFAULT false;

-- Meta updates
INSERT INTO __dibs_columns (table_name, column_name, source_file, source_line, ...)
VALUES ('users', 'email_verified', 'src/models/user.rs', 15, ...)
ON CONFLICT (table_name, column_name) DO UPDATE SET
    modified_by_migration = '20260117234801_add-email-verification',
    modified_at = now();
```

### TUI display

```
┌─ users ───────────────────────────────────────────────────────┐
│ src/models/user.rs:4                                          │
│ /// User accounts in the system                               │
│                                                               │
│ Created: 20260110_initial-schema                              │
│ Modified: 20260117_add-email-verification                     │
├───────────────────────────────────────────────────────────────┤
│ Columns:                                                      │
│                                                               │
│ ▸ id BIGINT PRIMARY KEY                     :7   initial      │
│   email TEXT UNIQUE NOT NULL                :11  initial      │
│   email_verified BOOLEAN NOT NULL           :15  +add-email.. │
│   created_at TIMESTAMPTZ                    :18  initial      │
└───────────────────────────────────────────────────────────────┘

Press Enter on a column for details, 'o' to open in editor
```

Terminals supporting OSC 8 hyperlinks get clickable source locations.

## Querying meta tables

```sql
-- Find all columns added by a specific migration
SELECT table_name, column_name, doc_comment
FROM __dibs_columns
WHERE created_by_migration = '20260117_add-email-verification';

-- Find columns without documentation
SELECT table_name, column_name, source_file, source_line
FROM __dibs_columns
WHERE doc_comment IS NULL;

-- Schema history for a table
SELECT 
    m.name as migration,
    m.applied_at,
    m.tables_created,
    m.tables_modified
FROM __dibs_migrations m
WHERE 'users' = ANY(m.tables_created) 
   OR 'users' = ANY(m.tables_modified)
ORDER BY m.applied_at;
```
