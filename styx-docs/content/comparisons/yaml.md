+++
title = "YAML"
weight = 2
slug = "yaml"
insert_anchor_links = "heading"
+++

YAML is the most common human-authored configuration format. STYX addresses several
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

In STYX, `NO` is always the text `NO`. The deserializer interprets based on target type.

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

## Key differences

| YAML | STYX |
|------|------|
| Indentation-based structure | Explicit `{}` and `()` delimiters |
| Implicit typing (Norway problem) | Opaque scalars |
| Anchors/aliases | Not supported |
| Multi-document (`---`) | One document per file |
