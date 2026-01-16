# Styx

A configuration language that's actually pleasant to use.

```styx
@ examples/server.schema.styx

name "my-server"
port 8080
enabled true

tls {
    cert "/etc/ssl/cert.pem"
    key "/etc/ssl/key.pem"
}

logging {
    level "info"
    format {
        timestamp true
        colors true
    }
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
