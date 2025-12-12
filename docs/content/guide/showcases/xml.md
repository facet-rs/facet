+++
title = "XML"
slug = "xml"
+++

<div class="showcase">

`facet-xml` maps Facet types to XML via explicit field annotations. This showcase highlights common serialization patterns and the new diagnostic you get when a field forgets to declare its XML role.


## Serialization


### Attributes, elements, and Vec fields

<section class="scenario">
<p class="description">Attributes live on the root <code>&lt;ContactBook&gt;</code> tag while <code>#[facet(xml::elements)]</code> turns a Vec into repeated <code>&lt;contacts&gt;</code> children.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">xml::ns_all</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ContactBook </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">xml</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">attribute)]
</span><span style="color:#9abdf5;">    owner</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">xml</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">elements)]
</span><span style="color:#9abdf5;">    contacts</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">Contact</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Contact </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">xml</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">attribute)]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">xml</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">element)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">xml</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">element)]
</span><span style="color:#9abdf5;">    email</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">ContactBook</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">owner</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Operations</span>"<span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">contacts</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;Contact&gt;</span><span style="opacity:0.7"> [</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Contact</span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">id</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">1</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">name</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Alice</span>"<span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">email</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">alice@example.com</span>"<span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Contact</span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">id</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">2</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">name</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Bob</span>"<span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">email</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="serialized-output">
<h4>XML Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">&lt;</span><span style="color:#f7768e;">ContactBook </span><span style="color:#bb9af7;">xmlns</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">https://example.com/contacts</span><span style="color:#89ddff;">&quot; </span><span style="color:#bb9af7;">owner</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Operations</span><span style="color:#89ddff;">&quot;&gt;
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&lt;</span><span style="color:#f7768e;">Contact </span><span style="color:#bb9af7;">id</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">1</span><span style="color:#89ddff;">&quot;&gt;
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&lt;</span><span style="color:#f7768e;">name</span><span style="color:#89ddff;">&gt;</span><span style="color:#c0caf5;">Alice</span><span style="color:#89ddff;">&lt;/</span><span style="color:#f7768e;">name</span><span style="color:#89ddff;">&gt;
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&lt;</span><span style="color:#f7768e;">email</span><span style="color:#89ddff;">&gt;</span><span style="color:#c0caf5;">alice@example.com</span><span style="color:#89ddff;">&lt;/</span><span style="color:#f7768e;">email</span><span style="color:#89ddff;">&gt;
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&lt;/</span><span style="color:#f7768e;">Contact</span><span style="color:#89ddff;">&gt;
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&lt;</span><span style="color:#f7768e;">Contact </span><span style="color:#bb9af7;">id</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">2</span><span style="color:#89ddff;">&quot;&gt;
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">&lt;</span><span style="color:#f7768e;">name</span><span style="color:#89ddff;">&gt;</span><span style="color:#c0caf5;">Bob</span><span style="color:#89ddff;">&lt;/</span><span style="color:#f7768e;">name</span><span style="color:#89ddff;">&gt;
</span><span style="color:#c0caf5;">    
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&lt;/</span><span style="color:#f7768e;">Contact</span><span style="color:#89ddff;">&gt;
</span><span style="color:#89ddff;">&lt;/</span><span style="color:#f7768e;">ContactBook</span><span style="color:#89ddff;">&gt;</span></pre>

</div>
</section>

### xml::text for content

<section class="scenario">
<p class="description"><code>#[facet(xml::text)]</code> captures character data inside an element, while attributes remain on the tag. This scenario deserializes the feed and pretty-prints the resulting Facet value.</p>
<div class="input">
<h4>XML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">&lt;</span><span style="color:#f7768e;">AlertFeed </span><span style="color:#bb9af7;">severity</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">warning</span><span style="color:#89ddff;">&quot;&gt;
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&lt;</span><span style="color:#f7768e;">title</span><span style="color:#89ddff;">&gt;</span><span style="color:#c0caf5;">System Notices</span><span style="color:#89ddff;">&lt;/</span><span style="color:#f7768e;">title</span><span style="color:#89ddff;">&gt;
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&lt;</span><span style="color:#f7768e;">messages </span><span style="color:#bb9af7;">code</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">OPS-201</span><span style="color:#89ddff;">&quot;&gt;</span><span style="color:#c0caf5;">Deploying new release at 02:00 UTC</span><span style="color:#89ddff;">&lt;/</span><span style="color:#f7768e;">messages</span><span style="color:#89ddff;">&gt;
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&lt;</span><span style="color:#f7768e;">messages </span><span style="color:#bb9af7;">code</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">DB-503</span><span style="color:#89ddff;">&quot;&gt;</span><span style="color:#c0caf5;">Database failover test scheduled</span><span style="color:#89ddff;">&lt;/</span><span style="color:#f7768e;">messages</span><span style="color:#89ddff;">&gt;
</span><span style="color:#89ddff;">&lt;/</span><span style="color:#f7768e;">AlertFeed</span><span style="color:#89ddff;">&gt;</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">AlertFeed </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">xml</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">attribute)]
</span><span style="color:#9abdf5;">    severity</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">xml</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">element)]
</span><span style="color:#9abdf5;">    title</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">xml</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">elements)]
</span><span style="color:#9abdf5;">    messages</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">AlertMessage</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">AlertMessage </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">xml</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">attribute)]
</span><span style="color:#9abdf5;">    code</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">xml</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">text)]
</span><span style="color:#9abdf5;">    body</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">AlertFeed</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">severity</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">warning</span>"<span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">title</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">System Notices</span>"<span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">messages</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;AlertMessage&gt;</span><span style="opacity:0.7"> [</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">AlertMessage</span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">code</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">OPS-201</span>"<span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">body</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Deploying new release at 02:00 UTC</span>"<span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">AlertMessage</span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">code</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">DB-503</span>"<span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">body</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Database failover test scheduled</span>"<span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Diagnostics


### Missing XML annotations

<section class="scenario">
<p class="description">Every field must opt into XML via <code>#[facet(xml::attribute/element/...)]</code> (or <code>#[facet(child)]</code>). Leaving a field unannotated now produces a descriptive error before serialization begins.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">MissingXmlAnnotations </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">title</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">details</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Value Input</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">MissingXmlAnnotations</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">title</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Weekly Report</span>"<span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">details</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Compile-time errors per crate</span>"<span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">xml::missing_xml_annotations</span>

  <span style="color:#e06c75">×</span> MissingXmlAnnotations cannot serialize because these fields lack XML annotations: title: String, details: String. Each field must opt into XML via one of:
  <span style="color:#e06c75">│</span> • #[facet(xml::attribute)]  → &lt;MissingXmlAnnotations field="…" /&gt; (attributes)
  <span style="color:#e06c75">│</span> • #[facet(xml::element)]    → &lt;MissingXmlAnnotations&gt;&lt;field&gt;…&lt;/field&gt;&lt;/MissingXmlAnnotations&gt; (single child)
  <span style="color:#e06c75">│</span> • #[facet(xml::elements)]   → &lt;MissingXmlAnnotations&gt;&lt;field&gt;…&lt;/field&gt;…&lt;/MissingXmlAnnotations&gt; (lists of children)
  <span style="color:#e06c75">│</span> • #[facet(xml::text)]       → &lt;MissingXmlAnnotations&gt;…&lt;/MissingXmlAnnotations&gt; (text content)
  <span style="color:#e06c75">│</span> • #[facet(xml::element_name)] to capture the element/tag name itself.
  <span style="color:#e06c75">│</span> &#96;#[facet(child)]&#96; is accepted as shorthand for xml::element. Use #[facet(flatten)] or #[facet(skip*)] if the field should be omitted.
</code></pre>
</div>
</section>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/a275f00e2c5593da5eaa528fe0b00814b555b5d7/facet-xml/examples/xml_showcase.rs"><code>facet-xml/examples/xml_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/a275f00e2c5593da5eaa528fe0b00814b555b5d7"><code>a275f00e2</code></a></dd>
<dt>Generated</dt><dd><time datetime="2025-12-12T07:18:58+01:00">2025-12-12T07:18:58+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
