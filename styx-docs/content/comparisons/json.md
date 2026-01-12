+++
title = "JSON"
weight = 1
slug = "json"
insert_anchor_links = "heading"
+++

JSON is the lingua franca of data interchange. STYX is designed for human authoring,
not machine interchange, which leads to different trade-offs.

## Simple object

```compare
/// json
{
  "name": "alice",
  "age": 30
}
/// styx
{ name alice, age 30 }
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

## Key differences

| JSON | STYX |
|------|------|
| Mandatory quotes on keys | Bare keys |
| Colons between key/value | Whitespace |
| No comments | `//` comments |
| `null` is a value | `@` is structural absence |
| Strings vs numbers distinguished | All scalars opaque until deserialization |
