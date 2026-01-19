# Styx for Kakoune

Styx syntax highlighting and LSP support for Kakoune.

## Installation

### Manual

Copy or symlink `styx.kak` to your autoload directory:

```bash
ln -s /path/to/styx/editors/kakoune-styx/styx.kak ~/.config/kak/autoload/
```

### With plug.kak

```kak
plug "bearcove/styx" subset %{
    styx.kak
}
```

## LSP Support

### With kak-lsp

Add to your `kak-lsp.toml`:

```toml
[language.styx]
filetypes = ["styx"]
roots = [".git", "*.styx"]
command = "styx"
args = ["lsp"]
```

Then in your `kakrc`:

```kak
hook global WinSetOption filetype=styx %{
    lsp-enable-window
}
```

## Requirements

The Styx CLI must be installed:

```bash
cargo install styx-cli
```

## Features

- Syntax highlighting
- Comment toggling
- Automatic indentation
- LSP integration via kak-lsp
