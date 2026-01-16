# Styx

A configuration language that's actually pleasant to use.

```styx
// Schema declaration - enables validation, completion, hover
@ examples/server.schema.styx

/// The server's display name (this is a doc comment)
name my-server
port 8080
enabled @true

// Nested objects
tls {
    cert /etc/ssl/cert.pem
    key /etc/ssl/key.pem
}

// Newlines or commas - your choice
logging {level info, format {timestamp @true, colors @true}}

// Sequences
allowed_methods (GET POST PUT DELETE)

// Tagged values for enums
status @ok
log_level @warn
maybe_value @some(42)

// Complex structures
routes (
    @route {path /api/v1, handler api}
    @route {path /health, handler health_check}
)

// Heredocs for multi-line content
query <<SQL
SELECT * FROM users
WHERE active = true
SQL
```

## Features

- **Schema validation** with helpful error messages
- **Comments** that don't get lost
- **Flexible syntax** - use newlines or commas, your choice
- **Tags** for type annotations and enums (`@optional`, `@default`, custom types)
- **LSP support** with completions, hover, go-to-definition, and more

## Editor Support

<p>
<a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="./static/sponsors/zed-dark.svg">
<img src="./static/sponsors/zed-light.svg" height="40" alt="Zed">
</picture>
</a>
</p>

Styx has first-class support for [Zed](https://zed.dev) with syntax highlighting, LSP integration, and more.

## Documentation

See [styx.bearcove.eu](https://styx.bearcove.eu) for full documentation.

## Sponsors

Thanks to all individual sponsors:

<p>
<a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="./static/sponsors/github-dark.svg">
<img src="./static/sponsors/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a>
<a href="https://patreon.com/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="./static/sponsors/patreon-dark.svg">
<img src="./static/sponsors/patreon-light.svg" height="40" alt="Patreon">
</picture>
</a>
</p>

...along with corporate sponsors:

<p>
<a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="./static/sponsors/zed-dark.svg">
<img src="./static/sponsors/zed-light.svg" height="40" alt="Zed">
</picture>
</a>
<a href="https://depot.dev?utm_source=styx">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="./static/sponsors/depot-dark.svg">
<img src="./static/sponsors/depot-light.svg" height="40" alt="Depot">
</picture>
</a>
</p>

CI runs on [Depot](https://depot.dev/) runners.

## License

MIT OR Apache-2.0
