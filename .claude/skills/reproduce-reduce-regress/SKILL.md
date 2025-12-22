---
name: reproduce-reduce-regress
description: Systematic workflow for debugging by reproducing bugs with real data, reducing test cases to minimal examples, and adding regression tests
---

# Reproduce, Reduce, Regress

When debugging a bug, follow this workflow:

## 1. Reproduce

**Goal:** Get a failing test that demonstrates the bug using REAL data.

- Copy the EXACT input that triggers the bug (don't paraphrase or simplify yet)
- Use the EXACT types/structs from the failing code
- Verify the test actually fails with the same error message

## 2. Reduce

**Goal:** Find the MINIMAL input that still triggers the bug.

- Create MULTIPLE test variants, don't comment things in/out
- Name them descriptively: `test_minimal_one_field`, `test_with_queries`, etc.
- Binary search: remove half the input, see if it still fails
- Keep narrowing until you find the smallest failing case
- Also find a PASSING case that's as close as possible to the failing one

The difference between your minimal failing case and your minimal passing case IS the bug.

## 3. Regress (Regression Tests)

**Goal:** Ensure the bug never comes back.

- Keep ALL your test variants - both passing and failing
- The failing ones become regression tests after the fix
- The passing ones document expected behavior
- Name tests after the issue number: `test_issue_1356_*`

## Anti-patterns

❌ Commenting code in/out to test different scenarios
❌ Modifying a single test repeatedly
❌ "Simplifying" input without verifying the bug still reproduces
❌ Deleting test variants after finding the bug
❌ Theorizing about what MIGHT cause the bug before reproducing

## Example Structure

```rust
// Minimal failing case
#[test]
fn test_issue_1356_fails_without_queries_default() { ... }

// Minimal passing case (shows the workaround)
#[test]
fn test_issue_1356_passes_with_queries_default() { ... }

// Original reproduction from user's code
#[test]
fn test_issue_1356_full_reproduction() { ... }
```
