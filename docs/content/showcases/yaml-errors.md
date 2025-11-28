+++
title = "facet-yaml Error Showcase"
+++

<div class="showcase">

## Syntax Error: Bad Indentation

<section class="scenario">
<p class="description">YAML indentation is inconsistent or invalid.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">name</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">test
</span><span style="color:#c0caf5;">  </span><span style="color:#f7768e;">nested</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">value
</span><span style="color:#c0caf5;"> </span><span style="color:#f7768e;">wrong</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">indent</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::parse</span>

  <span style="color:#e06c75">×</span> YAML parse error: mapping values are not allowed in this context at byte 19 line 2 column 9
</code></pre>
</div>
</section>

## Syntax Error: Invalid Character

<section class="scenario">
<p class="description">YAML contains an invalid character in an unexpected location.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">name</span><span style="color:#89ddff;">:</span><span style="color:#c0caf5;"> @</span><span style="color:#9ece6a;">invalid</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::parse</span>

  <span style="color:#e06c75">×</span> YAML parse error: unexpected character: `@' at byte 6 line 1 column 7
</code></pre>
</div>
</section>

## Syntax Error: Unclosed Quote

<section class="scenario">
<p class="description">String value has an opening quote but no closing quote.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">message</span><span style="color:#89ddff;">: &quot;</span><span style="color:#9ece6a;">hello world
</span><span style="color:#9ece6a;">name: test</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">message</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::parse</span>

  <span style="color:#e06c75">×</span> YAML parse error: invalid indentation in quoted scalar at byte 9 line 1 column 10
</code></pre>
</div>
</section>

## Unknown Field

<section class="scenario">
<p class="description">YAML contains a field that doesn't exist in the target struct.<br>The error shows the unknown field and lists valid alternatives.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">username</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">alice
</span><span style="color:#f7768e;">emial</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">alice@example.com</span></pre>

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
<pre><code><span style="color:#e06c75">yaml::unknown_field</span>

  <span style="color:#e06c75">×</span> unknown field `emial`, expected one of: ["username", "email"]
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.yaml:2:1</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(247,118,142)">username</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(158,206,106)">alice</span>
 <span style="opacity:0.7">2</span> │ <span style="color:rgb(247,118,142)">emial</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(158,206,106)">alice@example.com</span>
   · <span style="color:#c678dd;font-weight:bold">──┬──</span>
   ·   <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown field `emial`</span>
   ╰────
</code></pre>
</div>
</section>

## Type Mismatch: String for Integer

<section class="scenario">
<p class="description">YAML value is a string where an integer was expected.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">id</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">42
</span><span style="color:#f7768e;">count</span><span style="color:#89ddff;">: &quot;</span><span style="color:#9ece6a;">not a number</span><span style="color:#89ddff;">&quot;</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Item </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">id</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u64</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::invalid_value</span>

  <span style="color:#e06c75">×</span> invalid value: cannot parse `not a number` as signed integer
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.yaml:2:8</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(247,118,142)">id</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(255,158,100)">42</span>
 <span style="opacity:0.7">2</span> │ <span style="color:rgb(247,118,142)">count</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(137,221,255)">"</span><span style="color:rgb(158,206,106)">not a number</span><span style="color:rgb(137,221,255)">"</span>
   · <span style="color:#c678dd;font-weight:bold">       ───────┬──────</span>
   ·               <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">cannot parse `not a number` as signed integer</span>
   ╰────
</code></pre>
</div>
</section>

## Type Mismatch: Integer for String

<section class="scenario">
<p class="description">YAML value is an integer where a string was expected (may succeed with coercion).</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">id</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">42
</span><span style="color:#f7768e;">name</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">123</span></pre>

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
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Item</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">id</span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,179)">42</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">123</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Missing Required Field

<section class="scenario">
<p class="description">YAML is missing a required field that has no default.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">host</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">localhost</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ServerConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,  </span><span style="font-style:italic;color:#565f89;">// Required but missing from YAML
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::reflect</span>

  <span style="color:#e06c75">×</span> reflection error: Field 'ServerConfig::port' was not initialized
</code></pre>
</div>
</section>

## Number Out of Range

<section class="scenario">
<p class="description">YAML number is too large for the target integer type.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">count</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">999999999999</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Counter </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,  </span><span style="font-style:italic;color:#565f89;">// Max value is 4,294,967,295
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::number_out_of_range</span>

  <span style="color:#e06c75">×</span> number `999999999999` out of range for u32
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.yaml:1:8</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(247,118,142)">count</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(255,158,100)">999999999999</span>
   · <span style="color:#c678dd;font-weight:bold">       ──────┬─────</span>
   ·              <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">out of range for u32</span>
   ╰────
</code></pre>
</div>
</section>

## Expected Sequence, Got Scalar

<section class="scenario">
<p class="description">YAML has a scalar where a sequence was expected.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">items</span><span style="color:#89ddff;">: &quot;</span><span style="color:#9ece6a;">not a sequence</span><span style="color:#89ddff;">&quot;</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Container </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">items</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">i32</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,  </span><span style="font-style:italic;color:#565f89;">// Expected sequence, got string
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::unexpected_event</span>

  <span style="color:#e06c75">×</span> unexpected YAML event: got Scalar { value: "not a sequence", style: DoubleQuoted, anchor: 0 }, expected sequence start
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.yaml:1:8</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(247,118,142)">items</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(137,221,255)">"</span><span style="color:rgb(158,206,106)">not a sequence</span><span style="color:rgb(137,221,255)">"</span>
   · <span style="color:#c678dd;font-weight:bold">       ────────┬───────</span>
   ·                <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected Scalar { value: "not a sequence", style: DoubleQuoted, anchor: 0 }</span>
   ╰────
</code></pre>
</div>
</section>

## Expected Mapping, Got Scalar

<section class="scenario">
<p class="description">YAML has a scalar where a mapping was expected.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">config</span><span style="color:#89ddff;">: &quot;</span><span style="color:#9ece6a;">not a mapping</span><span style="color:#89ddff;">&quot;</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Nested </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">value</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Outer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">config</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Nested,  </span><span style="font-style:italic;color:#565f89;">// Expected mapping, got string
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::unexpected_event</span>

  <span style="color:#e06c75">×</span> unexpected YAML event: got Scalar { value: "not a mapping", style: DoubleQuoted, anchor: 0 }, expected mapping start
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.yaml:1:9</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(247,118,142)">config</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(137,221,255)">"</span><span style="color:rgb(158,206,106)">not a mapping</span><span style="color:rgb(137,221,255)">"</span>
   · <span style="color:#c678dd;font-weight:bold">        ───────┬───────</span>
   ·                <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected Scalar { value: "not a mapping", style: DoubleQuoted, anchor: 0 }</span>
   ╰────
</code></pre>
</div>
</section>

## Unknown Enum Variant

<section class="scenario">
<p class="description">YAML specifies a variant name that doesn't exist.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#9ece6a;">Unknown</span></pre>

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
</span><span style="color:#9abdf5;">}
</span><span style="font-style:italic;color:#565f89;">// YAML has &quot;Unknown&quot; which is not a valid variant</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::unexpected_event</span>

  <span style="color:#e06c75">×</span> unexpected YAML event: got Scalar { value: "Unknown", style: Plain, anchor: 0 }, expected mapping (externally tagged enum)
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.yaml:1:1</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(158,206,106)">Unknown</span>
   · <span style="color:#c678dd;font-weight:bold">───┬───</span>
   ·    <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected Scalar { value: "Unknown", style: Plain, anchor: 0 }</span>
   ╰────
</code></pre>
</div>
</section>

## Enum Wrong Format

<section class="scenario">
<p class="description">Externally tagged enum expects {Variant: content} but got wrong format.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">type</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">Text
</span><span style="color:#f7768e;">content</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">hello</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">Message </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Text(</span><span style="color:#0db9d7;">String</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Number(</span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="font-style:italic;color:#565f89;">// Externally tagged expects:
</span><span style="font-style:italic;color:#565f89;">//   Text: &quot;hello&quot;
</span><span style="font-style:italic;color:#565f89;">// But YAML has:
</span><span style="font-style:italic;color:#565f89;">//   type: Text
</span><span style="font-style:italic;color:#565f89;">//   content: hello</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::reflect</span>

  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape Message: No variant found with the given name
</code></pre>
</div>
</section>

## Internally Tagged Enum: Missing Tag Field

<section class="scenario">
<p class="description">Internally tagged enum requires the tag field to be present.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">id</span><span style="color:#89ddff;">: &quot;</span><span style="color:#9ece6a;">123</span><span style="color:#89ddff;">&quot;
</span><span style="color:#f7768e;">method</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">ping</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">C</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">tag </span><span style="color:#89ddff;">= &quot;</span><span style="color:#9ece6a;">type</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">Request </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Ping { id</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String </span><span style="color:#9abdf5;">}</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Echo { id</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> message</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String </span><span style="color:#9abdf5;">}</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="font-style:italic;color:#565f89;">// YAML is missing the &quot;type&quot; tag field</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::reflect</span>

  <span style="color:#e06c75">×</span> reflection error: Operation failed on shape Request: No variant found with the given name
</code></pre>
</div>
</section>

## Duplicate Key

<section class="scenario">
<p class="description">YAML mapping contains the same key more than once.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">name</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">first
</span><span style="color:#f7768e;">value</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">42
</span><span style="color:#f7768e;">name</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">second</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Config </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">value</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Config</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">second</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">value</span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">42</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Anchors and Aliases

<section class="scenario">
<p class="description">YAML anchors and aliases for value reuse.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">defaults</span><span style="color:#89ddff;">: &amp;</span><span style="color:#c0caf5;">defaults
</span><span style="color:#c0caf5;">  </span><span style="color:#f7768e;">timeout</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">30
</span><span style="color:#c0caf5;">  </span><span style="color:#f7768e;">retries</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">3
</span><span style="color:#c0caf5;">
</span><span style="color:#f7768e;">production</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">  </span><span style="color:#ff9e64;">&lt;&lt;</span><span style="color:#89ddff;">: </span><span style="font-style:italic;color:#89ddff;">*</span><span style="color:#c0caf5;">defaults
</span><span style="color:#c0caf5;">  </span><span style="color:#f7768e;">host</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">prod.example.com
</span><span style="color:#c0caf5;">
</span><span style="color:#f7768e;">staging</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">  </span><span style="color:#ff9e64;">&lt;&lt;</span><span style="color:#89ddff;">: </span><span style="font-style:italic;color:#89ddff;">*</span><span style="color:#c0caf5;">defaults
</span><span style="color:#c0caf5;">  </span><span style="color:#f7768e;">host</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">staging.example.com</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">AllConfigs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">defaults</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> ServerConfig,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">production</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> ServerConfig,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">staging</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> ServerConfig,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ServerConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">timeout</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">retries</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::reflect</span>

  <span style="color:#e06c75">×</span> reflection error: Field 'ServerConfig::host' was not initialized
</code></pre>
</div>
</section>

## Multiline String Styles

<section class="scenario">
<p class="description">YAML supports various multiline string styles.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">literal</span><span style="color:#89ddff;">: </span><span style="font-style:italic;color:#bb9af7;">|
</span><span style="color:#9ece6a;">  This is a literal block.
</span><span style="color:#9ece6a;">  Newlines are preserved.
</span><span style="color:#9ece6a;">
</span><span style="color:#f7768e;">folded</span><span style="color:#89ddff;">: </span><span style="font-style:italic;color:#bb9af7;">&gt;
</span><span style="color:#9ece6a;">  This is a folded block.
</span><span style="color:#9ece6a;">  Lines get folded into
</span><span style="color:#9ece6a;">  a single paragraph.</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">TextContent </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">literal</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">folded</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">TextContent</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">literal</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">This is a literal block.
Newlines are preserved.
</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">folded</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">This is a folded block. Lines get folded into a single paragraph.
</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Empty Input

<section class="scenario">
<p class="description">No YAML content at all.</p>
<div class="input">
<h4>YAML Input</h4>
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
<pre><code><span style="color:#e06c75">yaml::unexpected_event</span>

  <span style="color:#e06c75">×</span> unexpected YAML event: got StreamEnd, expected scalar
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.yaml:1:1</span>]
   ╰────
</code></pre>
</div>
</section>

## Null for Required Field

<section class="scenario">
<p class="description">YAML has explicit null where a value is required.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">name</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">~
</span><span style="color:#f7768e;">count</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">42</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Item </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">i32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold">Item</span><span style="opacity:0.7"> {</span>
  <span style="color:#56b6c2">name</span><span style="opacity:0.7">: </span><span style="color:rgb(188,224,81)">~</span><span style="opacity:0.7">,</span>
  <span style="color:#56b6c2">count</span><span style="opacity:0.7">: </span><span style="color:rgb(224,81,93)">42</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Error with Unicode Content

<section class="scenario">
<p class="description">Error reporting handles unicode correctly.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">emoji</span><span style="color:#89ddff;">: &quot;</span><span style="color:#9ece6a;">🎉🚀</span><span style="color:#89ddff;">&quot;
</span><span style="color:#f7768e;">count</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">nope</span></pre>

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
<pre><code><span style="color:#e06c75">yaml::invalid_value</span>

  <span style="color:#e06c75">×</span> invalid value: cannot parse `nope` as signed integer
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.yaml:2:2</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(247,118,142)">emoji</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(137,221,255)">"</span><span style="color:rgb(158,206,106)">🎉🚀</span><span style="color:rgb(137,221,255)">"</span>
 <span style="opacity:0.7">2</span> │ <span style="color:rgb(247,118,142)">count</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(158,206,106)">nope</span>
   · <span style="color:#c678dd;font-weight:bold"> ──┬─</span>
   ·    <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">cannot parse `nope` as signed integer</span>
   ╰────
</code></pre>
</div>
</section>

## Error in Nested Structure

<section class="scenario">
<p class="description">Error location is correctly identified in deeply nested YAML.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">  </span><span style="color:#f7768e;">host</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">localhost
</span><span style="color:#c0caf5;">  </span><span style="color:#f7768e;">ports</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">    </span><span style="color:#f7768e;">http</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">8080
</span><span style="color:#c0caf5;">    </span><span style="color:#f7768e;">https</span><span style="color:#89ddff;">: &quot;</span><span style="color:#9ece6a;">not a number</span><span style="color:#89ddff;">&quot;
</span><span style="color:#c0caf5;">  </span><span style="color:#f7768e;">database</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">    </span><span style="color:#f7768e;">url</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">postgres://localhost/db</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">AppConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Server,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Server </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">ports</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Ports,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">database</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> Database,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Database </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">url</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">Ports </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">http</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">https</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::invalid_value</span>

  <span style="color:#e06c75">×</span> invalid value: cannot parse `not a number` as unsigned integer
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.yaml:5:12</span>]
 <span style="opacity:0.7">4</span> │ <span style="color:rgb(192,202,245)">    </span><span style="color:rgb(247,118,142)">http</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(255,158,100)">8080</span>
 <span style="opacity:0.7">5</span> │ <span style="color:rgb(192,202,245)">    </span><span style="color:rgb(247,118,142)">https</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(137,221,255)">"</span><span style="color:rgb(158,206,106)">not a number</span><span style="color:rgb(137,221,255)">"</span>
   · <span style="color:#c678dd;font-weight:bold">           ───────┬──────</span>
   ·                   <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">cannot parse `not a number` as unsigned integer</span>
 <span style="opacity:0.7">6</span> │ <span style="color:rgb(192,202,245)">  </span><span style="color:rgb(247,118,142)">database</span><span style="color:rgb(137,221,255)">:</span>
   ╰────
</code></pre>
</div>
</section>

## Error in Sequence Item

<section class="scenario">
<p class="description">Error in one item of a sequence is reported with context.</p>
<div class="input">
<h4>YAML Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">users</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">- </span><span style="color:#f7768e;">name</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">Alice
</span><span style="color:#c0caf5;">    </span><span style="color:#f7768e;">age</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">30
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">- </span><span style="color:#f7768e;">name</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">Bob
</span><span style="color:#c0caf5;">    </span><span style="color:#f7768e;">age</span><span style="color:#89ddff;">: &quot;</span><span style="color:#9ece6a;">twenty-five</span><span style="color:#89ddff;">&quot;
</span><span style="color:#c0caf5;">  </span><span style="color:#9abdf5;">- </span><span style="color:#f7768e;">name</span><span style="color:#89ddff;">: </span><span style="color:#9ece6a;">Charlie
</span><span style="color:#c0caf5;">    </span><span style="color:#f7768e;">age</span><span style="color:#89ddff;">: </span><span style="color:#ff9e64;">35</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">UserList </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">users</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Vec</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">User</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">User </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">age</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">yaml::invalid_value</span>

  <span style="color:#e06c75">×</span> invalid value: cannot parse `twenty-five` as unsigned integer
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.yaml:5:10</span>]
 <span style="opacity:0.7">4</span> │ <span style="color:rgb(192,202,245)">  </span><span style="color:rgb(154,189,245)">-</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(247,118,142)">name</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(158,206,106)">Bob</span>
 <span style="opacity:0.7">5</span> │ <span style="color:rgb(192,202,245)">    </span><span style="color:rgb(247,118,142)">age</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(137,221,255)">"</span><span style="color:rgb(158,206,106)">twenty-five</span><span style="color:rgb(137,221,255)">"</span>
   · <span style="color:#c678dd;font-weight:bold">         ──────┬──────</span>
   ·                <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">cannot parse `twenty-five` as unsigned integer</span>
 <span style="opacity:0.7">6</span> │ <span style="color:rgb(192,202,245)">  </span><span style="color:rgb(154,189,245)">-</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(247,118,142)">name</span><span style="color:rgb(137,221,255)">:</span><span style="color:rgb(192,202,245)"> </span><span style="color:rgb(158,206,106)">Charlie</span>
   ╰────
</code></pre>
</div>
</section>
</div>
