# TODO-013: `styx @cache` CLI Command

## Status
TODO

## Description
Add CLI commands for managing the schema cache.

## Commands

### `styx @cache`
Show cache configuration and statistics:
```bash
$ styx @cache
Cache directory: /home/user/.cache/styx/schemas/
Embedded schemas: 3 (12 KB)
Crate schemas: 5 (48 KB)
```

### `styx @cache --open`
Open the cache directory in the system file explorer:
```bash
$ styx @cache --open
# Opens ~/.cache/styx/schemas/ in Finder/Nautilus/Explorer
```

Platform commands:
- macOS: `open <path>`
- Linux: `xdg-open <path>`
- Windows: `explorer <path>`

### `styx @cache --clear`
Remove all cached schemas:
```bash
$ styx @cache --clear
Cleared 8 cached schemas (60 KB)
```

## Implementation
Add to `crates/styx-cli/src/main.rs` in the subcommand handling section.

## Dependencies
- TODO-012: Schema cache implementation (for cache_dir() utility)
