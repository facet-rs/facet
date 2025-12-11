+++
title = "Assertions"
+++

<div class="showcase">

[`facet-assert`](https://docs.rs/facet-assert) provides structural assertions for any `Facet` type without requiring `PartialEq` or `Debug`. Compare values across different types with identical structure, and get precise structural diffs showing exactly which fields differ.


## Same Values

<section class="scenario">
<p class="description">Two values with identical content pass <code>assert_same!</code> — no <code>PartialEq</code> required.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">debug</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">tags</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Config</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">host</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">localhost</span>"<span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">port</span><span style="opacity:0.7">: </span><span style="color:rgb(224,186,81)">8080</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">debug</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">tags</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;String&gt;</span><span style="opacity:0.7"> [</span>"<span style="color:rgb(158,206,106)">prod</span>"<span style="opacity:0.7">,</span> "<span style="color:rgb(158,206,106)">api</span>"<span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Cross-Type Comparison

<section class="scenario">
<p class="description">Different type names (<code>Config</code> vs <code>ConfigV2</code>) with the same structure are considered "same". Useful for comparing DTOs across API versions or testing serialization roundtrips.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">debug</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">tags</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Config</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">host</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">localhost</span>"<span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">port</span><span style="opacity:0.7">: </span><span style="color:rgb(224,186,81)">8080</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">debug</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">tags</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;String&gt;</span><span style="opacity:0.7"> [</span>"<span style="color:rgb(158,206,106)">prod</span>"<span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Nested Structs

<section class="scenario">
<p class="description">Nested structs are compared recursively, field by field.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Person </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">age</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">address</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Address,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Address </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">street</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">city</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Person</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">name</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Alice</span>"<span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">age</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">30</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">address</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Address</span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">street</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">123 Main St</span>"<span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">city</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Springfield</span>"<span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Structural Diff

<section class="scenario">
<p class="description">When values differ, you get a precise structural diff showing exactly which fields changed and at what path — then render it as Rust, JSON, or XML for whichever toolchain you need.</p>
<div class="diff-output">
<h4>Rust Diff Output</h4>
<pre><code><span style="color:rgb(86,95,137)">{</span>
    <span style="color:rgb(115,218,202)">debug</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">true</span> → <span style="color:rgb(115,218,202)">false</span>
    <span style="color:rgb(115,218,202)">host</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">"localhost"</span> → <span style="color:rgb(115,218,202)">"prod.example.com"</span>
    <span style="color:rgb(115,218,202)">port</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(247,118,142)">8080</span> → <span style="color:rgb(115,218,202)">443</span>
    <span style="color:rgb(115,218,202)">tags</span><span style="color:rgb(86,95,137)">:</span> <span style="color:rgb(86,95,137)">[</span>
        <span style="color:rgb(86,95,137)">.. 1 unchanged item</span>
        <span style="color:rgb(247,118,142)">- "api"</span>
    <span style="color:rgb(86,95,137)">]</span>
<span style="color:rgb(86,95,137)">}</span></code></pre>
</div>
<div class="diff-output">
<h4>JSON Diff Output</h4>
<pre><code>    <span style="color:rgb(220,220,220)">{</span> <span style="color:rgb(100,100,100)">/* Config */</span>
      <span style="color:rgb(229,192,123);opacity:0.7">←</span> <span style="color:rgb(255,234,162);opacity:0.7">"debug": </span><span style="color:rgb(255,234,162);opacity:0.7">true</span><span style="color:rgb(224,209,189);opacity:0.7"></span> , <span style="color:rgb(255,234,162);opacity:0.7">"host": </span><span style="color:rgb(229,192,123);opacity:0.7">"localhost"</span><span style="color:rgb(224,209,189);opacity:0.7"></span>       , <span style="color:rgb(255,234,162);opacity:0.7">"port": </span><span style="color:rgb(255,234,162);opacity:0.7">8080</span><span style="color:rgb(224,209,189);opacity:0.7"></span>
      <span style="color:rgb(97,175,239);opacity:0.7">→</span> <span style="color:rgb(142,216,255);opacity:0.7">"debug": </span><span style="color:rgb(142,216,255);opacity:0.7">false</span><span style="color:rgb(184,204,228);opacity:0.7"></span>, <span style="color:rgb(142,216,255);opacity:0.7">"host": </span><span style="color:rgb(97,175,239);opacity:0.7">"prod.example.com"</span><span style="color:rgb(184,204,228);opacity:0.7"></span>, <span style="color:rgb(142,216,255);opacity:0.7">"port": </span><span style="color:rgb(142,216,255);opacity:0.7">443</span><span style="color:rgb(184,204,228);opacity:0.7"></span>
    <span style="color:rgb(220,220,220)"></span>
        <span style="color:rgb(220,220,220)">"tags": [</span><span style="color:rgb(220,220,220)">]</span><span style="color:rgb(100,100,100)">,</span>
    <span style="color:rgb(220,220,220)">}</span>
</code></pre>
</div>
<div class="diff-output">
<h4>XML Diff Output</h4>
<pre><code>    <span style="color:rgb(220,220,220)">&lt;Config</span>
      <span style="color:rgb(229,192,123);opacity:0.7">←</span> <span style="color:rgb(255,234,162);opacity:0.7">debug="</span><span style="color:rgb(255,234,162);opacity:0.7">true</span><span style="color:rgb(224,209,189);opacity:0.7">"</span>  <span style="color:rgb(255,234,162);opacity:0.7">host="</span><span style="color:rgb(229,192,123);opacity:0.7">localhost</span><span style="color:rgb(224,209,189);opacity:0.7">"</span>        <span style="color:rgb(255,234,162);opacity:0.7">port="</span><span style="color:rgb(255,234,162);opacity:0.7">8080</span><span style="color:rgb(224,209,189);opacity:0.7">"</span>
      <span style="color:rgb(97,175,239);opacity:0.7">→</span> <span style="color:rgb(142,216,255);opacity:0.7">debug="</span><span style="color:rgb(142,216,255);opacity:0.7">false</span><span style="color:rgb(184,204,228);opacity:0.7">"</span> <span style="color:rgb(142,216,255);opacity:0.7">host="</span><span style="color:rgb(97,175,239);opacity:0.7">prod.example.com</span><span style="color:rgb(184,204,228);opacity:0.7">"</span> <span style="color:rgb(142,216,255);opacity:0.7">port="</span><span style="color:rgb(142,216,255);opacity:0.7">443</span><span style="color:rgb(184,204,228);opacity:0.7">"</span>
    <span style="color:rgb(220,220,220)">&gt;</span>
        <span style="color:rgb(220,220,220)">&lt;tags&gt;</span><span style="color:rgb(220,220,220)">&lt;/tags&gt;</span><span style="color:rgb(100,100,100)"></span>
    <span style="color:rgb(220,220,220)">&lt;/Config&gt;</span>
</code></pre>
</div>
</section>

## Vector Differences

<section class="scenario">
<p class="description">Vector comparisons show exactly which indices differ, which elements were added, and which were removed.</p>
<div class="diff-output">
<h4>Diff Output</h4>
<pre><code><span style="color:rgb(86,95,137)">[</span>
    <span style="color:rgb(86,95,137)">.. 2 unchanged items</span>
    <span style="color:rgb(247,118,142)">3</span> → <span style="color:rgb(115,218,202)">99</span>
    <span style="color:rgb(86,95,137)">.. 1 unchanged item</span>
    <span style="color:rgb(247,118,142)">- 5</span>
<span style="color:rgb(86,95,137)">]</span></code></pre>
</div>
</section>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/c7bc095e123eeb10ec9201e9972b3d9d0a43ee01/facet-assert/examples/assert_showcase.rs"><code>facet-assert/examples/assert_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/c7bc095e123eeb10ec9201e9972b3d9d0a43ee01"><code>c7bc095e1</code></a></dd>
<dt>Generated</dt><dd><time datetime="2025-12-11T12:16:12+01:00">2025-12-11T12:16:12+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
