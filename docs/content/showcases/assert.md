+++
title = "facet-assert: Structural Assertions"
+++

<div class="showcase">

## Same Values

<section class="scenario">
<p class="description">Two values with identical content pass <code>assert_same!</code> — no <code>PartialEq</code> required.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">debug</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">tags</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Config</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">host</span><span style="opacity:0.7">: </span><span style="color:rgb(98,224,81)">localhost</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">port</span><span style="opacity:0.7">: </span><span style="color:rgb(224,186,81)">8080</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">debug</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">tags</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Vec&lt;String&gt;</span><span style="opacity:0.7"> [</span>
    <span style="color:rgb(98,224,81)">prod</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(98,224,81)">api</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Cross-Type Comparison

<section class="scenario">
<p class="description">Different type names (<code>Config</code> vs <code>ConfigV2</code>) with the same structure are considered "same". Useful for comparing DTOs across API versions or testing serialization roundtrips.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">debug</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">tags</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Config</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">host</span><span style="opacity:0.7">: </span><span style="color:rgb(98,224,81)">localhost</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">port</span><span style="opacity:0.7">: </span><span style="color:rgb(224,186,81)">8080</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">debug</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">tags</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Vec&lt;String&gt;</span><span style="opacity:0.7"> [</span>
    <span style="color:rgb(98,224,81)">prod</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Nested Structs

<section class="scenario">
<p class="description">Nested structs are compared recursively, field by field.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Person </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">age</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">address</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Address,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Address </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">street</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">city</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Person</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(98,224,81)">Alice</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">age</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">30</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">address</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Address</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">street</span><span style="opacity:0.7">: </span><span style="color:rgb(98,224,81)">123 Main St</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">city</span><span style="opacity:0.7">: </span><span style="color:rgb(98,224,81)">Springfield</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Structural Diff

<section class="scenario">
<p class="description">When values differ, you get a precise structural diff showing exactly which fields changed and at what path — not just a wall of red/green text.</p>
<div class="diff-output">
<h4>Diff Output</h4>
<pre><code><span style="font-weight:bold">.host</span>:
  <span style="color:#e06c75">- localhost</span>
  <span style="color:#98c379">+ prod.example.com</span>
<span style="font-weight:bold">.port</span>:
  <span style="color:#e06c75">- 8080</span>
  <span style="color:#98c379">+ 443</span>
<span style="font-weight:bold">.debug</span>:
  <span style="color:#e06c75">- true</span>
  <span style="color:#98c379">+ false</span>
<span style="font-weight:bold">.tags[1]</span> (only in left):
  <span style="color:#e06c75">- api</span>
</code></pre>
</div>
</section>

## Vector Differences

<section class="scenario">
<p class="description">Vector comparisons show exactly which indices differ, which elements were added, and which were removed.</p>
<div class="diff-output">
<h4>Diff Output</h4>
<pre><code><span style="font-weight:bold">[2]</span>:
  <span style="color:#e06c75">- 3</span>
  <span style="color:#98c379">+ 99</span>
<span style="font-weight:bold">[4]</span> (only in left):
  <span style="color:#e06c75">- 5</span>
</code></pre>
</div>
</section>
</div>
