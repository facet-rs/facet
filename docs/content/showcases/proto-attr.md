+++
title = "proto-attr Compile Error Showcase"
+++

<div class="showcase">

## Unknown Extension Attribute

<section class="scenario">
<p class="description">Using an unknown ORM attribute like <code>indexed</code> produces a clear error<br>listing all available attributes (skip, rename, column).</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">indexed)]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown attribute `indexed`; did you mean `index`?</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:24
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::indexed)]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                        <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^</span>

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
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">skp)]
</span><span style="color:#9abdf5;">    password_hash</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown attribute `skp`; did you mean `skip`?</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:24
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::skp)]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                        <span style="font-weight:bold"></span><span style="color:#e06c75">^^^</span>

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
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">skip(&quot;serialization&quot;))]
</span><span style="color:#9abdf5;">    password_hash</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `skip` does not take arguments; use just `skip`</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:24
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::skip("serialization"))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                        <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Newtype Attribute Missing Value

<section class="scenario">
<p class="description">The <code>rename</code> attribute requires a string value to specify the new name.<br>Omitting the value produces an error showing the expected syntax.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::rename</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">UserProfile </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `rename` requires a string value: `rename("name")` or `rename = "name"`</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:4:20
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">4</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span> #[faket(proto_ext::rename)]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                    <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Unknown Field in Struct Attribute

<section class="scenario">
<p class="description">Typos in field names like <code>nam</code> instead of <code>name</code> are caught<br>with a "did you mean?" suggestion and list of valid fields<br>(name, nullable, sql_type, primary_key, auto_increment).</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(nam = &quot;user_id&quot;, primary_key))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown field `nam`; did you mean `name`? Known fields: name, nullable, sql_type, primary_key, auto_increment</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:31
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::column(nam = "user_id", primary_key))]
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
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `name` requires a string value: `name = "value"`</span>
       <span style="font-weight:bold">  = help: Override the database column name; use `name = "col_name"` (defaults to Rust field name)</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:31
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::column(name, primary_key))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                               <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Index Field Typo (list_string)

<section class="scenario">
<p class="description">Typos in field names like <code>column</code> instead of <code>columns</code> are caught<br>with a helpful "did you mean?" suggestion.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">index</span><span style="color:#9abdf5;">(</span><span style="color:#7aa2f7;">column </span><span style="color:#89ddff;">=</span><span style="color:#7aa2f7;"> [</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">id</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">email</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">]</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">UserIndex </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown field `column`; did you mean `columns`? Known fields: name, columns, unique</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:4:26
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">4</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span> #[faket(proto_ext::index(column = ["id", "email"]))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                          <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Index Wrong Type for List

<section class="scenario">
<p class="description">The <code>columns</code> field expects a list like <code>["a", "b"]</code>, not a string.<br>Using the wrong type produces a clear error explaining the expected format.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">index</span><span style="color:#9abdf5;">(</span><span style="color:#7aa2f7;">columns </span><span style="color:#89ddff;">= &quot;</span><span style="color:#9ece6a;">email</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">UserIndex </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `columns` expects a list, not a single string; try `columns = ["email"]`</span>
       <span style="font-weight:bold">  = help: Columns in this index; use `columns = ["col1", "col2"]` (order matters for query optimization)</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:4:36
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">4</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span> #[faket(proto_ext::index(columns = "email"))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                    <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Range Wrong Type for Integer

<section class="scenario">
<p class="description">The <code>min</code> and <code>max</code> fields in <code>range</code> expect integers, not strings.<br>Using a string produces an error showing the correct syntax.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">range(min = &quot;zero&quot;, max = 100))]
</span><span style="color:#9abdf5;">    age</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `min` expected integer literal, got `"zero"`</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:36
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::range(min = "zero", max = 100))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                    <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## OnDelete String Instead of Identifier

<section class="scenario">
<p class="description">The <code>action</code> field expects a bare identifier like <code>cascade</code>, not a string.<br>The error message suggests removing the quotes: <code>action = cascade</code>.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Post </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">on_delete(action = &quot;cascade&quot;))]
</span><span style="color:#9abdf5;">    author_id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `action` expects a bare identifier, not a string; try `action = cascade` (without quotes)</span>
       <span style="font-weight:bold">  = help: What happens when referenced row is deleted; use `action = cascade` (delete), `set_null`, `restrict` (prevent), or `no_action`</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:43
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::on_delete(action = "cascade"))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                           <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Duplicate Field

<section class="scenario">
<p class="description">Specifying the same field twice in an attribute is an error.<br>Each field can only appear once in an attribute.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(name = &quot;user_id&quot;, name = &quot;id&quot;))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: duplicate field `name`; each field can only be specified once</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:49
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::column(name = "user_id", name = "id"))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                                 <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Mixed Types in List

<section class="scenario">
<p class="description">List fields require all elements to be the same type.<br>A string list like <code>columns</code> cannot contain integers.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">index</span><span style="color:#9abdf5;">(</span><span style="color:#7aa2f7;">columns </span><span style="color:#89ddff;">=</span><span style="color:#7aa2f7;"> [</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">email</span><span style="color:#89ddff;">&quot;,</span><span style="color:#7aa2f7;"> 123]</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">UserIndex </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: expected string literal in list, got `123`</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:4:46
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">4</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span> #[faket(proto_ext::index(columns = ["email", 123]))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                              <span style="font-weight:bold"></span><span style="color:#e06c75">^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Wrong Bracket Type for List

<section class="scenario">
<p class="description">Lists use square brackets <code>[...]</code>, not curly braces <code>{...}</code>.<br>The error specifically tells you to use square brackets.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">index</span><span style="color:#9abdf5;">(</span><span style="color:#7aa2f7;">columns </span><span style="color:#89ddff;">=</span><span style="color:#7aa2f7;"> {</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">email</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">}</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">UserIndex </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `columns` expects square brackets `[]`, not curly braces `{}`; try `columns = ["a", "b"]`</span>
       <span style="font-weight:bold">  = help: Columns in this index; use `columns = ["col1", "col2"]` (order matters for query optimization)</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:4:36
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">4</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span> #[faket(proto_ext::index(columns = {"email"}))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                    <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Integer Overflow

<section class="scenario">
<p class="description">The error shows the field name, the value, and the schema-defined type.<br>Each integer field in the grammar specifies its type (here: i64).</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">range(min = 99999999999999999999999))]
</span><span style="color:#9abdf5;">    score</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `min` value `99999999999999999999999` is too large; this field accepts i64 (range -9223372036854775808 to 9223372036854775807)</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:36
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::range(min = 99999999999999999999999))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                    <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^^^^^^^^^^^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Bool Field with String Value

<section class="scenario">
<p class="description">Boolean fields expect <code>true</code> or <code>false</code> literals, not strings.<br>The error suggests removing the quotes: <code>primary_key = true</code>.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(primary_key = &quot;true&quot;))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `primary_key` expects `true` or `false`, not a string; try `primary_key = true` (without quotes)</span>
       <span style="font-weight:bold">  = help: Mark as primary key; use `primary_key` or `primary_key = true` (tables typically have one primary key)</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:45
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::column(primary_key = "true"))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                             <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Integer Field Used as Flag

<section class="scenario">
<p class="description">Integer fields require a value; they cannot be used as flags.<br>Using <code>min</code> without <code>= value</code> produces an error.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">range(min, max = 100))]
</span><span style="color:#9abdf5;">    age</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `min` requires an integer value: `min = 42`</span>
       <span style="font-weight:bold">  = help: Minimum allowed value (inclusive); use `min = 0` to reject negative numbers</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:30
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::range(min, max = 100))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                              <span style="font-weight:bold"></span><span style="color:#e06c75">^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Identifier Instead of String

<section class="scenario">
<p class="description">String fields require quoted values, not bare identifiers.<br>The error suggests adding quotes: <code>name = "user_id"</code>.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(name = user_id))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `name` expects a string literal, not an identifier; try `name = "user_id"` (with quotes)</span>
       <span style="font-weight:bold">  = help: Override the database column name; use `name = "col_name"` (defaults to Rust field name)</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:38
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::column(name = user_id))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                      <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Single String Instead of List

<section class="scenario">
<p class="description">List fields require <code>[...]</code> syntax even for a single element.<br>The error suggests wrapping in brackets: <code>columns = ["email"]</code>.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">index</span><span style="color:#9abdf5;">(</span><span style="color:#7aa2f7;">columns </span><span style="color:#89ddff;">= &quot;</span><span style="color:#9ece6a;">email</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">UserIndex </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `columns` expects a list, not a single string; try `columns = ["email"]`</span>
       <span style="font-weight:bold">  = help: Columns in this index; use `columns = ["col1", "col2"]` (order matters for query optimization)</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:4:36
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">4</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span> #[faket(proto_ext::index(columns = "email"))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                    <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Help Text: Column Primary Key

<section class="scenario">
<p class="description">Error messages include contextual help explaining the field AND how to use it.<br>The help shows: correct syntax, typical usage, and semantic meaning.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(primary_key = &quot;yes&quot;))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `primary_key` expects `true` or `false`, not a string; try `primary_key = true` (without quotes)</span>
       <span style="font-weight:bold">  = help: Mark as primary key; use `primary_key` or `primary_key = true` (tables typically have one primary key)</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:45
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::column(primary_key = "yes"))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                             <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Help Text: Index Columns

<section class="scenario">
<p class="description">The help text explains that <code>columns</code> specifies which columns<br>to include in the index: "Columns to include in the index".</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">index</span><span style="color:#9abdf5;">(</span><span style="color:#7aa2f7;">columns</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">UserIndex </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `columns` requires a list value: `columns = ["a", "b"]`</span>
       <span style="font-weight:bold">  = help: Columns in this index; use `columns = ["col1", "col2"]` (order matters for query optimization)</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:4:26
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">4</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span> #[faket(proto_ext::index(columns))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                          <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Help Text: Range Min

<section class="scenario">
<p class="description">The help text clarifies that <code>min</code> is the "Minimum value (inclusive)".<br>Doc comments in the grammar DSL become contextual help in errors.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">range(min = &quot;zero&quot;))]
</span><span style="color:#9abdf5;">    age</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#bb9af7;">fn </span><span style="color:#7aa2f7;">main</span><span style="color:#9abdf5;">() {}
</span></pre>

</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: `min` expected integer literal, got `"zero"`</span>
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:36
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::range(min = "zero"))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                                    <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Valid Usage

<section class="scenario">
<p class="description">When ORM attributes are used correctly, everything compiles smoothly.<br>This shows realistic usage patterns:<br>• skip - exclude structs/fields from generation<br>• rename - map to different table/column names<br>• column - full control: name, nullable, sql_type, primary_key, auto_increment<br>• index - database indexes with columns list (list_string field type)<br>• range - validation bounds with min/max (opt_i64 field type)<br>• on_delete - foreign key behavior with bare identifiers (ident field type)</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">use </span><span style="color:#c0caf5;">proto_attr</span><span style="color:#89ddff;">::</span><span style="color:#c0caf5;">Faket</span><span style="color:#89ddff;">;
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// A table we want to exclude from ORM generation
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::skip</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">InternalCache </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">data</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">u8</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Map to a different table name
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">rename</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">user_profiles</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">UserProfile </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Full ORM column configuration example
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">index</span><span style="color:#9abdf5;">(</span><span style="color:#7aa2f7;">name </span><span style="color:#89ddff;">= &quot;</span><span style="color:#9ece6a;">idx_user_email</span><span style="color:#89ddff;">&quot;,</span><span style="color:#7aa2f7;"> columns </span><span style="color:#89ddff;">=</span><span style="color:#7aa2f7;"> [</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">email</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">]</span><span style="color:#89ddff;">,</span><span style="color:#7aa2f7;"> unique</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Primary key with auto-increment
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(name = &quot;id&quot;, primary_key, auto_increment))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Custom column name
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(name = &quot;user_name&quot;))]
</span><span style="color:#9abdf5;">    name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Nullable TEXT field for bio
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(nullable, sql_type = &quot;TEXT&quot;))]
</span><span style="color:#9abdf5;">    bio</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Non-nullable timestamp
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(nullable = false, sql_type = &quot;TIMESTAMP&quot;))]
</span><span style="color:#9abdf5;">    created_at</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Skip sensitive field from serialization
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">skip)]
</span><span style="color:#9abdf5;">    password_hash</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Rename field for API compatibility
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">rename(&quot;email_address&quot;))]
</span><span style="color:#9abdf5;">    email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Validation: age must be between 0 and 150
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">range(min = 0, max = 150, message = &quot;Age must be realistic&quot;))]
</span><span style="color:#9abdf5;">    age</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Foreign key with ON DELETE behavior
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Post </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(primary_key, auto_increment))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// When author is deleted, cascade delete their posts
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">on_delete(action = cascade))]
</span><span style="color:#9abdf5;">    author_id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// When category is deleted, set to null
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">on_delete(action = set_null))]
</span><span style="color:#9abdf5;">    category_id</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">i64</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">title</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Composite index example
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Faket</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">index</span><span style="color:#9abdf5;">(</span><span style="color:#7aa2f7;">columns </span><span style="color:#89ddff;">=</span><span style="color:#7aa2f7;"> [</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">user_id</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">created_at</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">]</span><span style="color:#9abdf5;">))]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">faket</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">proto_ext::</span><span style="color:#7aa2f7;">index</span><span style="color:#9abdf5;">(</span><span style="color:#7aa2f7;">name </span><span style="color:#89ddff;">= &quot;</span><span style="color:#9ece6a;">idx_status</span><span style="color:#89ddff;">&quot;,</span><span style="color:#7aa2f7;"> columns </span><span style="color:#89ddff;">=</span><span style="color:#7aa2f7;"> [</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">status</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">]</span><span style="color:#89ddff;">,</span><span style="color:#7aa2f7;"> unique</span><span style="color:#9abdf5;">))]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Order </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    #[faket(</span><span style="color:#7dcfff;">proto_ext</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">column(primary_key))]
</span><span style="color:#9abdf5;">    id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">user_id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">status</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">created_at</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i64</span><span style="color:#9abdf5;">,
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
