+++
title = "Pretty Printing"
+++

<div class="showcase">

[`facet-pretty`](https://docs.rs/facet-pretty) provides colorful, readable pretty-printing for any `Facet` type. But it can also print *the shape itself* — showing the structure of your types at compile time. Below we show each value alongside its shape.


## Primitives: Integers

<section class="scenario">
<p class="description">Simple numeric types show their value directly, and their shape reveals the underlying primitive type.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="color:rgb(224,81,93)">42</span><span style="color:inherit"></span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Primitives: Floats

<section class="scenario">
<p class="description">Floating-point numbers are displayed with their full precision.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="color:rgb(81,86,224)">3.141592653589793</span><span style="color:inherit"></span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Primitives: Booleans

<section class="scenario">
<p class="description">Boolean values are shown as `true` or `false`.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="color:rgb(81,224,114)">true</span><span style="color:inherit"></span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Primitives: Characters

<section class="scenario">
<p class="description">Character values are displayed with their Unicode representation.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="color:rgb(81,224,91)">🦀</span><span style="color:inherit"></span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Primitives: Strings

<section class="scenario">
<p class="description">String types show their content in quotes.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code>"<span style="color:rgb(158,206,106)">Hello, facet!</span><span style="color:inherit">"</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Tuples: Pair

<section class="scenario">
<p class="description">Tuples are displayed with their elements, and the shape shows each element's type.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">(f64, u32)</span><span style="color:inherit"></span> <span style="opacity:0.7">(</span>
  <span style="color:rgb(81,86,224)">3.5</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(207,81,224)">41</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">)</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-p>(</a-p>…<a-p>)(</a-p>f64<a-p>,</a-p> u32<a-p>);</a-p></code></pre>
</div>
</section>

## Tuples: Triple

<section class="scenario">
<p class="description">Larger tuples work the same way — each element type is tracked.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">(&amp;str, u32, bool)</span><span style="color:inherit"></span> <span style="opacity:0.7">(</span>
  "<span style="color:rgb(158,206,106)">Alice</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(207,81,224)">30</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(81,224,114)">true</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">)</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-p>(</a-p>…<a-p>)(</a-p><a-o>&amp;</a-o><a-cr>T</a-cr><a-p>,</a-p> u32<a-p>,</a-p> bool<a-p>);</a-p></code></pre>
</div>
</section>

## Structs: Simple

<section class="scenario">
<p class="description">Struct fields are displayed with their names and values. The shape shows field names, types, and offsets.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Point</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">x</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,86,224)">1.5</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">y</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,86,224)">2.5</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Point</a-t> <a-p>{</a-p>
    <a-pr>x</a-pr><a-p>:</a-p> <a-t>f64</a-t><a-p>,</a-p>
<br>    <a-pr>y</a-pr><a-p>:</a-p> <a-t>f64</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</div>
</section>

## Structs: With Optional Fields

<section class="scenario">
<p class="description">Optional fields show `Some(...)` or `None`. The shape includes the full Option type.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Person</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Alice</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">age</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">30</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">email</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">alice@example.com</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Person</a-t> <a-p>{</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<br>    <a-pr>age</a-pr><a-p>:</a-p> <a-t>u32</a-t><a-p>,</a-p>
<br>    <a-pr>email</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</div>
</section>

## Enums: Unit Variant

<section class="scenario">
<p class="description">Unit variants display just the variant name. The shape shows all possible variants.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Color</span><span style="color:inherit"></span><span style="opacity:0.7">::</span><span style="font-weight:bold">Blue</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>Color</a-t> <a-p>{</a-p>
    <a-cr>Red</a-cr><a-p>,</a-p>
<br>    <a-cr>Green</a-cr><a-p>,</a-p>
<br>    <a-cr>Blue</a-cr><a-p>,</a-p>
<br>    <a-cr>Rgb</a-cr><a-p>(</a-p><a-t>u8</a-t><a-p>,</a-p> <a-t>u8</a-t><a-p>,</a-p> <a-t>u8</a-t><a-p>),</a-p>
<a-p>}</a-p></code></pre>
</div>
</section>

## Enums: Tuple Variant

<section class="scenario">
<p class="description">Tuple variants show their contained values.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Color</span><span style="color:inherit"></span><span style="opacity:0.7">::</span><span style="font-weight:bold">Rgb</span><span style="opacity:0.7">(</span>
  <span style="color:rgb(222,81,224)">255</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(222,81,224)">128</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(222,81,224)">0</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">)</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>Color</a-t> <a-p>{</a-p>
    <a-cr>Red</a-cr><a-p>,</a-p>
<br>    <a-cr>Green</a-cr><a-p>,</a-p>
<br>    <a-cr>Blue</a-cr><a-p>,</a-p>
<br>    <a-cr>Rgb</a-cr><a-p>(</a-p><a-t>u8</a-t><a-p>,</a-p> <a-t>u8</a-t><a-p>,</a-p> <a-t>u8</a-t><a-p>),</a-p>
<a-p>}</a-p></code></pre>
</div>
</section>

## Enums: Struct Variant

<section class="scenario">
<p class="description">Struct variants display their field names and values.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Message</span><span style="color:inherit"></span><span style="opacity:0.7">::</span><span style="font-weight:bold">Move</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">x</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">10</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">y</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">20</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>Message</a-t> <a-p>{</a-p>
    <a-cr>Quit</a-cr><a-p>,</a-p>
<br>    <a-cr>Move</a-cr> <a-p>{</a-p>
        <a-pr>x</a-pr><a-p>:</a-p> <a-t>i32</a-t><a-p>,</a-p>
<br>        <a-pr>y</a-pr><a-p>:</a-p> <a-t>i32</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<br>    <a-cr>Write</a-cr><a-p>(</a-p><a-t>String</a-t><a-p>),</a-p>
<a-p>}</a-p></code></pre>
</div>
</section>

## Collections: Vec

<section class="scenario">
<p class="description">Vectors show their elements in a list. The shape describes the element type.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;i32&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span><span style="color:rgb(224,81,93)">1</span><span style="color:inherit"></span><span style="opacity:0.7">,</span> <span style="color:rgb(224,81,93)">2</span><span style="color:inherit"></span><span style="opacity:0.7">,</span> <span style="color:rgb(224,81,93)">3</span><span style="color:inherit"></span><span style="opacity:0.7">,</span> <span style="color:rgb(224,81,93)">4</span><span style="color:inherit"></span><span style="opacity:0.7">,</span> <span style="color:rgb(224,81,93)">5</span><span style="color:inherit"></span><span style="opacity:0.7">]</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Collections: Array

<section class="scenario">
<p class="description">Fixed-size arrays show their elements. The shape includes the array length.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">[u8; 4]</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span>
   <span style="color:rgb(224,81,145)">0a</span><span style="color:inherit"> </span><span style="color:rgb(224,138,81)">14</span><span style="color:inherit"> </span><span style="color:rgb(224,81,222)">1e</span><span style="color:inherit"> </span><span style="color:rgb(160,81,224)">28</span><span style="color:inherit">
</span><span style="opacity:0.7">]</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Collections: HashMap

<section class="scenario">
<p class="description">Maps display their key-value pairs. The shape describes both key and value types.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">HashMap&lt;String, i32&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span>
  "<span style="color:rgb(158,206,106)">two</span><span style="color:inherit">"</span><span style="opacity:0.7"> =&gt; </span><span style="color:rgb(224,81,93)">2</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  "<span style="color:rgb(158,206,106)">one</span><span style="color:inherit">"</span><span style="opacity:0.7"> =&gt; </span><span style="color:rgb(224,81,93)">1</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  "<span style="color:rgb(158,206,106)">three</span><span style="color:inherit">"</span><span style="opacity:0.7"> =&gt; </span><span style="color:rgb(224,81,93)">3</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">]</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Option: Some

<section class="scenario">
<p class="description">Option::Some displays its contained value.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">present</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Option: None

<section class="scenario">
<p class="description">Option::None displays cleanly without the type clutter.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Result: Ok

<section class="scenario">
<p class="description">Result::Ok displays its success value.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code>unsupported peek variant: Ok(42) (Result&lt;i32, String&gt;, User(Opaque))</code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Result: Err

<section class="scenario">
<p class="description">Result::Err displays the error value.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code>unsupported peek variant: Err("something went wrong") (Result&lt;i32, String&gt;, User(Opaque))</code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code></code></pre>
</div>
</section>

## Nested Structures

<section class="scenario">
<p class="description">Complex nested types are pretty-printed with proper indentation. The shape shows the full type hierarchy.</p>
<div class="value-output">
<h4>Value</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Company</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Acme Corp</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">address</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Address</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">street</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">123 Main St</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">city</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Springfield</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">zip</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">12345</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">employees</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;Person&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Person</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Alice</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">age</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">30</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">email</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">alice@acme.com</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Person</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Bob</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">age</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">25</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">email</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="shape-output">
<h4>Shape</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Company</a-t> <a-p>{</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<br>    <a-pr>address</a-pr><a-p>:</a-p> <a-t>Address</a-t><a-p>,</a-p>
<br>    <a-pr>employees</a-pr><a-p>:</a-p> <a-t>Vec</a-t><a-p>&lt;</a-p><a-t>Person</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Person</a-t> <a-p>{</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<br>    <a-pr>age</a-pr><a-p>:</a-p> <a-t>u32</a-t><a-p>,</a-p>
<br>    <a-pr>email</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Address</a-t> <a-p>{</a-p>
    <a-pr>street</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<br>    <a-pr>city</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<br>    <a-pr>zip</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</div>
</section>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/5c8df10b37be181e3a88be583c1eee213e28dbd5/facet-pretty/examples/pretty_showcase.rs"><code>facet-pretty/examples/pretty_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/5c8df10b37be181e3a88be583c1eee213e28dbd5"><code>5c8df10b3</code></a></dd>
<dt>Generated</dt><dd><time datetime="2026-01-16T05:16:07+01:00">2026-01-16T05:16:07+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
