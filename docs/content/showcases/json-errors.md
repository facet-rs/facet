+++
title = "facet-json Error Showcase"
+++

<div class="showcase">

## Syntax Error: Unexpected Character

<section class="scenario">
<p class="description">Invalid character at the start of JSON input.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#c0caf5;">@invalid</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::token</span>

  <span style="color:#e06c75">×</span> unexpected character: '@' (while parsing i32)
   ╭────
 <span style="opacity:0.7">1</span> │ @invalid
   · <span style="color:#c678dd;font-weight:bold">┬</span>
   · <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected '@', expected i32</span>
   ╰────
</code></pre>
</div>
</section>

## Syntax Error: Invalid Character in Object

<section class="scenario">
<p class="description">Invalid character appears mid-parse with surrounding context visible.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">name</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">test</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#7aa2f7;">value</span><span style="color:#89ddff;">&quot;: </span><span style="color:#f7768e;">@bad</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Data </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">value</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::token</span>

  <span style="color:#e06c75">×</span> unexpected character: '@' (while parsing i32)
   ╭────
 <span style="opacity:0.7">1</span> │ {"name": "test", "value": @bad}
   · <span style="color:#c678dd;font-weight:bold">                          ┬</span>
   ·                           <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected '@', expected i32</span>
   ╰────
</code></pre>
</div>
</section>

## Syntax Error: Multiline JSON

<section class="scenario">
<p class="description">Error location is correctly identified in multiline JSON.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">name</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">test</span><span style="color:#89ddff;">&quot;,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">count</span><span style="color:#89ddff;">&quot;: </span><span style="color:#f7768e;">???</span><span style="color:#89ddff;">,
</span><span style="color:#c0caf5;">  </span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">active</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">true
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">active</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::token</span>

  <span style="color:#e06c75">×</span> unexpected character: '?' (while parsing i32)
   ╭─[3:12]
 <span style="opacity:0.7">2</span> │   "name": "test",
 <span style="opacity:0.7">3</span> │   "count": ???,
   · <span style="color:#c678dd;font-weight:bold">           ┬</span>
   ·            <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected '?', expected i32</span>
 <span style="opacity:0.7">4</span> │   "active": true
   ╰────
</code></pre>
</div>
</section>

## Unknown Field

<section class="scenario">
<p class="description">JSON contains a field that doesn't exist in the target struct.<br>The error shows the unknown field and lists valid alternatives.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">username</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">alice</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#7aa2f7;">emial</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">alice@example.com</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">username</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">email</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::unknown_field</span>

  <span style="color:#e06c75">×</span> unknown field `emial`, expected one of: ["username", "email"] (did you mean `email`?)
   ╭────
 <span style="opacity:0.7">1</span> │ {"username": "alice", "emial": "alice@example.com"}
   · <span style="color:#c678dd;font-weight:bold">                      ───┬───</span>
   ·                          <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown field 'emial' - did you mean 'email'?</span>
   ╰────
</code></pre>
</div>
</section>

## Type Mismatch

<section class="scenario">
<p class="description">JSON value type doesn't match the expected Rust type.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">id</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">42</span><span style="color:#89ddff;">, &quot;</span><span style="color:#7aa2f7;">name</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">123</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Item </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::type_mismatch</span>

  <span style="color:#e06c75">×</span> type mismatch: expected String, got unsigned integer
   ╭────
 <span style="opacity:0.7">1</span> │ {"id": 42, "name": 123}
   · <span style="color:#c678dd;font-weight:bold">                   ─┬─</span>
   ·                     <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected String, got unsigned integer</span>
   ╰────
</code></pre>
</div>
</section>

## Missing Required Field

<section class="scenario">
<p class="description">JSON is missing a required field that has no default.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">host</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ServerConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::missing_field</span>

  <span style="color:#e06c75">×</span> missing required field `port`
   ╭────
 <span style="opacity:0.7">1</span> │ {"host": "localhost"}
   · <span style="color:#c678dd;font-weight:bold">┬</span><span style="color:#e5c07b;font-weight:bold">                   ┬</span>
   · <span style="color:#c678dd;font-weight:bold">│</span>                   <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">object ended without field `port`</span>
   · <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">object started here</span>
   ╰────
</code></pre>
</div>
</section>

## Number Out of Range

<section class="scenario">
<p class="description">JSON number is too large for the target integer type.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">count</span><span style="color:#89ddff;">&quot;: </span><span style="color:#ff9e64;">999999999999</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Counter </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::number_out_of_range</span>

  <span style="color:#e06c75">×</span> number `999999999999` out of range for u32
   ╭────
 <span style="opacity:0.7">1</span> │ {"count": 999999999999}
   · <span style="color:#c678dd;font-weight:bold">          ──────┬─────</span>
   ·                 <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">out of range for u32</span>
   ╰────
</code></pre>
</div>
</section>

## Expected Array, Got String

<section class="scenario">
<p class="description">JSON has a string where an array was expected.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">items</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">not an array</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Container </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">items</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">i32</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::unexpected_token</span>

  <span style="color:#e06c75">×</span> unexpected token: got "not an array", expected '['
   ╭────
 <span style="opacity:0.7">1</span> │ {"items": "not an array"}
   · <span style="color:#c678dd;font-weight:bold">          ───────┬──────</span>
   ·                  <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected '[', got '"not an array"'</span>
   ╰────
</code></pre>
</div>
</section>

## Tuple Size Mismatch

<section class="scenario">
<p class="description">JSON array has wrong number of elements for tuple type.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">[</span><span style="color:#ff9e64;">1</span><span style="color:#89ddff;">, </span><span style="color:#ff9e64;">2</span><span style="color:#89ddff;">, </span><span style="color:#ff9e64;">3</span><span style="color:#9abdf5;">]</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">(…)(i32, i32)</span><span style="color:#89ddff;">;</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::unexpected_token</span>

  <span style="color:#e06c75">×</span> unexpected token: got ,, expected ']'
   ╭────
 <span style="opacity:0.7">1</span> │ [1, 2, 3]
   · <span style="color:#c678dd;font-weight:bold">     ┬</span>
   ·      <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected ']', got ','</span>
   ╰────
</code></pre>
</div>
</section>

## Unknown Enum Variant

<section class="scenario">
<p class="description">JSON specifies a variant name that doesn't exist.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">Unknown</span><span style="color:#89ddff;">&quot;</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">Status </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Active</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Inactive</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Pending</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::reflect</span>

  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape Status: No variant found with the given name
</code></pre>
</div>
</section>

## Wrong Variant Format

<section class="scenario">
<p class="description">Externally tagged enum expects {"Variant": content} but got wrong format.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">type</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">Text</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#7aa2f7;">content</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">hello</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">Message </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Text(</span><span style="color:#0db9d7;">String</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Number(</span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::reflect</span>

  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape Message: No variant found with the given name
</code></pre>
</div>
</section>

## Internally Tagged Enum: Missing Tag Field

<section class="scenario">
<p class="description">Internally tagged enum requires the tag field to be present.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">id</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">123</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#7aa2f7;">method</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">ping</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u32</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">tag </span><span style="color:#89ddff;">= &quot;</span><span style="color:#9ece6a;">type</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">Request </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Ping {
</span><span style="color:#9abdf5;">        id</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Echo {
</span><span style="color:#9abdf5;">        id</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">        message</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::reflect</span>

  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape Request: No variant found with the given name
</code></pre>
</div>
</section>

## Trailing Data After Valid JSON

<section class="scenario">
<p class="description">Valid JSON followed by unexpected extra content.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#ff9e64;">42</span><span style="color:#c0caf5;"> extra stuff</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::token</span>

  <span style="color:#e06c75">×</span> unexpected character: 'e'
</code></pre>
</div>
</section>

## Empty Input

<section class="scenario">
<p class="description">No JSON content at all.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
</pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::unexpected_token</span>

  <span style="color:#e06c75">×</span> unexpected token: got EOF, expected scalar value
   ╭────
   ╰────
</code></pre>
</div>
</section>

## Error with Unicode Content

<section class="scenario">
<p class="description">Error reporting handles unicode correctly.</p>
<div class="input">
<h4>JSON Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9abdf5;">{</span><span style="color:#89ddff;">&quot;</span><span style="color:#7aa2f7;">emoji</span><span style="color:#89ddff;">&quot;: &quot;</span><span style="color:#9ece6a;">🎉🚀</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#7aa2f7;">count</span><span style="color:#89ddff;">&quot;: </span><span style="color:#f7768e;">nope</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">EmojiData </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">emoji</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">json::token</span>

  <span style="color:#e06c75">×</span> unexpected character: 'n' (while parsing i32)
   ╭────
 <span style="opacity:0.7">1</span> │ {"emoji": "🎉🚀", "count": nope}
   · <span style="color:#c678dd;font-weight:bold">                           ──┬─</span>
   ·                              <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected 'n', expected i32</span>
   ╰────
</code></pre>
</div>
</section>
</div>
