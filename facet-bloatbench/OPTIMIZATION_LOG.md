# Facet Bloat Optimization Log

This document tracks measurements before and after each optimization attempt.

## Baseline (2025-12-06)

**Commit:** 3a41ec03 (benchmark-report-and-bloat-analysis branch)
**Command:** `./facet-bloatbench/measure.sh`

### Binary Sizes (release, --features facet,json)

| Metric | Value |
|--------|-------|
| Binary size | 1,471 KB (1,506,319 bytes) |
| Stripped size | 1,187 KB (1,215,712 bytes) |

### LLVM Lines Analysis

| Metric | Value |
|--------|-------|
| Total LLVM IR lines | 279,129 |
| Total monomorphized copies | 8,009 |

### Top Bloat Contributors

| Lines | Copies | Function |
|-------|--------|----------|
| 60,363 (21.6%) | 1,972 | `FnOnce::call_once` |
| 16,889 (6.1%) | 427 | `Option<T>::SHAPE` inner const closure |
| 13,373 (4.8%) | 539 | `Vec<T>::SHAPE` inner const closure |
| 10,975 (3.9%) | 176 | `Option<T>::SHAPE` outer closure |
| 10,188 (3.6%) | 368 | `transmute_copy` |
| 6,958 (2.5%) | 98 | `Vec<T>::SHAPE` outer closure |
| 5,500 (2.0%) | 250 | `PtrMut::drop_in_place` |
| 5,049 (1.8%) | 145 | `PtrUninit::put` |

### Closure Breakdown

| Source | Copies | Lines |
|--------|--------|-------|
| Option<T> closures | 603 (427+176) | 27,864 |
| Vec<T> closures | 637 (539+98) | 20,331 |
| drop_in_place | 250 | 5,500 |
| PtrUninit::put | 145 | 5,049 |

### Comparison to Serde

| Metric | Facet | Serde | Ratio |
|--------|-------|-------|-------|
| Binary (release+json) | 1,471 KB | 561 KB | 2.62x |
| Stripped | 1,187 KB | 465 KB | 2.55x |
| LLVM IR lines | 279,129 | 22,925 | 12.18x |
| Monomorphized copies | 8,009 | 984 | 8.14x |

---

## Optimization Attempts

### Attempt 1: [TBD]

**Date:**
**Branch:**
**Change:**

**Results:**

| Metric | Before | After | Delta |
|--------|--------|-------|-------|
| Binary size | | | |
| Stripped size | | | |
| LLVM IR lines | | | |
| Copies | | | |

**Notes:**

---
