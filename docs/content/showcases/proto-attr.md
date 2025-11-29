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
<pre><code><span style="font-weight:bold"></span><span style="color:#e06c75">error</span><span style="font-weight:bold">: unknown attribute `indexed`; expected one of: skip, rename, column</span>
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
 <span style="font-weight:bold"></span><span style="color:#61afef">--&gt; </span>src/main.rs:5:31
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>
<span style="font-weight:bold"></span><span style="color:#61afef">5</span> <span style="font-weight:bold"></span><span style="color:#61afef">|</span>     #[faket(proto_ext::column(name, primary_key))]
  <span style="font-weight:bold"></span><span style="color:#61afef">|</span>                               <span style="font-weight:bold"></span><span style="color:#e06c75">^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile `test` (bin "test") due to 1 previous error</code></pre>
</div>
</section>

## Valid Usage

<section class="scenario">
<p class="description">When ORM attributes are used correctly, everything compiles smoothly.<br>This shows realistic usage patterns:<br>• skip - exclude structs/fields from generation<br>• rename - map to different table/column names<br>• column - full control: name, nullable, sql_type, primary_key, auto_increment</p>
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
