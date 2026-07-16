+++
title = "Running migrations in production"
description = "One image, one application binary, explicit execution modes"
weight = 5
+++

Production deployments should contain one application image and one application
binary. The binary links the `-db` library crate, so the schema and migration
inventory is exactly the code being deployed. An environment variable selects
whether that binary migrates the database or serves traffic.

There is no separately installed `dibs` CLI and no schema-service executable in
the production image.

## Application modes

The names are application-owned. This example uses `MY_APP_MODE`:

```rust
match mode {
    Mode::Migrate => {
        let (mut client, connection) =
            tokio_postgres::connect(&database_url, tokio_postgres::NoTls).await?;
        tokio::spawn(connection);
        dibs::MigrationRunner::new(&mut client).migrate().await?;
    }
    Mode::Serve => serve(database_url).await?,
    Mode::Dibs => {
        // Optional local-development tooling endpoint; not a production mode.
        dibs::serve("127.0.0.1:7764".parse()?).await?;
    }
}
```

Call a real symbol such as `my_app_db::ensure_linked()` before dispatching the
mode. This keeps the crate's inventory submissions linked into the application
binary. A `type_name` or `TypeId` reference is not sufficient.

## Build one image

```dockerfile
FROM rust:bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --locked --release -p my-app

FROM debian:bookworm-slim
COPY --from=builder /src/target/release/my-app /usr/local/bin/my-app
ENTRYPOINT ["/usr/local/bin/my-app"]
```

The migration and application containers must use the same immutable image
digest. This removes an entire compatibility problem: migration code, schema
inventory, and serving code cannot accidentally come from different builds.

## Kubernetes init container

Use the image once as an init container with migration mode, then again as the
application container with serve mode:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
spec:
  template:
    spec:
      initContainers:
        - name: migrate
          image: registry.example/my-app@sha256:0123456789abcdef
          env:
            - name: MY_APP_MODE
              value: migrate
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: my-app-postgres
                  key: url
      containers:
        - name: app
          image: registry.example/my-app@sha256:0123456789abcdef
          env:
            - name: MY_APP_MODE
              value: serve
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: my-app-postgres
                  key: url
```

Kubernetes does not start the application container unless the migration mode
exits successfully. Migration failures therefore remain distinguishable from
application startup failures without creating a second image or binary.

For migrations that should not gate a rollout directly, run the same image and
`MY_APP_MODE=migrate` in a `batch/v1 Job`. The executable contract stays the
same.
