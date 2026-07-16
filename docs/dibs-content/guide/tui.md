+++
title = "The Dibs TUI and CLI"
description = "Inspect and evolve a running application's database shape"
weight = 2
+++

Dibs provides a terminal interface and non-interactive commands for schema and
migration work. These tools talk to an application-owned Vox endpoint. They do
not discover or launch executables.

## Start the tooling endpoint

Run your application in its explicit Dibs mode:

```bash
MY_APP_MODE=dibs DIBS_LISTEN_ADDR=127.0.0.1:7764 cargo run -p my-app
```

The exact mode variable belongs to your application. `DIBS_LISTEN_ADDR` is the
convention used by the example application; pass the resulting address to
`dibs::serve`.

Your `.config/dibs.styx` points the CLI at that endpoint:

```styx
@schema {id crate:dibs@1, cli dibs}

db {
    crate my-app-db
    endpoint "127.0.0.1:7764"
}
```

## The TUI

Running `dibs` without a subcommand opens the terminal interface:

```bash
dibs
```

It can browse tables and constraints, inspect migration status, show migration
source, and compare the linked Rust schema with the database.

## CLI commands

```bash
dibs schema                        # Pretty-print the linked Rust schema
dibs schema --plain                # Plain text output
dibs schema --sql                  # SQL DDL rendering
dibs diff                          # Compare schema with the live database
dibs generate-from-diff <name>     # Generate a migration from that diff
dibs generate <name>               # Create a blank migration skeleton
dibs status                        # Show migration status
```

Production migrations should use the linked application's `migrate` mode and
`MigrationRunner` directly. `dibs migrate` remains useful as an interactive
development operation against the explicit tooling endpoint, but it is not a
deployment primitive.

Vox negotiates protocol compatibility from the service schema during the
handshake. Dibs package versions are logged as diagnostic metadata; they are
not required to be textually identical when the schemas are compatible.
