# snark-dsl

Boa spike for evaluating Tree-sitter grammar DSL files.

The runtime DSL is the official Tree-sitter DSL from the 0.26.9 release. The
npm package supplies `tree-sitter-cli/dsl` types, while the runtime `dsl.js`
used by `tree-sitter generate` lives in the matching `tree-sitter-generate`
crate source.

The default fixture is Arborium's Hazel Lua grammar:

```text
/Users/amos/oss/arborium/langs/group-hazel/lua/def/grammar/grammar.js
```

Commands:

```sh
cargo run -p snark-dsl -- emit [grammar.js]
cargo run -p snark-dsl -- oracle [grammar.js]
cargo run -p snark-dsl -- check [grammar.js]
cargo nextest run
```

`check` compares Boa's emitted `grammar.json` byte-for-byte with
`tree-sitter generate --no-parser`. For Lua, the outputs currently match.
