+++
title = "Styx"
insert_anchor_links = "heading"
+++

<div class="hero-intro">
<h1>Styx</h1>
<p class="tagline">A document language for mortals.</p>
</div>

```styx
server {
  host localhost
  port 8080
}

routes (
  @redirect{from /old, to /new}
  @proxy{path /api, upstream localhost:9000}
)
```

<div class="features">

<section class="feature">
<div class="feature-text">
<h2>Bare scalars</h2>
<p>Unquoted values. Quotes required for spaces or <code>=</code>.</p>
</div>
<div class="feature-code">

```styx
host localhost
port 8080
path /etc/nginx/nginx.conf
url "https://example.com/api?q=1"
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Sequences</h2>
<p>Parentheses. Whitespace-separated.</p>
</div>
<div class="feature-code">

```styx
ports (8080 8443 9000)

allowed-hosts (
  localhost
  example.com
  "*.internal.net"
)
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Objects</h2>
<p>Curly braces. Newline-separated or comma-separated.</p>
</div>
<div class="feature-code">

```styx
server {
  host localhost
  port 8080
}

point {x 10, y 20}
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Key paths</h2>
<p>Multiple keys in one entry create nested objects.</p>
</div>
<div class="feature-code">

```styx
selector matchLabels app web

// equivalent to:
selector {
  matchLabels {
    app web
  }
}
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Attributes</h2>
<p><code>key=value</code> syntax creates inline object entries.</p>
</div>
<div class="feature-code">

```styx
server host=localhost port=8080

// equivalent to:
server {host localhost, port 8080}
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Tags</h2>
<p>Labels on values. Can wrap objects, sequences, or scalars.</p>
</div>
<div class="feature-code">

```styx
color @rgb(255 128 0)
result @ok
error @err{code 404, message "Not found"}
path @env"HOME"
```

</div>
</section>

</div>

<div class="hero-links">

[Learn Styx](/learn/primer) — a 5-minute primer

[Reference](/reference) — the spec

</div>
