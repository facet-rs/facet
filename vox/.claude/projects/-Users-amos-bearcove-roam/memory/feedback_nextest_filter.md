---
name: nextest_filter_expr
description: Use -E 'package(...)' instead of -p when running cargo nextest to avoid thrashing the cargo cache
type: feedback
---

Use `cargo nextest run -E 'package(foo)'` instead of `cargo nextest run -p foo`.

**Why:** `-p` thrashes the cargo cache.
**How to apply:** Always use the `-E 'package(...)'` filter expression syntax with cargo nextest.
