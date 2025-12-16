# Performance Benchmarking Design System

**Purpose:** Single semantic system for colors, typography, and language across all perf.facet.rs pages.

## Color Semantics

Colors must carry consistent meaning across index and report pages:

### Semantic Colors

```css
/* Improvement / Faster / Better */
--good: light-dark(#1a7f37, #3fb950);

/* Regression / Slower / Worse */
--bad: light-dark(#cf222e, #f85149);

/* Neutral / Reference / No change */
--neutral: var(--muted);  /* light-dark(#656d76, #7d8590) */

/* Accent / Interactive / Highlighted */
--accent: light-dark(#2457f5, #7aa2f7);

/* Warning (use sparingly) */
--warn: light-dark(#9a6700, #f2cc60);
```

### Application Rules

**Delta States (with epsilon = 0.5%):**

| State | Condition | Color | Icon | Label |
|-------|-----------|-------|------|-------|
| **Improvement** | delta < -0.5% | `--good` (green) | ▲ | "faster" |
| **Stalemate** | \|delta\| ≤ 0.5% | `--neutral` (muted) | ▬ | "neutral" |
| **Regression** | delta > +0.5% | `--bad` (red) | ▼ | "slower" |

| Context | Color | Usage |
|---------|-------|-------|
| **Deltas** | `--good` = faster/lower | Green for improvements (< -0.5%) |
| | `--bad` = slower/higher | Red for regressions (> +0.5%) |
| | `--neutral` = stalemate | Muted gray for ≤0.5% (within noise) |
| **Table rows** | `--good` left border | Fastest implementation |
| | `--neutral` left border | Reference/baseline (serde_json, main) |
| | `--accent` left border | Highlighted implementation (JIT) |
| | `--bad` left border | Error state |
| **Charts** | `--accent` bars | Active/highlighted target |
| | `--chart-fade` bars | Inactive targets |
| **Errors** | `--bad` + text label | Always include "error" text, not color-only |

**Critical:** Never use color as the sole indicator. Always include text labels (e.g., "faster", "slower", "error").

## Typography Hierarchy

Use these exact sizes and weights globally:

### Hierarchy Levels

```css
/* Page title */
h1 {
  font-size: 24px;
  font-weight: 650;
  letter-spacing: -0.01em;
}

/* Section headers (category names) */
h2 {
  font-size: 18px;
  font-weight: 650;
}

/* Sub-section headers (benchmark names in reports) */
h3 {
  font-size: 14px;
  font-weight: 650;
  color: var(--muted);
}

/* Metadata / secondary text */
.meta {
  font-size: 12px;
  color: var(--muted);
}

/* Body text / table content */
body, td {
  font-size: 13px;
}

/* Small labels / helpers */
.small-label {
  font-size: 11px;
  color: var(--faint);
}
```

### Weight Hierarchy

1. **Primary content** (weight 650): Page title, section headers, fastest results
2. **Normal content** (weight 400): Body text, table data, commit subjects
3. **Secondary content** (weight 400, muted color): Metadata, timestamps, hashes

## Reference / Baseline Language

Make comparison context explicit and consistent:

### Index Page

When showing branch comparisons:

```
BASELINE: main @ a1b2c3d
702,426,274 instructions
updated 5m ago
```

If no main data:
```
REFERENCE (estimated)
Derived from median of branch results
```

### Report Page

When comparing implementations:

```
Instruction baseline: serde_json
Instr. Ratio: Instruction count relative to serde_json
  <1.0× = fewer instructions (green)
  >1.0× = more instructions (red)
```

**Rule:** Always state the reference **once per scope**, then rely on it implicitly in tables/charts.

## Instruction Ratio Semantics

**First-class metric:** `facet-format-json+jit / serde_json` instruction ratio

### Ratio Interpretation

| Ratio Value | Meaning | Color | Label |
|-------------|---------|-------|-------|
| < 0.995× | Fewer instructions than serde | Green (`--good`) | "fewer" |
| 0.995× - 1.005× | Within measurement noise | Muted (`--neutral`) | "neutral" |
| > 1.005× | More instructions than serde | Red (`--bad`) | "more" |

### Application Rules

- **Tables:** "Instr. Ratio" column shows `target_instructions / serde_json_instructions`
- **Always instruction-based:** Never show time-based ratios as "×"
- **Baseline:** serde_json is always 1.00× (the reference point)
- **Text labels:** Include "fewer", "more", or "neutral" alongside colored ratio

### Why This Matters

Instruction ratios are:
- Architecture-agnostic (stable across machines)
- Deterministic (no timing variance)
- Semantically meaningful (JIT efficiency measure)
- CI-friendly (reproducible in automation)

## Error Presentation

Standardize error rows across all pages:

```css
tr.errored {
  opacity: 0.5;
}

tr.errored td:first-child {
  border-left: 3px solid var(--bad);
  padding-left: 9px;
}

td.error {
  color: var(--bad);
  font-style: italic;
}
```

**Text content:** Always show "error" as the value, not just styling.

**Affordance:** If error details exist, provide tooltip or expandable reason.

## Navigation Consistency

### Breadcrumbs / "You are here"

All pages should have consistent navigation framing:

```html
<!-- Top navigation bar (sticky, full-width) -->
<nav class="top-nav">
  <a href="/">All branches</a> › <span>bench-improvements</span>
</nav>
```

Style:
```css
.top-nav {
  position: sticky;
  top: 0;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
  padding: 0.75rem 1rem;
  font-size: 13px;
  z-index: 100;
}
```

### Navigation Hierarchy

1. **Index** → **Branch reports** → **Full benchmark details**
2. Each level should have escape hatch to parent level
3. Breadcrumbs use same visual style everywhere

## Spacing System

Use consistent spacing scale:

```css
--s1: 4px;   /* Tight spacing within components */
--s2: 8px;   /* Component padding */
--s3: 12px;  /* Small gaps */
--s4: 16px;  /* Default gaps */
--s5: 20px;  /* Section spacing */
--s6: 32px;  /* Large section breaks */
```

## Commit Information

**Rule:** Display commit intent, not git mechanics.

### Extraction

```javascript
// Always extract first line of commit message
const subject = commit_message.split('\n')[0].trim();

// Fallback if empty
const display = subject || '(no message)';
```

### Display Context

**Index page:**
- Collapsed: Show commit subject (one line, truncated)
- Expanded: Show full commit subject + hash + time as secondary

**Report page:**
- Header: Show commit hash as link
- Breadcrumb: Show branch name only

**Never show:** "Merge X into Y" or other git merge metadata

## Chart Styling

### Color Usage

- **Summary charts:** Use `--accent` for facet, `--neutral` for serde
- **Individual charts:** Use `--chart-fade` (neutral) by default, `--accent` on hover
- **Grid lines:** Use `--border` opacity
- **Axis labels:** Use `--muted` color

### Interaction

- On table row hover → highlight corresponding chart bar
- On chart bar hover → highlight corresponding table row
- Use `opacity` transitions, not color changes

## Example Comparison

### Before (Inconsistent)

**Index:**
- Red arrow for regression ✗
- "last run 5m ago" ✓
- PR title prioritized ✗

**Report:**
- Green highlight for "best" ✗
- "Generated: timestamp" ✗
- Full commit hash in header ✗

### After (Consistent)

**Index:**
- Red delta with "slower" label ✓
- "last run 5m ago" ✓
- Commit subject prioritized ✓

**Report:**
- Green left border for "fastest" ✓
- "Generated: timestamp" ✓
- Commit subject in breadcrumb ✓

---

## Implementation Checklist

When adding new pages or components:

- [ ] Use semantic color variables (`--good`, `--bad`, `--neutral`, `--accent`)
- [ ] Include text labels with colors (not color-only)
- [ ] Match typography scale (24px/18px/14px/13px/12px/11px)
- [ ] Use weight hierarchy (650/400/400+muted)
- [ ] State reference context explicitly
- [ ] Show commit subjects (not hashes) as primary
- [ ] Use spacing scale (`--s1` through `--s6`)
- [ ] Add breadcrumbs/navigation framing
- [ ] Standardize error presentation
- [ ] Test in both light and dark modes
