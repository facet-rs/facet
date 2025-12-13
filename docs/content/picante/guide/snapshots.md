+++
title = "Snapshots"
weight = 4
+++

picante supports **database snapshots** — point-in-time views of the database that can be queried independently from the original.

## Creating a snapshot

The `#[picante::db]` macro generates a `{DbName}Snapshot` struct alongside your database:

```rust
#[picante::db(inputs(Item), tracked(item_length))]
pub struct Database {}

// Creates both `Database` and `DatabaseSnapshot`
```

Create a snapshot with:

```rust
let snapshot = DatabaseSnapshot::from_database(&db).await;
```

## Snapshot semantics

Snapshots freeze the database state at creation time:

```rust
let db = Database::new();
let item = Item::new(&db, 1, "hello".into())?;

// Create snapshot
let snapshot = DatabaseSnapshot::from_database(&db).await;

// Modify the database
Item::new(&db, 1, "world".into())?;

// Database sees new value
assert_eq!(item.value(&db)?, "world");

// Snapshot still sees old value
assert_eq!(item.value(&snapshot)?, "hello");
```

## Running queries on snapshots

Snapshots implement all the same traits as the database, so queries work identically:

```rust
// Query on database
let len = item_length(&db, item).await?;

// Same query on snapshot
let len_snap = item_length(&snapshot, item).await?;
```

If a query was already cached when the snapshot was created, the snapshot uses that cached result. If not, the snapshot computes and caches it independently.

## How it works

Each ingredient type handles snapshots differently:

| Ingredient | Snapshot behavior |
|------------|-------------------|
| **Input** | O(1) structural sharing via `im::HashMap`. Inputs are frozen at snapshot time. |
| **Derived** | Deep-clones cached cells. Each snapshot has independent cache state. |
| **Interned** | Shares the same `Arc`. New interns after snapshot are visible (append-only). |

### Structural sharing

Input ingredients use [`im::HashMap`](https://docs.rs/im), an immutable persistent data structure. Cloning is O(1) because the clone shares structure with the original — only modified paths are copied (copy-on-write).

This means creating a snapshot is cheap even for large databases.

### Deep-cloned derived cells

Derived query caches are deep-cloned to ensure snapshot independence. When the database recomputes a query, it updates its cells — but the snapshot's cells remain unchanged.

The `from_database()` method is async because it needs to lock each cell to clone its state.

## Multiple snapshots

You can create multiple snapshots at different points in time:

```rust
let item = Item::new(&db, 1, "v1".into())?;
let snap1 = DatabaseSnapshot::from_database(&db).await;

Item::new(&db, 1, "v2".into())?;
let snap2 = DatabaseSnapshot::from_database(&db).await;

Item::new(&db, 1, "v3".into())?;

// Each sees their respective version
assert_eq!(item.value(&snap1)?, "v1");
assert_eq!(item.value(&snap2)?, "v2");
assert_eq!(item.value(&db)?, "v3");
```

## Use cases

- **Consistent reads**: Query multiple values against the same database state
- **Diffing**: Compare query results between two points in time
- **Parallelism**: Run queries on a snapshot while the main database continues to be modified
- **Debugging**: Capture database state for later inspection
