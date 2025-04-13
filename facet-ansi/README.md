<h1>
<picture>
<source srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/logo-only.webp">
<img src="https://github.com/facet-rs/facet/raw/main/static/logo-v2/logo-only.png" height="35" alt="Facet logo - a reflection library for Rust">
</picture> &nbsp; facet-ansi
</h1>

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![free of syn](https://img.shields.io/badge/free%20of-syn-hotpink)](https://github.com/fasterthanlime/free-of-syn)
[![crates.io](https://img.shields.io/crates/v/facet-ansi.svg)](https://crates.io/crates/facet-ansi)
[![documentation](https://docs.rs/facet-ansi/badge.svg)](https://docs.rs/facet-ansi)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-ansi.svg)](./LICENSE)

_Logo by [Misiasart](https://misiasart.com/)_

Thanks to all individual and corporate sponsors, without whom this work could not exist:

<p> <a href="https://ko-fi.com/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/ko-fi-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/ko-fi-light.svg" height="40" alt="Ko-fi">
</picture>
</a> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/patreon-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/patreon-light.svg" height="40" alt="Patreon">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
    <img src="https://depot.dev/badges/built-with-depot.svg" alt="built with depot">
</a> </p>

Think owo-colors, but even lighter, and unconditional. And not comprehensive.

## Overview

`facet-ansi` is a lightweight library for adding ANSI colors and styles to terminal output. It provides a simple, ergonomic API without any conditional compilation or unnecessary complexity.

## Examples

### Basic Styling

```rust
use facet_ansi::Stylize;

# fn main() {
println!("{}", "This text is red".red());
println!("{}", "This text is blue".blue());
println!("{}", "This text is green".green());

// Combine with formatting
println!("{} and {}", "Red text".red(), "blue text".blue());
# }
```

### Style Methods

```rust
use facet_ansi::Stylize;

# fn main() {
// Apply emphasis styles
println!("{}", "Bold text".bold());
println!("{}", "Underlined text".underline());
println!("{}", "Dimmed text".dimmed());

// Combine styles with method chaining
println!("{}", "Bold and red".red().style(facet_ansi::Style::new().bold()));
# }
```

### Using the Style Module

```rust
use facet_ansi::{Stylize, ColorStyle, styles};

# fn main() {
// Use pre-defined styles
let warning_style = styles::yellow();
let error_style = styles::bright_red().bold();

println!("{}", "Warning message".style(warning_style));
println!("{}", "Error message".style(error_style));

// Create custom styles
let custom_style = facet_ansi::Style::new().fg_green().bold().underline();
println!("{}", "Custom styled text".style(custom_style));
# }
```

### Debug Output

```rust
use facet_ansi::Stylize;

# fn main() {
// Works with debug formatting too
let data = vec![1, 2, 3];
println!("{:?}", data.cyan());
# }
```

