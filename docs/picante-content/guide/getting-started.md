+++
title = "Getting Started"
weight = 1
+++

picante can be used in two ways:

- With macros (`#[picante::db]`, `#[picante::input]`, `#[picante::tracked]`, `#[picante::interned]`) for ergonomics.
- Manually by constructing ingredients (`InputIngredient`, `DerivedIngredient`, `InternedIngredient`) and an `IngredientRegistry`.

This page will grow into a short, task-focused walkthrough:

- define a database
- define a keyed input and a singleton input
- define a tracked query
- run a query and observe invalidation

