+++
title = "facet-kdl Serialization Showcase"
+++

<div class="showcase">

## Basic Node with Properties

<section class="scenario">
<p class="description">Simple struct with <code>#[facet(property)]</code> fields becomes KDL properties.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">person </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Alice</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">age</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">30 </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">alice@example.com</span><span style="color:#89ddff;">&quot;
</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">PersonDoc </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">person</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Person,
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
<pre><code><span style="font-weight:bold">PersonDoc</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">person</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Person</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Alice</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">age</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">30</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">email</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Option&lt;String&gt;</span><span style="opacity:0.7">::Some(</span><span style="color:rgb(188,224,81)">alice@example.com</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Node with Argument

<section class="scenario">
<p class="description"><code>#[facet(argument)]</code> field becomes a positional argument after the node name.<br>Result: <code>server "web-01" host="localhost" port=8080</code></p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">web-01</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">8080
</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ServerDoc </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Server,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Server </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">ServerDoc</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">server</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Server</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">web-01</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">host</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">localhost</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">port</span><span style="opacity:0.7">: </span><span style="color:rgb(224,186,81)">8080</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Nested Nodes (Children)

<section class="scenario">
<p class="description"><code>#[facet(child)]</code> fields become nested child nodes in braces.<br>The address struct becomes a child node of company.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">company </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Acme Corp</span><span style="color:#89ddff;">&quot; </span><span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">    </span><span style="color:#f7768e;">address </span><span style="color:#7dcfff;">street</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">123 Main St</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">city</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Springfield</span><span style="color:#89ddff;">&quot;
</span><span style="color:#9abdf5;">}
</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">CompanyDoc </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">company</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Company,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Company </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
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
<pre><code><span style="font-weight:bold">CompanyDoc</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">company</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Company</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Acme Corp</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">address</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Address</span><span style="opacity:0.7"> {</span>
      <span style="color:#56b6c2">street</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">123 Main St</span><span style="opacity:0.7">,</span>
      <span style="color:#56b6c2">city</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Springfield</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Vec as Repeated Children

<section class="scenario">
<p class="description"><code>#[facet(children)]</code> on a <code>Vec</code> field creates repeated child nodes.<br>Each <code>Member</code> becomes a separate <code>member</code> node.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">member </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Bob</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">role</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Engineer</span><span style="color:#89ddff;">&quot;
</span><span style="color:#f7768e;">member </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Carol</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">role</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Designer</span><span style="color:#89ddff;">&quot;
</span><span style="color:#f7768e;">member </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Dave</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">role</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">Manager</span><span style="color:#89ddff;">&quot;
</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">TeamDoc </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">member</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">Member</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Member </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">role</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">TeamDoc</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">member</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Vec&lt;Member&gt;</span><span style="opacity:0.7"> [</span>
    <span style="font-weight:bold">Member</span><span style="opacity:0.7"> {</span>
      <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Bob</span><span style="opacity:0.7">,</span>
      <span style="color:#56b6c2">role</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Engineer</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold">Member</span><span style="opacity:0.7"> {</span>
      <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Carol</span><span style="opacity:0.7">,</span>
      <span style="color:#56b6c2">role</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Designer</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold">Member</span><span style="opacity:0.7"> {</span>
      <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Dave</span><span style="opacity:0.7">,</span>
      <span style="color:#56b6c2">role</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">Manager</span><span style="opacity:0.7">,</span>
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
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">AppConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">debug</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> ServerConfig,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">database</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> DatabaseConfig,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">features</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">DatabaseConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">url</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">pool_size</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ServerConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">tls</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">TlsConfig</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">TlsConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">cert_path</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">key_path</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
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
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ConfigDoc </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">config</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Config,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">debug</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">max_connections</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">timeout_ms</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">ConfigDoc</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">config</span><span style="opacity:0.7">: </span><span style="font-weight:bold">Config</span><span style="opacity:0.7"> {</span>
    <span style="color:#56b6c2">debug</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">max_connections</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">100</span><span style="opacity:0.7">,</span>
    <span style="color:#56b6c2">timeout_ms</span><span style="opacity:0.7">: </span><span style="color:rgb(207,81,224)">5000</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>
</div>
