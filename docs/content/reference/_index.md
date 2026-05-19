+++
title = "Reference"
sort_by = "weight"
weight = 2
insert_anchor_links = "heading"
+++

Lookup-oriented documentation: complete catalogs and comparison tables for when
you need a specific answer. Jump straight to what you need — every `#[facet(...)]`
attribute, split by where it goes.

<div class="guide-cards">
<a class="guide-card" href="/reference/container-attributes">
  <h3 id="container-attributes">Container attributes</h3>
  <p class="tagline">On the struct or enum</p>
  <p class="description"><code>rename_all</code>, <code>transparent</code>, <code>deny_unknown_fields</code>, <code>opaque</code>, <code>metadata_container</code>, <code>pod</code>, and more.</p>
</a>
<a class="guide-card" href="/reference/enum-attributes">
  <h3 id="enum-attributes">Enum &amp; variant attributes</h3>
  <p class="tagline">Tagging &amp; variants</p>
  <p class="description"><code>untagged</code>, <code>tag</code>, <code>tag</code> + <code>content</code>, and the <code>other</code> catch-all variant.</p>
</a>
<a class="guide-card" href="/reference/field-attributes">
  <h3 id="field-attributes">Field attributes</h3>
  <p class="tagline">On individual fields</p>
  <p class="description"><code>rename</code>, <code>default</code>, the <code>skip_*</code> family, <code>flatten</code>, <code>sensitive</code>, <code>invariants</code>, <code>proxy</code>, and more.</p>
</a>
<a class="guide-card" href="/reference/extension-attributes">
  <h3 id="extension-attributes">Extension attributes</h3>
  <p class="tagline">Format-specific namespaces</p>
  <p class="description"><code>args::</code>, <code>xml::</code>, <code>json::</code> … plus how to define your own grammar.</p>
</a>
<a class="guide-card" href="/reference/format-crate-matrix/">
  <h3 id="format-matrix">Format support matrix</h3>
  <p class="tagline">What works where</p>
  <p class="description">Per-type, per-attribute parity across every facet format crate.</p>
</a>
</div>

Looking for the bigger picture instead of a specific knob? The
[Ecosystem map](@/ecosystem/_index.md) lists every facet crate, and the
[Guide](@/guide/_index.md) walks through tasks end to end.
