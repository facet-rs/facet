# TODO-010: Editor Extension Publishing

## Status
TODO

## Description
Publish Styx editor extensions to their respective marketplaces and registries.

## Publishing Checklist

### Zed
- [ ] Submit to Zed extension gallery
- [ ] URL: Extensions panel in Zed → "Install Dev Extension" for testing
- [ ] Docs: https://zed.dev/docs/extensions/developing-extensions

### VS Code
- [ ] Create publisher account on Visual Studio Marketplace
- [ ] Publish to **Visual Studio Marketplace**: https://marketplace.visualstudio.com/
- [ ] Publish to **Open VSX Registry** (for VSCodium, Gitpod, etc.): https://open-vsx.org/
- [ ] Commands:
  ```bash
  cd editors/vscode-styx
  npx vsce publish  # VS Marketplace
  npx ovsx publish  # Open VSX
  ```

### Neovim
- [ ] Add to **nvim-treesitter** parsers list: https://github.com/nvim-treesitter/nvim-treesitter
  - PR to add styx parser config
- [ ] Add to **mason.nvim** registry (for LSP): https://github.com/mason-org/mason-registry
- [ ] Consider standalone plugin repo for easier installation

### Helix
- [ ] PR to add Styx to **helix-editor/helix** `languages.toml`: https://github.com/helix-editor/helix
- [ ] PR to add queries to `runtime/queries/styx/`
- [ ] Docs: https://docs.helix-editor.com/guides/adding_languages.html

### Emacs
- [ ] Submit to **MELPA**: https://github.com/melpa/melpa
  - Create recipe file
  - Ensure package-lint passes
- [ ] Consider **GNU ELPA** (requires copyright assignment)
- [ ] Docs: https://github.com/melpa/melpa/blob/master/CONTRIBUTING.org

### Kakoune
- [ ] Add to **kakoune-lsp** default config: https://github.com/kakoune-lsp/kakoune-lsp
- [ ] Consider **plug.kak** registry or standalone repo

### Sublime Text
- [ ] Submit to **Package Control**: https://packagecontrol.io/
  - Add to package_control_channel: https://github.com/wbond/package_control_channel
- [ ] Docs: https://packagecontrol.io/docs/submitting_a_package

### JetBrains
- [ ] Create JetBrains Marketplace account
- [ ] Publish to **JetBrains Marketplace**: https://plugins.jetbrains.com/
- [ ] Commands:
  ```bash
  cd editors/jetbrains-styx
  ./gradlew publishPlugin
  ```
- [ ] Docs: https://plugins.jetbrains.com/docs/intellij/publishing-plugin.html

### Kate / KDE
- [ ] Submit to **KSyntaxHighlighting**: https://invent.kde.org/frameworks/syntax-highlighting
  - Add `styx.xml` to `data/syntax/`
  - Submit merge request
- [ ] Docs: https://kate-editor.org/syntax/
- [ ] Also benefits: KWrite, KDevelop, any KSyntaxHighlighting user

### nano
- [ ] Submit to **nano-syntax-highlighting**: https://github.com/scopatz/nanorc
  - Add `styx.nanorc`
- [ ] Or propose for inclusion in nano upstream

## Other Registries

### tree-sitter
- [ ] Add to **tree-sitter** org or create `tree-sitter-styx` repo
- [ ] Register in tree-sitter grammar list

### npm (for CodeMirror)
- [ ] Publish `@bearcove/codemirror-lang-styx` to npm
- [ ] See TODO-007 for CodeMirror details

### crates.io
- [ ] Publish `styx-cli` to crates.io (for `cargo install styx-cli`)
- [ ] Ensure binary is named `styx`

## Notes
- Most marketplaces require icons — create consistent branding
- Write good descriptions with screenshots
- Set up CI to auto-publish on release tags
