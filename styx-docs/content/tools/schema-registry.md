+++
title = "Schema Registry"
weight = 15
insert_anchor_links = "heading"
+++

The styx CLI includes a built-in registry of known schema patterns. When you open a `.styx` file that matches a pattern but has no `@schema` declaration, the editor shows a warning with a code action to add it.

## How it works

1. You open `.config/tracey/config.styx` in your editor
2. The LSP recognizes this path matches the tracey pattern
3. A warning appears: "This file matches a known schema pattern"
4. Click the code action to insert `@schema {id crate:tracey-config@1, cli tracey}`

## Registry format

The registry lives in the styx repo at `registry/schema-hints.styx`:

```styx
hints {
  tracey {
    patterns (
      "{git_root}/.config/tracey/config.styx"
      "{git_root}/.config/tracey/*.styx"
    )
    schema {id crate:tracey-config@1, cli tracey}
  }
  
  dodeca {
    patterns (
      "{git_root}/.config/dodeca.styx"
    )
    schema {id crate:dodeca-config@1, cli ddc}
  }
}
```

## Path variables

Patterns can use these variables:

| Variable | Description |
|----------|-------------|
| `{git_root}` | Nearest ancestor directory containing `.git/` |
| `{userconfig}` | Platform user config directory (see below) |
| `{home}` | User home directory |

Literal paths (e.g., `/etc/some-tool/config.styx`) are also supported for system-wide configs.

### `{userconfig}` resolution

| Platform | Path |
|----------|------|
| Linux | `$XDG_CONFIG_HOME` or `~/.config` |
| macOS | `~/Library/Application Support` |
| Windows | `%APPDATA%` |

## Adding to the registry

The registry is maintained in the [styx repository](https://github.com/bearcove/styx). To add your tool:

1. Fork the repo
2. Add an entry to `registry/schema-hints.styx`
3. Open a pull request

Requirements:
- Your schema must be published to crates.io (or have a plan to)
- Include the `cli` hint if your binary embeds the schema
- Use specific patterns that won't conflict with other tools

## Example entry

```styx
my-tool {
  patterns (
    "{git_root}/.config/my-tool.styx"
    "{git_root}/my-tool.styx"
    "{userconfig}/my-tool/config.styx"
  )
  schema {id crate:my-tool-config@1, cli my-tool}
}
```
