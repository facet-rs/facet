+++
title = "JSON"
weight = 1
slug = "json"
insert_anchor_links = "heading"
+++

JSON is the lingua franca of data interchange. Styx is designed for human authoring,
not machine interchange, which leads to different trade-offs.

## Simple object

```compare
/// json
{
  "name": "alice",
  "age": 30
}
/// styx
name alice
age 30
```

## Nested configuration

```compare
/// json
{
  "server": {
    "host": "localhost",
    "port": 8080,
    "tls": {
      "enabled": true,
      "cert": "/path/to/cert.pem"
    }
  }
}
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

## Arrays

```compare
/// json
{
  "features": ["auth", "logging", "metrics"]
}
/// styx
features (auth logging metrics)
```

## Null values

```compare
/// json
{
  "timeout": null
}
/// styx
timeout @
```

## Bare keys

```compare
/// json
{ "name": "alice" }
/// styx
name alice
```

## Comments

```compare
/// json
{
  "port": 8080
}
/// styx
port 8080  // default HTTP port
```

## Types are opaque

```compare
/// json
{ "count": 42, "label": "42" }
/// styx
count 42
label 42
```

In Styx, both are the scalar `42`. The deserializer interprets based on target type.

## Attribute syntax

```compare
/// json
{
  "server": {
    "host": "localhost",
    "port": 8080
  }
}
/// styx
server host>localhost port>8080
```
