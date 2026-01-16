+++
title = "HTML"
+++

<div class="showcase">

[`facet-html`](https://docs.rs/facet-html) parses and serializes HTML documents using Facet. Define your document structure with `#[facet(html::element)]` for child elements, `#[facet(html::attribute)]` for tag attributes, and `#[facet(html::text)]` for text content.


## Parsing HTML


### Simple Document

<section class="scenario">
<p class="description">Parse a basic HTML document with head, body, and nested elements.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple page structure with head and body.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimplePage</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>element</a-at><a-p>)]</a-p>
    <a-pr>head</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>SimpleHead</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>element</a-at><a-p>)]</a-p>
    <a-pr>body</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>SimpleBody</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleBody</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>class</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>elements</a-at><a-p>)]</a-p>
    <a-pr>children</a-pr><a-p>:</a-p> <a-t>Vec</a-t><a-p>&lt;</a-p><a-t>BodyElement</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>
<br><a-c>/// Elements that can appear in the body.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>BodyElement</a-t> <a-p>{</a-p>
    <a-cr>H1</a-cr><a-p>(</a-p><a-t>Heading</a-t><a-p>),</a-p>
<br>    <a-cr>P</a-cr><a-p>(</a-p><a-t>Paragraph</a-t><a-p>),</a-p>
<br>    <a-cr>Div</a-cr><a-p>(</a-p><a-t>DivElement</a-t><a-p>),</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>DivElement</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>id</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>class</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>text</a-at><a-p>)]</a-p>
    <a-pr>content</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Paragraph</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>class</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>text</a-at><a-p>)]</a-p>
    <a-pr>text</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Heading</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>id</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>text</a-at><a-p>)]</a-p>
    <a-pr>text</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleHead</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>element</a-at><a-p>)]</a-p>
    <a-pr>title</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>SimpleTitle</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleTitle</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>text</a-at><a-p>)]</a-p>
    <a-pr>text</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>HTML Input</h4>

```html
<html>
    <head><title>My Page</title></head>
    <body class="main">
        <h1 id="header">Welcome</h1>
        <p>Hello, world!</p>
    </body>
</html>
```

</div>
<div class="success">
<h4>Success</h4>
<div class="code-block"><pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimplePage</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">head</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimpleHead</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">title</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimpleTitle</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">text</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">My Page</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">body</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimpleBody</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">class</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">main</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">children</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;BodyElement&gt;</span><span style="color:inherit"></span><span style="opacity:0.7">[]</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre></div>
</div>
</section>

### Nested Elements

<section class="scenario">
<p class="description">Parse nested HTML elements into an enum-based content model.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A simple page structure with head and body.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimplePage</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>element</a-at><a-p>)]</a-p>
    <a-pr>head</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>SimpleHead</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>element</a-at><a-p>)]</a-p>
    <a-pr>body</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>SimpleBody</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleBody</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>class</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>elements</a-at><a-p>)]</a-p>
    <a-pr>children</a-pr><a-p>:</a-p> <a-t>Vec</a-t><a-p>&lt;</a-p><a-t>BodyElement</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>
<br><a-c>/// Elements that can appear in the body.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-at>#</a-at><a-p>[</a-p><a-at>repr</a-at><a-p>(</a-p><a-t>u8</a-t><a-p>)]</a-p>
<a-k>enum</a-k> <a-t>BodyElement</a-t> <a-p>{</a-p>
    <a-cr>H1</a-cr><a-p>(</a-p><a-t>Heading</a-t><a-p>),</a-p>
<br>    <a-cr>P</a-cr><a-p>(</a-p><a-t>Paragraph</a-t><a-p>),</a-p>
<br>    <a-cr>Div</a-cr><a-p>(</a-p><a-t>DivElement</a-t><a-p>),</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>DivElement</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>id</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>class</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>text</a-at><a-p>)]</a-p>
    <a-pr>content</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Paragraph</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>class</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>text</a-at><a-p>)]</a-p>
    <a-pr>text</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>Heading</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>id</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>text</a-at><a-p>)]</a-p>
    <a-pr>text</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleHead</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>element</a-at><a-p>)]</a-p>
    <a-pr>title</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>SimpleTitle</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>SimpleTitle</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>text</a-at><a-p>)]</a-p>
    <a-pr>text</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>HTML Input</h4>

```html
<html>
    <body>
        <div id="container" class="wrapper">
            <h1>Title</h1>
            <p class="intro">Introduction paragraph.</p>
            <div class="content">Main content here.</div>
        </div>
    </body>
</html>
```

</div>
<div class="success">
<h4>Success</h4>
<div class="code-block"><pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimplePage</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">head</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">body</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimpleBody</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">class</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">children</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;BodyElement&gt;</span><span style="color:inherit"></span><span style="opacity:0.7">[]</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre></div>
</div>
</section>

### Form Elements

<section class="scenario">
<p class="description">Parse HTML form elements with their attributes.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A form element with various input types.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>ContactForm</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>action</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>method</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>elements</a-at><a-p>)]</a-p>
    <a-pr>inputs</a-pr><a-p>:</a-p> <a-t>Vec</a-t><a-p>&lt;</a-p><a-t>FormInput</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>FormInput</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>input_type</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>placeholder</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>required</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>HTML Input</h4>

```html
<form action="/submit" method="post">
    <input type="text" name="username" placeholder="Username" required />
    <input type="email" name="email" placeholder="Email" />
    <input type="submit" name="submit" />
</form>
```

</div>
<div class="success">
<h4>Success</h4>
<div class="code-block"><pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">ContactForm</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">action</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">/submit</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">method</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">post</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">inputs</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Vec&lt;FormInput&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">FormInput</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">input_type</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">text</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">username</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">placeholder</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">Username</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">required</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)"></span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">FormInput</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">input_type</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">email</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">email</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">placeholder</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">Email</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">required</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
    <span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">FormInput</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">input_type</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">submit</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">name</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">submit</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">placeholder</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">required</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
    <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre></div>
</div>
</section>

## Serialization


### Minified Output

<section class="scenario">
<p class="description">Serialize to compact HTML without extra whitespace.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>DivElement</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>id</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>class</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>text</a-at><a-p>)]</a-p>
    <a-pr>content</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="serialized-output">
<h4>HTML Output</h4>

```html
<divElement id="main" class="container">Hello!</divElement>
```

</div>
</section>

### Pretty-Printed Output

<section class="scenario">
<p class="description">Serialize with indentation for readability.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A form element with various input types.
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>ContactForm</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>action</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>method</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>elements</a-at><a-p>)]</a-p>
    <a-pr>inputs</a-pr><a-p>:</a-p> <a-t>Vec</a-t><a-p>&lt;</a-p><a-t>FormInput</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p>
<br><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>FormInput</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>input_type</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>name</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>placeholder</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>required</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="serialized-output">
<h4>HTML Output</h4>

```html
<form action="/api/contact" method="post"><input type="text" name="name" placeholder="Your name" required><input type="email" name="email" placeholder="your@email.com"></form>
```

</div>
</section>

## Advanced Features


### Extra Attributes (data-*, aria-*)

<section class="scenario">
<p class="description">Unknown attributes like <code>data-*</code> and <code>aria-*</code> are captured in the <code>extra</code> field via <code>#[facet(flatten)]</code>.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26; color:#c0caf5; padding:12px; border-radius:8px; font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace); font-size:0.9rem; overflow:auto;"><code><a-c>/// A div that captures extra attributes (data-*, aria-*, etc.)
</a-c><a-at>#</a-at><a-p>[</a-p><a-at>derive</a-at><a-p>(</a-p><a-cr>Facet</a-cr><a-p>)]</a-p>
<a-k>struct</a-k> <a-t>DivWithExtras</a-t> <a-p>{</a-p>
    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>id</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>attribute</a-at><a-p>)]</a-p>
    <a-pr>class</a-pr><a-p>:</a-p> <a-t>Option</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-c>/// Captures data-*, aria-*, and other unknown attributes
</a-c>    <a-pr>extra</a-pr><a-p>:</a-p> <a-t>BTreeMap</a-t><a-p>&lt;</a-p><a-t>String</a-t><a-p>,</a-p> <a-t>String</a-t><a-p>&gt;,</a-p>
<br>    <a-at>#</a-at><a-p>[</a-p><a-at>facet</a-at><a-p>(</a-p><a-at>html</a-at><a-p>::</a-p><a-at>text</a-at><a-p>)]</a-p>
    <a-pr>content</a-pr><a-p>:</a-p> <a-t>String</a-t><a-p>,</a-p>
<a-p>}</a-p></code></pre>
</details>
<div class="input">
<h4>HTML Input</h4>

```html
<div id="widget" class="card" data-user-id="123" data-theme="dark" aria-label="User Card">Content</div>
```

</div>
<div class="success">
<h4>Success</h4>
<div class="code-block"><pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">DivWithExtras</span><span style="color:inherit"></span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">id</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">widget</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">class</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option</span><span style="color:inherit"></span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">card</span><span style="color:inherit">"</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">extra</span><span style="color:inherit"></span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">BTreeMap&lt;String, String&gt;</span><span style="color:inherit"></span><span style="opacity:0.7"> [</span>
    "<span style="color:rgb(158,206,106)">aria-label</span><span style="color:inherit">"</span><span style="opacity:0.7"> =&gt; </span>"<span style="color:rgb(158,206,106)">User Card</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    "<span style="color:rgb(158,206,106)">data-theme</span><span style="color:inherit">"</span><span style="opacity:0.7"> =&gt; </span>"<span style="color:rgb(158,206,106)">dark</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
    "<span style="color:rgb(158,206,106)">data-user-id</span><span style="color:inherit">"</span><span style="opacity:0.7"> =&gt; </span>"<span style="color:rgb(158,206,106)">123</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">]</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">content</span><span style="color:inherit"></span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">Content</span><span style="color:inherit">"</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre></div>
</div>
</section>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/5c8df10b37be181e3a88be583c1eee213e28dbd5/facet-html/examples/html_showcase.rs"><code>facet-html/examples/html_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/5c8df10b37be181e3a88be583c1eee213e28dbd5"><code>5c8df10b3</code></a></dd>
<dt>Generated</dt><dd><time datetime="2026-01-16T05:16:07+01:00">2026-01-16T05:16:07+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
