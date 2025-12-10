# XML Diff Rendering

How a diff-aware XML serializer should format output for various change scenarios.

## The Core Challenge

Normal XML serialization makes layout decisions (line breaks, indentation) based on:
- Element nesting depth
- Number of attributes
- Content length

Diff-aware serialization must ALSO consider:
- Which attributes/elements changed
- Which need `-`/`+` line treatment
- What context to show around changes
- What to collapse/hide

## Key Design Decisions

### No Indices

Indices are an implementation detail that shift when elements are added/removed/moved.
The diff output should NOT use indices. Instead:
- Collapse unchanged runs: `<!-- 5 unchanged -->`
- Show enough context that humans can orient themselves
- Let element content/ids identify what's what

### Moves Are Implicit

A moved element appears twice:
- `-` where it was (in the old position)
- `+` where it is now (in the new position)

Humans recognize it's the same element by its content. No need for arrows or explicit "moved from X to Y" annotations that break down with multiple moves.

### The Canonical Format

```xml
<svg>
  <!-- 2 unchanged -->
- <circle id="a"/>
  <rect id="3"/>
- <path id="b"/>
  <!-- 2 unchanged -->
  <rect id="7"
-   fill="red"
+   fill="blue"
  />
+ <ellipse id="new"/>
+ <circle id="a"/>
  <!-- 8 unchanged -->
</svg>
```

This shows:
- `circle#a` moved (appears as `-` then `+`)
- `path#b` deleted
- `rect#7` had attribute change
- `ellipse#new` inserted

---

## Scenario 1: Single Attribute Change

**Old:**
```xml
<rect fill="red" x="10" y="10" width="50" height="50"/>
```

**New:**
```xml
<rect fill="blue" x="10" y="10" width="50" height="50"/>
```

**Diff output:**
```xml
<rect
- fill="red"
+ fill="blue"
  x="10" y="10" width="50" height="50"
/>
```

**Layout decision:** `fill` gets its own lines because it changed. Unchanged attrs stay together.

---

## Scenario 2: Multiple Attribute Changes

**Old:**
```xml
<rect fill="red" x="10" y="10" width="50" height="50"/>
```

**New:**
```xml
<rect fill="blue" x="20" y="10" width="50" height="50"/>
```

**Diff output:**
```xml
<rect
- fill="red"
+ fill="blue"
- x="10"
+ x="20"
  y="10" width="50" height="50"
/>
```

**Layout decision:** Each changed attr gets `-`/`+` lines. Unchanged attrs grouped.

---

## Scenario 3: Attribute Added

**Old:**
```xml
<rect fill="red" x="10" y="10"/>
```

**New:**
```xml
<rect fill="red" x="10" y="10" stroke="black"/>
```

**Diff output:**
```xml
<rect
  fill="red" x="10" y="10"
+ stroke="black"
/>
```

**Layout decision:** New attr on its own `+` line.

---

## Scenario 4: Attribute Removed

**Old:**
```xml
<rect fill="red" x="10" y="10" stroke="black"/>
```

**New:**
```xml
<rect fill="red" x="10" y="10"/>
```

**Diff output:**
```xml
<rect
  fill="red" x="10" y="10"
- stroke="black"
/>
```

---

## Scenario 5: Child Element Added

**Old:**
```xml
<svg>
  <rect fill="red"/>
</svg>
```

**New:**
```xml
<svg>
  <rect fill="red"/>
  <circle cx="50" cy="50" r="25"/>
</svg>
```

**Diff output:**
```xml
<svg>
  <rect fill="red"/>
+ <circle cx="50" cy="50" r="25"/>
</svg>
```

**Layout decision:** Entire new element prefixed with `+`.

---

## Scenario 6: Child Element Removed

**Old:**
```xml
<svg>
  <rect fill="red"/>
  <circle cx="50" cy="50" r="25"/>
</svg>
```

**New:**
```xml
<svg>
  <rect fill="red"/>
</svg>
```

**Diff output:**
```xml
<svg>
  <rect fill="red"/>
- <circle cx="50" cy="50" r="25"/>
</svg>
```

---

## Scenario 7: Child Element Replaced (Different Type)

**Old:**
```xml
<svg>
  <rect fill="red"/>
</svg>
```

**New:**
```xml
<svg>
  <circle cx="50" cy="50" r="25"/>
</svg>
```

**Diff output:**
```xml
<svg>
- <rect fill="red"/>
+ <circle cx="50" cy="50" r="25"/>
</svg>
```

---

## Scenario 8: Children Swapped (Move Detection)

**Old:**
```xml
<svg>
  <rect id="a" fill="red"/>
  <circle id="b" cx="50" cy="50"/>
</svg>
```

**New:**
```xml
<svg>
  <circle id="b" cx="50" cy="50"/>
  <rect id="a" fill="red"/>
</svg>
```

**Diff output (with move detection):**
```xml
<svg>
  <circle id="b" cx="50" cy="50"/>  <!-- moved from [1] to [0] -->
  <rect id="a" fill="red"/>          <!-- moved from [0] to [1] -->
</svg>
```

**Alternative (inline annotation):**
```xml
<svg>
↑ <circle id="b" cx="50" cy="50"/>
↓ <rect id="a" fill="red"/>
</svg>
```

**Layout decision:** Show final state, annotate moves. Don't show as delete+insert.

---

## Scenario 9: Child Modified AND Moved

**Old:**
```xml
<svg>
  <rect id="a" fill="red"/>
  <circle id="b" cx="50" cy="50" r="25"/>
</svg>
```

**New:**
```xml
<svg>
  <circle id="b" cx="50" cy="50" r="30"/>
  <rect id="a" fill="red"/>
</svg>
```

**Diff output:**
```xml
<svg>
  <circle id="b" cx="50" cy="50"  <!-- moved from [1] to [0] -->
-   r="25"
+   r="30"
  />
  <rect id="a" fill="red"/>  <!-- moved from [0] to [1] -->
</svg>
```

**Layout decision:** Show move annotation AND the attribute change within.

---

## Scenario 10: Deep Nested Change

**Old:**
```xml
<svg>
  <g id="layer1">
    <g id="shapes">
      <rect fill="red"/>
    </g>
  </g>
</svg>
```

**New:**
```xml
<svg>
  <g id="layer1">
    <g id="shapes">
      <rect fill="blue"/>
    </g>
  </g>
</svg>
```

**Diff output (full context):**
```xml
<svg>
  <g id="layer1">
    <g id="shapes">
      <rect
-       fill="red"
+       fill="blue"
      />
    </g>
  </g>
</svg>
```

**Diff output (collapsed context):**
```xml
<svg>
  <g id="layer1">
    <g id="shapes">
      <rect
-       fill="red"
+       fill="blue"
      />
    </g>
  </g>
</svg>
```

Hmm, same in this case. But if there were siblings:

**Old:**
```xml
<svg>
  <g id="layer1">
    <!-- 50 unchanged elements -->
    <g id="shapes">
      <rect fill="red"/>
    </g>
    <!-- 50 more unchanged elements -->
  </g>
</svg>
```

**Diff output (collapsed):**
```xml
<svg>
  <g id="layer1">
    <!-- ... 50 unchanged elements ... -->
    <g id="shapes">
      <rect
-       fill="red"
+       fill="blue"
      />
    </g>
    <!-- ... 50 unchanged elements ... -->
  </g>
</svg>
```

---

## Scenario 11: Text Content Change

**Old:**
```xml
<text x="10" y="20">Hello</text>
```

**New:**
```xml
<text x="10" y="20">World</text>
```

**Diff output:**
```xml
<text x="10" y="20">
- Hello
+ World
</text>
```

**Layout decision:** Text content gets `-`/`+` treatment, element wraps it.

---

## Scenario 12: Mixed Content Changes

**Old:**
```xml
<text x="10" y="20" fill="red">Hello</text>
```

**New:**
```xml
<text x="10" y="20" fill="blue">World</text>
```

**Diff output:**
```xml
<text x="10" y="20"
- fill="red"
+ fill="blue"
>
- Hello
+ World
</text>
```

---

## Serializer Query API

The serializer needs to query the diff context. Key insight: **batch queries are better than one-at-a-time**.

Instead of checking each attribute individually, the serializer should get lists of changed vs unchanged children upfront. This enables grouping multiple changes on single lines.

```rust
trait DiffContext<'mem, 'facet> {
    /// Get all changed children at this path (attrs or elements)
    fn changed_children(&self, path: &Path) -> Vec<ChangedChild<'mem, 'facet>>;

    /// Get all unchanged children at this path
    fn unchanged_children(&self, path: &Path) -> Vec<UnchangedChild<'mem, 'facet>>;

    /// Get all deleted children (exist in old, not in new)
    fn deleted_children(&self, path: &Path) -> Vec<DeletedChild<'mem, 'facet>>;

    /// Get all inserted children (exist in new, not in old)
    fn inserted_children(&self, path: &Path) -> Vec<InsertedChild<'mem, 'facet>>;

    /// Should this entire subtree be collapsed?
    fn should_collapse(&self, path: &Path) -> bool;

    /// How many siblings are being collapsed at this point?
    fn collapsed_count(&self, path: &Path) -> Option<usize>;
}

struct ChangedChild<'mem, 'facet> {
    name: Cow<'static, str>,
    old_value: Peek<'mem, 'facet>,
    new_value: Peek<'mem, 'facet>,
}

struct UnchangedChild<'mem, 'facet> {
    name: Cow<'static, str>,
    value: Peek<'mem, 'facet>,
}

struct DeletedChild<'mem, 'facet> {
    name: Cow<'static, str>,
    value: Peek<'mem, 'facet>,
}

struct InsertedChild<'mem, 'facet> {
    name: Cow<'static, str>,
    value: Peek<'mem, 'facet>,
}
```

## Grouping Multiple Changes

When multiple attributes change, group them on single lines:

**Instead of:**
```xml
<rect
- fill="red"
+ fill="blue"
- x="10"
+ x="20"
  y="10" width="50" height="50"
/>
```

**Prefer:**
```xml
<rect
- fill="red" x="10"
+ fill="blue" x="20"
  y="10" width="50" height="50"
/>
```

One `-` line with all old values, one `+` line with all new values, then unchanged inline.

This requires the serializer to **collect before emitting** rather than streaming one-at-a-time.

## Serializer Decision Flow

```
for each element:
    if should_collapse(path):
        emit "<!-- ... N unchanged elements ... -->"
        skip children
        continue

    // Collect children by status
    changed = ctx.changed_children(path)
    unchanged = ctx.unchanged_children(path)
    deleted = ctx.deleted_children(path)
    inserted = ctx.inserted_children(path)

    emit "<tagname"

    if changed.is_empty() && deleted.is_empty() && inserted.is_empty():
        // All unchanged - single line
        emit attrs inline
        emit "/>" or ">"
    else:
        emit newline

        // Group changed attrs on two lines
        if !changed.is_empty():
            emit "- " + changed.map(|c| format!("{}={}", c.name, c.old_value)).join(" ")
            emit "+ " + changed.map(|c| format!("{}={}", c.name, c.new_value)).join(" ")

        // Deleted attrs
        for d in deleted:
            emit "- " + d.name + "=" + d.value

        // Inserted attrs
        for i in inserted:
            emit "+ " + i.name + "=" + i.value

        // Unchanged attrs inline (dimmed)
        if !unchanged.is_empty():
            emit "  " + unchanged inline (dimmed)

        emit ">"

    // Recurse for child elements with same logic...
```

## Implications for Serializer Architecture

This design means serializers can't just "walk and emit." They need to:

1. **Query the diff context** for batches of children
2. **Partition by change status** before emitting anything
3. **Buffer and group** changed items
4. **Control layout** based on what changed

This is a different pattern from normal serialization which is typically:
```rust
for field in fields {
    emit(field);
}
```

Diff-aware serialization is:
```rust
let (changed, unchanged, deleted, inserted) = partition_by_status(fields, ctx);
emit_changed_group(changed);
emit_deleted(deleted);
emit_inserted(inserted);
emit_unchanged_inline(unchanged);
```

---

## Open Questions

1. **Attribute ordering:** If attrs are reordered but values same, is that a change?
   - Probably not - XML attribute order is not significant

2. **Whitespace:** How to handle whitespace-only changes in text content?
   - Maybe a config option: `ignore_whitespace: bool`

3. **Namespace prefixes:** `svg:rect` vs `rect` with default namespace?
   - Serializer already handles this, diff shouldn't care

4. **CDATA sections:** `<![CDATA[...]]>` changes?
   - Treat as text content change

5. **Comments:** Should we diff comments?
   - Probably not by default, but could be an option
