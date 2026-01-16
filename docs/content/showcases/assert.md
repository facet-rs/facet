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
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Config</a-t> <a-p>{</a-p>
    <a-pr>host</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<br>    <a-pr>port</a-pr><a-p>:</a-p> <a-t>u16</a-t><a-p>,</a-p>
<br>    <a-pr>debug</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
<br>    <a-pr>tags</a-pr><a-p>:</a-p> <a-t>Vec</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<div class="code-block"><pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Config</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">host</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">localhost</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">port</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(224,186,81)">8080</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">debug</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">tags</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;String&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span>"<span style="color:rgb(158,206,106)">prod</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span> "<span style="color:rgb(158,206,106)">api</span><span style="color:inherit">"</span><span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre></div>
</div>
</section>

## Cross-Type Comparison

<section class="scenario">
<p class="description">Different type names (<code>Config</code> vs <code>ConfigV2</code>) with the same structure are considered "same". Useful for comparing DTOs across API versions or testing serialization roundtrips.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Config</a-t> <a-p>{</a-p>
    <a-pr>host</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<br>    <a-pr>port</a-pr><a-p>:</a-p> <a-t>u16</a-t><a-p>,</a-p>
<br>    <a-pr>debug</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
<br>    <a-pr>tags</a-pr><a-p>:</a-p> <a-t>Vec</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<div class="code-block"><pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Config</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">host</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">localhost</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">port</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(224,186,81)">8080</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">debug</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">tags</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;String&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span>"<span style="color:rgb(158,206,106)">prod</span><span style="color:inherit">"</span><span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre></div>
</div>
</section>

## Nested Structs

<section class="scenario">
<p class="description">Nested structs are compared recursively, field by field.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Person</a-t> <a-p>{</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<br>    <a-pr>age</a-pr><a-p>:</a-p> <a-t>u32</a-t><a-p>,</a-p>
<br>    <a-pr>address</a-pr><a-p>:</a-p> <a-t>Address</a-t><a-p>,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Address</a-t> <a-p>{</a-p>
    <a-pr>street</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<br>    <a-pr>city</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="success">
<h4>Success</h4>
<div class="code-block"><pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Person</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Alice</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">age</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">30</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">address</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Address</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">street</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">123 Main St</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">city</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Springfield</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre></div>
</div>
</section>

## Structural Diff

<section class="scenario">
<p class="description">When values differ, you get a precise structural diff showing exactly which fields changed and at what path — then render it as Rust, JSON, or XML for whichever toolchain you need.</p>
<div class="diff-output">
<h4>Rust Diff Output</h4>
<div class="code-block"><pre><code><span style="color:rgb(86,95,137)">{</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">debug</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(247,118,142)">true</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">false</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">host</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(247,118,142)">"localhost"</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">"prod.example.com"</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">port</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(247,118,142)">8080</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">443</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">tags</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">[</span><span style="color:inherit">
        </span><span style="color:rgb(86,95,137)">.. 1 unchanged item</span><span style="color:inherit">
        </span><span style="color:rgb(247,118,142)">- "api"</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">]</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">}</span><span style="color:inherit"></span></code></pre></div>
</div>
<div class="diff-output">
<h4>JSON Diff Output</h4>
<div class="code-block"><pre><code>    <span style="color:rgb(220,220,220)">{</span><span style="color:inherit"> </span><span style="color:rgb(100,100,100)">/* @Config */</span><span style="color:inherit">
      </span><span style="color:rgb(229,192,123);background-color:rgb(54,48,38)">←</span> <span style="color:rgb(255,234,162);background-color:rgb(85,81,77)">"debug": </span><span style="color:rgb(255,234,162);background-color:rgb(85,81,77)">true</span><span style="color:rgb(224,209,189);background-color:rgb(54,48,38)"></span> , <span style="color:rgb(255,234,162);background-color:rgb(85,81,77)">"host": </span><span style="color:rgb(229,192,123);background-color:rgb(85,81,77)">"localhost"</span><span style="color:rgb(224,209,189);background-color:rgb(54,48,38)"></span>       , <span style="color:rgb(255,234,162);background-color:rgb(85,81,77)">"port": </span><span style="color:rgb(255,234,162);background-color:rgb(85,81,77)">8080</span><span style="color:rgb(224,209,189);background-color:rgb(54,48,38)"></span>
      <span style="color:rgb(97,175,239);background-color:rgb(38,48,57)">→</span> <span style="color:rgb(142,216,255);background-color:rgb(69,73,77)">"debug": </span><span style="color:rgb(142,216,255);background-color:rgb(69,73,77)">false</span><span style="color:rgb(184,204,228);background-color:rgb(38,48,57)"></span>, <span style="color:rgb(142,216,255);background-color:rgb(69,73,77)">"host": </span><span style="color:rgb(97,175,239);background-color:rgb(69,73,77)">"prod.example.com"</span><span style="color:rgb(184,204,228);background-color:rgb(38,48,57)"></span>, <span style="color:rgb(142,216,255);background-color:rgb(69,73,77)">"port": </span><span style="color:rgb(142,216,255);background-color:rgb(69,73,77)">443</span><span style="color:rgb(184,204,228);background-color:rgb(38,48,57)"></span>
    <span style="color:rgb(220,220,220)"></span><span style="color:inherit">
        </span><span style="color:rgb(220,220,220)">"tags": [</span><span style="color:inherit">
            </span><span style="color:rgb(140,140,140)">"prod"</span><span style="color:inherit">
            </span><span style="color:rgb(229,192,123);background-color:rgb(54,48,38)">-</span> <span style="color:rgb(229,192,123);background-color:rgb(54,48,38)">"api"</span>
        <span style="color:rgb(220,220,220)">]</span><span style="color:inherit"></span><span style="color:rgb(100,100,100)">,</span><span style="color:inherit">
    </span><span style="color:rgb(220,220,220)">}</span><span style="color:inherit">
</span></code></pre></div>
</div>
<div class="diff-output">
<h4>XML Diff Output</h4>
<div class="code-block"><pre><code>    <span style="color:rgb(220,220,220)">&lt;@Config</span><span style="color:inherit">
      </span><span style="color:rgb(229,192,123);background-color:rgb(54,48,38)">←</span> <span style="color:rgb(255,234,162);background-color:rgb(85,81,77)">debug="</span><span style="color:rgb(255,234,162);background-color:rgb(85,81,77)">true</span><span style="color:rgb(224,209,189);background-color:rgb(54,48,38)">"</span>  <span style="color:rgb(255,234,162);background-color:rgb(85,81,77)">host="</span><span style="color:rgb(229,192,123);background-color:rgb(85,81,77)">localhost</span><span style="color:rgb(224,209,189);background-color:rgb(54,48,38)">"</span>        <span style="color:rgb(255,234,162);background-color:rgb(85,81,77)">port="</span><span style="color:rgb(255,234,162);background-color:rgb(85,81,77)">8080</span><span style="color:rgb(224,209,189);background-color:rgb(54,48,38)">"</span>
      <span style="color:rgb(97,175,239);background-color:rgb(38,48,57)">→</span> <span style="color:rgb(142,216,255);background-color:rgb(69,73,77)">debug="</span><span style="color:rgb(142,216,255);background-color:rgb(69,73,77)">false</span><span style="color:rgb(184,204,228);background-color:rgb(38,48,57)">"</span> <span style="color:rgb(142,216,255);background-color:rgb(69,73,77)">host="</span><span style="color:rgb(97,175,239);background-color:rgb(69,73,77)">prod.example.com</span><span style="color:rgb(184,204,228);background-color:rgb(38,48,57)">"</span> <span style="color:rgb(142,216,255);background-color:rgb(69,73,77)">port="</span><span style="color:rgb(142,216,255);background-color:rgb(69,73,77)">443</span><span style="color:rgb(184,204,228);background-color:rgb(38,48,57)">"</span>
    <span style="color:rgb(220,220,220)">&gt;</span><span style="color:inherit">
        </span><span style="color:rgb(220,220,220)"></span><span style="color:inherit">
            </span><span style="color:rgb(140,140,140)">prod</span><span style="color:inherit">
            </span><span style="color:rgb(229,192,123);background-color:rgb(54,48,38)">-</span> <span style="color:rgb(229,192,123);background-color:rgb(54,48,38)">api</span>
        <span style="color:rgb(220,220,220)"></span><span style="color:inherit"></span><span style="color:rgb(100,100,100)"></span><span style="color:inherit">
    </span><span style="color:rgb(220,220,220)">&lt;/@Config&gt;</span><span style="color:inherit">
</span></code></pre></div>
</div>
</section>

## Vector Differences

<section class="scenario">
<p class="description">Vector comparisons show exactly which indices differ, which elements were added, and which were removed.</p>
<div class="diff-output">
<h4>Diff Output</h4>
<div class="code-block"><pre><code><span style="color:rgb(86,95,137)">[</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">.. 2 unchanged items</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">3</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">99</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">.. 1 unchanged item</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 5</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">]</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/5c8df10b37be181e3a88be583c1eee213e28dbd5/facet-assert/examples/assert_showcase.rs"><code>facet-assert/examples/assert_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/5c8df10b37be181e3a88be583c1eee213e28dbd5"><code>5c8df10b3</code></a></dd>
<dt>Generated</dt><dd><time datetime="2026-01-16T05:16:07+01:00">2026-01-16T05:16:07+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
