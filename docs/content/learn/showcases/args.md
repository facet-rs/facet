+++
title = "Args"
+++

<div class="showcase">

## Successful Parsing


### Simple Arguments

<section class="scenario">
<p class="description">Parse a struct with flags, options, and positional arguments.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">-v</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">-j</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">4</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">input.txt</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">output.txt</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimpleArgs</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">verbose</span><span style="opacity:0.7">: </span><span style="color:rgb(81,164,224)">true</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">jobs</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option&lt;usize&gt;</span><span style="opacity:0.7">::Some(</span><span style="color:rgb(81,157,224)">4</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">input</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">input.txt</span>"<span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">output</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option&lt;String&gt;</span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">output.txt</span>"<span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Attached Short Flag Value

<section class="scenario">
<p class="description">Short flags can have their values attached directly without a space.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">-j4</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">input.txt</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimpleArgs</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">verbose</span><span style="opacity:0.7">: </span><span style="color:rgb(81,164,224)">false</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">jobs</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option&lt;usize&gt;</span><span style="opacity:0.7">::Some(</span><span style="color:rgb(81,157,224)">4</span><span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">input</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">input.txt</span>"<span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">output</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option&lt;String&gt;</span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Boolean Flag with Explicit Value

<section class="scenario">
<p class="description">Boolean flags can be explicitly set to true or false using <code>=</code>.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">--verbose=true</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">input.txt</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">SimpleArgs</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">verbose</span><span style="opacity:0.7">: </span><span style="color:rgb(81,164,224)">true</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">jobs</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option&lt;usize&gt;</span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">input</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">input.txt</span>"<span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">output</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option&lt;String&gt;</span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Subcommands

<section class="scenario">
<p class="description">Parse a CLI with subcommands, each with their own arguments.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// Git-like CLI with subcommands.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">GitLikeArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show version information
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    version</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Git command to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">subcommand)]
</span><span style="color:#9abdf5;">    command</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> GitCommand,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Available commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">GitCommand </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Clone a repository into a new directory
</span><span style="color:#9abdf5;">    </span><span style="color:#0db9d7;">Clone </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// The repository URL to clone
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Directory to clone into
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        directory</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Clone only the specified branch
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Create a shallow clone with limited history
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named)]
</span><span style="color:#9abdf5;">        depth</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show the working tree status
</span><span style="color:#9abdf5;">    Status {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show short-format output
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        short</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show the branch even in short-format
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Manage set of tracked repositories
</span><span style="color:#9abdf5;">    Remote {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Remote action to perform
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::subcommand)]
</span><span style="color:#9abdf5;">        action</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> RemoteAction</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Remote management commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">RemoteAction </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</span><span style="color:#9abdf5;">    Add {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// URL of the remote repository
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Remove the remote named &lt;name&gt;
</span><span style="color:#9abdf5;">    rm {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote to remove
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// List all remotes
</span><span style="color:#9abdf5;">    ls {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show remote URLs after names
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">clone</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">--branch</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">main</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">https://github.com/user/repo</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">GitLikeArgs</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">version</span><span style="opacity:0.7">: </span><span style="color:rgb(81,164,224)">false</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">command</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">GitCommand</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Clone</span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">url</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">https://github.com/user/repo</span>"<span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">directory</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option&lt;String&gt;</span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">branch</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option&lt;String&gt;</span><span style="opacity:0.7">::Some(</span>"<span style="color:rgb(158,206,106)">main</span>"<span style="opacity:0.7">)</span><span style="opacity:0.7">,</span>
    <span style="color:rgb(115,218,202)">depth</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">Option&lt;usize&gt;</span><span style="opacity:0.7">::None</span><span style="opacity:0.7">,</span>
  <span style="opacity:0.7">}</span><span style="opacity:0.7">,</span>
<span style="opacity:0.7">}</span></code></pre>
</div>
</section>

### Nested Subcommands

<section class="scenario">
<p class="description">Parse deeply nested subcommands like <code>git remote add</code>.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// Git-like CLI with subcommands.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">GitLikeArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show version information
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    version</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Git command to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">subcommand)]
</span><span style="color:#9abdf5;">    command</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> GitCommand,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Available commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">GitCommand </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Clone a repository into a new directory
</span><span style="color:#9abdf5;">    </span><span style="color:#0db9d7;">Clone </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// The repository URL to clone
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Directory to clone into
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        directory</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Clone only the specified branch
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Create a shallow clone with limited history
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named)]
</span><span style="color:#9abdf5;">        depth</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show the working tree status
</span><span style="color:#9abdf5;">    Status {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show short-format output
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        short</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show the branch even in short-format
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Manage set of tracked repositories
</span><span style="color:#9abdf5;">    Remote {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Remote action to perform
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::subcommand)]
</span><span style="color:#9abdf5;">        action</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> RemoteAction</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Remote management commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">RemoteAction </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</span><span style="color:#9abdf5;">    Add {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// URL of the remote repository
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Remove the remote named &lt;name&gt;
</span><span style="color:#9abdf5;">    rm {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote to remove
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// List all remotes
</span><span style="color:#9abdf5;">    ls {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show remote URLs after names
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">remote</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">add</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">origin</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">https://github.com/user/repo</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="success">
<h4>Success</h4>
<pre><code><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">GitLikeArgs</span><span style="opacity:0.7"> {</span>
  <span style="color:rgb(115,218,202)">version</span><span style="opacity:0.7">: </span><span style="color:rgb(81,164,224)">false</span><span style="opacity:0.7">,</span>
  <span style="color:rgb(115,218,202)">command</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">GitCommand</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Remote</span><span style="opacity:0.7"> {</span>
    <span style="color:rgb(115,218,202)">action</span><span style="opacity:0.7">: </span><span style="font-weight:bold"></span><span style="color:rgb(122,162,247)">RemoteAction</span><span style="opacity:0.7">::</span><span style="font-weight:bold">Add</span><span style="opacity:0.7"> {</span>
      <span style="color:rgb(115,218,202)">name</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">origin</span>"<span style="opacity:0.7">,</span>
      <span style="color:rgb(115,218,202)">url</span><span style="opacity:0.7">: </span>"<span style="color:rgb(158,206,106)">https://github.com/user/repo</span>"<span style="opacity:0.7">,</span>
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
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#c0caf5;">mytool </span><span style="color:#ff9e64;">1.0</span><span style="color:#c0caf5;">.</span><span style="color:#ff9e64;">0
</span><span style="color:#c0caf5;">
</span><span style="color:#c0caf5;">A simple </span><span style="color:#e0af68;">CLI</span><span style="color:#c0caf5;"> tool </span><span style="color:#bb9af7;">for</span><span style="color:#c0caf5;"> file processing.
</span><span style="color:#c0caf5;">
</span><span style="color:#e0af68;">USAGE</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">    mytool </span><span style="color:#9abdf5;">[</span><span style="color:#e0af68;">OPTIONS</span><span style="color:#9abdf5;">] </span><span style="color:#89ddff;">&lt;</span><span style="color:#e0af68;">INPUT</span><span style="color:#89ddff;">&gt; </span><span style="color:#9abdf5;">[</span><span style="color:#e0af68;">OUTPUT</span><span style="color:#9abdf5;">]
</span><span style="color:#c0caf5;">
</span><span style="color:#e0af68;">ARGUMENTS</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">        </span><span style="color:#89ddff;">&lt;</span><span style="color:#c0caf5;">INPUT</span><span style="color:#89ddff;">&gt;
</span><span style="color:#c0caf5;">            Input file to process
</span><span style="color:#c0caf5;">        </span><span style="color:#89ddff;">&lt;</span><span style="color:#e0af68;">OUTPUT</span><span style="color:#89ddff;">&gt;
</span><span style="color:#c0caf5;">            Output file </span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">defaults to stdout</span><span style="color:#9abdf5;">)
</span><span style="color:#c0caf5;">
</span><span style="color:#e0af68;">OPTIONS</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">v</span><span style="color:#89ddff;">, --</span><span style="color:#c0caf5;">verbose
</span><span style="color:#c0caf5;">            Enable verbose output
</span><span style="color:#c0caf5;">    </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">j</span><span style="color:#89ddff;">, --</span><span style="color:#c0caf5;">jobs </span><span style="color:#89ddff;">&lt;</span><span style="color:#e0af68;">OPTION</span><span style="color:#89ddff;">&gt;
</span><span style="color:#c0caf5;">            Number of parallel jobs to run
</span><span style="color:#c0caf5;">
</span></pre>

</div>
</section>

### Help with Subcommands

<section class="scenario">
<p class="description">Help text automatically lists available subcommands with descriptions.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// Git-like CLI with subcommands.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">GitLikeArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show version information
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    version</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Git command to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">subcommand)]
</span><span style="color:#9abdf5;">    command</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> GitCommand,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Available commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">GitCommand </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Clone a repository into a new directory
</span><span style="color:#9abdf5;">    </span><span style="color:#0db9d7;">Clone </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// The repository URL to clone
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Directory to clone into
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        directory</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Clone only the specified branch
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Create a shallow clone with limited history
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named)]
</span><span style="color:#9abdf5;">        depth</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show the working tree status
</span><span style="color:#9abdf5;">    Status {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show short-format output
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        short</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show the branch even in short-format
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Manage set of tracked repositories
</span><span style="color:#9abdf5;">    Remote {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Remote action to perform
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::subcommand)]
</span><span style="color:#9abdf5;">        action</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> RemoteAction</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Remote management commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">RemoteAction </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</span><span style="color:#9abdf5;">    Add {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// URL of the remote repository
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Remove the remote named &lt;name&gt;
</span><span style="color:#9abdf5;">    rm {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote to remove
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// List all remotes
</span><span style="color:#9abdf5;">    ls {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show remote URLs after names
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#c0caf5;">git </span><span style="color:#ff9e64;">2.40</span><span style="color:#c0caf5;">.</span><span style="color:#ff9e64;">0
</span><span style="color:#c0caf5;">
</span><span style="color:#c0caf5;">Git</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">like </span><span style="color:#e0af68;">CLI</span><span style="color:#c0caf5;"> with subcommands.
</span><span style="color:#c0caf5;">
</span><span style="color:#e0af68;">USAGE</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">    git </span><span style="color:#9abdf5;">[</span><span style="color:#e0af68;">OPTIONS</span><span style="color:#9abdf5;">] </span><span style="color:#89ddff;">&lt;</span><span style="color:#e0af68;">COMMAND</span><span style="color:#89ddff;">&gt;
</span><span style="color:#c0caf5;">
</span><span style="color:#e0af68;">OPTIONS</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">        </span><span style="color:#89ddff;">--</span><span style="color:#c0caf5;">version
</span><span style="color:#c0caf5;">            Show version information
</span><span style="color:#c0caf5;">
</span><span style="color:#e0af68;">COMMANDS</span><span style="color:#89ddff;">:
</span><span style="color:#c0caf5;">    clone
</span><span style="color:#c0caf5;">            </span><span style="color:#0db9d7;">Clone</span><span style="color:#c0caf5;"> a repository into a new directory
</span><span style="color:#c0caf5;">    status
</span><span style="color:#c0caf5;">            Show the working tree status
</span><span style="color:#c0caf5;">    remote
</span><span style="color:#c0caf5;">            Manage set of tracked repositories
</span><span style="color:#c0caf5;">
</span></pre>

</div>
</section>

## Shell Completions


### Bash Completions

<section class="scenario">
<p class="description">Generated Bash completion script for tab-completion support.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A build tool configuration
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">BuildArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Build in release mode with optimizations
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    release</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Package to build
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    package</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Build all packages in the workspace
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    workspace</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Space-separated list of features to enable
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    features</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Target triple to build for
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    target</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#c0caf5;">_cargo</span><span style="color:#89ddff;">-</span><span style="color:#2ac3de;">build</span><span style="color:#9abdf5;">() {
</span><span style="color:#9abdf5;">    local cur prev words cword
</span><span style="color:#9abdf5;">    _init_completion </span><span style="color:#89ddff;">|| </span><span style="color:#bb9af7;">return
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    local commands</span><span style="color:#89ddff;">=&quot;&quot;
</span><span style="color:#9abdf5;">    local flags</span><span style="color:#89ddff;">=&quot;&quot;
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    flags</span><span style="color:#89ddff;">=&quot;</span><span style="color:#9ece6a;">--release -r --jobs -j --package -p --workspace --features -F --target</span><span style="color:#89ddff;">&quot;
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    case </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">$prev</span><span style="color:#89ddff;">&quot; in
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;"> Add cases </span><span style="color:#bb9af7;">for</span><span style="color:#9abdf5;"> flags that take values
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">*</span><span style="color:#9abdf5;">)
</span><span style="color:#9abdf5;">            </span><span style="color:#89ddff;">;;
</span><span style="color:#9abdf5;">    esac
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="color:#bb9af7;">if </span><span style="color:#9abdf5;">[[ </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">$cur</span><span style="color:#89ddff;">&quot; == -* </span><span style="color:#9abdf5;">]]</span><span style="color:#89ddff;">;</span><span style="color:#9abdf5;"> then
</span><span style="color:#9abdf5;">        </span><span style="color:#e0af68;">COMPREPLY</span><span style="color:#89ddff;">=</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">$</span><span style="color:#9abdf5;">(compgen </span><span style="color:#89ddff;">-</span><span style="color:#9abdf5;">W </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">$flags</span><span style="color:#89ddff;">&quot; -- &quot;</span><span style="color:#9ece6a;">$cur</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">))
</span><span style="color:#9abdf5;">    elif [[ </span><span style="color:#89ddff;">-</span><span style="color:#9abdf5;">n </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">$commands</span><span style="color:#89ddff;">&quot; </span><span style="color:#9abdf5;">]]</span><span style="color:#89ddff;">;</span><span style="color:#9abdf5;"> then
</span><span style="color:#9abdf5;">        </span><span style="color:#e0af68;">COMPREPLY</span><span style="color:#89ddff;">=</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">$</span><span style="color:#9abdf5;">(compgen </span><span style="color:#89ddff;">-</span><span style="color:#9abdf5;">W </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">$commands</span><span style="color:#89ddff;">&quot; -- &quot;</span><span style="color:#9ece6a;">$cur</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">))
</span><span style="color:#9abdf5;">    fi
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#c0caf5;">complete </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">F _cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build
</span></pre>

</div>
</section>

### Zsh Completions

<section class="scenario">
<p class="description">Generated Zsh completion script with argument descriptions.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A build tool configuration
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">BuildArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Build in release mode with optimizations
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    release</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Package to build
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    package</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Build all packages in the workspace
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    workspace</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Space-separated list of features to enable
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    features</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Target triple to build for
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    target</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#c0caf5;">compdef cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build
</span><span style="color:#c0caf5;">
</span><span style="color:#c0caf5;">_cargo</span><span style="color:#89ddff;">-</span><span style="color:#2ac3de;">build</span><span style="color:#9abdf5;">() {
</span><span style="color:#9abdf5;">    local </span><span style="color:#89ddff;">-</span><span style="color:#9abdf5;">a commands
</span><span style="color:#9abdf5;">    local </span><span style="color:#89ddff;">-</span><span style="color:#9abdf5;">a options
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    options</span><span style="color:#89ddff;">=</span><span style="color:#9abdf5;">(
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">&#39;-</span><span style="color:#9abdf5;">r[Build </span><span style="color:#89ddff;">in</span><span style="color:#9abdf5;"> release mode with optimizations]</span><span style="color:#89ddff;">&#39;
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">&#39;--</span><span style="color:#9abdf5;">release[Build </span><span style="color:#89ddff;">in</span><span style="color:#9abdf5;"> release mode with optimizations]</span><span style="color:#89ddff;">&#39;
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">&#39;-</span><span style="color:#9abdf5;">j[Number of parallel jobs]</span><span style="color:#89ddff;">&#39;
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">&#39;--</span><span style="color:#9abdf5;">jobs[Number of parallel jobs]</span><span style="color:#89ddff;">&#39;
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">&#39;-</span><span style="color:#9abdf5;">p[Package to build]</span><span style="color:#89ddff;">&#39;
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">&#39;--</span><span style="color:#9abdf5;">package[Package to build]</span><span style="color:#89ddff;">&#39;
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">&#39;--</span><span style="color:#9abdf5;">workspace[Build all packages </span><span style="color:#89ddff;">in</span><span style="color:#9abdf5;"> the workspace]</span><span style="color:#89ddff;">&#39;
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">&#39;-</span><span style="color:#9abdf5;">F[Space</span><span style="color:#89ddff;">-</span><span style="color:#9abdf5;">separated list of features to enable]</span><span style="color:#89ddff;">&#39;
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">&#39;--</span><span style="color:#9abdf5;">features[Space</span><span style="color:#89ddff;">-</span><span style="color:#9abdf5;">separated list of features to enable]</span><span style="color:#89ddff;">&#39;
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">&#39;--</span><span style="color:#9abdf5;">target[Target triple to build </span><span style="color:#bb9af7;">for</span><span style="color:#9abdf5;">]</span><span style="color:#89ddff;">&#39;
</span><span style="color:#9abdf5;">    )
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    _arguments </span><span style="color:#c0caf5;">$options
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="color:#c0caf5;">_cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build </span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">$@</span><span style="color:#89ddff;">&quot;
</span></pre>

</div>
</section>

### Fish Completions

<section class="scenario">
<p class="description">Generated Fish shell completion script.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A build tool configuration
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">BuildArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Build in release mode with optimizations
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    release</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Package to build
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    package</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Build all packages in the workspace
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    workspace</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Space-separated list of features to enable
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    features</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Target triple to build for
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    target</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="serialized-output">
<h4>Rust Output</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#89ddff;">#</span><span style="color:#c0caf5;"> Fish completion </span><span style="color:#bb9af7;">for</span><span style="color:#c0caf5;"> cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build
</span><span style="color:#c0caf5;">
</span><span style="color:#c0caf5;">complete </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">c cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">s r </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">l release </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">d </span><span style="font-style:italic;color:#9d7cd8;">&#39;Build </span><span style="color:#89ddff;">in</span><span style="color:#c0caf5;"> release mode with optimizations</span><span style="color:#89ddff;">&#39;
</span><span style="color:#c0caf5;">complete </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">c cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">s j </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">l jobs </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">d </span><span style="font-style:italic;color:#9d7cd8;">&#39;Number</span><span style="color:#c0caf5;"> of parallel jobs</span><span style="color:#89ddff;">&#39;
</span><span style="color:#c0caf5;">complete </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">c cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">s p </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">l package </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">d </span><span style="font-style:italic;color:#9d7cd8;">&#39;Package</span><span style="color:#c0caf5;"> to build</span><span style="color:#89ddff;">&#39;
</span><span style="color:#c0caf5;">complete </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">c cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">l workspace </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">d </span><span style="font-style:italic;color:#9d7cd8;">&#39;Build</span><span style="color:#c0caf5;"> all packages </span><span style="color:#89ddff;">in</span><span style="color:#c0caf5;"> the workspace</span><span style="color:#89ddff;">&#39;
</span><span style="color:#c0caf5;">complete </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">c cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">s F </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">l features </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">d </span><span style="font-style:italic;color:#9d7cd8;">&#39;Space</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">separated list of features to enable</span><span style="color:#89ddff;">&#39;
</span><span style="color:#c0caf5;">complete </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">c cargo</span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">build </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">l target </span><span style="color:#89ddff;">-</span><span style="color:#c0caf5;">d </span><span style="font-style:italic;color:#9d7cd8;">&#39;Target</span><span style="color:#c0caf5;"> triple to build </span><span style="color:#bb9af7;">for</span><span style="color:#89ddff;">&#39;
</span></pre>

</div>
</section>

## Error Diagnostics


### Unknown Flag

<section class="scenario">
<p class="description">Error when an unrecognized flag is provided.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">--verbos</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">input.txt</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::unknown_long_flag</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ --verbos input.txt 
   · <span style="color:#c678dd;font-weight:bold">────┬───</span>
   ·     <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown flag &#96;--verbos&#96;</span>
   ╰────
<span style="color:#56b6c2">  help: </span>did you mean &#96;--verbose&#96;?
</code></pre>
</div>
</section>

### Unknown Flag with Suggestion

<section class="scenario">
<p class="description">When the flag name is close to a valid one, a suggestion is offered.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A build tool configuration
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">BuildArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Build in release mode with optimizations
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    release</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Package to build
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    package</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Build all packages in the workspace
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    workspace</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Space-separated list of features to enable
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    features</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Target triple to build for
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    target</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">--releas</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::unknown_long_flag</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ --releas 
   · <span style="color:#c678dd;font-weight:bold">────┬───</span>
   ·     <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown flag &#96;--releas&#96;</span>
   ╰────
<span style="color:#56b6c2">  help: </span>did you mean &#96;--release&#96;?
</code></pre>
</div>
</section>

### Invalid Short Flag

<section class="scenario">
<p class="description">Boolean short flags cannot have trailing characters attached.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">-vxyz</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">input.txt</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::unknown_short_flag</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ -vxyz input.txt 
   · <span style="color:#c678dd;font-weight:bold">──┬──</span>
   ·   <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown flag &#96;-vxyz&#96;</span>
   ╰────
<span style="color:#56b6c2">  help: </span>&#96;-vxyz&#96; is &#96;--verbose&#96;
</code></pre>
</div>
</section>

### Triple Dash Flag

<section class="scenario">
<p class="description">Flags with too many dashes are rejected.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">---verbose</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">input.txt</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::unknown_long_flag</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ ---verbose input.txt 
   · <span style="color:#c678dd;font-weight:bold">─────┬────</span>
   ·      <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown flag &#96;---verbose&#96;</span>
   ╰────
<span style="color:#56b6c2">  help: </span>available options:
          <span style="color:#56b6c2">-v, --verbose</span>  <span style="opacity:0.7">Enable verbose output</span>
          <span style="color:#56b6c2">-j, --jobs</span>     <span style="opacity:0.7">Number of parallel jobs to run</span>
          <span style="color:#56b6c2">    &lt;input&gt;</span>    <span style="opacity:0.7">Input file to process</span>
          <span style="color:#56b6c2">    &lt;output&gt;</span>   <span style="opacity:0.7">Output file (defaults to stdout)</span>
</code></pre>
</div>
</section>

### Single Dash with Long Name

<section class="scenario">
<p class="description">Long flag names require double dashes.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">-verbose</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">input.txt</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::unknown_short_flag</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ -verbose input.txt 
   · <span style="color:#c678dd;font-weight:bold">────┬───</span>
   ·     <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown flag &#96;-verbose&#96;</span>
   ╰────
<span style="color:#56b6c2">  help: </span>&#96;-verbose&#96; is &#96;--verbose&#96;
</code></pre>
</div>
</section>

### Missing Value

<section class="scenario">
<p class="description">Error when a flag that requires a value doesn't get one.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">-j</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::expected_value</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ -j 
   · <span style="color:#c678dd;font-weight:bold">─┬</span>
   ·  <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">expected &#96;usize&#96; value</span>
   ╰────
<span style="color:#56b6c2">  help: </span>provide a value after the flag
</code></pre>
</div>
</section>

### Missing Required Argument

<section class="scenario">
<p class="description">Error when a required positional argument is not provided.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">-v</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::missing_argument</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ -v 
   ╰────
<span style="color:#56b6c2">  help: </span>provide a value for &#96;&lt;input&gt;&#96;
</code></pre>
</div>
</section>

### Unexpected Positional Argument

<section class="scenario">
<p class="description">Error when a positional argument is provided but not expected.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A build tool configuration
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">BuildArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Build in release mode with optimizations
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    release</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Package to build
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    package</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Build all packages in the workspace
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    workspace</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Space-separated list of features to enable
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    features</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Target triple to build for
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    target</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">extra</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">--release</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::unexpected_positional</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ extra --release 
   · <span style="color:#c678dd;font-weight:bold">──┬──</span>
   ·   <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unexpected positional argument</span>
   ╰────
<span style="color:#56b6c2">  help: </span>available options:
          <span style="color:#56b6c2">-r, --release</span>    <span style="opacity:0.7">Build in release mode with optimizations</span>
          <span style="color:#56b6c2">-j, --jobs</span>       <span style="opacity:0.7">Number of parallel jobs</span>
          <span style="color:#56b6c2">-p, --package</span>    <span style="opacity:0.7">Package to build</span>
          <span style="color:#56b6c2">    --workspace</span>  <span style="opacity:0.7">Build all packages in the workspace</span>
          <span style="color:#56b6c2">-F, --features</span>   <span style="opacity:0.7">Space-separated list of features to enable</span>
          <span style="color:#56b6c2">    --target</span>     <span style="opacity:0.7">Target triple to build for</span>
</code></pre>
</div>
</section>

### Unknown Subcommand

<section class="scenario">
<p class="description">Error when an unrecognized subcommand is provided, with available options listed.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// Git-like CLI with subcommands.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">GitLikeArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show version information
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    version</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Git command to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">subcommand)]
</span><span style="color:#9abdf5;">    command</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> GitCommand,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Available commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">GitCommand </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Clone a repository into a new directory
</span><span style="color:#9abdf5;">    </span><span style="color:#0db9d7;">Clone </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// The repository URL to clone
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Directory to clone into
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        directory</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Clone only the specified branch
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Create a shallow clone with limited history
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named)]
</span><span style="color:#9abdf5;">        depth</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show the working tree status
</span><span style="color:#9abdf5;">    Status {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show short-format output
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        short</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show the branch even in short-format
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Manage set of tracked repositories
</span><span style="color:#9abdf5;">    Remote {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Remote action to perform
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::subcommand)]
</span><span style="color:#9abdf5;">        action</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> RemoteAction</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Remote management commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">RemoteAction </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</span><span style="color:#9abdf5;">    Add {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// URL of the remote repository
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Remove the remote named &lt;name&gt;
</span><span style="color:#9abdf5;">    rm {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote to remove
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// List all remotes
</span><span style="color:#9abdf5;">    ls {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show remote URLs after names
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">clon</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">https://example.com</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::unknown_subcommand</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ clon https://example.com 
   · <span style="color:#c678dd;font-weight:bold">──┬─</span>
   ·   <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">unknown subcommand &#96;clon&#96;</span>
   ╰────
<span style="color:#56b6c2">  help: </span>did you mean &#96;clone&#96;?
</code></pre>
</div>
</section>

### Missing Subcommand

<section class="scenario">
<p class="description">Error when a required subcommand is not provided.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// Git-like CLI with subcommands.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">GitLikeArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show version information
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    version</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Git command to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">subcommand)]
</span><span style="color:#9abdf5;">    command</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> GitCommand,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Available commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">GitCommand </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Clone a repository into a new directory
</span><span style="color:#9abdf5;">    </span><span style="color:#0db9d7;">Clone </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// The repository URL to clone
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Directory to clone into
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        directory</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Clone only the specified branch
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Create a shallow clone with limited history
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named)]
</span><span style="color:#9abdf5;">        depth</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show the working tree status
</span><span style="color:#9abdf5;">    Status {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show short-format output
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        short</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show the branch even in short-format
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Manage set of tracked repositories
</span><span style="color:#9abdf5;">    Remote {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Remote action to perform
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::subcommand)]
</span><span style="color:#9abdf5;">        action</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> RemoteAction</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Remote management commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">RemoteAction </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</span><span style="color:#9abdf5;">    Add {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// URL of the remote repository
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Remove the remote named &lt;name&gt;
</span><span style="color:#9abdf5;">    rm {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote to remove
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// List all remotes
</span><span style="color:#9abdf5;">    ls {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show remote URLs after names
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">--version</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::missing_subcommand</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ --version 
   ╰────
<span style="color:#56b6c2">  help: </span>available subcommands:
          <span style="color:#56b6c2">clone</span>   <span style="opacity:0.7">Clone a repository into a new directory</span>
          <span style="color:#56b6c2">status</span>  <span style="opacity:0.7">Show the working tree status</span>
          <span style="color:#56b6c2">remote</span>  <span style="opacity:0.7">Manage set of tracked repositories</span>
</code></pre>
</div>
</section>

### Missing Nested Subcommand Argument

<section class="scenario">
<p class="description">Error when a required argument in a nested subcommand is missing.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// Git-like CLI with subcommands.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">GitLikeArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show version information
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named)]
</span><span style="color:#9abdf5;">    version</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Git command to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">subcommand)]
</span><span style="color:#9abdf5;">    command</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> GitCommand,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Available commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">GitCommand </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Clone a repository into a new directory
</span><span style="color:#9abdf5;">    </span><span style="color:#0db9d7;">Clone </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// The repository URL to clone
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Directory to clone into
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        directory</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Clone only the specified branch
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Create a shallow clone with limited history
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named)]
</span><span style="color:#9abdf5;">        depth</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Show the working tree status
</span><span style="color:#9abdf5;">    Status {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show short-format output
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        short</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show the branch even in short-format
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        branch</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Manage set of tracked repositories
</span><span style="color:#9abdf5;">    Remote {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Remote action to perform
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::subcommand)]
</span><span style="color:#9abdf5;">        action</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> RemoteAction</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}
</span><span style="color:#c0caf5;">
</span><span style="font-style:italic;color:#565f89;">/// Remote management commands
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">repr</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">u8</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">enum </span><span style="color:#c0caf5;">RemoteAction </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Add a remote named &lt;name&gt; for the repository at &lt;url&gt;
</span><span style="color:#9abdf5;">    Add {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// URL of the remote repository
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        url</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Remove the remote named &lt;name&gt;
</span><span style="color:#9abdf5;">    rm {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Name of the remote to remove
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::positional)]
</span><span style="color:#9abdf5;">        name</span><span style="color:#89ddff;">: </span><span style="color:#0db9d7;">String</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// List all remotes
</span><span style="color:#9abdf5;">    ls {
</span><span style="color:#9abdf5;">        </span><span style="font-style:italic;color:#565f89;">/// Show remote URLs after names
</span><span style="color:#9abdf5;">        </span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">facet</span><span style="color:#9abdf5;">(args::named</span><span style="color:#89ddff;">,</span><span style="color:#9abdf5;"> args::short)]
</span><span style="color:#9abdf5;">        verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">    }</span><span style="color:#89ddff;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">remote</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">add</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">origin</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::missing_argument</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ remote add origin 
   ╰────
<span style="color:#56b6c2">  help: </span>provide a value for &#96;&lt;url&gt;&#96;
</code></pre>
</div>
</section>

### Invalid Value Type

<section class="scenario">
<p class="description">Error when a value cannot be parsed as the expected type.</p>
<details class="target-type">
<summary>Target Type</summary>
<pre style="background-color:#1a1b26;">
<span style="font-style:italic;color:#565f89;">/// A simple CLI tool for file processing.
</span><span style="color:#89ddff;">#</span><span style="color:#9abdf5;">[</span><span style="color:#c0caf5;">derive</span><span style="color:#9abdf5;">(</span><span style="color:#c0caf5;">Facet</span><span style="color:#9abdf5;">)]
</span><span style="color:#bb9af7;">struct </span><span style="color:#c0caf5;">SimpleArgs </span><span style="color:#9abdf5;">{
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Enable verbose output
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    verbose</span><span style="color:#89ddff;">: </span><span style="color:#bb9af7;">bool</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Number of parallel jobs to run
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">named, </span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">short)]
</span><span style="color:#9abdf5;">    jobs</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#bb9af7;">usize</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Input file to process
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    input</span><span style="color:#89ddff;">:</span><span style="color:#9abdf5;"> String,
</span><span style="color:#9abdf5;">
</span><span style="color:#9abdf5;">    </span><span style="font-style:italic;color:#565f89;">/// Output file (defaults to stdout)
</span><span style="color:#9abdf5;">    #[facet(</span><span style="color:#7dcfff;">args</span><span style="color:#89ddff;">::</span><span style="color:#9abdf5;">positional)]
</span><span style="color:#9abdf5;">    output</span><span style="color:#89ddff;">: </span><span style="color:#9abdf5;">Option</span><span style="color:#89ddff;">&lt;</span><span style="color:#9abdf5;">String</span><span style="color:#89ddff;">&gt;</span><span style="color:#9abdf5;">,
</span><span style="color:#9abdf5;">}</span></pre>

</details>
<div class="input">
<h4>Rust Input</h4>
<pre style="background-color:#1a1b26;">
<span style="color:#2ac3de;">from_slice</span><span style="color:#9abdf5;">(</span><span style="color:#89ddff;">&amp;</span><span style="color:#9abdf5;">[</span><span style="color:#89ddff;">&quot;</span><span style="color:#9ece6a;">-j</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">not-a-number</span><span style="color:#89ddff;">&quot;, &quot;</span><span style="color:#9ece6a;">input.txt</span><span style="color:#89ddff;">&quot;</span><span style="color:#9abdf5;">])</span></pre>

</div>
<div class="error">
<h4>Error</h4>
<pre><code><span style="color:#e06c75">args::reflect_error</span>

  <span style="color:#e06c75">×</span> Could not parse CLI arguments
   ╭────
 <span style="opacity:0.7">1</span> │ -j not-a-number input.txt 
   · <span style="color:#c678dd;font-weight:bold">   ──────┬─────</span>
   ·          <span style="color:#c678dd;font-weight:bold">╰── </span><span style="color:#c678dd;font-weight:bold">invalid value for &#96;usize&#96;</span>
   ╰────
</code></pre>
</div>
</section>
</div>
