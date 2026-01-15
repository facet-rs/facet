# Phase 008b: styx @tree (Debug Parse Tree)

Debug command to show the internal parse tree representation.

## Usage

```bash
styx @tree config.styx
styx @tree -              # from stdin
```

## Output

```
Object {
  key: Scalar("server")
  value: Object {
    key: Scalar("host")
    value: Scalar("localhost")
    key: Scalar("port")
    value: Scalar("8080")
  }
}
```

## Status

Already implemented in current CLI as `tree` subcommand.
Just needs to move to `@tree` syntax.
