+++
title = "YAML"
weight = 2
slug = "yaml"
insert_anchor_links = "heading"
+++

YAML is the most common human-authored configuration format. Styx addresses several
YAML pain points while preserving readability.

## Simple object

```compare
/// yaml
name: alice
age: 30
/// styx
name alice
age 30
```

## Nested configuration

```compare
/// yaml
server:
  host: localhost
  port: 8080
  tls:
    enabled: true
    cert: /path/to/cert.pem
/// styx
server {
  host localhost
  port 8080
  tls {
    enabled true
    cert /path/to/cert.pem
  }
}
```

## Lists

```compare
/// yaml
features:
  - auth
  - logging
  - metrics
/// styx
features (
  auth
  logging
  metrics
)
```

```compare
/// yaml
features: [auth, logging, metrics]
/// styx
features (auth logging metrics)
```

## The Norway problem

YAML's implicit typing causes bugs:

```compare
/// yaml
countries:
  - GB    # string
  - NO    # boolean false!
  - IE    # string
/// styx
countries (GB NO IE)
```

In Styx, `NO` is always the text `NO`. The deserializer interprets based on target type.

## Kubernetes-style config

```compare
/// yaml
apiVersion: v1
kind: Service
metadata:
  name: my-service
  labels:
    app: web
    tier: frontend
spec:
  ports:
    - port: 80
      targetPort: 8080
  selector:
    app: web
/// styx
apiVersion v1
kind Service
metadata {
  name my-service
  labels app=web tier=frontend
}
spec {
  ports ({
    port 80
    targetPort 8080
  })
  selector app=web
}
```

## Indentation vs delimiters

```compare
/// yaml
server:
  host: localhost
  port: 8080
    extra: oops
/// styx
server {
  host localhost
  port 8080
  extra oops
}
```

YAML's indentation can cause subtle bugs. Styx structure is always explicit.

## Anchors and aliases

```compare
/// yaml
defaults: &defaults
  timeout: 30s
  retries: 3

production:
  <<: *defaults
  timeout: 60s
/// styx
defaults {
  timeout 30s
  retries 3
}
production {
  timeout 60s
  retries 3
}
```

Styx does not support references. Use application-level merging instead.
