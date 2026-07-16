+++
title = "Configuration"
description = ".config/dibs.styx"
+++

Dibs searches the current directory and its parents for
`.config/dibs.styx`. This file configures local schema-authoring tools; the
production application and migration modes do not need it.

```styx
@schema {id crate:dibs@1, cli dibs}

db {
    crate my-app-db
    endpoint "127.0.0.1:7764"
}
```

- `db.crate` names the library containing schema and migrations. Dibs uses it
  to place generated migration source and watch the source tree.
- `db.endpoint` is the application-owned Vox tooling endpoint. Start the
  application's Dibs mode before using the TUI or schema commands.

With Figue's `DIBS` environment prefix, the endpoint can be overridden as
`DIBS__DB__ENDPOINT` without editing the Styx file.
