+++
title = "Diagnostics"
+++

<div class="showcase">

## Unknown Extension Attribute

<section class="scenario">
<p class="description">Using an unknown attribute like <code>kdl::nonexistent</code> produces a clear error<br>that points directly to the attribute and suggests valid options.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">facet</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Facet</span><span style="color:#89ddff;">;
</span><span style="color:#89ddff;">use</span><span style="color:#c0caf5;"> facet_kdl </span><span style="color:#89ddff;">as</span><span style="color:#c0caf5;"> kdl</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">nonexistent)]
</span><span style="color:#9abdf5;">    field</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown attribute &#96;nonexistent&#96;</span>
<span style="font-weight:bold">       available attributes: child, children, property, argument, arguments, node_name</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:6:18
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">6</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[facet(kdl::nonexistent)]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                  <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile &#96;test&#96; (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Typo in Attribute Name

<section class="scenario">
<p class="description">Common typos like <code>chld</code> instead of <code>child</code> or <code>proprty</code> instead of <code>property</code><br>are caught at compile time with helpful suggestions.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">facet</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Facet</span><span style="color:#89ddff;">;
</span><span style="color:#89ddff;">use</span><span style="color:#c0caf5;"> facet_kdl </span><span style="color:#89ddff;">as</span><span style="color:#c0caf5;"> kdl</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">chld)]
</span><span style="color:#9abdf5;">    nested</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Inner,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Inner </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">proprty)]
</span><span style="color:#9abdf5;">    value</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown attribute &#96;chld&#96;, did you mean &#96;child&#96;?</span>
<span style="font-weight:bold">       available attributes: child, children, property, argument, arguments, node_name</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:6:18
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">6</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[facet(kdl::chld)]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                  <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown attribute &#96;proprty&#96;, did you mean &#96;property&#96;?</span>
<span style="font-weight:bold">       available attributes: child, children, property, argument, arguments, node_name</span>
  <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:12:18
   <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">12</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[facet(kdl::proprty)]
   <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                  <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile &#96;test&#96; (bin "test") due to 2 previous errors</code></pre>
</div>
</section>

## Attribute with Unexpected Arguments

<section class="scenario">
<p class="description">Passing arguments to attributes that don't accept them produces a clear error.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">facet</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Facet</span><span style="color:#89ddff;">;
</span><span style="color:#89ddff;">use</span><span style="color:#c0caf5;"> facet_kdl </span><span style="color:#89ddff;">as</span><span style="color:#c0caf5;"> kdl</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child = &quot;unexpected&quot;)]
</span><span style="color:#9abdf5;">    nested</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Inner,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Inner </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">value</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code></code></pre>
</div>
</section>

## Valid Usage

<section class="scenario">
<p class="description">When extension attributes are used correctly, everything compiles smoothly.<br>This shows the intended usage pattern for KDL attributes.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">facet</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Facet</span><span style="color:#89ddff;">;
</span><span style="color:#89ddff;">use</span><span style="color:#c0caf5;"> facet_kdl </span><span style="color:#89ddff;">as</span><span style="color:#c0caf5;"> kdl</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">child)]
</span><span style="color:#9abdf5;">    server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Server,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">argument)]
</span><span style="color:#9abdf5;">    version</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Server </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">kdl</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">property)]
</span><span style="color:#9abdf5;">    port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {
</span><span style="color:#9abdf5;">    println!(</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Compiles successfully!</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">;
</span><span style="color:#9abdf5;">}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code>Compilation successful! No errors.</code></pre>
</div>
</section>
</div>
