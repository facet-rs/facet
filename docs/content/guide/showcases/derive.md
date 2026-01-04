+++
title = "Derive Macro Diagnostics"
slug = "derive-diagnostics"
+++

<div class="showcase">

The `#[derive(Facet)]` macro provides helpful compile-time error messages when attributes are used incorrectly. This showcase demonstrates the various error scenarios and their diagnostics.


## Representation Errors


### Conflicting repr: C and Rust

<section class="scenario">
<p class="description">Using both <code>#[repr(C)]</code> and <code>#[repr(Rust)]</code> is not allowed.<br>Facet defers to rustc's E0566 error for this - no duplicate diagnostic.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-k>use</a-k> facet<a-p>::</a-p><a-cr>Facet</a-cr><a-p>;</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-cr>C</a-cr><a-p>,</a-p><a-at> </a-at><a-cr>Rust</a-cr><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>Status</a-t> <a-p>{</a-p>
    <a-cr>Active</a-cr><a-p>,</a-p>
    <a-cr>Inactive</a-cr><a-p>,</a-p>
<a-p>}</a-p>

<a-k>fn</a-k> <a-f>main</a-f><a-p>()</a-p> <a-p>{}</a-p>
</code></pre>
</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#ff0000">error[E0566]</span><span style="font-weight:bold">: conflicting representation hints</span>
 <span style="font-weight:bold"></span><span style="color:#0000ff">--&gt; </span>src/main.rs:4:8
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">4</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> #[repr(C, Rust)]
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>        <span style="font-weight:bold"></span><span style="color:#ff0000">^</span>  <span style="font-weight:bold"></span><span style="color:#ff0000">^^^^</span>

<span style="font-weight:bold">For more information about this error, try &#96;rustc --explain E0566&#96;.</span>
<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile &#96;test&#96; (bin "test") due to 1 previous error</code></pre>
</div>
</section>

### Conflicting repr: C and transparent

<section class="scenario">
<p class="description">Combining <code>#[repr(C)]</code> with <code>#[repr(transparent)]</code> is not valid.<br>Facet defers to rustc's E0692 error for this - no duplicate diagnostic.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-k>use</a-k> facet<a-p>::</a-p><a-cr>Facet</a-cr><a-p>;</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-cr>C</a-cr><a-p>,</a-p><a-at> transparent</a-at><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Wrapper</a-t><a-p>(</a-p><a-t>u32</a-t><a-p>);</a-p>

<a-k>fn</a-k> <a-f>main</a-f><a-p>()</a-p> <a-p>{}</a-p>
</code></pre>
</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#ff0000">error[E0692]</span><span style="font-weight:bold">: transparent struct cannot have other repr hints</span>
 <span style="font-weight:bold"></span><span style="color:#0000ff">--&gt; </span>src/main.rs:4:8
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">4</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> #[repr(C, transparent)]
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>        <span style="font-weight:bold"></span><span style="color:#ff0000">^</span>  <span style="font-weight:bold"></span><span style="color:#ff0000">^^^^^^^^^^^</span>

<span style="font-weight:bold">For more information about this error, try &#96;rustc --explain E0692&#96;.</span>
<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile &#96;test&#96; (bin "test") due to 1 previous error</code></pre>
</div>
</section>

### Conflicting repr: transparent and primitive

<section class="scenario">
<p class="description">Using <code>#[repr(transparent)]</code> with a primitive type like <code>u8</code> is not allowed.<br>Facet defers to rustc's E0692 error for this - no duplicate diagnostic.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-k>use</a-k> facet<a-p>::</a-p><a-cr>Facet</a-cr><a-p>;</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-at>transparent</a-at><a-p>,</a-p><a-at> </a-at><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>Status</a-t> <a-p>{</a-p>
    <a-cr>Active</a-cr><a-p>,</a-p>
    <a-cr>Inactive</a-cr><a-p>,</a-p>
<a-p>}</a-p>

<a-k>fn</a-k> <a-f>main</a-f><a-p>()</a-p> <a-p>{}</a-p>
</code></pre>
</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#ff0000">error[E0692]</span><span style="font-weight:bold">: transparent enum cannot have other repr hints</span>
 <span style="font-weight:bold"></span><span style="color:#0000ff">--&gt; </span>src/main.rs:4:8
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">4</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> #[repr(transparent, u8)]
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>        <span style="font-weight:bold"></span><span style="color:#ff0000">^^^^^^^^^^^</span>  <span style="font-weight:bold"></span><span style="color:#ff0000">^^</span>

<span style="font-weight:bold"></span><span style="color:#ff0000">error[E0731]</span><span style="font-weight:bold">: transparent enum needs exactly one variant, but has 2</span>
 <span style="font-weight:bold"></span><span style="color:#0000ff">--&gt; </span>src/main.rs:5:1
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">5</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> enum Status {
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> <span style="font-weight:bold"></span><span style="color:#ff0000">^^^^^^^^^^^</span> <span style="font-weight:bold"></span><span style="color:#ff0000">needs exactly one variant, but has 2</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">6</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>     Active,
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>     <span style="font-weight:bold"></span><span style="color:#0000ff">------</span> <span style="font-weight:bold"></span><span style="color:#0000ff">variant here</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">7</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>     Inactive,
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>     <span style="font-weight:bold"></span><span style="color:#0000ff">--------</span> <span style="font-weight:bold"></span><span style="color:#0000ff">too many variants in &#96;Status&#96;</span>

<span style="font-weight:bold">Some errors have detailed explanations: E0692, E0731.</span>
<span style="font-weight:bold">For more information about an error, try &#96;rustc --explain E0692&#96;.</span>
<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile &#96;test&#96; (bin "test") due to 2 previous errors</code></pre>
</div>
</section>

### Multiple primitive types in repr

<section class="scenario">
<p class="description">Specifying multiple primitive types in <code>#[repr(...)]</code> is not allowed.<br>Facet defers to rustc's E0566 error for this - no duplicate diagnostic.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-k>use</a-k> facet<a-p>::</a-p><a-cr>Facet</a-cr><a-p>;</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>,</a-p><a-at> </a-at><a-t>u16</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>Priority</a-t> <a-p>{</a-p>
    <a-cr>Low</a-cr><a-p>,</a-p>
    <a-cr>Medium</a-cr><a-p>,</a-p>
    <a-cr>High</a-cr><a-p>,</a-p>
<a-p>}</a-p>

<a-k>fn</a-k> <a-f>main</a-f><a-p>()</a-p> <a-p>{}</a-p>
</code></pre>
</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#ff0000">error[E0566]</span><span style="font-weight:bold">: conflicting representation hints</span>
 <span style="font-weight:bold"></span><span style="color:#0000ff">--&gt; </span>src/main.rs:4:8
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">4</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> #[repr(u8, u16)]
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>        <span style="font-weight:bold"></span><span style="color:#ff0000">^^</span>  <span style="font-weight:bold"></span><span style="color:#ff0000">^^^</span>
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
  <span style="font-weight:bold"></span><span style="color:#0000ff">= </span><span style="font-weight:bold">warning</span>: this was previously accepted by the compiler but is being phased out; it will become a hard error in a future release!
  <span style="font-weight:bold"></span><span style="color:#0000ff">= </span><span style="font-weight:bold">note</span>: for more information, see issue #68585 &lt;https://github.com/rust-lang/rust/issues/68585&gt;
  <span style="font-weight:bold"></span><span style="color:#0000ff">= </span><span style="font-weight:bold">note</span>: &#96;#[deny(conflicting_repr_hints)]&#96; (part of &#96;#[deny(future_incompatible)]&#96;) on by default

<span style="font-weight:bold"></span><span style="color:#e5c07b">warning</span><span style="font-weight:bold">: enum &#96;Priority&#96; is never used</span>
 <span style="font-weight:bold"></span><span style="color:#0000ff">--&gt; </span>src/main.rs:5:6
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">5</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> enum Priority {
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>      <span style="font-weight:bold"></span><span style="color:#e5c07b">^^^^^^^^</span>
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
  <span style="font-weight:bold"></span><span style="color:#0000ff">= </span><span style="font-weight:bold">note</span>: &#96;#[warn(dead_code)]&#96; (part of &#96;#[warn(unused)]&#96;) on by default

<span style="font-weight:bold">For more information about this error, try &#96;rustc --explain E0566&#96;.</span>
<span style="font-weight:bold"></span><span style="color:#e5c07b">warning</span>: &#96;test&#96; (bin "test") generated 1 warning
<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile &#96;test&#96; (bin "test") due to 2 previous errors; 1 warning emitted</code></pre>
</div>
</section>

### Unsupported repr (facet-specific)

<section class="scenario">
<p class="description">Using <code>#[repr(packed)]</code> is valid Rust, but facet doesn't support it.<br>This is a facet-specific error with a helpful message.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-k>use</a-k> facet<a-p>::</a-p><a-cr>Facet</a-cr><a-p>;</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-at>packed</a-at><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Data</a-t> <a-p>{</a-p>
    <a-pr>a</a-pr><a-p>:</a-p> <a-t>u8</a-t><a-p>,</a-p>
    <a-pr>b</a-pr><a-p>:</a-p> <a-t>u32</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-k>fn</a-k> <a-f>main</a-f><a-p>()</a-p> <a-p>{}</a-p>
</code></pre>
</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#ff0000">error</span><span style="font-weight:bold">: unsupported repr &#96;packed&#96; - facet only supports C, Rust, transparent, and primitive integer types</span>
 <span style="font-weight:bold"></span><span style="color:#0000ff">--&gt; </span>src/main.rs:4:8
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">4</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> #[repr(packed)]
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>        <span style="font-weight:bold"></span><span style="color:#ff0000">^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile &#96;test&#96; (bin "test") due to 1 previous error</code></pre>
</div>
</section>

### Multiple #[repr] attributes

<section class="scenario">
<p class="description">Having multiple separate <code>#[repr(...)]</code> attributes triggers rustc's E0566.<br>Facet defers to rustc for this - no duplicate diagnostic.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-k>use</a-k> facet<a-p>::</a-p><a-cr>Facet</a-cr><a-p>;</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-cr>C</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>Status</a-t> <a-p>{</a-p>
    <a-cr>Active</a-cr><a-p>,</a-p>
    <a-cr>Inactive</a-cr><a-p>,</a-p>
<a-p>}</a-p>

<a-k>fn</a-k> <a-f>main</a-f><a-p>()</a-p> <a-p>{}</a-p>
</code></pre>
</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#ff0000">error[E0566]</span><span style="font-weight:bold">: conflicting representation hints</span>
 <span style="font-weight:bold"></span><span style="color:#0000ff">--&gt; </span>src/main.rs:4:8
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">4</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> #[repr(C)]
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>        <span style="font-weight:bold"></span><span style="color:#ff0000">^</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">5</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> #[repr(u8)]
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>        <span style="font-weight:bold"></span><span style="color:#ff0000">^^</span>
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
  <span style="font-weight:bold"></span><span style="color:#0000ff">= </span><span style="font-weight:bold">warning</span>: this was previously accepted by the compiler but is being phased out; it will become a hard error in a future release!
  <span style="font-weight:bold"></span><span style="color:#0000ff">= </span><span style="font-weight:bold">note</span>: for more information, see issue #68585 &lt;https://github.com/rust-lang/rust/issues/68585&gt;
  <span style="font-weight:bold"></span><span style="color:#0000ff">= </span><span style="font-weight:bold">note</span>: &#96;#[deny(conflicting_repr_hints)]&#96; (part of &#96;#[deny(future_incompatible)]&#96;) on by default

<span style="font-weight:bold"></span><span style="color:#e5c07b">warning</span><span style="font-weight:bold">: enum &#96;Status&#96; is never used</span>
 <span style="font-weight:bold"></span><span style="color:#0000ff">--&gt; </span>src/main.rs:6:6
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">6</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> enum Status {
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>      <span style="font-weight:bold"></span><span style="color:#e5c07b">^^^^^^</span>
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
  <span style="font-weight:bold"></span><span style="color:#0000ff">= </span><span style="font-weight:bold">note</span>: &#96;#[warn(dead_code)]&#96; (part of &#96;#[warn(unused)]&#96;) on by default

<span style="font-weight:bold">For more information about this error, try &#96;rustc --explain E0566&#96;.</span>
<span style="font-weight:bold"></span><span style="color:#e5c07b">warning</span>: &#96;test&#96; (bin "test") generated 1 warning
<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile &#96;test&#96; (bin "test") due to 2 previous errors; 1 warning emitted</code></pre>
</div>
</section>

## Rename Errors


### Unknown rename_all rule (facet-specific)

<section class="scenario">
<p class="description">Using an unknown case convention in <code>rename_all</code> is a facet-specific error.<br>Valid options: <code>camelCase</code>, <code>snake_case</code>, <code>kebab-case</code>, <code>PascalCase</code>, <code>SCREAMING_SNAKE_CASE</code>.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-k>use</a-k> facet<a-p>::</a-p><a-cr>Facet</a-cr><a-p>;</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>rename_all = </a-at><a-s>&quot;SCREAMING_SNAKE&quot;</a-s><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Config</a-t> <a-p>{</a-p>
    <a-pr>user_name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-pr>max_retries</a-pr><a-p>:</a-p> <a-t>u32</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-k>fn</a-k> <a-f>main</a-f><a-p>()</a-p> <a-p>{}</a-p>
</code></pre>
</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:#ff0000">error</span><span style="font-weight:bold">: unknown #[facet(rename_all = "...")] rule: &#96;SCREAMING_SNAKE&#96;. Valid options: camelCase, snake_case, kebab-case, PascalCase, SCREAMING_SNAKE_CASE, SCREAMING-KEBAB-CASE, lowercase, UPPERCASE</span>
 <span style="font-weight:bold"></span><span style="color:#0000ff">--&gt; </span>src/main.rs:4:9
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>
<span style="font-weight:bold"></span><span style="color:#0000ff">4</span> <span style="font-weight:bold"></span><span style="color:#0000ff">|</span> #[facet(rename_all = "SCREAMING_SNAKE")]
  <span style="font-weight:bold"></span><span style="color:#0000ff">|</span>         <span style="font-weight:bold"></span><span style="color:#ff0000">^^^^^^^^^^</span>

<span style="font-weight:bold"></span><span style="color:#e06c75">error</span>: could not compile &#96;test&#96; (bin "test") due to 1 previous error</code></pre>
</div>
</section>

### rename on container (facet-specific)

<section class="scenario">
<p class="description">Using <code>#[facet(rename = "...")]</code> on a struct/enum is a facet-specific error.<br>A container's name is controlled by its parent field, not by itself.</p>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-k>use</a-k> facet<a-p>::</a-p><a-cr>Facet</a-cr><a-p>;</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>rename = </a-at><a-s>&quot;MyConfig&quot;</a-s><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Config</a-t> <a-p>{</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-pr>value</a-pr><a-p>:</a-p> <a-t>u32</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-k>fn</a-k> <a-f>main</a-f><a-p>()</a-p> <a-p>{}</a-p>
</code></pre>
</div>
<div class="compiler-error">
<h4>Compiler Error</h4>
<pre><code></code></pre>
</div>
</section>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/c5842bc4cd833fedc52522b20f09daedff260a0e/facet/examples/derive_showcase.rs"><code>facet/examples/derive_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/c5842bc4cd833fedc52522b20f09daedff260a0e"><code>c5842bc4c</code></a></dd>
<dt>Generated</dt><dd><time datetime="2026-01-04T12:56:12+01:00">2026-01-04T12:56:12+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
