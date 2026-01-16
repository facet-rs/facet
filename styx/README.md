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

## Documentation

See [styx.bearcove.eu](https://styx.bearcove.eu) for full documentation.

## Acknowledgments

CI runs on [Depot](https://depot.dev/) runners.

## License

MIT OR Apache-2.0
