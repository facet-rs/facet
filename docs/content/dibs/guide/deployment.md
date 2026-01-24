+++
title = "Running migrations in prod"
description = "Deploy migrations to Kubernetes"
weight = 5
+++

In production, you'll want to run migrations as part of your deployment process. Here's how to do it with Kubernetes.

## The deployment model

dibs needs two things to run migrations:
1. The `dibs` CLI binary
2. Your `-db` binary (the schema service)

The `dibs` CLI spawns your `-db` binary to read the schema, then applies migrations to the database.

## Build the binaries

In your CI pipeline, build both binaries:

```dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY . .

# Build the db service binary
RUN cargo build --release -p my-app-db

# Install dibs CLI
RUN cargo install --git https://github.com/bearcove/dibs dibs-cli
```

## Option A: Init container

Run migrations in an init container before your app starts:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
spec:
  template:
    spec:
      initContainers:
        - name: migrations
          image: my-app-migrations:latest
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: db-credentials
                  key: url
          command:
            - /usr/local/bin/dibs
            - migrate
          volumeMounts:
            - name: config
              mountPath: /.config
      containers:
        - name: app
          image: my-app:latest
          # ... your app config
      volumes:
        - name: config
          configMap:
            name: dibs-config
```

Create the ConfigMap for `.config/dibs.styx`:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: dibs-config
data:
  dibs.styx: |
    @schema {id crate:dibs@1, cli dibs}

    db {
        crate my-app-db
        binary "/app/my-app-db"
    }
```

## Option B: Job

Run migrations as a separate Job:

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: db-migration-{{ .Release.Revision }}
spec:
  template:
    spec:
      restartPolicy: Never
      containers:
        - name: migrate
          image: my-app-migrations:latest
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: db-credentials
                  key: url
          command:
            - /usr/local/bin/dibs
            - migrate
          volumeMounts:
            - name: config
              mountPath: /.config
      volumes:
        - name: config
          configMap:
            name: dibs-config
```

## Container image

Your migration container needs:
- `/usr/local/bin/dibs` (the CLI)
- `/app/my-app-db` (your db binary)
- `/.config/dibs.styx` (config)

Example Dockerfile:

```dockerfile
FROM debian:bookworm-slim

# Install dibs CLI
COPY --from=builder /usr/local/cargo/bin/dibs /usr/local/bin/dibs

# Copy your db binary
COPY --from=builder /app/target/release/my-app-db /app/my-app-db

# Config will be mounted at runtime
RUN mkdir -p /.config

CMD ["/usr/local/bin/dibs", "migrate"]
```
