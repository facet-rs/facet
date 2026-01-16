# Styx

A configuration language that's actually pleasant to use.

```styx
// Schema declaration - enables validation, completion, hover
@ examples/server.schema.styx

/// The server's display name
name my-server
port 8080
enabled true

tls {
    cert /etc/ssl/cert.pem
    key /etc/ssl/key.pem
}

logging {
    level info
    format {timestamp true, colors true}  // inline style works too
}

// Tagged values for type annotations
endpoints @seq(
    @endpoint {path /api/v1, methods (GET POST)}
    @endpoint {path /health, methods (GET)}
)

metadata @map {
    version 1.0.0
    build-date 2024-01-15
}
```

## Features

- **Schema validation** with helpful error messages
- **Comments** that don't get lost
- **Flexible syntax** - use newlines or commas, your choice
- **Tags** for type annotations and enums (`@optional`, `@default`, custom types)
- **LSP support** with completions, hover, go-to-definition, and more

## Documentation

See [styx.bearcove.eu](https://styx.bearcove.eu) for full documentation.

## License

MIT OR Apache-2.0
