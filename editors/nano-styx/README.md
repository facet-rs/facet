# Styx for GNU nano

Syntax highlighting for Styx in GNU nano.

## Installation

### User installation

Copy to your nano syntax directory:

```bash
mkdir -p ~/.nano
cp /path/to/styx/editors/nano-styx/styx.nanorc ~/.nano/
```

Add to your `~/.nanorc`:

```
include ~/.nano/styx.nanorc
```

### System-wide installation

```bash
sudo cp styx.nanorc /usr/share/nano/
```

Add to `/etc/nanorc`:

```
include /usr/share/nano/styx.nanorc
```

## Features

- Syntax highlighting for:
  - Comments (`//` and `///` doc comments)
  - Tags (`@name`)
  - Unit (`@`)
  - Strings (quoted, raw, heredoc)
  - Escape sequences
  - Attribute arrows (`>`)
  - Brackets and punctuation

## Note

nano doesn't support LSP, so you only get syntax highlighting. For full language server features, consider using an editor with LSP support.
