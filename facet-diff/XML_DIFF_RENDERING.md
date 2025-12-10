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

The serializer needs to ask these questions at each point:

```rust
trait DiffContext {
    /// Is this path in the "changed set"? (changed, or ancestor of change)
    fn is_relevant(&self, path: &Path) -> bool;

    /// Did this specific leaf value change?
    fn is_changed(&self, path: &Path) -> bool;

    /// Get the old value (for showing `-` line)
    fn old_value(&self, path: &Path) -> Option<Peek>;

    /// Was this node moved? If so, from where?
    fn moved_from(&self, path: &Path) -> Option<Path>;

    /// Was this node moved? If so, to where? (for old tree traversal)
    fn moved_to(&self, path: &Path) -> Option<Path>;

    /// Should this node be collapsed? (unchanged, not near any changes)
    fn should_collapse(&self, path: &Path) -> bool;

    /// How many unchanged siblings are being collapsed here?
    fn collapsed_count(&self, path: &Path) -> Option<usize>;
}
```

## Serializer Decision Flow

```
for each element:
    if should_collapse(path):
        emit "<!-- ... N unchanged elements ... -->"
        skip children
        continue

    emit "<tagname"

    // Group attributes by change status
    changed_attrs = attrs.filter(|a| is_changed(a.path))
    unchanged_attrs = attrs.filter(|a| !is_changed(a.path))

    if changed_attrs.is_empty():
        // All on one line
        emit unchanged_attrs inline
    else:
        emit newline
        for attr in changed_attrs:
            emit "- " + attr.name + "=" + old_value(attr.path)
            emit "+ " + attr.name + "=" + attr.value
        if unchanged_attrs.not_empty():
            emit "  " + unchanged_attrs inline

    emit ">"

    // Similar logic for children...
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
