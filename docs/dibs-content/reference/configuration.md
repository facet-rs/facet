+++
title = "Configuration"
description = ".config/dibs.styx"
+++

dibs looks for `.config/dibs.styx` in the current directory, then walks up parent directories until it finds one.

## Example

```styx
@schema {id crate:dibs@1, cli dibs}

db {
    crate my-app-db
    // binary "target/debug/my-app-db"
}
```

## Fields

- `db.crate`: the Cargo package name that contains your schema + migrations
- `db.binary` (optional): path to a prebuilt binary to call instead of `cargo run -p ...`
