+++
title = "weavy"
description = "A shared lowered-program substrate for interpreters and copy-and-patch backends."
insert_anchor_links = "heading"
+++

**weavy** is the shared substrate for *lowered programs* — the common carrier that
sits between a front-end language (like [Fable](/fable)) and the backends that run
it.

It stays deliberately **format-agnostic**: callers bring their own schema
identities, parsers, and value models. weavy provides:

- the shared shape for lowered programs — flat programs, named blocks, and a small
  call-stack runner;
- a generic **typed-memory** descriptor and op vocabulary (`mem`).

Native copy-and-patch backends can reuse the exact same program/block shape, so an
interpreter and a JIT share one lowered representation.

[Source on GitHub](https://github.com/bearcove/weavy)
