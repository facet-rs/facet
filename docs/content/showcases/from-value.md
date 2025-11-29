+++
title = "facet-value: from_value Deserialization"
+++

<div class="showcase">

## Happy Path


### Simple Struct

<section class="scenario">
<p class="description">Deserialize a <code>Value</code> map into a struct with basic fields.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">"Alice"</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">age</span><span style="opacity:0.7">: </span><span style="color:#56b6c2">30</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">email</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">"alice@example.com"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
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
  <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(81,103,224)">Alice</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">age</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">30</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">email</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Option&lt;String&gt;</span><span style="opacity:0.7">Some(</span><span style="color:rgb(81,103,224)">alice@example.com</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Nested Structs

<section class="scenario">
<p class="description">Nested structs are deserialized recursively.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:#56b6c2">person</span><span style="opacity:0.7">: </span><span style="opacity:0.7">{</span>
    <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">"Bob"</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">age</span><span style="opacity:0.7">: </span><span style="color:#56b6c2">42</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">email</span><span style="opacity:0.7">: </span><span style="color:#c678dd">null</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">address</span><span style="opacity:0.7">: </span><span style="opacity:0.7">{</span>
    <span style="color:#56b6c2">street</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">"123 Main St"</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">city</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">"Springfield"</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">zip</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">"12345"</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">department</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">"Engineering"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Employee </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">person</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Person,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">address</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Address,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">department</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Address </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">street</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">city</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">zip</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Person </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">age</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Employee</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">person</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Person</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(81,103,224)">Bob</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">age</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">42</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">email</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Option&lt;String&gt;</span><span style="opacity:0.7">None</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">address</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Address</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">street</span><span style="opacity:0.7">: </span><span style="color:rgb(81,103,224)">123 Main St</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">city</span><span style="opacity:0.7">: </span><span style="color:rgb(81,103,224)">Springfield</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">zip</span><span style="opacity:0.7">: </span><span style="color:rgb(81,103,224)">12345</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">department</span><span style="opacity:0.7">: </span><span style="color:rgb(81,103,224)">Engineering</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Unit Enum Variant

<section class="scenario">
<p class="description">A string value deserializes into a unit variant.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="color:#e5c07b">"Active"</span></code></pre>
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
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Status</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Active</span></code></pre>
</div>
</section>

### Tuple Enum Variant

<section class="scenario">
<p class="description">Externally tagged enum: <code>{"Variant": content}</code>.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:#56b6c2">Text</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">"Hello world!"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">Message </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Text(</span><span style="color:#0db9d7;">String</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Number(</span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Data {
</span><span style="color:#9abdf5;">        id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u64</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">        payload</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Message</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Text</span><span style="opacity:0.7">(</span><span style="color:rgb(81,103,224)">Hello world!</span><span style="opacity:0.7">)</span></code></pre>
</div>
</section>

### Struct Enum Variant

<section class="scenario">
<p class="description">Struct variants deserialize with named fields.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:#56b6c2">Data</span><span style="opacity:0.7">: </span><span style="opacity:0.7">{</span>
    <span style="color:#56b6c2">id</span><span style="opacity:0.7">: </span><span style="color:#56b6c2">42</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">payload</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">"secret data"</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">Message </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Text(</span><span style="color:#0db9d7;">String</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Number(</span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Data {
</span><span style="color:#9abdf5;">        id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u64</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">        payload</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Message</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Data</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">id</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,179)">42</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">payload</span><span style="opacity:0.7">: </span><span style="color:rgb(81,103,224)">secret data</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Vec Deserialization

<section class="scenario">
<p class="description">Arrays deserialize into <code>Vec&lt;T&gt;</code>.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">[</span>
  <span style="color:#56b6c2">1</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">2</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">3</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">4</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">5</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Vec&lt;i32&gt;</span><span style="opacity:0.7"> [</span>
  <span style="color:rgb(224,81,93)">1</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(224,81,93)">2</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(224,81,93)">3</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(224,81,93)">4</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(224,81,93)">5</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
</section>

### Fixed-Size Array

<section class="scenario">
<p class="description">Arrays with exact length deserialize into <code>[T; N]</code>.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">[</span>
  <span style="color:#e5c07b">"a"</span><span style="opacity:0.7">,</span>
  <span style="color:#e5c07b">"b"</span><span style="opacity:0.7">,</span>
  <span style="color:#e5c07b">"c"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">[String; 3]</span><span style="opacity:0.7"> [</span>
  <span style="color:rgb(81,103,224)">a</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(81,103,224)">b</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(81,103,224)">c</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
</section>

### HashMap

<section class="scenario">
<p class="description">Objects deserialize into <code>HashMap&lt;String, T&gt;</code>.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:#56b6c2">x</span><span style="opacity:0.7">: </span><span style="color:#56b6c2">10</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">y</span><span style="opacity:0.7">: </span><span style="color:#56b6c2">20</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">z</span><span style="opacity:0.7">: </span><span style="color:#56b6c2">30</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">HashMap&lt;String, i32&gt;</span><span style="opacity:0.7"> [</span>
  <span style="color:rgb(81,103,224)">x</span><span style="opacity:0.7"> =&gt; </span><span style="color:rgb(224,81,93)">10</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(81,103,224)">y</span><span style="opacity:0.7"> =&gt; </span><span style="color:rgb(224,81,93)">20</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(81,103,224)">z</span><span style="opacity:0.7"> =&gt; </span><span style="color:rgb(224,81,93)">30</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
</section>

### Nested Collections

<section class="scenario">
<p class="description"><code>null</code> values become <code>None</code> in <code>Option&lt;T&gt;</code>.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">[</span>
  <span style="color:#56b6c2">1</span><span style="opacity:0.7">,</span>
  <span style="color:#c678dd">null</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">3</span><span style="opacity:0.7">,</span>
  <span style="color:#c678dd">null</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">5</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Vec&lt;Option&lt;i32&gt;&gt;</span><span style="opacity:0.7"> [</span>
  <span style="font-weight:bold">Option&lt;i32&gt;</span><span style="opacity:0.7">Some(</span><span style="color:rgb(224,81,93)">1</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="font-weight:bold">Option&lt;i32&gt;</span><span style="opacity:0.7">None</span><span style="opacity:0.7">,</span>
  <span style="font-weight:bold">Option&lt;i32&gt;</span><span style="opacity:0.7">Some(</span><span style="color:rgb(224,81,93)">3</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="font-weight:bold">Option&lt;i32&gt;</span><span style="opacity:0.7">None</span><span style="opacity:0.7">,</span>
  <span style="font-weight:bold">Option&lt;i32&gt;</span><span style="opacity:0.7">Some(</span><span style="color:rgb(224,81,93)">5</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
</section>

### Default Field Values

<section class="scenario">
<p class="description">Fields marked with <code>#[facet(default)]</code> use <code>Default::default()</code> when missing.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:#e5c07b">"minimal"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">enabled</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">max_retries</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">u32</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Config</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(81,103,224)">minimal</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">enabled</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">false</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">max_retries</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Option&lt;u32&gt;</span><span style="opacity:0.7">None</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Errors


### Error: Type Mismatch

<section class="scenario">
<p class="description">Trying to deserialize a string as an integer.</p>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape i32: Failed to parse string value

Error: 
  <span style="color:#e06c75">×</span> input.json
   ╭────
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(192,197,206)">"</span><span style="color:rgb(163,190,140)">not a number</span><span style="color:rgb(192,197,206)">"
   · </span><span style="color:#c678dd;font-weight:bold">───────┬──────</span>
   ·        <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">this value</span>
   ╰────
</code></pre>
</div>
</section>

### Error: Number Out of Range

<section class="scenario">
<p class="description">Value 1000 is too large for u8 (max 255).</p>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> number out of range: 1000 out of range for u8

Error: 
  <span style="color:#e06c75">×</span> input.json
   ╭────
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(208,135,112)">1000
   · </span><span style="color:#c678dd;font-weight:bold">──┬─</span>
   ·   <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">this value: 1000 out of range for u8</span>
   ╰────
</code></pre>
</div>
</section>

### Error: Wrong Array Length

<section class="scenario">
<p class="description">Array has 4 elements but target type expects exactly 3.</p>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> unsupported: fixed array has 3 elements but got 4

Error: 
  <span style="color:#e06c75">×</span> input.json
   ╭─[1:1]
 <span style="opacity:0.7">1</span> │ <span style="color:#c678dd;font-weight:bold">╭</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> <span style="color:rgb(192,197,206)">[
 </span><span style="opacity:0.7">2</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">  </span><span style="color:rgb(208,135,112)">1</span><span style="color:rgb(192,197,206)">,
 </span><span style="opacity:0.7">3</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">  </span><span style="color:rgb(208,135,112)">2</span><span style="color:rgb(192,197,206)">,
 </span><span style="opacity:0.7">4</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">  </span><span style="color:rgb(208,135,112)">3</span><span style="color:rgb(192,197,206)">,
 </span><span style="opacity:0.7">5</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">  </span><span style="color:rgb(208,135,112)">4
 </span><span style="opacity:0.7">6</span> │ <span style="color:#c678dd;font-weight:bold">├</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> <span style="color:rgb(192,197,206)">]
   · </span><span style="color:#c678dd;font-weight:bold">╰</span><span style="color:#c678dd;font-weight:bold">───</span><span style="color:#c678dd;font-weight:bold">─</span> <span style="color:#c678dd;font-weight:bold">this value</span>
   ╰────
</code></pre>
</div>
</section>

### Error: Invalid Enum Variant

<section class="scenario">
<p class="description">"Unknown" is not a valid variant of Status.</p>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape Status: No variant found with the given name

Error: 
  <span style="color:#e06c75">×</span> input.json
   ╭────
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(192,197,206)">"</span><span style="color:rgb(163,190,140)">Unknown</span><span style="color:rgb(192,197,206)">"
   · </span><span style="color:#c678dd;font-weight:bold">────┬────</span>
   ·     <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">this value</span>
   ╰────

Error: 
  <span style="color:#e06c75">×</span> target.rs
   ╭─[1:1]
 <span style="opacity:0.7">1</span> │ <span style="color:#c678dd;font-weight:bold">╭</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> <span style="color:rgb(192,197,206)">#</span><span style="color:rgb(192,197,206)">[</span><span style="color:rgb(191,97,106)">derive</span><span style="color:rgb(192,197,206)">(</span><span style="color:rgb(192,197,206)">Facet</span><span style="color:rgb(192,197,206)">)</span><span style="color:rgb(192,197,206)">]
 </span><span style="opacity:0.7">2</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">#</span><span style="color:rgb(192,197,206)">[</span><span style="color:rgb(191,97,106)">repr</span><span style="color:rgb(192,197,206)">(</span><span style="color:rgb(192,197,206)">u8</span><span style="color:rgb(192,197,206)">)</span><span style="color:rgb(192,197,206)">]
 </span><span style="opacity:0.7">3</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(180,142,173)">enum</span><span style="color:rgb(192,197,206)"> </span><span style="color:rgb(192,197,206)">Status</span><span style="color:rgb(192,197,206)"> </span><span style="color:rgb(192,197,206)">{
 </span><span style="opacity:0.7">4</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">    Active</span><span style="color:rgb(192,197,206)">,
 </span><span style="opacity:0.7">5</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">    Inactive</span><span style="color:rgb(192,197,206)">,
 </span><span style="opacity:0.7">6</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">    Pending</span><span style="color:rgb(192,197,206)">,
 </span><span style="opacity:0.7">7</span> │ <span style="color:#c678dd;font-weight:bold">├</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> <span style="color:rgb(192,197,206)">}
   · </span><span style="color:#c678dd;font-weight:bold">╰</span><span style="color:#c678dd;font-weight:bold">───</span><span style="color:#c678dd;font-weight:bold">─</span> <span style="color:#c678dd;font-weight:bold">target type</span>
   ╰────
</code></pre>
</div>
</section>

### Error: Expected Object, Got Array

<section class="scenario">
<p class="description">Cannot deserialize an array as a struct.</p>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> type mismatch: expected object, got Array

Error: 
  <span style="color:#e06c75">×</span> input.json
   ╭─[1:1]
 <span style="opacity:0.7">1</span> │ <span style="color:#c678dd;font-weight:bold">╭</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> <span style="color:rgb(192,197,206)">[
 </span><span style="opacity:0.7">2</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">  </span><span style="color:rgb(208,135,112)">1</span><span style="color:rgb(192,197,206)">,
 </span><span style="opacity:0.7">3</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">  </span><span style="color:rgb(208,135,112)">2</span><span style="color:rgb(192,197,206)">,
 </span><span style="opacity:0.7">4</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">  </span><span style="color:rgb(208,135,112)">3
 </span><span style="opacity:0.7">5</span> │ <span style="color:#c678dd;font-weight:bold">├</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> <span style="color:rgb(192,197,206)">]
   · </span><span style="color:#c678dd;font-weight:bold">╰</span><span style="color:#c678dd;font-weight:bold">───</span><span style="color:#c678dd;font-weight:bold">─</span> <span style="color:#c678dd;font-weight:bold">got Array</span>
   ╰────

Error: 
  <span style="color:#e06c75">×</span> target.rs
   ╭─[1:1]
 <span style="opacity:0.7">1</span> │ <span style="color:#c678dd;font-weight:bold">╭</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> <span style="color:rgb(192,197,206)">#</span><span style="color:rgb(192,197,206)">[</span><span style="color:rgb(191,97,106)">derive</span><span style="color:rgb(192,197,206)">(</span><span style="color:rgb(192,197,206)">Facet</span><span style="color:rgb(192,197,206)">)</span><span style="color:rgb(192,197,206)">]
 </span><span style="opacity:0.7">2</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(180,142,173)">struct</span><span style="color:rgb(192,197,206)"> </span><span style="color:rgb(192,197,206)">Person</span><span style="color:rgb(192,197,206)"> </span><span style="color:rgb(192,197,206)">{
 </span><span style="opacity:0.7">3</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">    </span><span style="color:rgb(191,97,106)">name</span><span style="color:rgb(192,197,206)">:</span><span style="color:rgb(192,197,206)"> String,
 </span><span style="opacity:0.7">4</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">    </span><span style="color:rgb(191,97,106)">age</span><span style="color:rgb(192,197,206)">:</span><span style="color:rgb(192,197,206)"> </span><span style="color:rgb(180,142,173)">u32</span><span style="color:rgb(192,197,206)">,
 </span><span style="opacity:0.7">5</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(192,197,206)">    </span><span style="color:rgb(191,97,106)">email</span><span style="color:rgb(192,197,206)">:</span><span style="color:rgb(192,197,206)"> </span><span style="color:rgb(192,197,206)">Option</span><span style="color:rgb(192,197,206)">&lt;</span><span style="color:rgb(192,197,206)">String</span><span style="color:rgb(192,197,206)">&gt;</span><span style="color:rgb(192,197,206)">,
 </span><span style="opacity:0.7">6</span> │ <span style="color:#c678dd;font-weight:bold">├</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> <span style="color:rgb(192,197,206)">}
   · </span><span style="color:#c678dd;font-weight:bold">╰</span><span style="color:#c678dd;font-weight:bold">───</span><span style="color:#c678dd;font-weight:bold">─</span> <span style="color:#c678dd;font-weight:bold">expected object</span>
   ╰────
</code></pre>
</div>
</section>
</div>
