+++
title = "From Value"
+++

<div class="showcase">

[`facet-value`](https://docs.rs/facet-value) provides a dynamic `Value` type and conversion to/from any `Facet` type. Use it for format-agnostic data manipulation, testing, or bridging between different serialization formats.


## Happy Path


### Simple Struct

<section class="scenario">
<p class="description">Deserialize a <code>Value</code> map into a struct with basic fields.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Alice</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">age</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(255,158,100)">30</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">email</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">alice@example.com</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Person</a-t> <a-p>{</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-pr>age</a-pr><a-p>:</a-p> <a-t>u32</a-t><a-p>,</a-p>

    <a-pr>email</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Person</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Alice</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">age</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">30</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">email</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">alice@example.com</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Nested Structs

<section class="scenario">
<p class="description">Nested structs are deserialized recursively.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:rgb(115,218,202)">person</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="opacity:0.7">{</span>
    <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Bob</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">age</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(255,158,100)">42</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">email</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(187,154,247)">null</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">address</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="opacity:0.7">{</span>
    <span style="color:rgb(115,218,202)">street</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">123 Main St</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">city</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Springfield</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">zip</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">12345</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">department</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Engineering</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Employee</a-t> <a-p>{</a-p>
    <a-pr>person</a-pr><a-p>:</a-p> <a-t>Person</a-t><a-p>,</a-p>

    <a-pr>address</a-pr><a-p>:</a-p> <a-t>Address</a-t><a-p>,</a-p>

    <a-pr>department</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Address</a-t> <a-p>{</a-p>
    <a-pr>street</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-pr>city</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-pr>zip</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Person</a-t> <a-p>{</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-pr>age</a-pr><a-p>:</a-p> <a-t>u32</a-t><a-p>,</a-p>

    <a-pr>email</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Employee</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">person</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Person</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Bob</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">age</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">42</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">email</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">address</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Address</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">street</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">123 Main St</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">city</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Springfield</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">zip</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">12345</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">department</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Engineering</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Unit Enum Variant

<section class="scenario">
<p class="description">A string value deserializes into a unit variant.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code>"<span style="color:rgb(158,206,106)">Active</span><span style="color:inherit">"</span></code></pre>
</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>Status</a-t> <a-p>{</a-p>
    <a-cr>Active</a-cr><a-p>,</a-p>

    <a-cr>Inactive</a-cr><a-p>,</a-p>

    <a-cr>Pending</a-cr><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Status</span><span style="color:inherit"></span><span style="opacity:0.7">::</span><span style="font-weight:bold">Active</span></code></pre>
</div>
</section>

### Tuple Enum Variant

<section class="scenario">
<p class="description">Externally tagged enum: <code>{"Variant": content}</code>.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:rgb(115,218,202)">Text</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Hello world!</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>Message</a-t> <a-p>{</a-p>
    <a-cr>Text</a-cr><a-p>(</a-p><a-t>String</a-t><a-p>),</a-p>

    <a-cr>Number</a-cr><a-p>(</a-p><a-t>i32</a-t><a-p>),</a-p>

    <a-cr>Data</a-cr> <a-p>{</a-p>
        <a-pr>id</a-pr><a-p>:</a-p> <a-t>u64</a-t><a-p>,</a-p>

        <a-pr>payload</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Message</span><span style="color:inherit"></span><span style="opacity:0.7">::</span><span style="font-weight:bold">Text</span><span style="opacity:0.7">(</span>"<span style="color:rgb(158,206,106)">Hello world!</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span></code></pre>
</div>
</section>

### Struct Enum Variant

<section class="scenario">
<p class="description">Struct variants deserialize with named fields.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:rgb(115,218,202)">Data</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="opacity:0.7">{</span>
    <span style="color:rgb(115,218,202)">id</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(255,158,100)">42</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">payload</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">secret data</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>Message</a-t> <a-p>{</a-p>
    <a-cr>Text</a-cr><a-p>(</a-p><a-t>String</a-t><a-p>),</a-p>

    <a-cr>Number</a-cr><a-p>(</a-p><a-t>i32</a-t><a-p>),</a-p>

    <a-cr>Data</a-cr> <a-p>{</a-p>
        <a-pr>id</a-pr><a-p>:</a-p> <a-t>u64</a-t><a-p>,</a-p>

        <a-pr>payload</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Message</span><span style="color:inherit"></span><span style="opacity:0.7">::</span><span style="font-weight:bold">Data</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">id</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,179)">42</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">payload</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">secret data</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Vec Deserialization

<section class="scenario">
<p class="description">Arrays deserialize into <code>Vec&lt;T&gt;</code>.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">[</span>
  <span style="color:rgb(255,158,100)">1</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(255,158,100)">2</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(255,158,100)">3</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(255,158,100)">4</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(255,158,100)">5</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;i32&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span><span style="color:rgb(224,81,93)">1</span><span style="color:inherit"></span><span style="opacity:0.7">,</span> <span style="color:rgb(224,81,93)">2</span><span style="color:inherit"></span><span style="opacity:0.7">,</span> <span style="color:rgb(224,81,93)">3</span><span style="color:inherit"></span><span style="opacity:0.7">,</span> <span style="color:rgb(224,81,93)">4</span><span style="color:inherit"></span><span style="opacity:0.7">,</span> <span style="color:rgb(224,81,93)">5</span><span style="color:inherit"></span><span style="opacity:0.7">]</span></code></pre>
</div>
</section>

### Fixed-Size Array

<section class="scenario">
<p class="description">Arrays with exact length deserialize into <code>[T; N]</code>.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">[</span>
  "<span style="color:rgb(158,206,106)">a</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  "<span style="color:rgb(158,206,106)">b</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  "<span style="color:rgb(158,206,106)">c</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">[String; 3]</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span>"<span style="color:rgb(158,206,106)">a</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span> "<span style="color:rgb(158,206,106)">b</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span> "<span style="color:rgb(158,206,106)">c</span><span style="color:inherit">"</span><span style="opacity:0.7">]</span></code></pre>
</div>
</section>

### HashMap

<section class="scenario">
<p class="description">Objects deserialize into <code>HashMap&lt;String, T&gt;</code>.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:rgb(115,218,202)">x</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(255,158,100)">10</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">y</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(255,158,100)">20</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">z</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(255,158,100)">30</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">HashMap&lt;String, i32&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span>
  "<span style="color:rgb(158,206,106)">y</span><span style="color:inherit">"</span><span style="opacity:0.7"> =&gt; </span><span style="color:rgb(224,81,93)">20</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  "<span style="color:rgb(158,206,106)">x</span><span style="color:inherit">"</span><span style="opacity:0.7"> =&gt; </span><span style="color:rgb(224,81,93)">10</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  "<span style="color:rgb(158,206,106)">z</span><span style="color:inherit">"</span><span style="opacity:0.7"> =&gt; </span><span style="color:rgb(224,81,93)">30</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
</section>

### Nested Collections

<section class="scenario">
<p class="description"><code>null</code> values become <code>None</code> in <code>Option&lt;T&gt;</code>.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">[</span>
  <span style="color:rgb(255,158,100)">1</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(187,154,247)">null</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(255,158,100)">3</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(187,154,247)">null</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(255,158,100)">5</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;Option&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="color:rgb(224,81,93)">1</span><span style="color:inherit"></span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span> <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span> <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="color:rgb(224,81,93)">3</span><span style="color:inherit"></span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span> <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span> <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="color:rgb(224,81,93)">5</span><span style="color:inherit"></span><span style="opacity:0.7">)</span><span style="opacity:0.7">]</span></code></pre>
</div>
</section>

### Default Field Values

<section class="scenario">
<p class="description">Fields marked with <code>#[facet(default)]</code> use <code>Default::default()</code> when missing.</p>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="opacity:0.7">{</span>
  <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">minimal</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Config</a-t> <a-p>{</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-pr>enabled</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-pr>max_retries</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>u32</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Config</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">minimal</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">enabled</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">false</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">max_retries</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Errors


### Error: Type Mismatch

<section class="scenario">
<p class="description">Trying to deserialize a string as an integer.</p>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> reflection error: failed to parse "not a number" as i32

Error: 
  <span style="color:#e06c75">×</span> input.json
   ╭────
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(125,207,255)">"not a number"</span>
   · <span style="color:#c678dd;font-weight:bold">───────┬──────</span>
   ·        <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">this value</span>
   ╰────
</code></pre>
</div>

### Error: Number Out of Range

<section class="scenario">
<p class="description">Value 1000 is too large for u8 (max 255).</p>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> number out of range: 1000 out of range for u8

Error: 
  <span style="color:#e06c75">×</span> input.json
   ╭────
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(122,162,247)">1000</span>
   · <span style="color:#c678dd;font-weight:bold">──┬─</span>
   ·   <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">this value: 1000 out of range for u8</span>
   ╰────
</code></pre>
</div>

### Error: Wrong Array Length

<section class="scenario">
<p class="description">Array has 4 elements but target type expects exactly 3.</p>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> unsupported: fixed array has 3 elements but got 4

Error: 
  <span style="color:#e06c75">×</span> input.json
   ╭─[1:1]
 <span style="opacity:0.7">1</span> │ <span style="color:#c678dd;font-weight:bold">╭</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> [
 <span style="opacity:0.7">2</span> │ <span style="color:#c678dd;font-weight:bold">│</span>     <span style="color:rgb(122,162,247)">1</span>,
 <span style="opacity:0.7">3</span> │ <span style="color:#c678dd;font-weight:bold">│</span>     <span style="color:rgb(122,162,247)">2</span>,
 <span style="opacity:0.7">4</span> │ <span style="color:#c678dd;font-weight:bold">│</span>     <span style="color:rgb(122,162,247)">3</span>,
 <span style="opacity:0.7">5</span> │ <span style="color:#c678dd;font-weight:bold">│</span>     <span style="color:rgb(122,162,247)">4</span>
 <span style="opacity:0.7">6</span> │ <span style="color:#c678dd;font-weight:bold">├</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> ]
   · <span style="color:#c678dd;font-weight:bold">╰</span><span style="color:#c678dd;font-weight:bold">───</span><span style="color:#c678dd;font-weight:bold">─</span> <span style="color:#c678dd;font-weight:bold">this value</span>
   ╰────
</code></pre>
</div>

### Error: Invalid Enum Variant

<section class="scenario">
<p class="description">"Unknown" is not a valid variant of Status.</p>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape Status: No variant found with the given name

Error: 
  <span style="color:#e06c75">×</span> input.json
   ╭────
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(125,207,255)">"Unknown"</span>
   · <span style="color:#c678dd;font-weight:bold">────┬────</span>
   ·     <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">this value</span>
   ╰────

Error: 
  <span style="color:#e06c75">×</span> target.rs
   ╭─[1:1]
 <span style="opacity:0.7">1</span> │ <span style="color:#c678dd;font-weight:bold">╭</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> #[derive(Facet)]
 <span style="opacity:0.7">2</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   #[repr(u8)]
 <span style="opacity:0.7">3</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(224,175,104)">enum</span> Status {
 <span style="opacity:0.7">4</span> │ <span style="color:#c678dd;font-weight:bold">│</span>       Active,
 <span style="opacity:0.7">5</span> │ <span style="color:#c678dd;font-weight:bold">│</span>       Inactive,
 <span style="opacity:0.7">6</span> │ <span style="color:#c678dd;font-weight:bold">│</span>       Pending,
 <span style="opacity:0.7">7</span> │ <span style="color:#c678dd;font-weight:bold">├</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> }
   · <span style="color:#c678dd;font-weight:bold">╰</span><span style="color:#c678dd;font-weight:bold">───</span><span style="color:#c678dd;font-weight:bold">─</span> <span style="color:#c678dd;font-weight:bold">target type</span>
   ╰────
</code></pre>
</div>

### Error: Expected Object, Got Array

<section class="scenario">
<p class="description">Cannot deserialize an array as a struct.</p>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> type mismatch: expected object, got Array

Error: 
  <span style="color:#e06c75">×</span> input.json
   ╭─[1:1]
 <span style="opacity:0.7">1</span> │ <span style="color:#c678dd;font-weight:bold">╭</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> [
 <span style="opacity:0.7">2</span> │ <span style="color:#c678dd;font-weight:bold">│</span>     <span style="color:rgb(122,162,247)">1</span>,
 <span style="opacity:0.7">3</span> │ <span style="color:#c678dd;font-weight:bold">│</span>     <span style="color:rgb(122,162,247)">2</span>,
 <span style="opacity:0.7">4</span> │ <span style="color:#c678dd;font-weight:bold">│</span>     <span style="color:rgb(122,162,247)">3</span>
 <span style="opacity:0.7">5</span> │ <span style="color:#c678dd;font-weight:bold">├</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> ]
   · <span style="color:#c678dd;font-weight:bold">╰</span><span style="color:#c678dd;font-weight:bold">───</span><span style="color:#c678dd;font-weight:bold">─</span> <span style="color:#c678dd;font-weight:bold">got Array</span>
   ╰────

Error: 
  <span style="color:#e06c75">×</span> target.rs
   ╭─[1:1]
 <span style="opacity:0.7">1</span> │ <span style="color:#c678dd;font-weight:bold">╭</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> #[derive(Facet)]
 <span style="opacity:0.7">2</span> │ <span style="color:#c678dd;font-weight:bold">│</span>   <span style="color:rgb(224,175,104)">struct</span> Person {
 <span style="opacity:0.7">3</span> │ <span style="color:#c678dd;font-weight:bold">│</span>       name: String,
 <span style="opacity:0.7">4</span> │ <span style="color:#c678dd;font-weight:bold">│</span>       age: u32,
 <span style="opacity:0.7">5</span> │ <span style="color:#c678dd;font-weight:bold">│</span>       email: Option&lt;String&gt;,
 <span style="opacity:0.7">6</span> │ <span style="color:#c678dd;font-weight:bold">├</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> }
   · <span style="color:#c678dd;font-weight:bold">╰</span><span style="color:#c678dd;font-weight:bold">───</span><span style="color:#c678dd;font-weight:bold">─</span> <span style="color:#c678dd;font-weight:bold">expected object</span>
   ╰────
</code></pre>
</div>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/c5842bc4cd833fedc52522b20f09daedff260a0e/facet-value/examples/from_value_showcase.rs"><code>facet-value/examples/from_value_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/c5842bc4cd833fedc52522b20f09daedff260a0e"><code>c5842bc4c</code></a></dd>
<dt>Generated</dt><dd><time datetime="2026-01-04T12:56:12+01:00">2026-01-04T12:56:12+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
