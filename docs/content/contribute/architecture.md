+++
title = "Architecture"
weight = 3
insert_anchor_links = "heading"
+++

## Crate graph

```
┌─────────────────────────────────────────────────────────────────┐
│                         User Code                               │
│                    #[derive(Facet)]                             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                          facet                                  │
│              Re-exports from core + macros + reflect            │
└─────────────────────────────────────────────────────────────────┘
          │                   │                   │
          ▼                   ▼                   ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│   facet-core    │  │  facet-macros   │  │ facet-reflect   │
│                 │  │                 │  │                 │
│ • Facet trait   │  │ • #[derive]     │  │ • Peek (read)   │
│ • Shape         │  │ • Proc macros   │  │ • Partial (build)│
│ • Def, Type     │  │                 │  │                 │
│ • VTables       │  │        │        │  │                 │
│ • no_std        │  │        ▼        │  │                 │
└─────────────────┘  │ facet-macros-   │  └─────────────────┘
                     │ impl            │           │
                     │                 │           │
                     │ • unsynn parser │           │
                     │ • Code gen      │           │
                     └─────────────────┘           │
                                                   │
                              ┌────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│              Format Crates & Utility Crates                     │
│  facet-json, facet-yaml, facet-toml, facet-args                 │
│  facet-pretty, facet-diff, facet-assert, facet-value            │
└─────────────────────────────────────────────────────────────────┘
```

Format crates and utility crates primarily interact with `facet-reflect` — they use `Peek` to read values and `Partial` to build them.

## Key crates

| Crate | Purpose |
|-------|---------|
| `facet-core` | Core types: `Facet` trait, `Shape`, `Def`, vtables. Supports `no_std`. |
| `facet-macros` | The `#[derive(Facet)]` proc macro (thin wrapper). |
| `facet-macros-impl` | Actual derive implementation using [unsynn](https://docs.rs/unsynn). |
| `facet-reflect` | High-level reflection: `Peek` for reading, `Partial` for building. |
| `facet` | Umbrella crate that re-exports everything. |
