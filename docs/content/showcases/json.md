+++
title = "facet-json Comprehensive Showcase"
+++

<div class="showcase">

## Basic Struct

<section class="scenario">
<p class="description">Simple struct with optional field serialized to JSON.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Person </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">age</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Person</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Alice</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">age</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">30</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">email</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Option&lt;String&gt;</span><span style="opacity:0.7">::Some(</span><span style="color:rgb(188,224,81)">alice@example.com</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">name</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Alice</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">age</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">30</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">email</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">alice@example.com</span><span style="color:#89ddff;">&quot;
</span><span style="color:#9abdf5;">}</span></pre>

</div>
</section>

## Nested Structs

<section class="scenario">
<p class="description">Struct containing nested struct and vector.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Company </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">address</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Address,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">employees</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
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
<pre><code><span style="font-weight:bold">Company</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Acme Corp</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">address</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Address</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">street</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">123 Main St</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">city</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Springfield</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">employees</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Vec&lt;String&gt;</span><span style="opacity:0.7"> [</span>
    <span style="color:rgb(188,224,81)">Bob</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(188,224,81)">Carol</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(188,224,81)">Dave</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">name</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Acme Corp</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">address</span><span style="color:#89ddff;">&quot;: </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#0db9d7;">street</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">123 Main St</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#0db9d7;">city</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Springfield</span><span style="color:#89ddff;">&quot;
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">}</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">employees</span><span style="color:#89ddff;">&quot;: </span><span style="color:#9abdf5;">[
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Bob</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Carol</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Dave</span><span style="color:#89ddff;">&quot;
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">]
</span><span style="color:#9abdf5;">}</span></pre>

</div>
</section>

## Externally Tagged Enum (default)

<section class="scenario">
<p class="description">Default enum serialization with external tagging: <code>{"Variant": content}</code></p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">[Message; 3]</span><span style="opacity:0.7"> [</span>
  <span style="font-weight:bold">Message</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Text</span><span style="opacity:0.7">(</span>
    <span style="color:rgb(188,224,81)">Hello, world!</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="font-weight:bold">Message</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Image</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">url</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">https://example.com/cat.jpg</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">width</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">800</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="font-weight:bold">Message</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Ping</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">[
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">Text</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Hello, world!</span><span style="color:#89ddff;">&quot;
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">}</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">Image</span><span style="color:#89ddff;">&quot;: </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">      </span><span style="color:#89ddff;">&quot;</span><span style="color:#0db9d7;">url</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">https://example.com/cat.jpg</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">      </span><span style="color:#89ddff;">&quot;</span><span style="color:#0db9d7;">width</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">800
</span><span style="color:#c0caf5;">    </span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">}</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Ping</span><span style="color:#89ddff;">&quot;
</span><span style="color:#9abdf5;">]</span></pre>

</div>
</section>

## Internally Tagged Enum

<section class="scenario">
<p class="description">Enum with internal tagging using <code>#[facet(tag = "type")]</code> - variant name becomes a field.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">[ApiResponse; 2]</span><span style="opacity:0.7"> [</span>
  <span style="font-weight:bold">ApiResponse</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Success</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">data</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Operation completed</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="font-weight:bold">ApiResponse</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Error</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">code</span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">404</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">message</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Not found</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">[
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">type</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Success</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">data</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Operation completed</span><span style="color:#89ddff;">&quot;
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">}</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">type</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Error</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">code</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">404</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">message</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Not found</span><span style="color:#89ddff;">&quot;
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">}
</span><span style="color:#9abdf5;">]</span></pre>

</div>
</section>

## Adjacently Tagged Enum

<section class="scenario">
<p class="description">Enum with adjacent tagging using <code>#[facet(tag = "t", content = "c")]</code> - variant name and content are separate fields.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">[Event; 3]</span><span style="opacity:0.7"> [</span>
  <span style="font-weight:bold">Event</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Click</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">x</span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">100</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">y</span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">200</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="font-weight:bold">Event</span><span style="opacity:0.7">::</span><span style="font-weight:bold">KeyPress</span><span style="opacity:0.7">(</span>
    <span style="color:rgb(81,224,91)">A</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="font-weight:bold">Event</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Resize</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">[
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">t</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Click</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">c</span><span style="color:#89ddff;">&quot;: </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">      </span><span style="color:#89ddff;">&quot;</span><span style="color:#0db9d7;">x</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">100</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">      </span><span style="color:#89ddff;">&quot;</span><span style="color:#0db9d7;">y</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">200
</span><span style="color:#c0caf5;">    </span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">}</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">t</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">KeyPress</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">c</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">A</span><span style="color:#89ddff;">&quot;
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">}</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">t</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Resize</span><span style="color:#89ddff;">&quot;
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">}
</span><span style="color:#9abdf5;">]</span></pre>

</div>
</section>

## Untagged Enum

<section class="scenario">
<p class="description">Enum with <code>#[facet(untagged)]</code> - no tagging, relies on JSON structure to determine variant.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">[StringOrNumber; 2]</span><span style="opacity:0.7"> [</span>
  <span style="font-weight:bold">StringOrNumber</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Str</span><span style="opacity:0.7">(</span>
    <span style="color:rgb(188,224,81)">hello</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="font-weight:bold">StringOrNumber</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Num</span><span style="opacity:0.7">(</span>
    <span style="color:rgb(222,81,224)">42</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">[
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">hello</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">  </span><span style="color:#ff9e64;">42
</span><span style="color:#9abdf5;">]</span></pre>

</div>
</section>

## Maps with String Keys

<section class="scenario">
<p class="description">HashMap with string keys serializes to JSON object.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">HashMap&lt;String, i32&gt;</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(188,224,81)">one</span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">1</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(188,224,81)">two</span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">2</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">one</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">1</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">two</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">2
</span><span style="color:#9abdf5;">}</span></pre>

</div>
</section>

## Maps with Integer Keys

<section class="scenario">
<p class="description">HashMap with integer keys - keys are stringified for JSON compatibility.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">HashMap&lt;i32, &amp;str&gt;</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(224,81,93)">2</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">two</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(224,81,93)">1</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">one</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">2</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">two</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">1</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">one</span><span style="color:#89ddff;">&quot;
</span><span style="color:#9abdf5;">}</span></pre>

</div>
</section>

## Tuple Struct

<section class="scenario">
<p class="description">Tuple struct serializes as JSON array.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Point</span><span style="color:#9abdf5;">(</span><span style="color:#bb9af7;">i32</span><span style="color:#c0caf5;">, </span><span style="color:#bb9af7;">i32</span><span style="color:#c0caf5;">, </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">;</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Point</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">0</span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">10</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">1</span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">20</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">2</span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">30</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">[
</span><span style="color:#c0caf5;">  </span><span style="color:#ff9e64;">10</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#ff9e64;">20</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#ff9e64;">30
</span><span style="color:#9abdf5;">]</span></pre>

</div>
</section>

## Compact JSON Output

<section class="scenario">
<p class="description">Compact serialization - all on one line, minimal whitespace.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">debug</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">max_connections</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">endpoints</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Config</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">debug</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">max_connections</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">100</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">endpoints</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Vec&lt;String&gt;</span><span style="opacity:0.7"> [</span>
    <span style="color:rgb(188,224,81)">https://api1.example.com</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(188,224,81)">https://api2.example.com</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">debug</span><span style="color:#89ddff;">&quot;:</span><span style="color:#ff9e64;">true</span><span style="color:#89ddff;">,&quot;</span><span style="color:#7aa2f7;">max_connections</span><span style="color:#89ddff;">&quot;:</span><span style="color:#ff9e64;">100</span><span style="color:#89ddff;">,&quot;</span><span style="color:#7aa2f7;">endpoints</span><span style="color:#89ddff;">&quot;:</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">https://api1.example.com</span><span style="color:#89ddff;">&quot;,&quot;</span><span style="color:#9ece6a;">https://api2.example.com</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">]}</span></pre>

</div>
</section>

## Pretty JSON Output

<section class="scenario">
<p class="description">Pretty-printed serialization - formatted with indentation and newlines.</p>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">debug</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">max_connections</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">endpoints</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Config</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">debug</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">max_connections</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">100</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">endpoints</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Vec&lt;String&gt;</span><span style="opacity:0.7"> [</span>
    <span style="color:rgb(188,224,81)">https://api1.example.com</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(188,224,81)">https://api2.example.com</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="serialized-output">
<h4>JSON Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">debug</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">true</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">max_connections</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">100</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">endpoints</span><span style="color:#89ddff;">&quot;: </span><span style="color:#9abdf5;">[
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">https://api1.example.com</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">https://api2.example.com</span><span style="color:#89ddff;">&quot;
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">]
</span><span style="color:#9abdf5;">}</span></pre>

</div>
</section>

## Syntax Error: Unexpected Character

<section class="scenario">
<p class="description">Invalid character at the start of JSON input.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#c0caf5;">@invalid</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::token</span>

  <span style="color:#e06c75">×</span> unexpected character: '@' (while parsing i32)
   ╭────
 <span style="opacity:0.7">1</span> │ @invalid
   · <span style="color:#c678dd;font-weight:bold">┬</span>
   · <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected '@', expected i32</span>
   ╰────
</code></pre>
</div>
</section>

## Syntax Error: Invalid Character in Object

<section class="scenario">
<p class="description">Invalid character appears mid-parse with surrounding context visible.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">name</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">test</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#7aa2f7;">value</span><span style="color:#89ddff;">&quot;: </span><span style="color:#f7768e;">@bad</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Data </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">value</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::token</span>

  <span style="color:#e06c75">×</span> unexpected character: '@' (while parsing i32)
   ╭────
 <span style="opacity:0.7">1</span> │ {"name": "test", "value": @bad}
   · <span style="color:#c678dd;font-weight:bold">                          ┬</span>
   ·                           <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected '@', expected i32</span>
   ╰────
</code></pre>
</div>
</section>

## Syntax Error: Multiline JSON

<section class="scenario">
<p class="description">Error location is correctly identified in multiline JSON.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">name</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">test</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">count</span><span style="color:#89ddff;">&quot;: </span><span style="color:#f7768e;">???</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">active</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">true
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">active</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::token</span>

  <span style="color:#e06c75">×</span> unexpected character: '?' (while parsing i32)
   ╭─[3:12]
 <span style="opacity:0.7">2</span> │   "name": "test",
 <span style="opacity:0.7">3</span> │   "count": ???,
   · <span style="color:#c678dd;font-weight:bold">           ┬</span>
   ·            <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected '?', expected i32</span>
 <span style="opacity:0.7">4</span> │   "active": true
   ╰────
</code></pre>
</div>
</section>

## Unknown Field

<section class="scenario">
<p class="description">JSON contains a field that doesn't exist in the target struct.<br>The error shows the unknown field and lists valid alternatives.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">username</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">alice</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#7aa2f7;">emial</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">alice@example.com</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">username</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::unknown_field</span>

  <span style="color:#e06c75">×</span> unknown field `emial`, expected one of: ["username", "email"] (did you mean `email`?)
   ╭────
 <span style="opacity:0.7">1</span> │ {"username": "alice", "emial": "alice@example.com"}
   · <span style="color:#c678dd;font-weight:bold">                      ───┬───</span>
   ·                          <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown field 'emial' - did you mean 'email'?</span>
   ╰────
</code></pre>
</div>
</section>

## Type Mismatch

<section class="scenario">
<p class="description">JSON value type doesn't match the expected Rust type.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">id</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">42</span><span style="color:#89ddff;">, &quot;</span><span style="color:#7aa2f7;">name</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">123</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Item </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::type_mismatch</span>

  <span style="color:#e06c75">×</span> type mismatch: expected String, got unsigned integer
   ╭────
 <span style="opacity:0.7">1</span> │ {"id": 42, "name": 123}
   · <span style="color:#c678dd;font-weight:bold">                   ─┬─</span>
   ·                     <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected String, got unsigned integer</span>
   ╰────
</code></pre>
</div>
</section>

## Missing Required Field

<section class="scenario">
<p class="description">JSON is missing a required field that has no default.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">host</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ServerConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::missing_field</span>

  <span style="color:#e06c75">×</span> missing required field `port`
   ╭────
 <span style="opacity:0.7">1</span> │ {"host": "localhost"}
   · <span style="color:#c678dd;font-weight:bold">┬</span><span style="color:#e5c07b;font-weight:bold">                   ┬</span>
   · <span style="color:#c678dd;font-weight:bold">│</span>                   <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">object ended without field `port`</span>
   · <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">object started here</span>
   ╰────
</code></pre>
</div>
</section>

## Number Out of Range

<section class="scenario">
<p class="description">JSON number is too large for the target integer type.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">count</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">999999999999</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Counter </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::number_out_of_range</span>

  <span style="color:#e06c75">×</span> number `999999999999` out of range for u32
   ╭────
 <span style="opacity:0.7">1</span> │ {"count": 999999999999}
   · <span style="color:#c678dd;font-weight:bold">          ──────┬─────</span>
   ·                 <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">out of range for u32</span>
   ╰────
</code></pre>
</div>
</section>

## Expected Array, Got String

<section class="scenario">
<p class="description">JSON has a string where an array was expected.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">items</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">not an array</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Container </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">items</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">i32</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::unexpected_token</span>

  <span style="color:#e06c75">×</span> unexpected token: got "not an array", expected '['
   ╭────
 <span style="opacity:0.7">1</span> │ {"items": "not an array"}
   · <span style="color:#c678dd;font-weight:bold">          ───────┬──────</span>
   ·                  <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected '[', got '"not an array"'</span>
   ╰────
</code></pre>
</div>
</section>

## Tuple Size Mismatch

<section class="scenario">
<p class="description">JSON array has wrong number of elements for tuple type.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">[</span><span style="color:#ff9e64;">1</span><span style="color:#89ddff;">, </span><span style="color:#ff9e64;">2</span><span style="color:#89ddff;">, </span><span style="color:#ff9e64;">3</span><span style="color:#9abdf5;">]</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">(…)(i32, i32)</span><span style="color:#89ddff;">;</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::unexpected_token</span>

  <span style="color:#e06c75">×</span> unexpected token: got ,, expected ']'
   ╭────
 <span style="opacity:0.7">1</span> │ [1, 2, 3]
   · <span style="color:#c678dd;font-weight:bold">     ┬</span>
   ·      <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected ']', got ','</span>
   ╰────
</code></pre>
</div>
</section>

## Unknown Enum Variant

<section class="scenario">
<p class="description">JSON specifies a variant name that doesn't exist.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Unknown</span><span style="color:#89ddff;">&quot;</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">Status </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Active</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Inactive</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Pending</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::reflect</span>

  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape Status: No variant found with the given name
</code></pre>
</div>
</section>

## Wrong Variant Format

<section class="scenario">
<p class="description">Externally tagged enum expects {"Variant": content} but got wrong format.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">type</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Text</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#7aa2f7;">content</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">hello</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">MessageError </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Text(</span><span style="color:#0db9d7;">String</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Number(</span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::reflect</span>

  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape MessageError: No variant found with the given name
</code></pre>
</div>
</section>

## Internally Tagged Enum: Missing Tag Field

<section class="scenario">
<p class="description">Internally tagged enum requires the tag field to be present.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">id</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">123</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#7aa2f7;">method</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">ping</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u32</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">tag </span><span style="color:#89ddff;">= &quot;</span><span style="color:#9ece6a;">type</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">Request </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Ping {
</span><span style="color:#9abdf5;">        id</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Echo {
</span><span style="color:#9abdf5;">        id</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">        message</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::reflect</span>

  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape Request: No variant found with the given name
</code></pre>
</div>
</section>

## Trailing Data After Valid JSON

<section class="scenario">
<p class="description">Valid JSON followed by unexpected extra content.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#ff9e64;">42</span><span style="color:#c0caf5;"> extra stuff</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::token</span>

  <span style="color:#e06c75">×</span> unexpected character: 'e'
</code></pre>
</div>
</section>

## Empty Input

<section class="scenario">
<p class="description">No JSON content at all.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::unexpected_token</span>

  <span style="color:#e06c75">×</span> unexpected token: got EOF, expected scalar value
   ╭────
   ╰────
</code></pre>
</div>
</section>

## Error with Unicode Content

<section class="scenario">
<p class="description">Error reporting handles unicode correctly.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">emoji</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">🎉🚀</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#7aa2f7;">count</span><span style="color:#89ddff;">&quot;: </span><span style="color:#f7768e;">nope</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">EmojiData </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">emoji</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::token</span>

  <span style="color:#e06c75">×</span> unexpected character: 'n' (while parsing i32)
   ╭────
 <span style="opacity:0.7">1</span> │ {"emoji": "🎉🚀", "count": nope}
   · <span style="color:#c678dd;font-weight:bold">                           ──┬─</span>
   ·                              <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected 'n', expected i32</span>
   ╰────
</code></pre>
</div>
</section>
</div>
