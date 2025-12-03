+++
title = "Architecture"
weight = 3
insert_anchor_links = "heading"
+++

## Crate Graph

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
│ • VTables       │  │                 │  │                 │
│ • no_std        │  │                 │  │                 │
└─────────────────┘  └─────────────────┘  └─────────────────┘
          │                   │
          │                   ▼
          │          ┌─────────────────┐
          │          │facet-macros-impl│
          │          │                 │
          │          │ • unsynn parser │
          │          │ • Code gen      │
          │          └─────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Format Crates                              │
│  facet-json, facet-yaml, facet-kdl, facet-toml, facet-args...   │
└─────────────────────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Utility Crates                             │
│  facet-pretty, facet-diff, facet-assert, facet-value...         │
└─────────────────────────────────────────────────────────────────┘
```

## Key Crates

| Crate | Purpose |
|-------|---------|
| [`facet-core`](https://docs.rs/facet-core) | Core types: `Facet` trait, `Shape`, `Def`, vtables. Supports `no_std`. |
| [`facet-macros`](https://docs.rs/facet-macros) | The `#[derive(Facet)]` proc macro (thin wrapper). |
| `facet-macros-impl` | Actual derive implementation using [unsynn](https://docs.rs/unsynn). |
| [`facet-reflect`](https://docs.rs/facet-reflect) | Safe reflection APIs: `Peek` for reading, `Partial` for building. |
| [`facet`](https://docs.rs/facet) | Umbrella crate that re-exports everything. |
