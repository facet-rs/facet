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
<h2>No quotes required</h2>
<p>Hostnames, paths, numbers — just type them. Quotes when you need spaces or special chars like <code>=</code>.</p>
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
<p>Lists use parentheses. Elements separated by whitespace — no commas, no fuss.</p>
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
<p>Curly braces for structure. Newlines separate entries. Commas for single-line.</p>
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
<p>Chain keys to build nested structure in a single line.</p>
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
<p>Inline <code>key=value</code> pairs for compact configuration.</p>
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
<p>Label values with types. Tags can wrap objects, sequences, or scalars.</p>
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
