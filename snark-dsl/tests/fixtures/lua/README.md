# Lua grammar fixture

`grammar.js` is the tree-sitter-lua grammar by Munif Tanjim (MIT license),
vendored here purely as test input for the Boa-vs-tree-sitter oracle
comparison in `snark-dsl/src/lib.rs`.

`grammar.tree-sitter.json` is the corresponding `tree-sitter generate
--no-parser` output, committed so the oracle test doesn't need the
`tree-sitter` CLI in CI. Regenerate it with
`snark-dsl/scripts/regenerate-lua-fixture.sh` after editing `grammar.js` or
upgrading tree-sitter-cli.
