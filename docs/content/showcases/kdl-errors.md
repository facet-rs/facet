+++
title = "facet-kdl Error Showcase"
+++

<div class="showcase">

## Ambiguous Flattened Enum

<section class="scenario">
<p class="description">Both TypeA and TypeB variants have identical fields (value, priority).<br>The solver cannot determine which variant to use.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">resource </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">test</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">value</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">hello</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">priority</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">10</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">AmbiguousConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">resource</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> AmbiguousResource,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">AmbiguousResource </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">kind</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> AmbiguousKind,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">AmbiguousKind </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    TypeA(CommonFields)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    TypeB(CommonFields)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">CommonFields </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">value</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">priority</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::solver</span>

  <span style="color:#e06c75">×</span> Ambiguous: multiple resolutions match: ["AmbiguousKind::TypeA", "AmbiguousKind::TypeB"]
<span style="color:#56b6c2">  help: </span>multiple variants match: AmbiguousKind::TypeA, AmbiguousKind::TypeB
        use a KDL type annotation to specify the variant, e.g.: (VariantName)node-name ...
</code></pre>
</div>
</section>

## NoMatch with Per-Candidate Failures

<section class="scenario">
<p class="description">Provide field names that don't exactly match any variant.<br>The solver shows WHY each candidate failed with 'did you mean?' suggestions.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">backend </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">cache</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">hst</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">conn_str</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">pg</span><span style="color:#89ddff;">&quot;</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">NoMatchConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">backend</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> NoMatchBackend,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">NoMatchBackend </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">kind</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> NoMatchKind,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">NoMatchKind </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Sqlite(SqliteBackend)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Postgres(PostgresBackend)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Redis(RedisBackend)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">RedisBackend </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">password</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">PostgresBackend </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">connection_string</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">pool_size</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SqliteBackend </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">database_path</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">journal_mode</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::solver</span>

  <span style="color:#e06c75">×</span> No matching configuration for fields ["conn_str", "hst", "name"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> No variant matched:
  <span style="color:#e06c75">│</span>   - NoMatchKind::Redis: missing fields ["host", "port"], unknown fields ["conn_str", "hst"]
  <span style="color:#e06c75">│</span>   - NoMatchKind::Postgres: missing fields ["connection_string", "pool_size"], unknown fields ["conn_str", "hst"]
  <span style="color:#e06c75">│</span>   - NoMatchKind::Sqlite: missing fields ["database_path", "journal_mode"], unknown fields ["conn_str", "hst"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> Unknown fields: ["conn_str", "hst"]
  <span style="color:#e06c75">│</span>   Did you mean 'connection_string' instead of 'conn_str'?
  <span style="color:#e06c75">│</span>   Did you mean 'host' instead of 'hst'?
   ╭────
 <span style="opacity:0.7">1</span> │ backend "cache" hst="localhost" conn_str="pg"
   · <span style="color:#c678dd;font-weight:bold">                ─┬─</span><span style="color:#e5c07b;font-weight:bold">             ────┬───</span>
   ·                  <span style="color:#c678dd;font-weight:bold">│</span>                  <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">did you mean `connection_string`?</span>
   ·                  <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">did you mean `host`?</span>
   ╰────
<span style="color:#56b6c2">  help: </span>did you mean NoMatchKind::Redis?
        
        all variants checked:
          - NoMatchKind::Redis: missing host, port, unexpected conn_str, hst
          - NoMatchKind::Postgres: missing connection_string, pool_size, unexpected conn_str, hst
          - NoMatchKind::Sqlite: missing database_path, journal_mode, unexpected conn_str, hst
        
          conn_str -&gt; connection_string (did you mean connection_string?)
          hst -&gt; host (did you mean host?)
        
</code></pre>
</div>
</section>

## Unknown Fields with 'Did You Mean?' Suggestions

<section class="scenario">
<p class="description">Misspell field names and see the solver suggest corrections!<br>Uses Jaro-Winkler similarity to find close matches.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">web</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">hostnam</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">prot</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">8080</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">TypoConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> TypoServer,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">TypoServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">kind</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> TypoKind,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">TypoKind </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Web(WebServer)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Api(ApiServer)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ApiServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">endpoint</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">timeout_ms</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">retry_count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u8</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">WebServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">hostname</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">ssl_enabled</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::solver</span>

  <span style="color:#e06c75">×</span> No matching configuration for fields ["hostnam", "name", "prot"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> No variant matched:
  <span style="color:#e06c75">│</span>   - TypoKind::Web: missing fields ["hostname", "port", "ssl_enabled"], unknown fields ["hostnam", "prot"]
  <span style="color:#e06c75">│</span>   - TypoKind::Api: missing fields ["endpoint", "retry_count", "timeout_ms"], unknown fields ["hostnam", "prot"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> Unknown fields: ["hostnam", "prot"]
  <span style="color:#e06c75">│</span>   Did you mean 'hostname' instead of 'hostnam'?
  <span style="color:#e06c75">│</span>   Did you mean 'port' instead of 'prot'?
   ╭────
 <span style="opacity:0.7">1</span> │ server "web" hostnam="localhost" prot=8080
   · <span style="color:#c678dd;font-weight:bold">             ───┬───</span><span style="color:#e5c07b;font-weight:bold">             ──┬─</span>
   ·                 <span style="color:#c678dd;font-weight:bold">│</span>                  <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">did you mean `port`?</span>
   ·                 <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">did you mean `hostname`?</span>
   ╰────
<span style="color:#56b6c2">  help: </span>did you mean TypoKind::Web?
        
        all variants checked:
          - TypoKind::Web: missing hostname, port, ssl_enabled, unexpected hostnam, prot
          - TypoKind::Api: missing endpoint, retry_count, timeout_ms, unexpected hostnam, prot
        
          hostnam -&gt; hostname (did you mean hostname?)
          prot -&gt; port (did you mean port?)
        
</code></pre>
</div>
</section>

## Value Overflow Detection

<section class="scenario">
<p class="description">When a value doesn't fit ANY candidate type, the solver reports it.<br>count=5000000000 exceeds both u8 (max 255) and u32 (max ~4 billion).</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">data </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">5000000000</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ValueConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">data</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> ValueData,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">ValueData </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">payload</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> ValuePayload,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">ValuePayload </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    Small(SmallValue)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Large(LargeValue)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">LargeValue </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u32</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SmallValue </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">count</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u8</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::invalid_value</span>

  <span style="color:#e06c75">×</span> invalid value for shape: value Integer(5000000000) doesn't fit any candidate type for field 'count'
</code></pre>
</div>
</section>

## Multi-Line Config with Typos

<section class="scenario">
<p class="description">A more realistic multi-line configuration file with several typos.<br>Shows how the solver sorts candidates by closeness to the input.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">database </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">production</span><span style="color:#89ddff;">&quot;</span><span style="color:#c0caf5;"> \
</span><span style="color:#c0caf5;">    </span><span style="color:#7dcfff;">hots</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">db.example.com</span><span style="color:#89ddff;">&quot;</span><span style="color:#c0caf5;"> \
</span><span style="color:#c0caf5;">    </span><span style="color:#7dcfff;">prot</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">3306</span><span style="color:#c0caf5;"> \
</span><span style="color:#c0caf5;">    </span><span style="color:#7dcfff;">usernme</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">admin</span><span style="color:#89ddff;">&quot;</span><span style="color:#c0caf5;"> \
</span><span style="color:#c0caf5;">    </span><span style="color:#7dcfff;">pasword</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">secret123</span><span style="color:#89ddff;">&quot;
</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">MultiLineConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">database</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> MultiLineDatabase,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">MultiLineDatabase </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">name</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">kind</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> MultiLineDbKind,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">MultiLineDbKind </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    MySql(MySqlConfig)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Postgres(PgConfig)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    Mongo(MongoConfig)</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">MongoConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">uri</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">replica_set</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">PgConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">database</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">ssl_mode</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">MySqlConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">username</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">password</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::solver</span>

  <span style="color:#e06c75">×</span> No matching configuration for fields ["hots", "name", "pasword", "prot", "usernme"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> No variant matched:
  <span style="color:#e06c75">│</span>   - MultiLineDbKind::MySql: missing fields ["host", "password", "port", "username"], unknown fields ["hots", "pasword", "prot", "usernme"]
  <span style="color:#e06c75">│</span>   - MultiLineDbKind::Postgres: missing fields ["database", "host", "port", "ssl_mode"], unknown fields ["hots", "pasword", "prot", "usernme"]
  <span style="color:#e06c75">│</span>   - MultiLineDbKind::Mongo: missing field 'uri', unknown fields ["hots", "pasword", "prot", "usernme"]
  <span style="color:#e06c75">│</span> 
  <span style="color:#e06c75">│</span> Unknown fields: ["hots", "pasword", "prot", "usernme"]
  <span style="color:#e06c75">│</span>   Did you mean 'host' instead of 'hots'?
  <span style="color:#e06c75">│</span>   Did you mean 'password' instead of 'pasword'?
  <span style="color:#e06c75">│</span>   Did you mean 'port' instead of 'prot'?
  <span style="color:#e06c75">│</span>   Did you mean 'username' instead of 'usernme'?
   ╭─[2:5]
 <span style="opacity:0.7">1</span> │ database "production" \
 <span style="opacity:0.7">2</span> │     hots="db.example.com" \
   · <span style="color:#c678dd;font-weight:bold">    ──┬─</span>
   ·       <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">did you mean `host`?</span>
 <span style="opacity:0.7">3</span> │     prot=3306 \
   · <span style="color:#e5c07b;font-weight:bold">    ──┬─</span>
   ·       <span style="color:#e5c07b;font-weight:bold">╰── </span><span style="color:#e5c07b;font-weight:bold">did you mean `port`?</span>
 <span style="opacity:0.7">4</span> │     usernme="admin" \
   · <span style="color:#98c379;font-weight:bold">    ───┬───</span>
   ·        <span style="color:#98c379;font-weight:bold">╰── </span><span style="color:#98c379;font-weight:bold">did you mean `username`?</span>
 <span style="opacity:0.7">5</span> │     pasword="secret123"
   · <span style="color:#c678dd;font-weight:bold">    ───┬───</span>
   ·        <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">did you mean `password`?</span>
   ╰────
<span style="color:#56b6c2">  help: </span>did you mean MultiLineDbKind::MySql?
        
        all variants checked:
          - MultiLineDbKind::MySql: missing host, password, port, username, unexpected hots, pasword, prot, usernme
          - MultiLineDbKind::Postgres: missing database, host, port, ssl_mode, unexpected hots, pasword, prot, usernme
          - MultiLineDbKind::Mongo: missing uri, unexpected hots, pasword, prot, usernme
        
          hots -&gt; host (did you mean host?)
          pasword -&gt; password (did you mean password?)
          prot -&gt; port (did you mean port?)
          usernme -&gt; username (did you mean username?)
        
</code></pre>
</div>
</section>

## Unknown Field

<section class="scenario">
<p class="description">KDL contains a property that doesn't exist in the target struct.<br>With #[facet(deny_unknown_fields)], this is an error.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot; </span><span style="color:#7dcfff;">prot</span><span style="color:#89ddff;">=</span><span style="color:#ff9e64;">8080</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> SimpleServer,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::unknown_property</span>

  <span style="color:#e06c75">×</span> unknown property 'prot', expected one of: host, port
   ╭────
 <span style="opacity:0.7">1</span> │ server host="localhost" prot=8080
   · <span style="color:#c678dd;font-weight:bold">                        ──┬─</span>
   ·                           <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown property `prot`</span>
   ╰────
<span style="color:#56b6c2">  help: </span>expected one of: host, port
</code></pre>
</div>
</section>

## Missing Required Field

<section class="scenario">
<p class="description">KDL is missing a required field that has no default.</p>
<div class="input">
<h4>KDL Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#f7768e;">server </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">localhost</span><span style="color:#89ddff;">&quot;</span></pre>

</div>
<div class="target-type">
<h4>Target Type</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleConfig </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">server</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> SimpleServer,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">deny_unknown_fields</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleServer </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">host</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">    </span><span style="color:#7dcfff;">port</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">u16</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">kdl::reflect</span>

  <span style="color:#e06c75">×</span> Field 'SimpleServer::port' was not initialized
</code></pre>
</div>
</section>
</div>
