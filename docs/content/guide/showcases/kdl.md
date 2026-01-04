+++
title = "KDL"
+++

<div class="showcase">

[`facet-kdl`](https://docs.rs/facet-kdl) parses KDL documents into Rust types using `Facet` attributes. Map KDL arguments with `kdl::argument`, properties with `kdl::property`, and child nodes with `kdl::child` or `kdl::children`.


## Successful Parsing


### Simple Node with Argument and Property

<section class="scenario">
<p class="description">Parse a node with a positional argument and a property.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple server configuration.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>ServerConfig</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>child</a-at><a-p>)]</a-p>
    <a-pr>server</a-pr><a-p>:</a-p> <a-t>Server</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Server</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>argument</a-at><a-p>)]</a-p>
    <a-pr>host</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>port</a-pr><a-p>:</a-p> <a-t>u16</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-v>server</a-v> <a-s>&quot;localhost&quot;</a-s> <a-v>port</a-v><a-o>=</a-o><a-n>8080</a-n></code></pre>
</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">ServerConfig</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">server</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Server</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">host</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">localhost</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">port</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="color:rgb(224,186,81)">8080</span><span style="color:inherit"></span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Node with Properties

<section class="scenario">
<p class="description">Parse a node with multiple key=value properties.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// Configuration with child nodes.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>DatabaseConfig</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>child</a-at><a-p>)]</a-p>
    <a-pr>database</a-pr><a-p>:</a-p> <a-t>Database</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Database</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>url</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>pool_size</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>u32</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-v>database</a-v> <a-v>url</a-v><a-o>=</a-o><a-s>&quot;postgres://localhost/mydb&quot;</a-s> <a-v>pool_size</a-v><a-o>=</a-o><a-n>10</a-n></code></pre>
</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">DatabaseConfig</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">database</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Database</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">url</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">postgres://localhost/mydb</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">pool_size</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="color:rgb(207,81,224)">10</span><span style="color:inherit"></span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Multiple Child Nodes

<section class="scenario">
<p class="description">Parse multiple nodes of the same type into a Vec.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// Configuration expecting multiple children.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>UsersConfig</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>children</a-at><a-p>)]</a-p>
    <a-pr>users</a-pr><a-p>:</a-p> <a-t>Vec</a-t><a-p>&lt;</a-p><a-t>User</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>User</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>argument</a-at><a-p>)]</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>admin</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>bool</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code>
<a-v>user</a-v> <a-s>&quot;alice&quot;</a-s> <a-v>admin</a-v><a-o>=</a-o><a-co>#true</a-co>
<a-v>user</a-v> <a-s>&quot;bob&quot;</a-s>
<a-v>user</a-v> <a-s>&quot;charlie&quot;</a-s> <a-v>admin</a-v><a-o>=</a-o><a-co>#false</a-co>
</code></pre>
</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">UsersConfig</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">users</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;User&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">User</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">alice</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">admin</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="color:rgb(81,224,114)">true</span><span style="color:inherit"></span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">User</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">bob</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">admin</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">User</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">charlie</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">admin</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="color:rgb(81,224,114)">false</span><span style="color:inherit"></span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

## KDL Syntax Errors


### Unclosed String

<section class="scenario">
<p class="description">KDL syntax error when a string literal is not closed.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple server configuration.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>ServerConfig</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>child</a-at><a-p>)]</a-p>
    <a-pr>server</a-pr><a-p>:</a-p> <a-t>Server</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Server</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>argument</a-at><a-p>)]</a-p>
    <a-pr>host</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>port</a-pr><a-p>:</a-p> <a-t>u16</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-v>server</a-v> &quot;localhost port=8080</code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> Failed to parse KDL document
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.kdl:1:8</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(205,214,244)">server</span> "localhost port=8080
   · <span style="color:#c678dd;font-weight:bold">       ──────────┬─────────</span>
   ·                  <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">not quoted string</span>
   ╰────
</code></pre>
</div>
</section>

### Unclosed Brace

<section class="scenario">
<p class="description">KDL syntax error when a children block is not closed.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple server configuration.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>ServerConfig</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>child</a-at><a-p>)]</a-p>
    <a-pr>server</a-pr><a-p>:</a-p> <a-t>Server</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Server</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>argument</a-at><a-p>)]</a-p>
    <a-pr>host</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>port</a-pr><a-p>:</a-p> <a-t>u16</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code>
<a-v>parent</a-v> <a-p>{</a-p>
    <a-v>child</a-v> <a-s>&quot;value&quot;</a-s>
</code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> Failed to parse KDL document
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.kdl:2:8</span>]
 <span style="opacity:0.7">1</span> │     
 <span style="opacity:0.7">2</span> │ <span style="color:#e5c07b;font-weight:bold">╭</span><span style="color:#e5c07b;font-weight:bold">─</span><span style="color:#e5c07b;font-weight:bold">▶</span> <span style="color:rgb(205,214,244)">parent</span> <span style="color:rgb(147,153,178)">{</span>
   · <span style="color:#e5c07b;font-weight:bold">│</span>   <span style="color:#c678dd;font-weight:bold">       ┬</span>
   · <span style="color:#e5c07b;font-weight:bold">│</span>          <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">not closed</span>
 <span style="opacity:0.7">3</span> │ <span style="color:#e5c07b;font-weight:bold">├</span><span style="color:#e5c07b;font-weight:bold">─</span><span style="color:#e5c07b;font-weight:bold">▶</span>     <span style="color:rgb(205,214,244)">child</span> <span style="color:rgb(166,227,161)">"value"</span>
   · <span style="color:#e5c07b;font-weight:bold">╰</span><span style="color:#e5c07b;font-weight:bold">───</span><span style="color:#e5c07b;font-weight:bold">─</span> <span style="color:#e5c07b;font-weight:bold">not closed</span>
   ╰────
</code></pre>
</div>
</section>

### Invalid Number

<section class="scenario">
<p class="description">Error when a property value looks like a number but isn't valid.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple server configuration.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>ServerConfig</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>child</a-at><a-p>)]</a-p>
    <a-pr>server</a-pr><a-p>:</a-p> <a-t>Server</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Server</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>argument</a-at><a-p>)]</a-p>
    <a-pr>host</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>port</a-pr><a-p>:</a-p> <a-t>u16</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-v>server</a-v> <a-s>&quot;localhost&quot;</a-s> <a-v>port</a-v><a-o>=</a-o><a-n>808</a-n>O</code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> Failed to parse KDL document
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.kdl:1:1</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(205,214,244)">server</span> <span style="color:rgb(166,227,161)">"localhost"</span> <span style="color:rgb(205,214,244)">port</span><span style="color:rgb(148,226,213)">=</span><span style="color:rgb(250,179,135)">808</span>O
   · <span style="color:#c678dd;font-weight:bold">─────────────┬─────────────</span>
   ·              <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">not property value</span>
   ╰────
</code></pre>
</div>
</section>

## Schema Mismatch Errors


### Expected Scalar, Got Struct

<section class="scenario">
<p class="description">Error when a field expects a scalar value but receives a child node. This happens when using <code>kdl::property</code> for what should be <code>kdl::child</code>.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// Rust config similar to dodeca&#39;s RustConfig.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>RustConfigWrapper</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>child</a-at><a-p>)]</a-p>
    <a-pr>rust</a-pr><a-p>:</a-p> <a-t>RustConfig</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>RustConfig</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>command</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>

    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>args</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>Vec</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code>
<a-v>rust</a-v> <a-p>{</a-p>
    <a-v>command</a-v> <a-s>&quot;cargo&quot;</a-s>
    <a-v>args</a-v> <a-s>&quot;run&quot;</a-s> <a-s>&quot;--quiet&quot;</a-s> <a-s>&quot;--release&quot;</a-s>
<a-p>}</a-p>
</code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> expected &#96;String&#96; value, got element
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.kdl:3:5</span>]
 <span style="opacity:0.7">2</span> │ <span style="color:rgb(205,214,244)">rust</span> <span style="color:rgb(147,153,178)">{</span>
 <span style="opacity:0.7">3</span> │     <span style="color:rgb(205,214,244)">command</span> <span style="color:rgb(166,227,161)">"cargo"</span>
   · <span style="color:#c678dd;font-weight:bold">    ───────┬───────</span>
   ·            <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">got element here</span>
 <span style="opacity:0.7">4</span> │     <span style="color:rgb(205,214,244)">args</span> <span style="color:rgb(166,227,161)">"run"</span> <span style="color:rgb(166,227,161)">"--quiet"</span> <span style="color:rgb(166,227,161)">"--release"</span>
   ╰────
<span style="color:#56b6c2">  help: </span>field &#96;command&#96; is marked with &#96;#[facet(kdl::property)]&#96;, so use &#96;command="value"&#96; syntax instead of &#96;command "value"&#96;

Error: 
  <span style="color:#e06c75">×</span> expected type &#96;RustConfigWrapper&#96;
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">RustConfig.rs:2:8</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(249,226,175)">#[derive(Facet)]</span>
 <span style="opacity:0.7">2</span> │ <span style="color:rgb(203,166,247)">struct</span> RustConfig <span style="color:rgb(147,153,178)">{</span>
   · <span style="color:#c678dd;font-weight:bold">       ──────────</span>
 <span style="opacity:0.7">3</span> │     <span style="color:rgb(249,226,175)">#[facet(kdl::property)]</span>
 <span style="opacity:0.7">4</span> │     <span style="color:rgb(137,180,250)">command</span><span style="color:rgb(147,153,178)">:</span> <span style="color:rgb(249,226,175)">Option</span><span style="color:rgb(147,153,178)">&lt;</span><span style="color:rgb(249,226,175)">String</span><span style="color:rgb(147,153,178)">&gt;,</span>
   · <span style="color:#e5c07b;font-weight:bold">    ───┬───</span>
   ·        <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">as requested here</span>
 <span style="opacity:0.7">5</span> │     <span style="color:rgb(249,226,175)">#[facet(kdl::property)]</span>
   ╰────

Error: 
  <span style="color:#e06c75">×</span> in type &#96;RustConfigWrapper&#96;
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">RustConfigWrapper.rs:2:8</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(249,226,175)">#[derive(Facet)]</span>
 <span style="opacity:0.7">2</span> │ <span style="color:rgb(203,166,247)">struct</span> <span style="color:rgb(249,226,175)">RustConfigWrapper</span> <span style="color:rgb(147,153,178)">{</span>
   · <span style="color:#c678dd;font-weight:bold">       ─────────────────</span>
 <span style="opacity:0.7">3</span> │     <span style="color:rgb(249,226,175)">#[facet(kdl::child)]</span>
 <span style="opacity:0.7">4</span> │     <span style="color:rgb(137,180,250)">rust</span><span style="color:rgb(147,153,178)">:</span> <span style="color:rgb(249,226,175)">RustConfig</span><span style="color:rgb(147,153,178)">,</span>
   · <span style="color:#e5c07b;font-weight:bold">    ──┬─</span>
   ·       <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">via this field</span>
 <span style="opacity:0.7">5</span> │ <span style="color:rgb(147,153,178)">}</span>
   ╰────
</code></pre>
</div>
</section>

### Missing Required Field

<section class="scenario">
<p class="description">Error when a required field (without <code>default</code>) is not provided.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple server configuration.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>ServerConfig</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>child</a-at><a-p>)]</a-p>
    <a-pr>server</a-pr><a-p>:</a-p> <a-t>Server</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Server</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>argument</a-at><a-p>)]</a-p>
    <a-pr>host</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>port</a-pr><a-p>:</a-p> <a-t>u16</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-v>server</a-v> <a-v>port</a-v><a-o>=</a-o><a-n>8080</a-n></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">facet::missing_field</span>

  <span style="color:#e06c75">×</span> missing field &#96;host&#96; in type &#96;Server&#96;
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.kdl:1:1</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(205,214,244)">server</span> <span style="color:rgb(205,214,244)">port</span><span style="color:rgb(148,226,213)">=</span><span style="color:rgb(250,179,135)">8080</span>
   · <span style="color:#c678dd;font-weight:bold">────────┬───────</span>
   ·         <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">missing field 'host'</span>
   ╰────
<span style="color:#56b6c2">  help: </span>add &#96;host&#96; to your input, or mark the field as optional with #[facet(default)]

Error: 
  <span style="color:#e06c75">×</span> expected type &#96;ServerConfig&#96;
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">Server.rs:2:8</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(249,226,175)">#[derive(Facet)]</span>
 <span style="opacity:0.7">2</span> │ <span style="color:rgb(203,166,247)">struct</span> Server <span style="color:rgb(147,153,178)">{</span>
   · <span style="color:#c678dd;font-weight:bold">       ──────</span>
 <span style="opacity:0.7">3</span> │     <span style="color:rgb(249,226,175)">#[facet(kdl::argument)]</span>
 <span style="opacity:0.7">4</span> │     <span style="color:rgb(137,180,250)">host</span><span style="color:rgb(147,153,178)">:</span> <span style="color:rgb(249,226,175)">String</span><span style="color:rgb(147,153,178)">,</span>
   · <span style="color:#e5c07b;font-weight:bold">    ──┬─</span>
   ·       <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">as requested here</span>
 <span style="opacity:0.7">5</span> │     <span style="color:rgb(249,226,175)">#[facet(kdl::property)]</span>
   ╰────

Error: 
  <span style="color:#e06c75">×</span> in type &#96;ServerConfig&#96;
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">ServerConfig.rs:2:8</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(249,226,175)">#[derive(Facet)]</span>
 <span style="opacity:0.7">2</span> │ <span style="color:rgb(203,166,247)">struct</span> <span style="color:rgb(249,226,175)">ServerConfig</span> <span style="color:rgb(147,153,178)">{</span>
   · <span style="color:#c678dd;font-weight:bold">       ────────────</span>
 <span style="opacity:0.7">3</span> │     <span style="color:rgb(249,226,175)">#[facet(kdl::child)]</span>
 <span style="opacity:0.7">4</span> │     <span style="color:rgb(137,180,250)">server</span><span style="color:rgb(147,153,178)">:</span> <span style="color:rgb(249,226,175)">Server</span><span style="color:rgb(147,153,178)">,</span>
   · <span style="color:#e5c07b;font-weight:bold">    ───┬──</span>
   ·        <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">via this field</span>
 <span style="opacity:0.7">5</span> │ <span style="color:rgb(147,153,178)">}</span>
   ╰────
</code></pre>
</div>
</section>

### Wrong Value Type

<section class="scenario">
<p class="description">Error when a property value cannot be parsed as the expected type.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple server configuration.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>ServerConfig</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>child</a-at><a-p>)]</a-p>
    <a-pr>server</a-pr><a-p>:</a-p> <a-t>Server</a-t><a-p>,</a-p>
<a-p>}</a-p>

<a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Server</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>argument</a-at><a-p>)]</a-p>
    <a-pr>host</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>

    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>kdl</a-at><a-p>::</a-p><a-at>property</a-at><a-p>)]</a-p>
    <a-pr>port</a-pr><a-p>:</a-p> <a-t>u16</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-v>server</a-v> <a-s>&quot;localhost&quot;</a-s> <a-v>port</a-v><a-o>=</a-o><a-s>&quot;not-a-number&quot;</a-s></code></pre>
</div>
<div class="error">
<h4>Error</h4>
<pre><code>  <span style="color:#e06c75">×</span> failed to parse "not-a-number" as u16
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">input.kdl:1:20</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(205,214,244)">server</span> <span style="color:rgb(166,227,161)">"localhost"</span> <span style="color:rgb(205,214,244)">port</span><span style="color:rgb(148,226,213)">=</span><span style="color:rgb(166,227,161)">"not-a-number"</span>
   · <span style="color:#c678dd;font-weight:bold">                   ─────────┬─────────</span>
   ·                             <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">invalid value for &#96;u16&#96;</span>
   ╰────

Error: 
  <span style="color:#e06c75">×</span> expected type &#96;ServerConfig&#96;
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">Server.rs:2:8</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(249,226,175)">#[derive(Facet)]</span>
 <span style="opacity:0.7">2</span> │ <span style="color:rgb(203,166,247)">struct</span> Server <span style="color:rgb(147,153,178)">{</span>
   · <span style="color:#c678dd;font-weight:bold">       ──────</span>
 <span style="opacity:0.7">3</span> │     <span style="color:rgb(249,226,175)">#[facet(kdl::argument)]</span>
   ╰────
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">Server.rs:6:5</span>]
 <span style="opacity:0.7">5</span> │     <span style="color:rgb(249,226,175)">#[facet(kdl::property)]</span>
 <span style="opacity:0.7">6</span> │     port<span style="color:rgb(147,153,178)">:</span> u16<span style="color:rgb(147,153,178)">,</span>
   · <span style="color:#e5c07b;font-weight:bold">    ──┬─</span>
   ·       <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">as requested here</span>
 <span style="opacity:0.7">7</span> │ <span style="color:rgb(147,153,178)">}</span>
   ╰────

Error: 
  <span style="color:#e06c75">×</span> in type &#96;ServerConfig&#96;
   ╭─[<span style="color:#56b6c2;font-weight:bold;text-decoration:underline">ServerConfig.rs:2:8</span>]
 <span style="opacity:0.7">1</span> │ <span style="color:rgb(249,226,175)">#[derive(Facet)]</span>
 <span style="opacity:0.7">2</span> │ <span style="color:rgb(203,166,247)">struct</span> <span style="color:rgb(249,226,175)">ServerConfig</span> <span style="color:rgb(147,153,178)">{</span>
   · <span style="color:#c678dd;font-weight:bold">       ────────────</span>
 <span style="opacity:0.7">3</span> │     <span style="color:rgb(249,226,175)">#[facet(kdl::child)]</span>
 <span style="opacity:0.7">4</span> │     <span style="color:rgb(137,180,250)">server</span><span style="color:rgb(147,153,178)">:</span> <span style="color:rgb(249,226,175)">Server</span><span style="color:rgb(147,153,178)">,</span>
   · <span style="color:#e5c07b;font-weight:bold">    ───┬──</span>
   ·        <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">via this field</span>
 <span style="opacity:0.7">5</span> │ <span style="color:rgb(147,153,178)">}</span>
   ╰────
</code></pre>
</div>
</section>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/c5842bc4cd833fedc52522b20f09daedff260a0e/facet-kdl/examples/kdl_showcase.rs"><code>facet-kdl/examples/kdl_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/c5842bc4cd833fedc52522b20f09daedff260a0e"><code>c5842bc4c</code></a></dd>
<dt>Generated</dt><dd><time datetime="2026-01-04T12:56:12+01:00">2026-01-04T12:56:12+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
