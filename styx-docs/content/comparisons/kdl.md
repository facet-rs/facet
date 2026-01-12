+++
title = "KDL"
weight = 4
slug = "kdl"
insert_anchor_links = "heading"
+++

KDL is a modern document format with similar goals to STYX.
Both reject YAML's indentation-sensitivity and JSON's verbosity.

## Simple object

```compare
/// kdl
server {
    host "localhost"
    port 8080
}
/// styx
server {
  host localhost
  port 8080
}
```

## Positional arguments

KDL nodes can have positional arguments:

```compare
/// kdl
person "Alice" age=30
/// styx
person { name Alice, age 30 }
```

## Type annotations

```compare
/// kdl
port (u16)8080
timeout (duration)"30s"
/// styx
port 8080
timeout 30s
```

STYX uses schema-defined types instead of inline annotations.

## Null handling

```compare
/// kdl
optional null
/// styx
optional @
```

## Package manifest

```compare
/// kdl
package {
    name "my-app"
    version "1.0.0"
    dependencies {
        serde "1.0" features=["derive"]
        tokio "1.0" optional=true
    }
}
/// styx
package {
  name my-app
  version 1.0.0
  dependencies {
    serde {
      version 1.0
      features (derive)
    }
    tokio {
      version 1.0
      optional true
    }
  }
}
```

## Key differences

| KDL | STYX |
|-----|------|
| Node with args + properties | Key-value pairs only |
| Inline type annotations `(type)` | Schema-defined types |
| Strings always quoted | Bare or quoted |
| `null` keyword | `@` unit value |
| `//` and `/* */` comments | `//` only |
