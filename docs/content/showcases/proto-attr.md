+++
title = "proto-attr Compile Error Showcase"
+++

<div class="showcase">

## Unknown Extension Attribute

<section class="scenario">
<p class="description">Using an unknown attribute like <code>proto_ext::foobar</code> produces a clear error<br>listing all available attributes.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">foobar)]
</span><span style="color:#9abdf5;">    field</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown attribute `foobar`</span>
       <span style="font-weight:bold">available proto-ext attributes: skip, rename, column</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:24
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::foobar)]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                        <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Typo in Attribute Name

<section class="scenario">
<p class="description">Common typos like <code>skp</code> instead of <code>skip</code> are caught at compile time<br>with a helpful "did you mean?" suggestion.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::skp</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">field</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown attribute `skp`, did you mean `skip`?</span>
       <span style="font-weight:bold">available proto-ext attributes: skip, rename, column</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:4:20
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">4</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span> #[faket(proto_ext::skp)]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                    <span style="font-weight:bold"></span><span style="color:#e06c75">^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Unit Attribute with Arguments

<section class="scenario">
<p class="description">The <code>skip</code> attribute is a unit variant that takes no arguments.<br>Passing arguments produces a clear error explaining the correct usage.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">skip</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">unexpected</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">field</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `skip` does not take arguments; use just `skip`</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:4:25
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">4</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span> #[faket(proto_ext::skip("unexpected"))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                         <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Newtype Attribute Missing Value

<section class="scenario">
<p class="description">The <code>rename</code> attribute requires a string value.<br>Omitting the value produces an error showing the expected syntax.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::rename</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">field</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `rename` requires a string value: `rename("name")` or `rename = "name"`</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:4:1
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">4</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span> #[faket(proto_ext::rename)]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span> <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^^^^^^^^^^^^^^^^^^^^^</span>
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
  <span style="font-weight:bold"></span><span style="color:#61afef">= </span><span style="font-weight:bold">note</span>: this error originates in the macro `$crate::__parse_rename` which comes from the expansion of the macro `proto_ext::__parse_attr` (in Nightly builds, run with -Z macro-backtrace for more info)

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Unknown Field in Struct Attribute

<section class="scenario">
<p class="description">Typos in field names like <code>nam</code> instead of <code>name</code> are caught<br>with a "did you mean?" suggestion and list of valid fields.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(nam = &quot;user_id&quot;))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown field `nam` in `Column`, did you mean `name`?</span>
       <span style="font-weight:bold">available fields: name, primary_key</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:31
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::column(nam = "user_id"))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                               <span style="font-weight:bold"></span><span style="color:#e06c75">^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Struct Field Missing Value

<section class="scenario">
<p class="description">The <code>name</code> field in <code>column</code> requires a string value.<br>Using it as a flag produces an error showing the correct syntax.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(name, primary_key))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `name` requires a string value: `name = "column_name"`</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:5
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::column(name, primary_key))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^</span>
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
  <span style="font-weight:bold"></span><span style="color:#61afef">= </span><span style="font-weight:bold">note</span>: this error originates in the macro `$crate::__parse_name_field` which comes from the expansion of the macro `proto_ext::__parse_attr` (in Nightly builds, run with -Z macro-backtrace for more info)

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Valid Usage

<section class="scenario">
<p class="description">When extension attributes are used correctly, everything compiles smoothly.<br>This shows the intended usage patterns for proto-ext attributes.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::skip</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SkippedStruct </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">field</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">rename</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">NewName</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">RenamedStruct </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">field</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(name = &quot;user_id&quot;, primary_key))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(name = &quot;user_name&quot;))]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">rename(&quot;email_address&quot;))]
</span><span style="color:#9abdf5;">    email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {
</span><span style="color:#9abdf5;">    println!(</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Compiles successfully!</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">;
</span><span style="color:#9abdf5;">}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code>✓ Compilation successful! No errors.</code></pre>
</div>
</section>
</div>
