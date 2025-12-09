+++
title = "KDL"
+++

<div class="showcase">

[`facet-kdl`](https://docs.rs/facet-kdl) provides serialization and deserialization for [KDL](https://kdl.dev), a document language with a focus on human readability. Use attributes like `kdl::property`, `kdl::argument`, and `kdl::child` to control how your types map to KDL's node-based structure.


## Basic Node with Properties

<section class="scenario">
<p class="description">Simple struct with <code>#[facet(kdl::property)]</code> fields becomes KDL properties.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">person </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Alice</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">age</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">30 </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">alice@example.com</span><span style="color:#89ddff;">&quot;
</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">PersonDoc </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    person</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Person,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Person </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    age</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    email</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">PersonDoc</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">person</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Person</span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">name</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Alice</span>"<span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">age</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">30</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">email</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">alice@example.com</span>"<span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Node with Argument

<section class="scenario">
<p class="description"><code>#[facet(kdl::argument)]</code> field becomes a positional argument after the node name.<br>Result: <code>server "web-01" host="localhost" port=8080</code></p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">web-01</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">8080
</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ServerDoc </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Server,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Server </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">argument)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">ServerDoc</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">server</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Server</span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">name</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">web-01</span>"<span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">host</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">localhost</span>"<span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">port</span><span style="opacity:0.7">: </span><span style="color:rgb(224,186,81)">8080</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Nested Nodes (Children)

<section class="scenario">
<p class="description"><code>#[facet(kdl::child)]</code> fields become nested child nodes in braces.<br>The address struct becomes a child node of company.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">company </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Acme Corp</span><span style="color:#89ddff;">&quot; </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#f7768e;">address </span><span style="color:#7dcfff;">street</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">123 Main St</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">city</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Springfield</span><span style="color:#89ddff;">&quot;
</span><span style="color:#9abdf5;">}
</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">CompanyDoc </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    company</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Company,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Company </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    address</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Address,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Address </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    street</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    city</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">CompanyDoc</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">company</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Company</span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">name</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Acme Corp</span>"<span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">address</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Address</span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">street</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">123 Main St</span>"<span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">city</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Springfield</span>"<span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Vec as Repeated Children

<section class="scenario">
<p class="description"><code>#[facet(kdl::children)]</code> on a <code>Vec</code> field creates repeated child nodes.<br>Each <code>Member</code> becomes a separate <code>member</code> node.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">member </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Bob</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">role</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Engineer</span><span style="color:#89ddff;">&quot;
</span><span style="color:#f7768e;">member </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Carol</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">role</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Designer</span><span style="color:#89ddff;">&quot;
</span><span style="color:#f7768e;">member </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Dave</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">role</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Manager</span><span style="color:#89ddff;">&quot;
</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">TeamDoc </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">children)]
</span><span style="color:#9abdf5;">    member</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">Member</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Member </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">argument)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    role</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">TeamDoc</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">member</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;Member&gt;</span><span style="opacity:0.7"> [</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Member</span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">name</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Bob</span>"<span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">role</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Engineer</span>"<span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Member</span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">name</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Carol</span>"<span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">role</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Designer</span>"<span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Member</span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">name</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Dave</span>"<span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">role</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Manager</span>"<span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Complex Nested Config

<section class="scenario">
<p class="description">A realistic application config showing:<br>- Top-level properties (<code>debug</code>, <code>features</code>)<br>- Child nodes with arguments (<code>server</code>, <code>database</code>)<br>- Nested children (<code>tls</code> inside <code>server</code>)<br>- Optional children (<code>tls</code> is <code>Option&lt;TlsConfig&gt;</code>)</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">api-gateway</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">0.0.0.0</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">443 </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#f7768e;">tls </span><span style="color:#7dcfff;">cert_path</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">/etc/ssl/cert.pem</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">key_path</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">/etc/ssl/key.pem</span><span style="color:#89ddff;">&quot;
</span><span style="color:#9abdf5;">}
</span><span style="color:#f7768e;">database </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">primary</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">url</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">postgres://localhost/mydb</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">pool_size</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">10
</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">AppConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    debug</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> ServerConfig,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    database</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> DatabaseConfig,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    features</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">DatabaseConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">argument)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    url</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    pool_size</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ServerConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">argument)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    tls</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">TlsConfig</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">TlsConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    cert_path</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    key_path</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::invalid_document_shape</span>

  <span style="color:#e06c75">×</span> invalid shape Undefined — needed struct with child/children fields
</code></pre>
</div>
</section>

## Roundtrip: Rust → KDL → Rust

<section class="scenario">
<p class="description">Demonstrates serialization followed by deserialization.<br>The value survives the roundtrip intact.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">config </span><span style="color:#7dcfff;">debug</span><span style="color:#89ddff;">=</span><span style="color:#c0caf5;">#</span><span style="color:#ff9e64;">true </span><span style="color:#7dcfff;">max_connections</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">100 </span><span style="color:#7dcfff;">timeout_ms</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">5000
</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ConfigDoc </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    config</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Config,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    debug</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    max_connections</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    timeout_ms</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">ConfigDoc</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">config</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Config</span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">debug</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">max_connections</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">100</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">timeout_ms</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">5000</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Ambiguous Flattened Enum

<section class="scenario">
<p class="description">Both TypeA and TypeB variants have identical fields (value, priority).<br>The solver cannot determine which variant to use.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">resource </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">test</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">value</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">hello</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">priority</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">10</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">AmbiguousConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    resource</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> AmbiguousResource,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">AmbiguousResource </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">argument)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">kind</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> AmbiguousKind,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">AmbiguousKind </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    TypeA(CommonFields)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    TypeB(CommonFields)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">CommonFields </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    value</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    priority</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::solver</span>

  <span style="color:#e06c75">×</span> Ambiguous: multiple resolutions match: ["AmbiguousKind::TypeA", "AmbiguousKind::TypeB"]
<span style="color:#56b6c2">  help: </span>multiple variants match: AmbiguousKind::TypeA, AmbiguousKind::TypeB
        use a KDL type annotation to specify the variant, e.g.: (VariantName)node-name ...
</code></pre>
</div>
</section>

## NoMatch with Per-Candidate Failures

<section class="scenario">
<p class="description">Provide field names that don't exactly match any variant.<br>The solver shows WHY each candidate failed with 'did you mean?' suggestions.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">backend </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">cache</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">hst</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">conn_str</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">pg</span><span style="color:#89ddff;">&quot;</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">NoMatchConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    backend</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> NoMatchBackend,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">NoMatchBackend </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">argument)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">kind</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> NoMatchKind,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">NoMatchKind </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Sqlite(SqliteBackend)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    Postgres(PostgresBackend)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    Redis(RedisBackend)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">RedisBackend </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    password</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">PostgresBackend </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    connection_string</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    pool_size</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SqliteBackend </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    database_path</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    journal_mode</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::solver</span>

  <span style="color:#e06c75">×</span> No matching configuration for fields ["conn_str", "hst", "name"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> No variant matched:
  <span style="color:#e06c75">│</span>   - NoMatchKind::Redis: missing fields ["host", "port"], unknown fields ["conn_str", "hst"]
  <span style="color:#e06c75">│</span>   - NoMatchKind::Postgres: missing fields ["connection_string", "pool_size"], unknown fields ["conn_str", "hst"]
  <span style="color:#e06c75">│</span>   - NoMatchKind::Sqlite: missing fields ["database_path", "journal_mode"], unknown fields ["conn_str", "hst"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> Unknown fields: ["conn_str", "hst"]
  <span style="color:#e06c75">│</span>   Did you mean 'connection_string' instead of 'conn_str'?
  <span style="color:#e06c75">│</span>   Did you mean 'host' instead of 'hst'?
   ╭────
 <span style="opacity:0.7">1</span> │ backend "cache" hst="localhost" conn_str="pg"
   · <span style="color:#c678dd;font-weight:bold">                ─┬─</span><span style="color:#e5c07b;font-weight:bold">             ────┬───</span>
   ·                  <span style="color:#c678dd;font-weight:bold">│</span>                  <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">did you mean &#96;connection_string&#96;?</span>
   ·                  <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">did you mean &#96;host&#96;?</span>
   ╰────
<span style="color:#56b6c2">  help: </span>did you mean NoMatchKind::Redis?
        
        all variants checked:
          - NoMatchKind::Redis: missing host, port, unexpected conn_str, hst
          - NoMatchKind::Postgres: missing connection_string, pool_size, unexpected conn_str, hst
          - NoMatchKind::Sqlite: missing database_path, journal_mode, unexpected conn_str, hst
        
          conn_str -&gt; connection_string (did you mean connection_string?)
          hst -&gt; host (did you mean host?)
        
</code></pre>
</div>
</section>

## Unknown Fields with 'Did You Mean?' Suggestions

<section class="scenario">
<p class="description">Misspell field names and see the solver suggest corrections!<br>Uses Jaro-Winkler similarity to find close matches.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">web</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">hostnam</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">prot</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">8080</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">TypoConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> TypoServer,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">TypoServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">argument)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">kind</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> TypoKind,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">TypoKind </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Web(WebServer)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    Api(ApiServer)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ApiServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    endpoint</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    timeout_ms</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    retry_count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u8</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">WebServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    hostname</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    ssl_enabled</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::solver</span>

  <span style="color:#e06c75">×</span> No matching configuration for fields ["hostnam", "name", "prot"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> No variant matched:
  <span style="color:#e06c75">│</span>   - TypoKind::Web: missing fields ["hostname", "port", "ssl_enabled"], unknown fields ["hostnam", "prot"]
  <span style="color:#e06c75">│</span>   - TypoKind::Api: missing fields ["endpoint", "retry_count", "timeout_ms"], unknown fields ["hostnam", "prot"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> Unknown fields: ["hostnam", "prot"]
  <span style="color:#e06c75">│</span>   Did you mean 'hostname' instead of 'hostnam'?
  <span style="color:#e06c75">│</span>   Did you mean 'port' instead of 'prot'?
   ╭────
 <span style="opacity:0.7">1</span> │ server "web" hostnam="localhost" prot=8080
   · <span style="color:#c678dd;font-weight:bold">             ───┬───</span><span style="color:#e5c07b;font-weight:bold">             ──┬─</span>
   ·                 <span style="color:#c678dd;font-weight:bold">│</span>                  <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">did you mean &#96;port&#96;?</span>
   ·                 <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">did you mean &#96;hostname&#96;?</span>
   ╰────
<span style="color:#56b6c2">  help: </span>did you mean TypoKind::Web?
        
        all variants checked:
          - TypoKind::Web: missing hostname, port, ssl_enabled, unexpected hostnam, prot
          - TypoKind::Api: missing endpoint, retry_count, timeout_ms, unexpected hostnam, prot
        
          hostnam -&gt; hostname (did you mean hostname?)
          prot -&gt; port (did you mean port?)
        
</code></pre>
</div>
</section>

## Value Overflow Detection

<section class="scenario">
<p class="description">When a value doesn't fit ANY candidate type, the solver reports it.<br>count=5000000000 exceeds both u8 (max 255) and u32 (max ~4 billion).</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">data </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">5000000000</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ValueConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    data</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> ValueData,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ValueData </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">payload</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> ValuePayload,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">ValuePayload </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Small(SmallValue)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    Large(LargeValue)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">LargeValue </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SmallValue </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u8</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::invalid_value</span>

  <span style="color:#e06c75">×</span> invalid value for shape: value Integer(5000000000) doesn't fit any candidate type for field 'count'
</code></pre>
</div>
</section>

## Multi-Line Config with Typos

<section class="scenario">
<p class="description">A more realistic multi-line configuration file with several typos.<br>Shows how the solver sorts candidates by closeness to the input.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">database </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">production</span><span style="color:#89ddff;">&quot;</span><span style="color:#c0caf5;"> \
</span><span style="color:#c0caf5;">    </span><span style="color:#7dcfff;">hots</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">db.example.com</span><span style="color:#89ddff;">&quot;</span><span style="color:#c0caf5;"> \
</span><span style="color:#c0caf5;">    </span><span style="color:#7dcfff;">prot</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">3306</span><span style="color:#c0caf5;"> \
</span><span style="color:#c0caf5;">    </span><span style="color:#7dcfff;">usernme</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">admin</span><span style="color:#89ddff;">&quot;</span><span style="color:#c0caf5;"> \
</span><span style="color:#c0caf5;">    </span><span style="color:#7dcfff;">pasword</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">secret123</span><span style="color:#89ddff;">&quot;
</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">MultiLineConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    database</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> MultiLineDatabase,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">MultiLineDatabase </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">argument)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">kind</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> MultiLineDbKind,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">MultiLineDbKind </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    MySql(MySqlConfig)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    Postgres(PgConfig)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    Mongo(MongoConfig)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">MongoConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    uri</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    replica_set</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">PgConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    database</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    ssl_mode</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">MySqlConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    username</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    password</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::solver</span>

  <span style="color:#e06c75">×</span> No matching configuration for fields ["hots", "name", "pasword", "prot", "usernme"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> No variant matched:
  <span style="color:#e06c75">│</span>   - MultiLineDbKind::MySql: missing fields ["host", "password", "port", "username"], unknown fields ["hots", "pasword", "prot", "usernme"]
  <span style="color:#e06c75">│</span>   - MultiLineDbKind::Postgres: missing fields ["database", "host", "port", "ssl_mode"], unknown fields ["hots", "pasword", "prot", "usernme"]
  <span style="color:#e06c75">│</span>   - MultiLineDbKind::Mongo: missing field 'uri', unknown fields ["hots", "pasword", "prot", "usernme"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> Unknown fields: ["hots", "pasword", "prot", "usernme"]
  <span style="color:#e06c75">│</span>   Did you mean 'host' instead of 'hots'?
  <span style="color:#e06c75">│</span>   Did you mean 'password' instead of 'pasword'?
  <span style="color:#e06c75">│</span>   Did you mean 'port' instead of 'prot'?
  <span style="color:#e06c75">│</span>   Did you mean 'username' instead of 'usernme'?
   ╭─[2:5]
 <span style="opacity:0.7">1</span> │ database "production" \
 <span style="opacity:0.7">2</span> │     hots="db.example.com" \
   · <span style="color:#c678dd;font-weight:bold">    ──┬─</span>
   ·       <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">did you mean &#96;host&#96;?</span>
 <span style="opacity:0.7">3</span> │     prot=3306 \
   · <span style="color:#e5c07b;font-weight:bold">    ──┬─</span>
   ·       <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">did you mean &#96;port&#96;?</span>
 <span style="opacity:0.7">4</span> │     usernme="admin" \
   · <span style="color:#98c379;font-weight:bold">    ───┬───</span>
   ·        <span style="color:#98c379;font-weight:bold">╰── </span><span style="color:#98c379;font-weight:bold">did you mean &#96;username&#96;?</span>
 <span style="opacity:0.7">5</span> │     pasword="secret123"
   · <span style="color:#c678dd;font-weight:bold">    ───┬───</span>
   ·        <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">did you mean &#96;password&#96;?</span>
   ╰────
<span style="color:#56b6c2">  help: </span>did you mean MultiLineDbKind::MySql?
        
        all variants checked:
          - MultiLineDbKind::MySql: missing host, password, port, username, unexpected hots, pasword, prot, usernme
          - MultiLineDbKind::Postgres: missing database, host, port, ssl_mode, unexpected hots, pasword, prot, usernme
          - MultiLineDbKind::Mongo: missing uri, unexpected hots, pasword, prot, usernme
        
          hots -&gt; host (did you mean host?)
          pasword -&gt; password (did you mean password?)
          prot -&gt; port (did you mean port?)
          usernme -&gt; username (did you mean username?)
        
</code></pre>
</div>
</section>

## Unknown Field

<section class="scenario">
<p class="description">KDL contains a property that doesn't exist in the target struct.<br>With #[facet(deny_unknown_fields)], this is an error.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">prot</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">8080</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> SimpleServer,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::unknown_property</span>

  <span style="color:#e06c75">×</span> unknown property 'prot', expected one of: host, port
   ╭────
 <span style="opacity:0.7">1</span> │ server host="localhost" prot=8080
   · <span style="color:#c678dd;font-weight:bold">                        ──┬─</span>
   ·                           <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown property &#96;prot&#96;</span>
   ╰────
<span style="color:#56b6c2">  help: </span>expected one of: host, port
</code></pre>
</div>
</section>

## Missing Required Field

<section class="scenario">
<p class="description">KDL is missing a required field that has no default.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot;</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> SimpleServer,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::reflect</span>

  <span style="color:#e06c75">×</span> Field 'SimpleServer::port' was not initialized. If you need to leave fields partially initialized and come back later, use deferred mode (begin_deferred/finish_deferred)
</code></pre>
</div>
</section>

## Syntax Error: Unquoted Boolean

<section class="scenario">
<p class="description">KDL 2.0 requires booleans to be written as #true/#false.<br>Bare <code>true</code> or <code>false</code> is a syntax error with a helpful message.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">enabled</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">true</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> SimpleServer,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::parse</span>

  <span style="color:#e06c75">×</span> Failed to parse KDL document

Error: 
  <span style="color:#e06c75">×</span> Expected identifier string
   ╭────
 <span style="opacity:0.7">1</span> │ server host="localhost" enabled=true
   · <span style="color:#c678dd;font-weight:bold">                                ──┬─</span>
   ·                                   <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">not identifier string</span>
   ╰────
</code></pre>
</div>
</section>

## Syntax Error: Unclosed Brace

<section class="scenario">
<p class="description">Missing closing brace in nested node structure.<br>The parser provides line/column information for the error.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">8080 </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#f7768e;">tls </span><span style="color:#7dcfff;">cert</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">/path/to/cert</span><span style="color:#89ddff;">&quot;
</span></pre>

</div>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> SimpleServer,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::parse</span>

  <span style="color:#e06c75">×</span> Failed to parse KDL document

Error: 
  <span style="color:#e06c75">×</span> No closing '}' for child block
   ╭─[1:35]
 <span style="opacity:0.7">1</span> │ server host="localhost" port=8080 {
   · <span style="color:#c678dd;font-weight:bold">                                  ┬</span>
   ·                                   <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">not closed</span>
 <span style="opacity:0.7">2</span> │     tls cert="/path/to/cert"
   ╰────

Error: 
  <span style="color:#e06c75">×</span> Closing '}' was not found after nodes
   ╭─[1:36]
 <span style="opacity:0.7">1</span> │ <span style="color:#c678dd;font-weight:bold">╭</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span> server host="localhost" port=8080 {
 <span style="opacity:0.7">2</span> │ <span style="color:#c678dd;font-weight:bold">├</span><span style="color:#c678dd;font-weight:bold">─</span><span style="color:#c678dd;font-weight:bold">▶</span>     tls cert="/path/to/cert"
   · <span style="color:#c678dd;font-weight:bold">╰</span><span style="color:#c678dd;font-weight:bold">───</span><span style="color:#c678dd;font-weight:bold">─</span> <span style="color:#c678dd;font-weight:bold">not closed</span>
   ╰────
</code></pre>
</div>
</section>
</div>
