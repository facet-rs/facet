+++
title = "Diff"
+++

<div class="showcase">

facet-diff provides comprehensive diffing capabilities for any type that implements `Facet`. It includes compact and tree formats with syntax highlighting and confusable character detection.


## Struct field changes

<section class="scenario">
<p class="description">Changes to multiple fields in a struct including nested settings.</p>
<div class="output">
<div class="code-block"><pre><code><span style="color:rgb(115,218,202)">settings.theme</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"dark"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"light"</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">age</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">30</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">31</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">email</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"alice@example.com"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"alice@newdomain.com"</span><span style="color:inherit">

</span><span style="color:rgb(86,95,137)">{</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">.. 1 unchanged field</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">age</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(247,118,142)">30</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">31</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">email</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(247,118,142)">"alice@example.com"</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">"alice@newdomain.com"</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">settings</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">{</span><span style="color:inherit">
        </span><span style="color:rgb(86,95,137)">.. 1 unchanged field</span><span style="color:inherit">
        </span><span style="color:rgb(115,218,202)">theme</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(247,118,142)">"dark"</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">"light"</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">}</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">}</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## Nested structures

<section class="scenario">
<p class="description">Changes to fields deep within nested structures.</p>
<div class="output">
<div class="code-block"><pre><code><span style="color:rgb(115,218,202)">sections.[0].heading</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"Intro"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"Introduction"</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">sections.[1].content</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"Some content here"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"Updated content"</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## Sequences (lists/arrays)

<section class="scenario">
<p class="description">Various operations on sequences including single element changes, insertions, deletions, and reordering.</p>
<div class="output">
<div class="code-block"><pre><code>a) Single element change:
<span style="color:rgb(115,218,202)">[2]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">3</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">99</span><span style="color:inherit">

b) Insertions and deletions:
</span><span style="color:rgb(86,95,137)">[</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 1</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 2</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 1</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 4</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 5</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">.. 1 unchanged item</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">]</span><span style="color:inherit">

c) Reordering:
</span><span style="color:rgb(86,95,137)">[</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ "c"</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">.. 2 unchanged items</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- "c"</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">]</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## Enums

<section class="scenario">
<p class="description">Enum diffing including same variant with different data and different variants.</p>
<div class="output">
<div class="code-block"><pre><code>a) Same variant, different data:
<span style="color:rgb(115,218,202)">::Inactive.reason</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"vacation"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"sick leave"</span><span style="color:inherit">

b) Different variants:
</span><span style="color:rgb(247,118,142)">Status::Active</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">Status::Pending {
  since: 42,
}</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## Options

<section class="scenario">
<p class="description">Option types including inner value changes and None to Some transitions.</p>
<div class="output">
<div class="code-block"><pre><code>a) Some to Some (inner change):
<span style="color:rgb(115,218,202)">email</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"bob@example.com"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"bob@company.com"</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">settings.notifications</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">false</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">true</span><span style="color:inherit">

b) None to Some:
</span><span style="color:rgb(247,118,142)">None</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">Some(42)</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## Many changes (truncated)

<section class="scenario">
<p class="description">Large number of changes that get truncated to show summary.</p>
<div class="output">
<div class="code-block"><pre><code><span style="color:rgb(115,218,202)">[2]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">2</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">200</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">[4]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">4</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">400</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">[6]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">6</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">600</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">[8]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">8</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">800</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">[10]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">10</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">1000</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">[12]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">12</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">1200</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">[14]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">14</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">1400</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">[16]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">16</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">1600</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">[18]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">18</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">1800</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">[20]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">20</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">2000</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">... and 4 more changes</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## No changes

<section class="scenario">
<p class="description">Comparing a value with itself shows no differences.</p>
<div class="output">
<div class="code-block"><pre><code><span style="color:rgb(86,95,137)">(no changes)</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## Scalar types

<section class="scenario">
<p class="description">Diffing primitive types including integers, floats, booleans, characters, and strings.</p>
<div class="output">
<div class="code-block"><pre><code>a) Integers:
  i32: <span style="color:rgb(247,118,142)">42</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">-42</span><span style="color:inherit">
  i128 min→max: </span><span style="color:rgb(247,118,142)">-170141183460469231731687303715884105728</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">170141183460469231731687303715884105727</span><span style="color:inherit">
  u64 0→max: </span><span style="color:rgb(247,118,142)">0</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">18446744073709551615</span><span style="color:inherit">

b) Floats:
  f64: </span><span style="color:rgb(247,118,142)">3.141592653589793</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">2.718281828459045</span><span style="color:inherit">
  f64 inf→-inf: </span><span style="color:rgb(247,118,142)">inf</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">-inf</span><span style="color:inherit">
  f64 NaN→NaN: </span><span style="color:rgb(247,118,142)">NaN</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">NaN</span><span style="color:inherit">

c) Booleans:
  bool: </span><span style="color:rgb(247,118,142)">true</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">false</span><span style="color:inherit">

d) Characters:
  char: </span><span style="color:rgb(247,118,142)">A</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">Z</span><span style="color:inherit">
  emoji: </span><span style="color:rgb(247,118,142)">🦀</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">🐍</span><span style="color:inherit">

e) Strings:
  &amp;str: </span><span style="color:rgb(247,118,142)">"hello"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"world"</span><span style="color:inherit">
  String unicode: </span><span style="color:rgb(247,118,142)">"Hello 世界"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"Hello 🌍"</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## Confusable strings

<section class="scenario">
<p class="description">Detection of Unicode confusable characters using the Unicode TR39 confusables database. These include homoglyphs that look similar but are from different scripts.</p>
<div class="output">
<div class="code-block"><pre><code>a) Latin 'a' vs Cyrillic 'а' (detected):
<span style="color:rgb(247,118,142)">"abc"</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">"аbc"</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">(strings are visually confusable but differ in 1 position):</span><span style="color:inherit">
  [0]: </span><span style="color:rgb(247,118,142)">'a' (U+0061)</span><span style="color:inherit"> vs </span><span style="color:rgb(115,218,202)">'\u{0430}'</span><span style="color:inherit">

b) Latin 'o' vs Greek 'ο' (detected):
</span><span style="color:rgb(247,118,142)">"foo"</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">"fοo"</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">(strings are visually confusable but differ in 1 position):</span><span style="color:inherit">
  [1]: </span><span style="color:rgb(247,118,142)">'o' (U+006F)</span><span style="color:inherit"> vs </span><span style="color:rgb(115,218,202)">'\u{03BF}'</span><span style="color:inherit">

c) Latin 'e' vs Cyrillic 'е' (detected):
</span><span style="color:rgb(247,118,142)">"hello"</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">"hеllo"</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">(strings are visually confusable but differ in 1 position):</span><span style="color:inherit">
  [1]: </span><span style="color:rgb(247,118,142)">'e' (U+0065)</span><span style="color:inherit"> vs </span><span style="color:rgb(115,218,202)">'\u{0435}'</span><span style="color:inherit">

d) With zero-width joiner (not in TR39):
</span><span style="color:rgb(247,118,142)">"test"</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">"te‍st"</span><span style="color:inherit">

e) Different quote styles (not in TR39):
</span><span style="color:rgb(247,118,142)">r""quoted""</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">"“quoted”"</span><span style="color:inherit">

f) Greek Iota vs Latin I (not in TR39):
</span><span style="color:rgb(247,118,142)">"userId"</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">"userΙd"</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## Byte slices

<section class="scenario">
<p class="description">Diffing byte arrays including ASCII and binary data.</p>
<div class="output">
<div class="code-block"><pre><code>a) ASCII bytes:
  <span style="color:rgb(86,95,137)">[</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 104</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 101</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 108</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 108</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 119</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">.. 1 unchanged item</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 114</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 108</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 100</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">]</span><span style="color:inherit">

b) Binary data:
  </span><span style="color:rgb(86,95,137)">[</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 0</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 255</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 66</span><span style="color:inherit">
    </span><span style="color:rgb(247,118,142)">- 19</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 0</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 254</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 66</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">+ 55</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">]</span><span style="color:inherit">

c) Vec&lt;u8&gt;:
  </span><span style="color:rgb(115,218,202)">[2]</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">3</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">99</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## Deep tree (6 levels)

<section class="scenario">
<p class="description">Deeply nested structures demonstrating change detection at multiple nesting levels.</p>
<div class="output">
<div class="code-block"><pre><code>a) Change at deepest level (level 6):
<span style="color:rgb(115,218,202)">inner.inner.inner.inner.inner.tag</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"original"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"modified"</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">inner.inner.inner.inner.inner.value</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">42</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">999</span><span style="color:inherit">

b) Changes at multiple levels (2, 4, 6):
</span><span style="color:rgb(115,218,202)">inner.inner.inner.enabled</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">true</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">false</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">inner.inner.inner.inner.inner.value</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">42</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">100</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">inner.priority</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">1</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">5</span><span style="color:inherit">

c) Changes at every level:
</span><span style="color:rgb(115,218,202)">label</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"label-old"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"label-new"</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">inner.priority</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">1</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">2</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">inner.inner.name</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"old"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"new"</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">inner.inner.inner.enabled</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">true</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">false</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">inner.inner.inner.inner.count</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">10</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">20</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">inner.inner.inner.inner.inner.tag</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"a"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"b"</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">inner.inner.inner.inner.inner.value</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">1</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">2</span><span style="color:inherit">

d) Tree format for deep change:
</span><span style="color:rgb(86,95,137)">{</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">.. 1 unchanged field</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">inner</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">{</span><span style="color:inherit">
        </span><span style="color:rgb(86,95,137)">.. 1 unchanged field</span><span style="color:inherit">
        </span><span style="color:rgb(115,218,202)">inner</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">{</span><span style="color:inherit">
            </span><span style="color:rgb(86,95,137)">.. 1 unchanged field</span><span style="color:inherit">
            </span><span style="color:rgb(115,218,202)">inner</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">{</span><span style="color:inherit">
                </span><span style="color:rgb(86,95,137)">.. 1 unchanged field</span><span style="color:inherit">
                </span><span style="color:rgb(115,218,202)">inner</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">{</span><span style="color:inherit">
                    </span><span style="color:rgb(86,95,137)">.. 1 unchanged field</span><span style="color:inherit">
                    </span><span style="color:rgb(115,218,202)">inner</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">{</span><span style="color:inherit">
                        </span><span style="color:rgb(86,95,137)">.. 1 unchanged field</span><span style="color:inherit">
                        </span><span style="color:rgb(115,218,202)">value</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(247,118,142)">42</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">999</span><span style="color:inherit">
                    </span><span style="color:rgb(86,95,137)">}</span><span style="color:inherit">
                </span><span style="color:rgb(86,95,137)">}</span><span style="color:inherit">
            </span><span style="color:rgb(86,95,137)">}</span><span style="color:inherit">
        </span><span style="color:rgb(86,95,137)">}</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">}</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">}</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

## Wide tree (20 fields)

<section class="scenario">
<p class="description">Structure with many fields demonstrating diff truncation and summarization.</p>
<div class="output">
<div class="code-block"><pre><code>a) Single field change (among 20 fields):
<span style="color:rgb(115,218,202)">field_18</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">300</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">999</span><span style="color:inherit">

b) Scattered changes (fields 2, 8, 14, 19):
</span><span style="color:rgb(115,218,202)">field_19</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">400</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">888</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_14</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">true</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">false</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_08</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">3</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">999</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_02</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"b"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"CHANGED"</span><span style="color:inherit">

c) Many changes (exceeds truncation limit):
</span><span style="color:rgb(115,218,202)">field_16</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">100</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">1000</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_06</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">1</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">10</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_02</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"b"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"B"</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_18</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">300</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">3000</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_11</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">true</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">false</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_01</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"a"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"A"</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_14</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">true</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">false</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_20</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">500</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">5000</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_05</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">"e"</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">"E"</span><span style="color:inherit">
</span><span style="color:rgb(115,218,202)">field_08</span><span style="color:inherit">: </span><span style="color:rgb(247,118,142)">3</span><span style="color:inherit"> </span><span style="color:rgb(86,95,137)">→</span><span style="color:inherit"> </span><span style="color:rgb(115,218,202)">30</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">... and 10 more changes</span><span style="color:inherit">

d) Tree format with few changes:
</span><span style="color:rgb(86,95,137)">{</span><span style="color:inherit">
    </span><span style="color:rgb(86,95,137)">.. 19 unchanged fields</span><span style="color:inherit">
    </span><span style="color:rgb(115,218,202)">field_12</span><span style="color:inherit"></span><span style="color:rgb(86,95,137)">:</span><span style="color:inherit"> </span><span style="color:rgb(247,118,142)">true</span><span style="color:inherit"> → </span><span style="color:rgb(115,218,202)">false</span><span style="color:inherit">
</span><span style="color:rgb(86,95,137)">}</span><span style="color:inherit"></span></code></pre></div>
</div>
</section>

<footer class="showcase-provenance">
<p>This showcase was auto-generated from source code.</p>
<dl>
<dt>Source</dt><dd><a href="https://github.com/facet-rs/facet/blob/5c8df10b37be181e3a88be583c1eee213e28dbd5/facet-diff/examples/diff_showcase.rs"><code>facet-diff/examples/diff_showcase.rs</code></a></dd>
<dt>Commit</dt><dd><a href="https://github.com/facet-rs/facet/commit/5c8df10b37be181e3a88be583c1eee213e28dbd5"><code>5c8df10b3</code></a></dd>
<dt>Generated</dt><dd><time datetime="2026-01-16T05:16:07+01:00">2026-01-16T05:16:07+01:00</time></dd>
<dt>Compiler</dt><dd><code>rustc 1.91.1 (ed61e7d7e 2025-11-07)</code></dd>
</dl>
</footer>
</div>
