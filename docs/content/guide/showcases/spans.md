+++
title = "Spans"
+++

<div class="showcase">

[`facet-pretty`](https://docs.rs/facet-pretty) formats type shapes with syntax highlighting and span tracking. Use it to build rich error diagnostics that point to specific fields or variants in type definitions, integrating with [miette](https://docs.rs/miette) for beautiful error reports.


## Highlight Modes


### Highlight Field Name

<section class="scenario">
<p class="description">Point to the field name when it's unknown or unexpected.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Config</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">name</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">max_retries</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">timeout_ms</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u32</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">enabled</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">bool</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> unknown field &#96;max_retries&#96;
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:4:5</span>]
 <span style="opacity:0.7">3</span> │     name: String,
 <span style="opacity:0.7">4</span> │     max_retries: u8,
   · <span style="color:#c678dd;font-weight:bold">    ─────┬─────</span>
   ·          <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">not expected here</span>
 <span style="opacity:0.7">5</span> │     timeout_ms: u32,
   ╰────
</code></pre>
</div>
</section>

### Highlight Type

<section class="scenario">
<p class="description">Point to the type when the value doesn't match.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Config</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">name</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">max_retries</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">timeout_ms</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u32</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">enabled</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">bool</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> value 1000 is out of range for u8
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:4:18</span>]
 <span style="opacity:0.7">3</span> │     name: String,
 <span style="opacity:0.7">4</span> │     max_retries: u8,
   · <span style="color:#c678dd;font-weight:bold">                 ─┬</span>
   ·                   <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected 0..255</span>
 <span style="opacity:0.7">5</span> │     timeout_ms: u32,
   ╰────
</code></pre>
</div>
</section>

### Highlight Entire Field

<section class="scenario">
<p class="description">Point to both name and type for context.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Config</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">name</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">max_retries</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">timeout_ms</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u32</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">enabled</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">bool</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> missing required field
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:5:5</span>]
 <span style="opacity:0.7">4</span> │     max_retries: u8,
 <span style="opacity:0.7">5</span> │     timeout_ms: u32,
   · <span style="color:#c678dd;font-weight:bold">    ───────┬───────</span>
   ·            <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">this field is required</span>
 <span style="opacity:0.7">6</span> │     enabled: bool,
   ╰────
</code></pre>
</div>
</section>

## Nested Structures


### Nested Struct Field

<section class="scenario">
<p class="description">Highlight a field inside a nested struct.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Employee</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">person</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(192,202,245)">Person</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">address</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(192,202,245)">Address</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">tags</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">Vec</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">metadata</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">HashMap</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span> <span style="color:rgb(115,218,202)">i32</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span>

<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Address</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">street</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">city</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">zip</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span>

<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Person</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">name</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">age</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">email</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">Option</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> invalid person data
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:3:13</span>]
 <span style="opacity:0.7">2</span> │ struct Employee {
 <span style="opacity:0.7">3</span> │     person: Person,
   · <span style="color:#c678dd;font-weight:bold">            ───┬──</span>
   ·                <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected valid Person</span>
 <span style="opacity:0.7">4</span> │     address: Address,
   ╰────
</code></pre>
</div>
</section>

### Deeply Nested Field

<section class="scenario">
<p class="description">Highlight a deeply nested field path.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Employee</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">person</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(192,202,245)">Person</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">address</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(192,202,245)">Address</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">tags</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">Vec</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">metadata</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">HashMap</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span> <span style="color:rgb(115,218,202)">i32</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span>

<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Address</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">street</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">city</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">zip</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span>

<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Person</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">name</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">age</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">email</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">Option</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> address validation failed
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:4:5</span>]
 <span style="opacity:0.7">3</span> │     person: Person,
 <span style="opacity:0.7">4</span> │     address: Address,
   · <span style="color:#c678dd;font-weight:bold">    ────────┬───────</span>
   ·             <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">city is required</span>
 <span style="opacity:0.7">5</span> │     tags: Vec&lt;String&gt;,
   ╰────
</code></pre>
</div>
</section>

## Enum Variants


### Unit Variant

<section class="scenario">
<p class="description">Highlight an enum variant name.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">repr</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">enum</span> <span style="color:rgb(192,202,245)">Status</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(192,202,245)">Active</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(192,202,245)">Pending</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(192,202,245)">Error</span> <span style="color:rgb(154,165,206)">{</span>
        <span style="color:rgb(125,207,255)">code</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">i32</span><span style="color:rgb(154,165,206)">,</span>

        <span style="color:rgb(125,207,255)">message</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>
    <span style="color:rgb(154,165,206)">}</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> invalid variant
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:4:5</span>]
 <span style="opacity:0.7">3</span> │ enum Status {
 <span style="opacity:0.7">4</span> │     Active,
   · <span style="color:#c678dd;font-weight:bold">    ───┬──</span>
   ·        <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">not allowed in this context</span>
 <span style="opacity:0.7">5</span> │     Pending,
   ╰────
</code></pre>
</div>
</section>

### Tuple Variant

<section class="scenario">
<p class="description">Highlight a tuple variant.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">repr</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">enum</span> <span style="color:rgb(192,202,245)">Message</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(192,202,245)">Text</span><span style="color:rgb(154,165,206)">(</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">)</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(192,202,245)">Number</span><span style="color:rgb(154,165,206)">(</span><span style="color:rgb(115,218,202)">i32</span><span style="color:rgb(154,165,206)">)</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(192,202,245)">Pair</span><span style="color:rgb(154,165,206)">(</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span> <span style="color:rgb(115,218,202)">i32</span><span style="color:rgb(154,165,206)">)</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(192,202,245)">Data</span> <span style="color:rgb(154,165,206)">{</span>
        <span style="color:rgb(125,207,255)">id</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u64</span><span style="color:rgb(154,165,206)">,</span>

        <span style="color:rgb(125,207,255)">payload</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">Vec</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>
    <span style="color:rgb(154,165,206)">}</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> type mismatch
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:4:10</span>]
 <span style="opacity:0.7">3</span> │ enum Message {
 <span style="opacity:0.7">4</span> │     Text(String),
   · <span style="color:#c678dd;font-weight:bold">         ───┬───</span>
   ·             <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected Number, got Text</span>
 <span style="opacity:0.7">5</span> │     Number(i32),
   ╰────
</code></pre>
</div>
</section>

### Struct Variant Field

<section class="scenario">
<p class="description">Highlight a field inside a struct variant.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">repr</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">enum</span> <span style="color:rgb(192,202,245)">Status</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(192,202,245)">Active</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(192,202,245)">Pending</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(192,202,245)">Error</span> <span style="color:rgb(154,165,206)">{</span>
        <span style="color:rgb(125,207,255)">code</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">i32</span><span style="color:rgb(154,165,206)">,</span>

        <span style="color:rgb(125,207,255)">message</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>
    <span style="color:rgb(154,165,206)">}</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> error code out of range
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:7:15</span>]
 <span style="opacity:0.7">6</span> │     Error {
 <span style="opacity:0.7">7</span> │         code: i32,
   · <span style="color:#c678dd;font-weight:bold">              ─┬─</span>
   ·                <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">must be positive</span>
 <span style="opacity:0.7">8</span> │         message: String,
   ╰────
</code></pre>
</div>
</section>

## Collections


### Vec Field

<section class="scenario">
<p class="description">Highlight a Vec field type.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Employee</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">person</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(192,202,245)">Person</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">address</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(192,202,245)">Address</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">tags</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">Vec</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">metadata</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">HashMap</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span> <span style="color:rgb(115,218,202)">i32</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span>

<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Address</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">street</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">city</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">zip</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span>

<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Person</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">name</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">age</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">email</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">Option</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> invalid tags
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:5:11</span>]
 <span style="opacity:0.7">4</span> │     address: Address,
 <span style="opacity:0.7">5</span> │     tags: Vec&lt;String&gt;,
   · <span style="color:#c678dd;font-weight:bold">          ─────┬─────</span>
   ·                <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected array of strings</span>
 <span style="opacity:0.7">6</span> │     metadata: HashMap&lt;String, i32&gt;,
   ╰────
</code></pre>
</div>
</section>

### Option Field

<section class="scenario">
<p class="description">Highlight an Option field.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Person</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">name</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">age</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">email</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">Option</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> invalid email format
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:5:5</span>]
 <span style="opacity:0.7">4</span> │     age: u8,
 <span style="opacity:0.7">5</span> │     email: Option&lt;String&gt;,
   · <span style="color:#c678dd;font-weight:bold">    ──────────┬──────────</span>
   ·               <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">must be a valid email address</span>
 <span style="opacity:0.7">6</span> │ }
   ╰────
</code></pre>
</div>
</section>

### HashMap Field

<section class="scenario">
<p class="description">Highlight a HashMap field.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre><code><span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Employee</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">person</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(192,202,245)">Person</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">address</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(192,202,245)">Address</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">tags</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">Vec</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">metadata</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">HashMap</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span> <span style="color:rgb(115,218,202)">i32</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span>

<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Address</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">street</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">city</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">zip</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span>

<span style="color:rgb(137,221,255)">#[</span><span style="color:rgb(122,162,247)">derive</span><span style="color:rgb(137,221,255)">(</span><span style="color:rgb(122,162,247)">Facet</span><span style="color:rgb(137,221,255)">)]</span>
<span style="color:rgb(187,154,247)">struct</span> <span style="color:rgb(192,202,245)">Person</span> <span style="color:rgb(154,165,206)">{</span>
    <span style="color:rgb(125,207,255)">name</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">age</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(115,218,202)">u8</span><span style="color:rgb(154,165,206)">,</span>

    <span style="color:rgb(125,207,255)">email</span><span style="color:rgb(154,165,206)">:</span> <span style="color:rgb(255,158,100)">Option</span><span style="color:rgb(154,165,206)">&lt;</span><span style="color:rgb(115,218,202)">String</span><span style="color:rgb(154,165,206)">&gt;</span><span style="color:rgb(154,165,206)">,</span>
<span style="color:rgb(154,165,206)">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> invalid metadata
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">target type:6:15</span>]
 <span style="opacity:0.7">5</span> │     tags: Vec&lt;String&gt;,
 <span style="opacity:0.7">6</span> │     metadata: HashMap&lt;String, i32&gt;,
   · <span style="color:#c678dd;font-weight:bold">              ──────────┬─────────</span>
   ·                         <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">keys must be alphanumeric</span>
 <span style="opacity:0.7">7</span> │ }
   ╰────
</code></pre>
</div>
</section>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/c5842bc4cd833fedc52522b20f09daedff260a0e/facet-pretty/examples/spans_showcase.rs"><code>facet-pretty/examples/spans_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/c5842bc4cd833fedc52522b20f09daedff260a0e"><code>c5842bc4c</code></a></dd>
<dt>Generated</dt><dd><time datetime="2026-01-04T12:56:12+01:00">2026-01-04T12:56:12+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
