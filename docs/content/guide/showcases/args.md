+++
title = "Args"
+++

<div class="showcase">

[`facet-args`](https://docs.rs/facet-args) turns any `Facet` struct into a command-line interface. Define your CLI with doc comments and attributes like `args::named`, `args::positional`, and `args::subcommand`. Get auto-generated help text, shell completions for bash/zsh/fish, and rich error diagnostics with typo suggestions.


## Successful Parsing


### Simple Arguments

<section class="scenario">
<p class="description">Parse a struct with flags, options, and positional arguments.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;-v&quot;</a-s><a-p>,</a-p> <a-s>&quot;-j&quot;</a-s><a-p>,</a-p> <a-s>&quot;4&quot;</a-s><a-p>,</a-p> <a-s>&quot;input.txt&quot;</a-s><a-p>,</a-p> <a-s>&quot;output.txt&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimpleArgs</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">verbose</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">jobs</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="color:rgb(81,114,224)">4</span><span style="color:inherit"></span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">input</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">input.txt</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">output</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">output.txt</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Attached Short Flag Value

<section class="scenario">
<p class="description">Short flags can have their values attached directly without a space.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;-j4&quot;</a-s><a-p>,</a-p> <a-s>&quot;input.txt&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimpleArgs</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">verbose</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">false</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">jobs</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="color:rgb(81,114,224)">4</span><span style="color:inherit"></span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">input</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">input.txt</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">output</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Boolean Flag with Explicit Value

<section class="scenario">
<p class="description">Boolean flags can be explicitly set to true or false using <code>=</code>.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;--verbose=true&quot;</a-s><a-p>,</a-p> <a-s>&quot;input.txt&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimpleArgs</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">verbose</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">jobs</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">input</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">input.txt</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">output</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Short Flag Chaining

<section class="scenario">
<p class="description">Multiple boolean short flags can be combined: <code>-sb</code> is equivalent to <code>-s -b</code>.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// Git-like CLI with subcommands.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>GitLikeArgs</a-t> <a-p>{</a-p>
    <a-c>/// Show version information
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>version</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Git command to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
    <a-pr>command</a-pr><a-p>:</a-p> <a-t>GitCommand</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-c>/// Available commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>GitCommand</a-t> <a-p>{</a-p>
    <a-c>/// Clone a repository into a new directory
</a-c>    <a-cr>Clone</a-cr> <a-p>{</a-p>
        <a-c>/// The repository URL to clone
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// Directory to clone into
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>directory</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Clone only the specified branch
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Create a shallow clone with limited history
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
        <a-pr>depth</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>
    <a-p>},</a-p>

    <a-c>/// Show the working tree status
</a-c>    <a-cr>Status</a-cr> <a-p>{</a-p>
        <a-c>/// Show short-format output
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>short</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

        <a-c>/// Show the branch even in short-format
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Manage set of tracked repositories
</a-c>    <a-cr>Remote</a-cr> <a-p>{</a-p>
        <a-c>/// Remote action to perform
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
        <a-pr>action</a-pr><a-p>:</a-p> <a-t>RemoteAction</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p>

<a-c>/// Remote management commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>RemoteAction</a-t> <a-p>{</a-p>
    <a-c>/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</a-c>    <a-cr>Add</a-cr> <a-p>{</a-p>
        <a-c>/// Name of the remote
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// URL of the remote repository
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Remove the remote named &lt;name&gt;
</a-c>    rm <a-p>{</a-p>
        <a-c>/// Name of the remote to remove
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// List all remotes
</a-c>    ls <a-p>{</a-p>
        <a-c>/// Show remote URLs after names
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;status&quot;</a-s><a-p>,</a-p> <a-s>&quot;-sb&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">GitLikeArgs</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">version</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">false</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">command</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">GitCommand</span><span style="color:inherit"></span><span style="opacity:0.7">::</span><span style="font-weight:bold">Status</span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">short</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">branch</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">true</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Subcommands

<section class="scenario">
<p class="description">Parse a CLI with subcommands, each with their own arguments.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// Git-like CLI with subcommands.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>GitLikeArgs</a-t> <a-p>{</a-p>
    <a-c>/// Show version information
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>version</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Git command to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
    <a-pr>command</a-pr><a-p>:</a-p> <a-t>GitCommand</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-c>/// Available commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>GitCommand</a-t> <a-p>{</a-p>
    <a-c>/// Clone a repository into a new directory
</a-c>    <a-cr>Clone</a-cr> <a-p>{</a-p>
        <a-c>/// The repository URL to clone
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// Directory to clone into
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>directory</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Clone only the specified branch
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Create a shallow clone with limited history
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
        <a-pr>depth</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>
    <a-p>},</a-p>

    <a-c>/// Show the working tree status
</a-c>    <a-cr>Status</a-cr> <a-p>{</a-p>
        <a-c>/// Show short-format output
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>short</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

        <a-c>/// Show the branch even in short-format
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Manage set of tracked repositories
</a-c>    <a-cr>Remote</a-cr> <a-p>{</a-p>
        <a-c>/// Remote action to perform
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
        <a-pr>action</a-pr><a-p>:</a-p> <a-t>RemoteAction</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p>

<a-c>/// Remote management commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>RemoteAction</a-t> <a-p>{</a-p>
    <a-c>/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</a-c>    <a-cr>Add</a-cr> <a-p>{</a-p>
        <a-c>/// Name of the remote
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// URL of the remote repository
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Remove the remote named &lt;name&gt;
</a-c>    rm <a-p>{</a-p>
        <a-c>/// Name of the remote to remove
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// List all remotes
</a-c>    ls <a-p>{</a-p>
        <a-c>/// Show remote URLs after names
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;clone&quot;</a-s><a-p>,</a-p> <a-s>&quot;--branch&quot;</a-s><a-p>,</a-p> <a-s>&quot;main&quot;</a-s><a-p>,</a-p> <a-s>&quot;https://github.com/user/repo&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">GitLikeArgs</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">version</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">false</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">command</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">GitCommand</span><span style="color:inherit"></span><span style="opacity:0.7">::</span><span style="font-weight:bold">Clone</span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">url</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">https://github.com/user/repo</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">directory</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">branch</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">main</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">depth</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Nested Subcommands

<section class="scenario">
<p class="description">Parse deeply nested subcommands like <code>git remote add</code>.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// Git-like CLI with subcommands.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>GitLikeArgs</a-t> <a-p>{</a-p>
    <a-c>/// Show version information
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>version</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Git command to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
    <a-pr>command</a-pr><a-p>:</a-p> <a-t>GitCommand</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-c>/// Available commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>GitCommand</a-t> <a-p>{</a-p>
    <a-c>/// Clone a repository into a new directory
</a-c>    <a-cr>Clone</a-cr> <a-p>{</a-p>
        <a-c>/// The repository URL to clone
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// Directory to clone into
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>directory</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Clone only the specified branch
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Create a shallow clone with limited history
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
        <a-pr>depth</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>
    <a-p>},</a-p>

    <a-c>/// Show the working tree status
</a-c>    <a-cr>Status</a-cr> <a-p>{</a-p>
        <a-c>/// Show short-format output
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>short</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

        <a-c>/// Show the branch even in short-format
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Manage set of tracked repositories
</a-c>    <a-cr>Remote</a-cr> <a-p>{</a-p>
        <a-c>/// Remote action to perform
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
        <a-pr>action</a-pr><a-p>:</a-p> <a-t>RemoteAction</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p>

<a-c>/// Remote management commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>RemoteAction</a-t> <a-p>{</a-p>
    <a-c>/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</a-c>    <a-cr>Add</a-cr> <a-p>{</a-p>
        <a-c>/// Name of the remote
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// URL of the remote repository
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Remove the remote named &lt;name&gt;
</a-c>    rm <a-p>{</a-p>
        <a-c>/// Name of the remote to remove
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// List all remotes
</a-c>    ls <a-p>{</a-p>
        <a-c>/// Show remote URLs after names
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;remote&quot;</a-s><a-p>,</a-p> <a-s>&quot;add&quot;</a-s><a-p>,</a-p> <a-s>&quot;origin&quot;</a-s><a-p>,</a-p> <a-s>&quot;https://github.com/user/repo&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">GitLikeArgs</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">version</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(81,224,114)">false</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">command</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">GitCommand</span><span style="color:inherit"></span><span style="opacity:0.7">::</span><span style="font-weight:bold">Remote</span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">action</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">RemoteAction</span><span style="color:inherit"></span><span style="opacity:0.7">::</span><span style="font-weight:bold">Add</span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">origin</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">url</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">https://github.com/user/repo</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## Help Generation


### Simple Help

<section class="scenario">
<p class="description">Auto-generated help text from struct definition and doc comments.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code>mytool <a-co>1</a-co><a-p>.</a-p><a-co>0</a-co><a-p>.</a-p><a-co>0</a-co>

<a-cr>A</a-cr> simple <a-cr>CLI</a-cr> tool <a-k>for</a-k> file processing<a-p>.</a-p>

<a-p>[</a-p><a-co>1</a-co>m<a-p>[</a-p><a-co>33</a-co>mUSAGE<a-p>[</a-p><a-co>39</a-co>m<a-p>[</a-p><a-co>0</a-co>m<a-p>:</a-p>
    mytool <a-p>[</a-p><a-cr>OPTIONS</a-cr><a-p>]</a-p> &lt;<a-cr>INPUT</a-cr>&gt; <a-p>[</a-p><a-cr>OUTPUT</a-cr><a-p>]</a-p>

<a-p>[</a-p><a-co>1</a-co>m<a-p>[</a-p><a-co>33</a-co>mARGUMENTS<a-p>[</a-p><a-co>39</a-co>m<a-p>[</a-p><a-co>0</a-co>m<a-p>:</a-p>
        <a-p>[</a-p><a-co>32</a-co><a-t>m</a-t><a-p>&lt;</a-p><a-t>INPUT</a-t><a-p>&gt;</a-p><a-p>[</a-p><a-co>39</a-co>m
            <a-cr>Input</a-cr> file to process
        <a-p>[</a-p><a-co>32</a-co><a-t>m</a-t><a-p>&lt;</a-p><a-t>OUTPUT</a-t><a-p>&gt;</a-p><a-p>[</a-p><a-co>39</a-co>m
            <a-cr>Output</a-cr> file <a-p>(</a-p>defaults to stdout<a-p>)</a-p>

<a-p>[</a-p><a-co>1</a-co>m<a-p>[</a-p><a-co>33</a-co>mOPTIONS<a-p>[</a-p><a-co>39</a-co>m<a-p>[</a-p><a-co>0</a-co>m<a-p>:</a-p>
    <a-p>[</a-p><a-co>32</a-co>m-v<a-p>[</a-p><a-co>39</a-co>m<a-p>,</a-p> <a-p>[</a-p><a-co>32</a-co>m--verbose<a-p>[</a-p><a-co>39</a-co>m
            <a-cr>Enable</a-cr> verbose output
    <a-p>[</a-p><a-co>32</a-co>m-j<a-p>[</a-p><a-co>39</a-co>m<a-p>,</a-p> <a-p>[</a-p><a-co>32</a-co>m--jobs<a-p>[</a-p><a-co>39</a-co><a-t>m</a-t> <a-p>&lt;</a-p><a-t>OPTION</a-t><a-p>&gt;</a-p>
            <a-cr>Number</a-cr> of parallel jobs to run

</code></pre>
</div>
</section>

### Automatic --help Detection

<section class="scenario">
<p class="description">When <code>-h</code>, <code>--help</code>, <code>-help</code>, or <code>/?</code> is the first argument, help is automatically generated and returned.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;--help&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code>target/debug/examples/args_showcase

<a-cr>A</a-cr> simple <a-cr>CLI</a-cr> tool <a-k>for</a-k> file processing<a-p>.</a-p>

<a-p>[</a-p><a-co>1</a-co>m<a-p>[</a-p><a-co>33</a-co>mUSAGE<a-p>[</a-p><a-co>39</a-co>m<a-p>[</a-p><a-co>0</a-co>m<a-p>:</a-p>
    target/debug/examples/args_showcase <a-p>[</a-p><a-cr>OPTIONS</a-cr><a-p>]</a-p> &lt;<a-cr>INPUT</a-cr>&gt; <a-p>[</a-p><a-cr>OUTPUT</a-cr><a-p>]</a-p>

<a-p>[</a-p><a-co>1</a-co>m<a-p>[</a-p><a-co>33</a-co>mARGUMENTS<a-p>[</a-p><a-co>39</a-co>m<a-p>[</a-p><a-co>0</a-co>m<a-p>:</a-p>
        <a-p>[</a-p><a-co>32</a-co><a-t>m</a-t><a-p>&lt;</a-p><a-t>INPUT</a-t><a-p>&gt;</a-p><a-p>[</a-p><a-co>39</a-co>m
            <a-cr>Input</a-cr> file to process
        <a-p>[</a-p><a-co>32</a-co><a-t>m</a-t><a-p>&lt;</a-p><a-t>OUTPUT</a-t><a-p>&gt;</a-p><a-p>[</a-p><a-co>39</a-co>m
            <a-cr>Output</a-cr> file <a-p>(</a-p>defaults to stdout<a-p>)</a-p>

<a-p>[</a-p><a-co>1</a-co>m<a-p>[</a-p><a-co>33</a-co>mOPTIONS<a-p>[</a-p><a-co>39</a-co>m<a-p>[</a-p><a-co>0</a-co>m<a-p>:</a-p>
    <a-p>[</a-p><a-co>32</a-co>m-v<a-p>[</a-p><a-co>39</a-co>m<a-p>,</a-p> <a-p>[</a-p><a-co>32</a-co>m--verbose<a-p>[</a-p><a-co>39</a-co>m
            <a-cr>Enable</a-cr> verbose output
    <a-p>[</a-p><a-co>32</a-co>m-j<a-p>[</a-p><a-co>39</a-co>m<a-p>,</a-p> <a-p>[</a-p><a-co>32</a-co>m--jobs<a-p>[</a-p><a-co>39</a-co><a-t>m</a-t> <a-p>&lt;</a-p><a-t>OPTION</a-t><a-p>&gt;</a-p>
            <a-cr>Number</a-cr> of parallel jobs to run

</code></pre>
</div>
</section>

### Help with Subcommands

<section class="scenario">
<p class="description">Help text automatically lists available subcommands with descriptions.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// Git-like CLI with subcommands.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>GitLikeArgs</a-t> <a-p>{</a-p>
    <a-c>/// Show version information
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>version</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Git command to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
    <a-pr>command</a-pr><a-p>:</a-p> <a-t>GitCommand</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-c>/// Available commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>GitCommand</a-t> <a-p>{</a-p>
    <a-c>/// Clone a repository into a new directory
</a-c>    <a-cr>Clone</a-cr> <a-p>{</a-p>
        <a-c>/// The repository URL to clone
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// Directory to clone into
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>directory</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Clone only the specified branch
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Create a shallow clone with limited history
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
        <a-pr>depth</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>
    <a-p>},</a-p>

    <a-c>/// Show the working tree status
</a-c>    <a-cr>Status</a-cr> <a-p>{</a-p>
        <a-c>/// Show short-format output
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>short</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

        <a-c>/// Show the branch even in short-format
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Manage set of tracked repositories
</a-c>    <a-cr>Remote</a-cr> <a-p>{</a-p>
        <a-c>/// Remote action to perform
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
        <a-pr>action</a-pr><a-p>:</a-p> <a-t>RemoteAction</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p>

<a-c>/// Remote management commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>RemoteAction</a-t> <a-p>{</a-p>
    <a-c>/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</a-c>    <a-cr>Add</a-cr> <a-p>{</a-p>
        <a-c>/// Name of the remote
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// URL of the remote repository
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Remove the remote named &lt;name&gt;
</a-c>    rm <a-p>{</a-p>
        <a-c>/// Name of the remote to remove
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// List all remotes
</a-c>    ls <a-p>{</a-p>
        <a-c>/// Show remote URLs after names
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code>git <a-co>2</a-co><a-p>.</a-p><a-co>40</a-co><a-p>.</a-p><a-co>0</a-co>

<a-pr>Git</a-pr>-like <a-cr>CLI</a-cr> with subcommands<a-p>.</a-p>

<a-p>[</a-p><a-co>1</a-co><a-pr>m</a-pr><a-p>[</a-p><a-co>33</a-co>mUSAGE<a-p>[</a-p><a-co>39</a-co>m<a-p>[</a-p><a-co>0</a-co>m<a-p>:</a-p>
    git <a-p>[</a-p><a-cr>OPTIONS</a-cr><a-p>]</a-p> &lt;<a-cr>COMMAND</a-cr>&gt;

<a-p>[</a-p><a-co>1</a-co>m<a-p>[</a-p><a-co>33</a-co>mOPTIONS<a-p>[</a-p><a-co>39</a-co>m<a-p>[</a-p><a-co>0</a-co>m<a-p>:</a-p>
        <a-p>[</a-p><a-co>32</a-co>m--version<a-p>[</a-p><a-co>39</a-co>m
            <a-cr>Show</a-cr> version information

<a-p>[</a-p><a-co>1</a-co>m<a-p>[</a-p><a-co>33</a-co>mCOMMANDS<a-p>[</a-p><a-co>39</a-co>m<a-p>[</a-p><a-co>0</a-co>m<a-p>:</a-p>
    <a-p>[</a-p><a-co>32</a-co>mclone<a-p>[</a-p><a-co>39</a-co>m
            <a-cr>Clone</a-cr> a repository into a new directory
    <a-p>[</a-p><a-co>32</a-co>mstatus<a-p>[</a-p><a-co>39</a-co>m
            <a-cr>Show</a-cr> the working tree status
    <a-p>[</a-p><a-co>32</a-co>mremote<a-p>[</a-p><a-co>39</a-co>m
            <a-cr>Manage</a-cr> set of tracked repositories

</code></pre>
</div>
</section>

## Shell Completions


### Bash Completions

<section class="scenario">
<p class="description">Generated Bash completion script for tab-completion support.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A build tool configuration
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>BuildArgs</a-t> <a-p>{</a-p>
    <a-c>/// Build in release mode with optimizations
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>release</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Package to build
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>package</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-c>/// Build all packages in the workspace
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>workspace</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Space-separated list of features to enable
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>features</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-c>/// Target triple to build for
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>target</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code>_cargo-<a-f>build</a-f><a-p>()</a-p> <a-p>{</a-p>
    local cur prev words cword
    _init_completion || <a-k>return</a-k>

    local commands=<a-s>&quot;&quot;</a-s>
    local flags=<a-s>&quot;&quot;</a-s>

    flags=<a-s>&quot;--release -r --jobs -j --package -p --workspace --features -F --target&quot;</a-s>

    case &quot;$prev&quot; <a-k>in</a-k>
        # <a-cr>Add</a-cr> cases <a-k>for</a-k> flags that take values
        <a-o>*</a-o><a-p>)</a-p>
            <a-p>;;</a-p>
    esac

    if <a-p>[[</a-p> <a-s>&quot;$cur&quot;</a-s> == -<a-o>*</a-o> <a-p>]];</a-p> then
        <a-cr>COMPREPLY</a-cr>=<a-p>(</a-p>$<a-p>(</a-p>compgen -<a-cr>W</a-cr> <a-s>&quot;$flags&quot;</a-s> -- <a-s>&quot;$cur&quot;</a-s><a-p>))</a-p>
    elif <a-p>[[</a-p> -n <a-s>&quot;$commands&quot;</a-s> <a-p>]];</a-p> then
        <a-cr>COMPREPLY</a-cr>=<a-p>(</a-p>$<a-p>(</a-p>compgen -<a-cr>W</a-cr> <a-s>&quot;$commands&quot;</a-s> -- <a-s>&quot;$cur&quot;</a-s><a-p>))</a-p>
    fi
<a-p>}</a-p>

complete -<a-cr>F</a-cr> _cargo-build cargo-build
</code></pre>
</div>
</section>

### Zsh Completions

<section class="scenario">
<p class="description">Generated Zsh completion script with argument descriptions.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A build tool configuration
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>BuildArgs</a-t> <a-p>{</a-p>
    <a-c>/// Build in release mode with optimizations
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>release</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Package to build
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>package</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-c>/// Build all packages in the workspace
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>workspace</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Space-separated list of features to enable
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>features</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-c>/// Target triple to build for
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>target</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code>#compdef cargo-build

_cargo-<a-f>build</a-f><a-p>()</a-p> <a-p>{</a-p>
    local -a commands
    local -a options

    options=<a-p>(</a-p>
        <a-o>&#39;</a-o>-r<a-p>[</a-p><a-cr>Build</a-cr> in release mode with optimizations<a-p>]</a-p><a-o>&#39;</a-o>
        <a-o>&#39;</a-o>--release<a-p>[</a-p><a-cr>Build</a-cr> in release mode with optimizations<a-p>]</a-p><a-o>&#39;</a-o>
        <a-o>&#39;</a-o>-j<a-p>[</a-p><a-cr>Number</a-cr> of parallel jobs<a-p>]</a-p><a-o>&#39;</a-o>
        <a-o>&#39;</a-o>--jobs<a-p>[</a-p><a-cr>Number</a-cr> of parallel jobs<a-p>]</a-p><a-o>&#39;</a-o>
        <a-o>&#39;</a-o>-p<a-p>[</a-p><a-cr>Package</a-cr> to build<a-p>]</a-p><a-o>&#39;</a-o>
        <a-o>&#39;</a-o>--package<a-p>[</a-p><a-cr>Package</a-cr> to build<a-p>]</a-p><a-o>&#39;</a-o>
        <a-o>&#39;</a-o>--workspace<a-p>[</a-p><a-cr>Build</a-cr> all packages in the workspace<a-p>]</a-p><a-o>&#39;</a-o>
        <a-o>&#39;</a-o>-<a-cr>F</a-cr><a-p>[</a-p><a-cr>Space</a-cr>-separated list of features to enable<a-p>]</a-p><a-o>&#39;</a-o>
        <a-o>&#39;</a-o>--features<a-p>[</a-p><a-cr>Space</a-cr>-separated list of features to enable<a-p>]</a-p><a-o>&#39;</a-o>
        <a-o>&#39;</a-o>--target<a-p>[</a-p><a-cr>Target</a-cr> triple to build <a-k>for</a-k><a-p>]</a-p><a-o>&#39;</a-o>
    <a-p>)</a-p>

    _arguments $options
<a-p>}</a-p>

_cargo-build <a-s>&quot;$@&quot;</a-s>
</code></pre>
</div>
</section>

### Fish Completions

<section class="scenario">
<p class="description">Generated Fish shell completion script.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A build tool configuration
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>BuildArgs</a-t> <a-p>{</a-p>
    <a-c>/// Build in release mode with optimizations
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>release</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Package to build
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>package</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-c>/// Build all packages in the workspace
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>workspace</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Space-separated list of features to enable
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>features</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-c>/// Target triple to build for
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>target</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code># <a-cr>Fish</a-cr> completion for cargo-build

complete -c cargo-build -s r -l release -d <a-o>&#39;</a-o><a-cr>Build</a-cr> in release mode with optimizations<a-o>&#39;</a-o>
complete -c cargo-build -s j -l jobs -d <a-o>&#39;</a-o><a-cr>Number</a-cr> of parallel jobs<a-o>&#39;</a-o>
complete -c cargo-build -s p -l package -d <a-o>&#39;</a-o><a-cr>Package</a-cr> to build<a-o>&#39;</a-o>
complete -c cargo-build -l workspace -d <a-o>&#39;</a-o><a-cr>Build</a-cr> all packages in the workspace<a-o>&#39;</a-o>
complete -c cargo-build -s <a-cr>F</a-cr> -l features -d <a-o>&#39;</a-o><a-cr>Space</a-cr>-separated list of features to enable<a-o>&#39;</a-o>
complete -c cargo-build -l target -d <a-o>&#39;</a-o><a-cr>Target</a-cr> triple to build <a-k>for</a-k><a-o>&#39;</a-o>
</code></pre>
</div>
</section>

## Error Diagnostics


### Unknown Flag

<section class="scenario">
<p class="description">Error when an unrecognized flag is provided.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;--verbos&quot;</a-s><a-p>,</a-p> <a-s>&quot;input.txt&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::unknown_long_flag] Error:</span> unknown flag &#96;--verbos&#96;
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:1 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#e06c75">-</span><span style="color:#e06c75">-</span><span style="color:#e06c75">v</span><span style="color:#e06c75">e</span><span style="color:#e06c75">r</span><span style="color:#e06c75">b</span><span style="color:#e06c75">o</span><span style="color:#e06c75">s</span><span style="color:#888888"> </span><span style="color:#888888">i</span><span style="color:#888888">n</span><span style="color:#888888">p</span><span style="color:#888888">u</span><span style="color:#888888">t</span><span style="color:#888888">.</span><span style="color:#888888">t</span><span style="color:#888888">x</span><span style="color:#888888">t</span>
 <span style="color:#888888">  │</span> <span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">┬</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span>  
 <span style="color:#888888">  │</span>     <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span> unknown flag &#96;--verbos&#96;
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: did you mean &#96;--verbose&#96;?
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Unknown Flag with Suggestion

<section class="scenario">
<p class="description">When the flag name is close to a valid one, a suggestion is offered.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A build tool configuration
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>BuildArgs</a-t> <a-p>{</a-p>
    <a-c>/// Build in release mode with optimizations
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>release</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Package to build
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>package</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-c>/// Build all packages in the workspace
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>workspace</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Space-separated list of features to enable
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>features</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-c>/// Target triple to build for
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>target</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;--releas&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::unknown_long_flag] Error:</span> unknown flag &#96;--releas&#96;
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:1 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#e06c75">-</span><span style="color:#e06c75">-</span><span style="color:#e06c75">r</span><span style="color:#e06c75">e</span><span style="color:#e06c75">l</span><span style="color:#e06c75">e</span><span style="color:#e06c75">a</span><span style="color:#e06c75">s</span>
 <span style="color:#888888">  │</span> <span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">┬</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span>  
 <span style="color:#888888">  │</span>     <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span> unknown flag &#96;--releas&#96;
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: did you mean &#96;--release&#96;?
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Invalid Short Flag in Chain

<section class="scenario">
<p class="description">When chaining short flags, an unknown flag is reported with available options.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;-vxyz&quot;</a-s><a-p>,</a-p> <a-s>&quot;input.txt&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::unknown_short_flag] Error:</span> unknown flag &#96;-x&#96;
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:3 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#888888">-</span><span style="color:#888888">v</span><span style="color:#e06c75">x</span><span style="color:#888888">y</span><span style="color:#888888">z</span><span style="color:#888888"> </span><span style="color:#888888">i</span><span style="color:#888888">n</span><span style="color:#888888">p</span><span style="color:#888888">u</span><span style="color:#888888">t</span><span style="color:#888888">.</span><span style="color:#888888">t</span><span style="color:#888888">x</span><span style="color:#888888">t</span>
 <span style="color:#888888">  │</span>   <span style="color:#e06c75">┬</span>  
 <span style="color:#888888">  │</span>   <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span> unknown flag &#96;-x&#96;
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: available options:
 <span style="color:#888888">  │</span>         -v, --verbose  Enable verbose output
 <span style="color:#888888">  │</span>         -j, --jobs     Number of parallel jobs to run
 <span style="color:#888888">  │</span>             &lt;input&gt;    Input file to process
 <span style="color:#888888">  │</span>             &lt;output&gt;   Output file (defaults to stdout)
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Triple Dash Flag

<section class="scenario">
<p class="description">Flags with too many dashes are rejected.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;---verbose&quot;</a-s><a-p>,</a-p> <a-s>&quot;input.txt&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::unknown_long_flag] Error:</span> unknown flag &#96;---verbose&#96;
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:1 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#e06c75">-</span><span style="color:#e06c75">-</span><span style="color:#e06c75">-</span><span style="color:#e06c75">v</span><span style="color:#e06c75">e</span><span style="color:#e06c75">r</span><span style="color:#e06c75">b</span><span style="color:#e06c75">o</span><span style="color:#e06c75">s</span><span style="color:#e06c75">e</span><span style="color:#888888"> </span><span style="color:#888888">i</span><span style="color:#888888">n</span><span style="color:#888888">p</span><span style="color:#888888">u</span><span style="color:#888888">t</span><span style="color:#888888">.</span><span style="color:#888888">t</span><span style="color:#888888">x</span><span style="color:#888888">t</span>
 <span style="color:#888888">  │</span> <span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">┬</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span>  
 <span style="color:#888888">  │</span>      <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span> unknown flag &#96;---verbose&#96;
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: available options:
 <span style="color:#888888">  │</span>         -v, --verbose  Enable verbose output
 <span style="color:#888888">  │</span>         -j, --jobs     Number of parallel jobs to run
 <span style="color:#888888">  │</span>             &lt;input&gt;    Input file to process
 <span style="color:#888888">  │</span>             &lt;output&gt;   Output file (defaults to stdout)
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Single Dash with Long Name

<section class="scenario">
<p class="description">Long flag names require double dashes.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;-verbose&quot;</a-s><a-p>,</a-p> <a-s>&quot;input.txt&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::unknown_short_flag] Error:</span> unknown flag &#96;-e&#96;
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:3 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#888888">-</span><span style="color:#888888">v</span><span style="color:#e06c75">e</span><span style="color:#888888">r</span><span style="color:#888888">b</span><span style="color:#888888">o</span><span style="color:#888888">s</span><span style="color:#888888">e</span><span style="color:#888888"> </span><span style="color:#888888">i</span><span style="color:#888888">n</span><span style="color:#888888">p</span><span style="color:#888888">u</span><span style="color:#888888">t</span><span style="color:#888888">.</span><span style="color:#888888">t</span><span style="color:#888888">x</span><span style="color:#888888">t</span>
 <span style="color:#888888">  │</span>   <span style="color:#e06c75">┬</span>  
 <span style="color:#888888">  │</span>   <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span> unknown flag &#96;-e&#96;
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: available options:
 <span style="color:#888888">  │</span>         -v, --verbose  Enable verbose output
 <span style="color:#888888">  │</span>         -j, --jobs     Number of parallel jobs to run
 <span style="color:#888888">  │</span>             &lt;input&gt;    Input file to process
 <span style="color:#888888">  │</span>             &lt;output&gt;   Output file (defaults to stdout)
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Missing Value

<section class="scenario">
<p class="description">Error when a flag that requires a value doesn't get one.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;-j&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::expected_value] Error:</span> expected &#96;usize&#96; value
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:1 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#e06c75">-</span><span style="color:#e06c75">j</span>
 <span style="color:#888888">  │</span> <span style="color:#e06c75">─</span><span style="color:#e06c75">┬</span>  
 <span style="color:#888888">  │</span>  <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span> expected &#96;usize&#96; value
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: provide a value after the flag
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Missing Required Argument

<section class="scenario">
<p class="description">Error when a required positional argument is not provided.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;-v&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::missing_argument] Error:</span> missing required argument &#96;&lt;input&gt;&#96; (Input file to process)
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:4 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#888888">-</span><span style="color:#888888">v</span>
 <span style="color:#888888">  │</span>    <span style="color:#e06c75">│</span> 
 <span style="color:#888888">  │</span>    <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span> missing required argument &#96;&lt;input&gt;&#96; (Input file to process)
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: provide a value for &#96;&lt;input&gt;&#96;
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Unexpected Positional Argument

<section class="scenario">
<p class="description">Error when a positional argument is provided but not expected.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A build tool configuration
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>BuildArgs</a-t> <a-p>{</a-p>
    <a-c>/// Build in release mode with optimizations
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>release</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Package to build
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>package</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-c>/// Build all packages in the workspace
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>workspace</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Space-separated list of features to enable
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>features</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-c>/// Target triple to build for
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>target</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;extra&quot;</a-s><a-p>,</a-p> <a-s>&quot;--release&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::unexpected_positional] Error:</span> unexpected positional argument
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:1 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#e06c75">e</span><span style="color:#e06c75">x</span><span style="color:#e06c75">t</span><span style="color:#e06c75">r</span><span style="color:#e06c75">a</span><span style="color:#888888"> </span><span style="color:#888888">-</span><span style="color:#888888">-</span><span style="color:#888888">r</span><span style="color:#888888">e</span><span style="color:#888888">l</span><span style="color:#888888">e</span><span style="color:#888888">a</span><span style="color:#888888">s</span><span style="color:#888888">e</span>
 <span style="color:#888888">  │</span> <span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">┬</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span>  
 <span style="color:#888888">  │</span>   <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span> unexpected positional argument
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: available options:
 <span style="color:#888888">  │</span>         -r, --release    Build in release mode with optimizations
 <span style="color:#888888">  │</span>         -j, --jobs       Number of parallel jobs
 <span style="color:#888888">  │</span>         -p, --package    Package to build
 <span style="color:#888888">  │</span>             --workspace  Build all packages in the workspace
 <span style="color:#888888">  │</span>         -F, --features   Space-separated list of features to enable
 <span style="color:#888888">  │</span>             --target     Target triple to build for
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Unknown Subcommand

<section class="scenario">
<p class="description">Error when an unrecognized subcommand is provided, with available options listed.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// Git-like CLI with subcommands.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>GitLikeArgs</a-t> <a-p>{</a-p>
    <a-c>/// Show version information
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>version</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Git command to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
    <a-pr>command</a-pr><a-p>:</a-p> <a-t>GitCommand</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-c>/// Available commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>GitCommand</a-t> <a-p>{</a-p>
    <a-c>/// Clone a repository into a new directory
</a-c>    <a-cr>Clone</a-cr> <a-p>{</a-p>
        <a-c>/// The repository URL to clone
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// Directory to clone into
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>directory</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Clone only the specified branch
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Create a shallow clone with limited history
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
        <a-pr>depth</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>
    <a-p>},</a-p>

    <a-c>/// Show the working tree status
</a-c>    <a-cr>Status</a-cr> <a-p>{</a-p>
        <a-c>/// Show short-format output
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>short</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

        <a-c>/// Show the branch even in short-format
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Manage set of tracked repositories
</a-c>    <a-cr>Remote</a-cr> <a-p>{</a-p>
        <a-c>/// Remote action to perform
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
        <a-pr>action</a-pr><a-p>:</a-p> <a-t>RemoteAction</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p>

<a-c>/// Remote management commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>RemoteAction</a-t> <a-p>{</a-p>
    <a-c>/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</a-c>    <a-cr>Add</a-cr> <a-p>{</a-p>
        <a-c>/// Name of the remote
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// URL of the remote repository
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Remove the remote named &lt;name&gt;
</a-c>    rm <a-p>{</a-p>
        <a-c>/// Name of the remote to remove
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// List all remotes
</a-c>    ls <a-p>{</a-p>
        <a-c>/// Show remote URLs after names
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;clon&quot;</a-s><a-p>,</a-p> <a-s>&quot;https://example.com&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::unknown_subcommand] Error:</span> unknown subcommand &#96;clon&#96;
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:1 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#e06c75">c</span><span style="color:#e06c75">l</span><span style="color:#e06c75">o</span><span style="color:#e06c75">n</span><span style="color:#888888"> </span><span style="color:#888888">h</span><span style="color:#888888">t</span><span style="color:#888888">t</span><span style="color:#888888">p</span><span style="color:#888888">s</span><span style="color:#888888">:</span><span style="color:#888888">/</span><span style="color:#888888">/</span><span style="color:#888888">e</span><span style="color:#888888">x</span><span style="color:#888888">a</span><span style="color:#888888">m</span><span style="color:#888888">p</span><span style="color:#888888">l</span><span style="color:#888888">e</span><span style="color:#888888">.</span><span style="color:#888888">c</span><span style="color:#888888">o</span><span style="color:#888888">m</span>
 <span style="color:#888888">  │</span> <span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">┬</span><span style="color:#e06c75">─</span>  
 <span style="color:#888888">  │</span>   <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span> unknown subcommand &#96;clon&#96;
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: did you mean &#96;clone&#96;?
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Missing Subcommand

<section class="scenario">
<p class="description">Error when a required subcommand is not provided.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// Git-like CLI with subcommands.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>GitLikeArgs</a-t> <a-p>{</a-p>
    <a-c>/// Show version information
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>version</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Git command to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
    <a-pr>command</a-pr><a-p>:</a-p> <a-t>GitCommand</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-c>/// Available commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>GitCommand</a-t> <a-p>{</a-p>
    <a-c>/// Clone a repository into a new directory
</a-c>    <a-cr>Clone</a-cr> <a-p>{</a-p>
        <a-c>/// The repository URL to clone
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// Directory to clone into
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>directory</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Clone only the specified branch
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Create a shallow clone with limited history
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
        <a-pr>depth</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>
    <a-p>},</a-p>

    <a-c>/// Show the working tree status
</a-c>    <a-cr>Status</a-cr> <a-p>{</a-p>
        <a-c>/// Show short-format output
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>short</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

        <a-c>/// Show the branch even in short-format
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Manage set of tracked repositories
</a-c>    <a-cr>Remote</a-cr> <a-p>{</a-p>
        <a-c>/// Remote action to perform
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
        <a-pr>action</a-pr><a-p>:</a-p> <a-t>RemoteAction</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p>

<a-c>/// Remote management commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>RemoteAction</a-t> <a-p>{</a-p>
    <a-c>/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</a-c>    <a-cr>Add</a-cr> <a-p>{</a-p>
        <a-c>/// Name of the remote
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// URL of the remote repository
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Remove the remote named &lt;name&gt;
</a-c>    rm <a-p>{</a-p>
        <a-c>/// Name of the remote to remove
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// List all remotes
</a-c>    ls <a-p>{</a-p>
        <a-c>/// Show remote URLs after names
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;--version&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::missing_subcommand] Error:</span> expected a subcommand
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:11 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#888888">-</span><span style="color:#888888">-</span><span style="color:#888888">v</span><span style="color:#888888">e</span><span style="color:#888888">r</span><span style="color:#888888">s</span><span style="color:#888888">i</span><span style="color:#888888">o</span><span style="color:#888888">n</span>
 <span style="color:#888888">  │</span>           <span style="color:#e06c75">│</span> 
 <span style="color:#888888">  │</span>           <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span> expected a subcommand
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: available subcommands:
 <span style="color:#888888">  │</span>         clone   Clone a repository into a new directory
 <span style="color:#888888">  │</span>         status  Show the working tree status
 <span style="color:#888888">  │</span>         remote  Manage set of tracked repositories
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Missing Nested Subcommand Argument

<section class="scenario">
<p class="description">Error when a required argument in a nested subcommand is missing.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// Git-like CLI with subcommands.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>GitLikeArgs</a-t> <a-p>{</a-p>
    <a-c>/// Show version information
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
    <a-pr>version</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Git command to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
    <a-pr>command</a-pr><a-p>:</a-p> <a-t>GitCommand</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-c>/// Available commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>GitCommand</a-t> <a-p>{</a-p>
    <a-c>/// Clone a repository into a new directory
</a-c>    <a-cr>Clone</a-cr> <a-p>{</a-p>
        <a-c>/// The repository URL to clone
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// Directory to clone into
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>directory</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Clone only the specified branch
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

        <a-c>/// Create a shallow clone with limited history
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>)]</a-p>
        <a-pr>depth</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>
    <a-p>},</a-p>

    <a-c>/// Show the working tree status
</a-c>    <a-cr>Status</a-cr> <a-p>{</a-p>
        <a-c>/// Show short-format output
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>short</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

        <a-c>/// Show the branch even in short-format
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>branch</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Manage set of tracked repositories
</a-c>    <a-cr>Remote</a-cr> <a-p>{</a-p>
        <a-c>/// Remote action to perform
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>subcommand</a-at><a-p>)]</a-p>
        <a-pr>action</a-pr><a-p>:</a-p> <a-t>RemoteAction</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p>

<a-c>/// Remote management commands
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>RemoteAction</a-t> <a-p>{</a-p>
    <a-c>/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</a-c>    <a-cr>Add</a-cr> <a-p>{</a-p>
        <a-c>/// Name of the remote
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

        <a-c>/// URL of the remote repository
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// Remove the remote named &lt;name&gt;
</a-c>    rm <a-p>{</a-p>
        <a-c>/// Name of the remote to remove
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
        <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
    <a-p>},</a-p>

    <a-c>/// List all remotes
</a-c>    ls <a-p>{</a-p>
        <a-c>/// Show remote URLs after names
</a-c>        <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
        <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>
    <a-p>},</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;remote&quot;</a-s><a-p>,</a-p> <a-s>&quot;add&quot;</a-s><a-p>,</a-p> <a-s>&quot;origin&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::missing_argument] Error:</span> missing required argument &#96;&lt;url&gt;&#96; (URL of the remote repository)
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:19 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#888888">r</span><span style="color:#888888">e</span><span style="color:#888888">m</span><span style="color:#888888">o</span><span style="color:#888888">t</span><span style="color:#888888">e</span><span style="color:#888888"> </span><span style="color:#888888">a</span><span style="color:#888888">d</span><span style="color:#888888">d</span><span style="color:#888888"> </span><span style="color:#888888">o</span><span style="color:#888888">r</span><span style="color:#888888">i</span><span style="color:#888888">g</span><span style="color:#888888">i</span><span style="color:#888888">n</span>
 <span style="color:#888888">  │</span>                   <span style="color:#e06c75">│</span> 
 <span style="color:#888888">  │</span>                   <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span> missing required argument &#96;&lt;url&gt;&#96; (URL of the remote repository)
 <span style="color:#888888">  │</span> 
 <span style="color:#888888">  │</span> <span style="color:#888888">Help</span>: provide a value for &#96;&lt;url&gt;&#96;
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

### Invalid Value Type

<section class="scenario">
<p class="description">Error when a value cannot be parsed as the expected type.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple CLI tool for file processing.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleArgs</a-t> <a-p>{</a-p>
    <a-c>/// Enable verbose output
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>verbose</a-pr><a-p>:</a-p> <a-t>bool</a-t><a-p>,</a-p>

    <a-c>/// Number of parallel jobs to run
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>named</a-at><a-p>,</a-p><a-at> args</a-at><a-p>::</a-p><a-at>short</a-at><a-p>)]</a-p>
    <a-pr>jobs</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>usize</a-t><a-p>&gt;,</a-p>

    <a-c>/// Input file to process
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>input</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-c>/// Output file (defaults to stdout)
</a-c>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>args</a-at><a-p>::</a-p><a-at>positional</a-at><a-p>)]</a-p>
    <a-pr>output</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-f>from_slice</a-f><a-p>(</a-p><a-o>&amp;</a-o><a-p>[</a-p><a-s>&quot;-j&quot;</a-s><a-p>,</a-p> <a-s>&quot;not-a-number&quot;</a-s><a-p>,</a-p> <a-s>&quot;input.txt&quot;</a-s><a-p>])</a-p></code></pre>
</div>
<div class="output">
<pre><code><span style="color:#e06c75">[args::reflect_error] Error:</span> invalid value for &#96;usize&#96;
   <span style="color:#888888">╭</span><span style="color:#888888">─</span><span style="color:#888888">[</span> &lt;unknown&gt;:1:4 <span style="color:#888888">]</span>
   <span style="color:#888888">│</span>
 <span style="color:#888888">1 │</span> <span style="color:#888888">-</span><span style="color:#888888">j</span><span style="color:#888888"> </span><span style="color:#e06c75">n</span><span style="color:#e06c75">o</span><span style="color:#e06c75">t</span><span style="color:#e06c75">-</span><span style="color:#e06c75">a</span><span style="color:#e06c75">-</span><span style="color:#e06c75">n</span><span style="color:#e06c75">u</span><span style="color:#e06c75">m</span><span style="color:#e06c75">b</span><span style="color:#e06c75">e</span><span style="color:#e06c75">r</span><span style="color:#888888"> </span><span style="color:#888888">i</span><span style="color:#888888">n</span><span style="color:#888888">p</span><span style="color:#888888">u</span><span style="color:#888888">t</span><span style="color:#888888">.</span><span style="color:#888888">t</span><span style="color:#888888">x</span><span style="color:#888888">t</span>
 <span style="color:#888888">  │</span>    <span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">┬</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span>  
 <span style="color:#888888">  │</span>          <span style="color:#e06c75">╰</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span><span style="color:#e06c75">─</span> invalid value for &#96;usize&#96;
<span style="color:#888888">───╯</span>
</code></pre>
</div>
</section>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/9a08052331c141ce69a86655e9bdddd14c7e07e0/facet-args/examples/args_showcase.rs"><code>facet-args/examples/args_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/9a08052331c141ce69a86655e9bdddd14c7e07e0"><code>9a080523</code></a></dd>
<dt>Generated</dt><dd><time datetime="2026-01-11T16:22:42+01:00">2026-01-11T16:22:42+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
