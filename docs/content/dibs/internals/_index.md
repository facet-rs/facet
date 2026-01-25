+++
title = "Internals"
description = "How dibs works under the hood"
sort_by = "weight"
+++

# How dibs works

dibs keeps your Postgres schema in sync with your Rust types. It constantly reconciles **intent** (your Rust schema) with **reality** (the live database), then generates the <abbr title="Structured Query Language">SQL</abbr> to make them match.

## The pipeline

Most dibs commands follow the same steps:

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
    <div class="flow-body">Compare "intent vs reality" into a list of typed schema operations.</div>
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

## Architecture

Your "db crate" is a small process that speaks <abbr title="Remote Procedure Call">RPC</abbr> to the CLI (via [Roam](https://github.com/bearcove/roam)):

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

A typical exchange looks like:

<div class="chat">
  <div class="bubble bubble-cli">
    <div class="bubble-who">dibs-cli</div>
    <div class="bubble-text">What's the intended schema?</div>
  </div>

  <div class="bubble bubble-service">
    <div class="bubble-who">myapp-db</div>
    <div class="bubble-text">Here are the tables and columns I collected from your Rust schema.</div>
  </div>
</div>

dibs scans your crate for registered tables (Facet annotations), builds an internal schema model, and returns it over RPC.

<div class="chat">
  <div class="bubble bubble-cli">
    <div class="bubble-who">dibs-cli</div>
    <div class="bubble-text">Given this database URL, what changes are needed?</div>
  </div>

  <div class="bubble bubble-service">
    <div class="bubble-who">myapp-db</div>
    <div class="bubble-text">I connected to Postgres, introspected the current schema, and computed the diff against your Rust schema.</div>
  </div>
</div>

The result is a structured diff - see below.

<div class="chat">
  <div class="bubble bubble-cli">
    <div class="bubble-who">dibs-cli</div>
    <div class="bubble-text">What SQL should we run, in what order?</div>
  </div>

  <div class="bubble bubble-service">
    <div class="bubble-who">myapp-db</div>
    <div class="bubble-text">Here's ordered SQL that should execute cleanly.</div>
  </div>
</div>

The solver orders these operations - see below.

<div class="chat">
  <div class="bubble bubble-cli">
    <div class="bubble-who">dibs-cli</div>
    <div class="bubble-text">Run migrations and stream logs back to me.</div>
  </div>

  <div class="bubble bubble-service">
    <div class="bubble-who">myapp-db</div>
    <div class="bubble-text">Running pending migrations in transactions - streaming progress as they apply.</div>
  </div>
</div>

Why RPC? So you don't have to boot your entire app just to work with schemas and migrations.

A typical workspace has three crates: **myapp-db** (schema + migrations), **myapp-queries** (generated query helpers), and **myapp** (your application). The CLI talks to myapp-db via RPC; your app imports myapp-queries for typed database access.

## The diff

When intent and reality don't match, dibs produces a **diff**: a typed list of schema operations - create/drop/rename tables, add/alter/drop columns, indexes, constraints, foreign keys. This isn't raw SQL; it's structured data that dibs can reason about for ordering and safety.

## The solver

The same set of changes can succeed or fail depending on order - you can't add a foreign key before the referenced table exists, or drop a table while others still reference it.

The solver orders operations by simulating them on a virtual schema:

1. Start from the current schema state
2. Pick any change whose preconditions are satisfied
3. Apply it to the virtual schema
4. Repeat until all changes are scheduled

After ordering, dibs verifies that applying the changes to "current" produces "desired". If the solver can't make progress, there's either a true dependency cycle or a bug in diff generation.

## SQL generation

The solver's ordered operations become concrete <abbr title="Data Definition Language">DDL</abbr>: `CREATE TABLE`, `ALTER TABLE`, `CREATE INDEX`, etc. Because dibs knows *what* it's trying to do (not just the final SQL text), it can provide better errors and tooling.

## Running migrations

Migrations are Rust functions. Each runs in its own transaction - if it fails, the transaction rolls back and subsequent migrations don't run.

When something fails, dibs attaches context (what SQL was running, source location when available) so you get actionable errors instead of "postgres said no."

## Metadata tables

dibs maintains `__dibs_*` tables to record source locations (file/line/column), doc comments, and which migration created what. This powers richer tooling in the CLI, editors, and admin UIs - but it's separate from your app schema and ignored during diffing.
