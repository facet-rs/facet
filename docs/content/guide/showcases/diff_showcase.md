+++
title = "Structural Diff"
+++

<div class="showcase">

[`facet-diff`](https://docs.rs/facet-diff) provides structural diffing for any `Facet` type. Get readable, colored diffs showing exactly what changed between two values — perfect for debugging, testing, and understanding data transformations.


## Struct Field Changes

<section class="scenario">
<p class="description">Compare two structs and see exactly which fields changed. Unchanged fields are collapsed into a summary.</p>
<div class="diff-output">
<h4>Diff Output</h4>
<pre><code><span style="color:rgb(86,95,137)">{</span>
  <span style="color:rgb(86,95,137)">.. 1 unchanged field</span>
  <span style="color:rgb(115,218,202)">debug</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">true</span> → <span style="color:rgb(115,218,202)">false</span>
  <span style="color:rgb(115,218,202)">features</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(86,95,137)">[</span>
    <span style="color:rgb(86,95,137)">.. 1 unchanged item</span>
    <span style="color:rgb(247,118,142)">"metrics"</span> → <span style="color:rgb(115,218,202)">"tracing"</span>
  <span style="color:rgb(86,95,137)">]</span>
  <span style="color:rgb(115,218,202)">version</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">1</span> → <span style="color:rgb(115,218,202)">2</span>
<span style="color:rgb(86,95,137)">}</span></code></pre>
</div>
</section>

## Option Field Changes

<section class="scenario">
<p class="description">Option fields show clean `None` → `Some(...)` transitions without verbose type names.</p>
<div class="diff-output">
<h4>Diff Output</h4>
<pre><code><span style="color:rgb(86,95,137)">{</span>
  <span style="color:rgb(86,95,137)">.. 1 unchanged field</span>
  <span style="color:rgb(115,218,202)">age</span><span style="color:rgb(86,95,137)">:</span> <span style="font-weight:bold">Some</span> <span style="color:rgb(247,118,142)">30</span> → <span style="color:rgb(115,218,202)">31</span>
  
  <span style="color:rgb(115,218,202)">bio</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">None</span> → <span style="color:rgb(115,218,202)">Some("Software engineer")</span>
  <span style="color:rgb(115,218,202)">email</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">None</span> → <span style="color:rgb(115,218,202)">Some("alice@example.com")</span>
<span style="color:rgb(86,95,137)">}</span></code></pre>
</div>
</section>

## Nested Structure Diffs

<section class="scenario">
<p class="description">Structs with nested vectors are diffed recursively, showing changes at any depth.</p>
<div class="diff-output">
<h4>Diff Output</h4>
<pre><code><span style="color:rgb(86,95,137)">{</span>
  <span style="color:rgb(115,218,202)">author</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">"Alice"</span> → <span style="color:rgb(115,218,202)">"Bob"</span>
  <span style="color:rgb(115,218,202)">tags</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(86,95,137)">[</span>
    <span style="color:rgb(86,95,137)">.. 1 unchanged item</span>
    <span style="color:rgb(247,118,142)">"guide"</span> → <span style="color:rgb(115,218,202)">"reference"</span>
  <span style="color:rgb(86,95,137)">]</span>
  <span style="color:rgb(115,218,202)">title</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">"API Guide"</span> → <span style="color:rgb(115,218,202)">"API Reference"</span>
<span style="color:rgb(86,95,137)">}</span></code></pre>
</div>
</section>

## Vector Diffs

<section class="scenario">
<p class="description">Vector comparisons identify which elements changed while preserving context around the changes.</p>
<div class="diff-output">
<h4>Diff Output</h4>
<pre><code><span style="color:rgb(86,95,137)">[</span>
  <span style="color:rgb(86,95,137)">.. 2 unchanged items</span>
  <span style="color:rgb(247,118,142)">3</span> → <span style="color:rgb(115,218,202)">99</span>
  <span style="color:rgb(86,95,137)">.. 2 unchanged items</span>
<span style="color:rgb(86,95,137)">]</span></code></pre>
</div>
</section>

## Enum Variant Changes

<section class="scenario">
<p class="description">When enum variants differ entirely, the diff shows a clean replacement. When only the variant's fields differ, those specific changes are highlighted.</p>
<div class="diff-output">
<h4>Diff Output</h4>
<pre><code><span style="color:rgb(247,118,142)">Status::InProgress {
  assignee: "Alice",
}</span> → <span style="color:rgb(115,218,202)">Status::Completed {
  by: "Alice",
  notes: Some("Shipped in v2.0"),
}</span></code></pre>
</div>
</section>

## Same Variant, Different Fields

<section class="scenario">
<p class="description">When comparing the same enum variant with different field values, only the changed fields are shown.</p>
<div class="diff-output">
<h4>Diff Output</h4>
<pre><code><span style="font-weight:bold">Completed</span> <span style="color:rgb(86,95,137)">{</span>
  <span style="color:rgb(115,218,202)">by</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">"Alice"</span> → <span style="color:rgb(115,218,202)">"Bob"</span>
  <span style="color:rgb(115,218,202)">notes</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">None</span> → <span style="color:rgb(115,218,202)">Some("Peer reviewed")</span>
<span style="color:rgb(86,95,137)">}</span></code></pre>
</div>
</section>
</div>
