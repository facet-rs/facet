+++
title = "fable"
description = "A tiny typed language over Facet-reflected Rust values, lowered toward Weavy IR."
insert_anchor_links = "heading"
+++

**fable** is a tiny typed language for inspecting and mutating
[facet](https://facet.rs)-reflected Rust values, then lowering toward canonical
[Weavy](/weavy) IR.

The crate currently owns the **syntax layer**: a lossless lexer/parser, the
[cstree](https://docs.rs/cstree) language tags, and a small typed AST facade for
the first grammar slice. "Lossless" means the concrete syntax tree preserves every
byte — whitespace, comments, and trivia — so tools can round-trip and rewrite Fable
source without losing anything.

> Early days: this documents the syntax foundation. Evaluation and the lowering to
> Weavy IR are coming.

[Source on GitHub](https://github.com/bearcove/fable)
