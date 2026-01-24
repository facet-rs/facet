+++
title = "Internals"
description = "How dibs works, explained from the outside in"
+++

dibs is a toolchain for keeping a Postgres schema in sync with a schema you define in Rust. This page explains how it works from the outside in: the service/RPC boundary, how schema diffs become ordered SQL, how migrations are executed and tracked, and where code generation fits in.

<div class="callout info">
<strong>Info</strong>
<p>dibs constantly reconciles <em>intent</em> (your Rust schema) with <em>reality</em> (the live database), then generates and/or runs the steps needed to make them match.</p>
</div>

## How dibs works (in one picture)

At a high level, dibs treats your schema as two things:

- **Intent**: the schema you *meant* to have (defined in Rust).
- **Reality**: the schema the database *currently* has (introspected from Postgres).

Most dibs commands follow the same pipeline:

<div class="flow flow-vertical">
  <div class="flow-step">
    <div class="flow-title">Load intent</div>
    <div class="flow-body">Read your schema + migration registry from the <code>myapp-db</code> crate.</div>
  </div>

  <div class="flow-arrow" aria-hidden="true">
    <svg viewBox="0 0 16 64" width="16" height="32">
      <path d="M8 2v46" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      <path d="M2 40l6 8 6-8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>
  </div>

  <div class="flow-step">
    <div class="flow-title">Inspect reality</div>
    <div class="flow-body">Query Postgres catalogs to reconstruct the live schema.</div>
  </div>

  <div class="flow-arrow" aria-hidden="true">
    <svg viewBox="0 0 16 64" width="16" height="32">
      <path d="M8 2v46" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      <path d="M2 40l6 8 6-8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>
  </div>

  <div class="flow-step">
    <div class="flow-title">Compute diff</div>
    <div class="flow-body">Compare “intent vs reality” into a list of typed schema operations.</div>
  </div>

  <div class="flow-arrow" aria-hidden="true">
    <svg viewBox="0 0 16 64" width="16" height="32">
      <path d="M8 2v46" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      <path d="M2 40l6 8 6-8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>
  </div>

  <div class="flow-step">
    <div class="flow-title">Solve</div>
    <div class="flow-body">Simulate and reorder operations so the SQL can run (FKs, renames, drops).</div>
  </div>

  <div class="flow-arrow" aria-hidden="true">
    <svg viewBox="0 0 16 64" width="16" height="32">
      <path d="M8 2v46" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      <path d="M2 40l6 8 6-8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>
  </div>

  <div class="flow-step">
    <div class="flow-title">SQL / apply</div>
    <div class="flow-body">Emit ordered DDL, and optionally run it while streaming progress.</div>
  </div>
</div>

<div class="callout recap">
<strong>Recap</strong>
<p>You describe the schema and write migrations as Rust. dibs handles introspection, diffing, ordering, SQL generation, and safe application.</p>
</div>

## How the pieces fit together

dibs uses a small, dedicated process (your “db crate”) that speaks a narrow RPC protocol to the CLI (implemented using [Roam](https://github.com/bearcove/roam)).

Typical shape:

<div class="callout aside">
<strong>How the CLI, your db crate, and Postgres interact</strong>

<div class="flow flow-vertical">
  <div class="flow-step">
    <div class="flow-title">dibs-cli</div>
    <div class="flow-body">Spawns the db process and drives the UX (TUI, prompts, logs, formatting).</div>
  </div>

  <div class="flow-arrow" aria-hidden="true">
    <svg viewBox="0 0 16 64" width="16" height="36">
      <path d="M8 4v56" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      <path d="M2 10l6-8 6 8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
      <path d="M2 54l6 8 6-8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>
  </div>

  <div class="flow-step">
    <div class="flow-title">myapp-db</div>
    <div class="flow-body">Loads your Rust schema + migrations, connects to Postgres, and serves RPC methods.</div>
  </div>

  <div class="flow-arrow" aria-hidden="true">
    <svg viewBox="0 0 16 64" width="16" height="36">
      <path d="M8 4v56" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      <path d="M2 10l6-8 6 8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
      <path d="M2 54l6 8 6-8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>
  </div>

  <div class="flow-step">
    <div class="flow-title">Postgres</div>
    <div class="flow-body">Source of reality (introspection) and target for migrations (DDL + backfills).</div>
  </div>
</div>
</div>

A typical exchange looks like:

<div class="chat">
  <div class="bubble bubble-cli">
    <div class="bubble-who">dibs-cli</div>
    <div class="bubble-text">What’s the intended schema?</div>
  </div>

  <div class="bubble bubble-service">
    <div class="bubble-who">myapp-db</div>
    <div class="bubble-text">Here are the tables and columns I collected from your Rust schema.</div>
    <div class="bubble-meta">Under the hood: dibs scans your crate for registered tables (Facet/dibs annotations), builds an internal schema model, and returns it over RPC.</div>
  </div>

  <div class="bubble bubble-cli">
    <div class="bubble-who">dibs-cli</div>
    <div class="bubble-text">Given this database URL, what changes are needed?</div>
  </div>

  <div class="bubble bubble-service">
    <div class="bubble-who">myapp-db</div>
    <div class="bubble-text">I connected to Postgres, introspected the current schema, and computed the diff against your Rust schema.</div>
    <div class="bubble-meta">Result: a structured list of changes (add/alter/drop/rename) per table — not just raw SQL.</div>
  </div>

  <div class="bubble bubble-cli">
    <div class="bubble-who">dibs-cli</div>
    <div class="bubble-text">What SQL should we run, in what order?</div>
  </div>

  <div class="bubble bubble-service">
    <div class="bubble-who">myapp-db</div>
    <div class="bubble-text">Here’s ordered SQL that should execute cleanly.</div>
    <div class="bubble-meta">Under the hood: the solver simulates the migration on a virtual schema, orders operations to satisfy dependencies, then verifies the final simulated state matches the desired schema.</div>
  </div>

  <div class="bubble bubble-cli">
    <div class="bubble-who">dibs-cli</div>
    <div class="bubble-text">Run migrations and stream logs back to me.</div>
  </div>

  <div class="bubble bubble-service">
    <div class="bubble-who">myapp-db</div>
    <div class="bubble-text">Running pending migrations in transactions — streaming progress as they apply.</div>
    <div class="bubble-meta">If a migration fails, dibs rolls back that migration’s transaction and reports the error with SQL context (and source location when available).</div>
  </div>
</div>

<div class="callout info">
<strong>Why RPC?</strong>
<p>So you don’t have to boot your entire app just to work with schemas and migrations.</p>
</div>

<div class="callout aside">
<strong>Workspace shape</strong>
<p>A common setup is three crates that keep schema, queries, and app code decoupled:</p>

<div class="flow flow-vertical">
  <div class="flow-step">
    <div class="flow-title">myapp</div>
    <div class="flow-body">Depends on <code>myapp-queries</code> for typed query helpers.</div>
  </div>

  <div class="flow-arrow" aria-hidden="true">
    <svg viewBox="0 0 16 64" width="16" height="32">
      <path d="M8 2v46" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      <path d="M2 40l6 8 6-8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>
  </div>

  <div class="flow-step">
    <div class="flow-title">myapp-queries</div>
    <div class="flow-body">Generated query code validated against the schema (types, columns, nullability).</div>
  </div>

  <div class="flow-arrow" aria-hidden="true">
    <svg viewBox="0 0 16 64" width="16" height="32">
      <path d="M8 2v46" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      <path d="M2 40l6 8 6-8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>
  </div>

  <div class="flow-step">
    <div class="flow-title">myapp-db</div>
    <div class="flow-body">Schema + migrations source of truth (what tables/columns should exist).</div>
  </div>
</div>

<p>In day-to-day use, <strong>dibs-cli</strong> talks to <strong>myapp-db (bin)</strong> via RPC. The CLI is mostly UX and orchestration; the db service is where the schema work happens: load schema/migrations, introspect Postgres, compute diffs, order changes, generate SQL, and run migrations.</p>

<p>Separately, your <strong>myapp</strong> talks to <strong>myapp-queries</strong> for typed query helpers; and <strong>myapp-queries</strong> is generated/validated against what <strong>myapp-db</strong> says the schema is.</p>
</div>

## Intent, reality, and “the diff”

dibs keeps two schema models side-by-side:

- **Intent**: what your Rust schema says should exist.
- **Reality**: what Postgres actually has.

When they don’t match, dibs produces a **diff**: a typed list of schema operations (create/alter/drop/rename, constraints, indexes, foreign keys) that can later be ordered and rendered to SQL.

<div class="callout aside">
<strong>Aside</strong>
<p>dibs keeps some internal bookkeeping tables for migration state and optional metadata. Those are ignored during schema introspection and diffing of your app schema.</p>
</div>

Once dibs has:

- the **desired** schema (intent),
- and the **current** schema (reality),

it computes a **schema diff**. A diff is not just “SQL text” — it’s a structured list of operations such as:

- create or drop a table,
- rename a table,
- add/rename/drop a column,
- alter column type/nullability/default,
- add/drop indexes,
- add/drop unique constraints,
- add/drop foreign keys.

That structured representation is crucial: dibs can reason about dependencies and safety before generating SQL.

## The solver (making the diff executable)

If you’ve ever written migrations by hand, you already know the main trap: the *same* set of changes can succeed or fail depending on order.

Example class of problems:

- adding a foreign key before the referenced table exists,
- dropping a table while other tables still reference it,
- renaming a table and then trying to reference the old name,
- circular dependency situations that require careful sequencing.

dibs addresses this with a solver that does two things:

### (a) Validate preconditions

Each change has preconditions: “the table must exist,” “the column must not exist,” “the foreign key target must exist,” “no other tables may reference this table when dropping,” etc.

The solver checks these preconditions *against a simulated schema state* rather than guessing based on the raw diff.

### (b) Order the operations

Rather than hard-coding a few rules (“renames first, constraints last”), dibs uses a more robust approach:

- Start from a virtual representation of the current schema.
- Repeatedly pick any change whose preconditions are satisfied *right now*.
- Apply it to the virtual schema.
- Continue until all changes are scheduled.

If the solver can’t make progress, it means one of:

- the diff is impossible to execute as written (true dependency cycle or conflicting intent),
- the database contains dependencies that prevent the requested change (for example, another table’s FK blocks a drop),
- or there is a bug/mismatch in diff generation.

### Simulation-based verification

After ordering all changes, dibs checks that applying them to the simulated “current” state actually results in the simulated “desired” state.

This is an important internal invariant: if the solver produces ordered SQL, dibs wants high confidence that running it will get you where you expected.

## SQL generation (rendering changes into Postgres statements)

Once the solver has an ordered list of changes, dibs renders them into SQL statements. This is where structured operations turn into concrete Postgres DDL like:

- `CREATE TABLE ...`
- `ALTER TABLE ...`
- `CREATE INDEX ...`
- constraints, uniques, foreign keys, etc.

Separating “diff” from “SQL text” is a big part of why dibs can provide better errors and tooling: it knows *what* it’s trying to do, not just the final text blob.

## Running migrations (Rust functions, transactions, and tracking)

dibs supports migrations as Rust functions registered with the program. At runtime, dibs can:

- list which migrations are applied/pending,
- run pending migrations,
- stream progress/log output back to the CLI UI.

### Execution model

- Each migration runs inside its own database transaction.
- If a migration fails, its transaction is rolled back and subsequent migrations are not run.
- Applied migrations are recorded in a dedicated migrations table.

### Better errors

When a migration fails due to SQL execution, dibs attaches context (what SQL was running, plus caller/source information when available) so the CLI can show you a more actionable error than “postgres said no.”

## Meta tracking (`__dibs_*` tables)

In addition to the minimal migrations table, dibs can maintain richer metadata tables under the `__dibs_*` prefix. Conceptually, these are there to record:

- where a table/column/index came from in source (file/line/column),
- documentation comments and other human metadata,
- relationships like “this was created by migration X,”
- and other introspection-friendly details.

This metadata is intentionally separate from your application tables and is ignored during schema introspection/diffing of your app schema.

You can think of this as dibs building a “catalog” that makes tools (CLI, editors, admin UIs) more informative.

## Code generation (how dibs powers tooling)

dibs has codegen components to avoid making humans hand-maintain repetitive glue:

- The RPC protocol needs clients/dispatchers and strongly typed messages.
- Tooling (like editor integrations) needs structured schema info to power completions, hovers, and navigation.
- Some schema representations benefit from generating consistent Rust or SQL fragments.

From a user perspective, codegen exists so:

- the CLI and service can evolve together without you writing boilerplate,
- editor tooling can be “schema-aware” without re-implementing your schema DSL,
- you get typed, consistent behavior instead of stringly-typed hacks.

## Putting it all together (the lifecycle of a typical command)

When you run a command like “diff” or “generate migration SQL”:

1. The CLI spawns your db crate’s dibs service (fast, minimal startup).
2. The CLI asks the service to collect your intended schema.
3. The service connects to the database to introspect the current schema.
4. The service diffs intent vs reality into structured changes.
5. The solver orders and validates those changes by simulating the migration.
6. dibs renders the ordered changes to SQL.
7. The CLI displays the result (or requests that the service execute it and stream logs).

That layered design is the core of dibs: keep the user workflow simple, keep the “brain” close to your schema code, and make the hard parts (ordering, validation, error context) systematic rather than ad-hoc.
