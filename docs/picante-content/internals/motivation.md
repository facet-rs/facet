+++
title = "Motivation"
weight = 1
+++

picante exists because the "[salsa](https://salsa-rs.netlify.app/) model" is extremely useful for large pipelines, but real-world systems need **async** queries.

## The Problem

Consider a static site generator like [dodeca](https://dodeca.dev). Building a site involves:

```
SourceFile → parse_file → build_tree → render_page → all_rendered_html → build_site
                               ↑
TemplateFile → load_template ──┘
```

Every arrow is a query. Queries depend on other queries. When a file changes, you want to rebuild *only* what actually depends on that file — not the entire site.

The salsa model solves this beautifully:
- Queries are memoized functions
- Dependencies are tracked automatically during execution
- When inputs change, only affected queries recompute

But dodeca's queries naturally want to:
- Read files concurrently
- Call plugins in separate processes (via RPC)
- Spawn work on a thread pool
- Stream large data through shared memory

Salsa's queries are synchronous. Wrapping async in `block_on` everywhere defeats the purpose of async. picante provides the same incremental model, but with first-class async support.

## The Stack

picante is the foundation of a layered architecture:

```
┌────────────────────────────────────────┐
│  your application (e.g., dodeca)       │  ← domain-specific queries
├────────────────────────────────────────┤
│  tacos                                 │  ← file watching, CAS, hashing
├────────────────────────────────────────┤
│  picante                               │  ← pure incremental queries
└────────────────────────────────────────┘
```

**picante** handles the core incremental computation: dependency tracking, memoization, cache invalidation, and persistence.

**[tacos](https://github.com/bearcove/tacos)** builds on picante to provide build-system infrastructure: file watching, content-addressed storage for large blobs, and content hashing for cache keys.

Your application defines domain-specific queries on top of this stack.

## Real-World Example: dodeca

Here's how these layers work together in [dodeca](https://dodeca.dev):

```
┌─────────────────────────────────────────────────────────────┐
│                    HOST (dodeca binary)                     │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                      PICANTE                          │  │
│  │  • Tracks dependencies between queries                │  │
│  │  • Caches query results (memoization)                 │  │
│  │  • Knows what's stale when inputs change              │  │
│  │  • Persists cache to disk via facet-postcard          │  │
│  └───────────────────────────────────────────────────────┘  │
│                            │                                │
│  ┌────────────────────────┼────────────────────────────┐   │
│  │          CAS           │                            │   │
│  │  • Content-addressed   │  Host reads/writes         │   │
│  │  • Large blobs on disk │  Plugins never touch       │   │
│  │  • Survives restarts   │                            │   │
│  └────────────────────────┼────────────────────────────┘   │
│                           │                                 │
│  ┌────────────────────────▼────────────────────────────┐   │
│  │              PROVIDER SERVICES                       │   │
│  │  • resolve_template(name) → content                 │   │
│  │  • resolve_import(path) → content                   │   │
│  │  • get_data(key) → value                            │   │
│  │  (all picante-tracked!)                             │   │
│  └────────────────────────┬────────────────────────────┘   │
└───────────────────────────┼─────────────────────────────────┘
                            │ rapace RPC + SHM
┌───────────────────────────▼─────────────────────────────────┐
│                  PLUGINS (separate processes)               │
│  • Pure async functions                                     │
│  • No caching knowledge                                     │
│  • Call back to host for dependencies                       │
│  • Return large blobs via shared memory                     │
└─────────────────────────────────────────────────────────────┘
```

When a plugin (like the template renderer) needs data, it calls back to the host:

```
Host                              Plugin (template renderer)
  │                                        │
  │── render(page, template_name) ────────▶│
  │                                        │
  │◀── resolve_template("base.html") ──────│
  │    (picante tracks this dependency!)   │
  │                                        │
  │── template content ───────────────────▶│
  │                                        │
  │◀── resolve_template("partials/nav") ───│
  │    (another tracked dependency!)       │
  │                                        │
  │── template content ───────────────────▶│
  │                                        │
  │◀── rendered HTML ──────────────────────│
```

The key insight: **plugin callbacks flow through picante-tracked host APIs**. When `base.html` changes later, picante knows to re-render pages that included it — even though the actual rendering happened in a separate process.

## Why Not Just Memoize?

Simple memoization caches function results, but without dependency tracking you can't answer:

- "What depends on this input?"
- "What needs to be recomputed when this file changes?"

You'd have to either:
1. Rebuild everything (slow)
2. Manually specify dependencies (error-prone, stale)

picante records dependencies automatically during query execution. Change a template, and only pages using that template rebuild. Change a Markdown file, and only that page re-renders. The dependency graph emerges from actual runtime behavior.

## Cache Persistence

The dependency graph and cached query results can be saved to disk via picante's persistence APIs (encoded with `facet-postcard`). Even a cold start can benefit from previous work if your application loads a previously-saved cache:

```
<app cache dir>/
├── cas/         # optional content-addressed storage (large blobs, app-specific)
└── picante.bin  # picante's serialized cache (path/name chosen by the app)
```

Large outputs (processed images, subsetted fonts) are typically stored outside picante (for example in a CAS), keeping the picante database lean. Only small values/hashes need to live in picante; large blobs are retrieved by your application on demand.

## Summary

picante provides:
- **Async queries** that work naturally with tokio
- **Automatic dependency tracking** during execution
- **Memoization** with proper invalidation
- **Persistence** via facet-postcard
- **Snapshots** for consistent reads (MVCC)

It's the incremental computation engine, focused and minimal. Higher-level concerns (file watching, blob storage, RPC) live in other crates like [tacos](https://github.com/bearcove/tacos).
