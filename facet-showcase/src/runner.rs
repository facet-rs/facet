//! Showcase runner - the main API for creating showcases.

use crate::highlighter::{Highlighter, Language, ansi_to_html};
use crate::output::OutputMode;
use owo_colors::OwoColorize;

/// Build provenance information for tracking where showcase output came from.
#[derive(Debug, Clone, Default)]
pub struct Provenance {
    /// Git commit SHA (full)
    pub commit: Option<String>,
    /// Git commit SHA (short, 7 chars)
    pub commit_short: Option<String>,
    /// Timestamp when generated (ISO 8601)
    pub timestamp: Option<String>,
    /// Rust compiler version
    pub rustc_version: Option<String>,
    /// GitHub repository (e.g., "facet-rs/facet")
    pub github_repo: Option<String>,
    /// Relative path to the source file from repo root
    pub source_file: Option<String>,
}

impl Provenance {
    /// Create provenance from environment variables set by xtask.
    ///
    /// Expected env vars:
    /// - `FACET_SHOWCASE_COMMIT`: full git commit SHA
    /// - `FACET_SHOWCASE_COMMIT_SHORT`: short git commit SHA
    /// - `FACET_SHOWCASE_TIMESTAMP`: ISO 8601 timestamp
    /// - `FACET_SHOWCASE_RUSTC_VERSION`: rustc version string
    /// - `FACET_SHOWCASE_GITHUB_REPO`: GitHub repo (e.g., "facet-rs/facet")
    /// - `FACET_SHOWCASE_SOURCE_FILE`: relative path to source file
    pub fn from_env() -> Self {
        Self {
            commit: std::env::var("FACET_SHOWCASE_COMMIT").ok(),
            commit_short: std::env::var("FACET_SHOWCASE_COMMIT_SHORT").ok(),
            timestamp: std::env::var("FACET_SHOWCASE_TIMESTAMP").ok(),
            rustc_version: std::env::var("FACET_SHOWCASE_RUSTC_VERSION").ok(),
            github_repo: std::env::var("FACET_SHOWCASE_GITHUB_REPO").ok(),
            source_file: std::env::var("FACET_SHOWCASE_SOURCE_FILE").ok(),
        }
    }

    /// Generate a GitHub URL to the source file at the exact commit.
    pub fn github_source_url(&self) -> Option<String> {
        match (&self.github_repo, &self.commit, &self.source_file) {
            (Some(repo), Some(commit), Some(file)) => {
                Some(format!("https://github.com/{repo}/blob/{commit}/{file}"))
            }
            _ => None,
        }
    }

    /// Check if we have meaningful provenance info.
    pub const fn has_info(&self) -> bool {
        self.commit.is_some() || self.timestamp.is_some() || self.rustc_version.is_some()
    }
}

/// Main entry point for running showcases.
pub struct ShowcaseRunner {
    /// Title of the showcase collection
    title: String,
    /// URL slug for Zola (optional)
    slug: Option<String>,
    /// Output mode (terminal or HTML)
    mode: OutputMode,
    /// Syntax highlighter
    highlighter: Highlighter,
    /// Primary language for this showcase (for error highlighting)
    primary_language: Language,
    /// Count of scenarios run
    scenario_count: usize,
    /// Whether we're currently inside a section (affects heading levels)
    in_section: bool,
    /// Filter for scenario names (case-insensitive contains)
    filter: Option<String>,
    /// Build provenance information
    provenance: Provenance,
}

impl ShowcaseRunner {
    /// Create a new showcase runner with the given title.
    ///
    /// The filter can be set via the `SHOWCASE_FILTER` environment variable.
    /// Only scenarios whose names contain the filter string (case-insensitive) will be shown.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            slug: None,
            mode: OutputMode::from_env(),
            highlighter: Highlighter::new(),
            primary_language: Language::Json,
            scenario_count: 0,
            in_section: false,
            filter: std::env::var("SHOWCASE_FILTER").ok(),
            provenance: Provenance::from_env(),
        }
    }

    /// Set a filter for scenario names (case-insensitive contains).
    ///
    /// Only scenarios whose names contain this string will be displayed.
    pub fn filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Set the URL slug for Zola (overrides the default derived from filename).
    pub fn slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = Some(slug.into());
        self
    }

    /// Set the primary language for this showcase.
    pub const fn language(mut self, lang: Language) -> Self {
        self.primary_language = lang;
        self
    }

    /// Print the showcase header.
    pub fn header(&self) {
        match self.mode {
            OutputMode::Terminal => {
                println!();
                self.print_box(&self.title, "cyan");
            }
            OutputMode::Markdown => {
                // Emit TOML frontmatter for Zola
                println!("+++");
                println!("title = \"{}\"", self.title);
                if let Some(ref slug) = self.slug {
                    println!("slug = \"{slug}\"");
                }
                println!("+++");
                println!();
                println!("<div class=\"showcase\">");
            }
        }
    }

    /// Print an intro paragraph after the header.
    ///
    /// This should be called immediately after `header()` to add context
    /// about what this showcase demonstrates.
    pub fn intro(&self, text: &str) {
        match self.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", text.dimmed());
                println!();
            }
            OutputMode::Markdown => {
                println!();
                println!("{text}");
                println!();
            }
        }
    }

    /// Start a new scenario.
    ///
    /// If a filter is set, scenarios that don't match are skipped (all methods become no-ops).
    pub fn scenario(&mut self, name: impl Into<String>) -> Scenario<'_> {
        let name = name.into();
        let skipped = match &self.filter {
            Some(filter) => !name.to_lowercase().contains(&filter.to_lowercase()),
            None => false,
        };
        if !skipped {
            self.scenario_count += 1;
        }
        Scenario::new(self, name, skipped)
    }

    /// Start a new section (h2 heading).
    ///
    /// When sections are used, scenarios within them become h3 headings.
    /// This creates a nice hierarchy in the table of contents.
    pub fn section(&mut self, name: &str) {
        self.in_section = true;

        match self.mode {
            OutputMode::Terminal => {
                println!();
                println!();
                println!("{}", "━".repeat(78).bold().yellow());
                println!("  {}", name.bold().yellow());
                println!("{}", "━".repeat(78).bold().yellow());
            }
            OutputMode::Markdown => {
                println!();
                println!("## {name}");
                println!();
            }
        }
    }

    /// Finish the showcase and print footer.
    pub fn footer(&self) {
        match self.mode {
            OutputMode::Terminal => {
                println!();
                self.print_box("END OF SHOWCASE", "green");
                if self.provenance.has_info() {
                    println!();
                    println!("{}", "Provenance:".dimmed());
                    if let Some(ref commit) = self.provenance.commit_short {
                        println!("  {} {}", "Commit:".dimmed(), commit);
                    }
                    if let Some(ref ts) = self.provenance.timestamp {
                        println!("  {} {}", "Generated:".dimmed(), ts);
                    }
                    if let Some(ref rustc) = self.provenance.rustc_version {
                        println!("  {} {}", "Rustc:".dimmed(), rustc);
                    }
                    if let Some(url) = self.provenance.github_source_url() {
                        println!("  {} {}", "Source:".dimmed(), url);
                    }
                }
            }
            OutputMode::Markdown => {
                // Add provenance footer before closing the showcase div
                if self.provenance.has_info() {
                    println!();
                    println!("<footer class=\"showcase-provenance\">");
                    println!("<p>This showcase was auto-generated from source code.</p>");
                    println!("<dl>");
                    if let Some(url) = self.provenance.github_source_url()
                        && let Some(ref file) = self.provenance.source_file
                    {
                        println!(
                            "<dt>Source</dt><dd><a href=\"{url}\"><code>{file}</code></a></dd>"
                        );
                    }
                    if let Some(ref commit) = self.provenance.commit_short {
                        if let Some(ref repo) = self.provenance.github_repo {
                            if let Some(ref full_commit) = self.provenance.commit {
                                println!(
                                    "<dt>Commit</dt><dd><a href=\"https://github.com/{repo}/commit/{full_commit}\"><code>{commit}</code></a></dd>"
                                );
                            }
                        } else {
                            println!("<dt>Commit</dt><dd><code>{commit}</code></dd>");
                        }
                    }
                    if let Some(ref ts) = self.provenance.timestamp {
                        println!("<dt>Generated</dt><dd><time datetime=\"{ts}\">{ts}</time></dd>");
                    }
                    if let Some(ref rustc) = self.provenance.rustc_version {
                        println!("<dt>Compiler</dt><dd><code>{rustc}</code></dd>");
                    }
                    println!("</dl>");
                    println!("</footer>");
                }
                println!("</div>");
            }
        }
    }

    /// Get a reference to the highlighter.
    pub const fn highlighter(&self) -> &Highlighter {
        &self.highlighter
    }

    /// Get the output mode.
    pub const fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Get the primary language.
    pub const fn primary_language(&self) -> Language {
        self.primary_language
    }

    /// Print a boxed header/footer (terminal mode).
    fn print_box(&self, text: &str, color: &str) {
        // Simple box using Unicode box-drawing characters
        let width = 70;
        let inner_width = width - 2; // Account for left/right borders

        let top = format!("╭{}╮", "─".repeat(inner_width));
        let bottom = format!("╰{}╯", "─".repeat(inner_width));
        let empty_line = format!("│{}│", " ".repeat(inner_width));

        // Center the text
        let text_padding = (inner_width.saturating_sub(text.len())) / 2;
        let text_line = format!(
            "│{}{}{}│",
            " ".repeat(text_padding),
            text,
            " ".repeat(inner_width - text_padding - text.len())
        );

        let output = match color {
            "cyan" => {
                format!(
                    "{}\n{}\n{}\n{}\n{}",
                    top.cyan(),
                    empty_line.cyan(),
                    text_line.cyan(),
                    empty_line.cyan(),
                    bottom.cyan()
                )
            }
            "green" => {
                format!(
                    "{}\n{}\n{}\n{}\n{}",
                    top.green(),
                    empty_line.green(),
                    text_line.green(),
                    empty_line.green(),
                    bottom.green()
                )
            }
            _ => {
                format!("{top}\n{empty_line}\n{text_line}\n{empty_line}\n{bottom}")
            }
        };
        println!("{output}");
    }
}

/// A single scenario within a showcase.
pub struct Scenario<'a> {
    runner: &'a mut ShowcaseRunner,
    name: String,
    description: Option<String>,
    printed_header: bool,
    /// Whether this scenario is skipped due to filtering
    skipped: bool,
}

impl<'a> Scenario<'a> {
    const fn new(runner: &'a mut ShowcaseRunner, name: String, skipped: bool) -> Self {
        Self {
            runner,
            name,
            description: None,
            printed_header: false,
            skipped,
        }
    }

    /// Set a description for this scenario.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Print the scenario header (called automatically on first content).
    fn ensure_header(&mut self) {
        if self.skipped || self.printed_header {
            return;
        }
        self.printed_header = true;

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", "═".repeat(78).dimmed());
                println!("{} {}", "SCENARIO:".bold().cyan(), self.name.bold().white());
                println!("{}", "─".repeat(78).dimmed());
                if let Some(ref desc) = self.description {
                    println!("{}", desc.dimmed());
                }
                println!("{}", "═".repeat(78).dimmed());
            }
            OutputMode::Markdown => {
                // Emit heading as Markdown so Zola can build a table of contents
                // Use h3 if we're inside a section, h2 otherwise
                let heading = if self.runner.in_section { "###" } else { "##" };
                println!();
                println!("{} {}", heading, self.name);
                println!();
                println!("<section class=\"scenario\">");
                if let Some(ref desc) = self.description {
                    println!(
                        "<p class=\"description\">{}</p>",
                        markdown_inline_to_html(desc)
                    );
                }
            }
        }
    }

    /// Display input code with syntax highlighting.
    pub fn input(mut self, lang: Language, code: &str) -> Self {
        if self.skipped {
            return self;
        }
        self.ensure_header();

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", format!("{} Input:", lang.name()).bold().green());
                println!("{}", "─".repeat(60).dimmed());
                print!(
                    "{}",
                    self.runner
                        .highlighter
                        .highlight_to_terminal_with_line_numbers(code, lang)
                );
                println!("{}", "─".repeat(60).dimmed());
            }
            OutputMode::Markdown => {
                println!("<div class=\"input\">");
                println!("<h4>{} Input</h4>", lang.name());
                println!();
                println!("```{}", lang.extension());
                println!("{}", code);
                println!("```");
                println!();
                println!("</div>");
            }
        }
        self
    }

    /// Display a Facet value as input using facet-pretty.
    pub fn input_value<'b, T: facet::Facet<'b>>(mut self, value: &'b T) -> Self {
        if self.skipped {
            return self;
        }
        self.ensure_header();

        use facet_pretty::FacetPretty;

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", "Value Input:".bold().green());
                println!("{}", "─".repeat(60).dimmed());
                println!("  {}", value.pretty());
                println!("{}", "─".repeat(60).dimmed());
            }
            OutputMode::Markdown => {
                let pretty_output = format!("{}", value.pretty());
                println!("<div class=\"input\">");
                println!("<h4>Value Input</h4>");
                println!(
                    "<div class=\"code-block\"><pre><code>{}</code></pre></div>",
                    ansi_to_html(&pretty_output)
                );
                println!("</div>");
            }
        }
        self
    }

    /// Display serialized output with syntax highlighting.
    pub fn serialized_output(mut self, lang: Language, code: &str) -> Self {
        if self.skipped {
            return self;
        }
        self.ensure_header();

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", format!("{} Output:", lang.name()).bold().magenta());
                println!("{}", "─".repeat(60).dimmed());
                print!(
                    "{}",
                    self.runner
                        .highlighter
                        .highlight_to_terminal_with_line_numbers(code, lang)
                );
                println!("{}", "─".repeat(60).dimmed());
            }
            OutputMode::Markdown => {
                println!("<div class=\"serialized-output\">");
                println!("<h4>{} Output</h4>", lang.name());
                println!();
                println!("```{}", lang.extension());
                println!("{}", code);
                println!("```");
                println!();
                println!("</div>");
            }
        }
        self
    }

    /// Display the target type definition using facet-pretty.
    pub fn target_type<T: facet::Facet<'static>>(mut self) -> Self {
        if self.skipped {
            return self;
        }
        self.ensure_header();

        let type_def = facet_pretty::format_shape(T::SHAPE);

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", "Target Type:".bold().blue());
                println!("{}", "─".repeat(60).dimmed());
                print!(
                    "{}",
                    self.runner
                        .highlighter
                        .highlight_to_terminal(&type_def, Language::Rust)
                );
                println!("{}", "─".repeat(60).dimmed());
            }
            OutputMode::Markdown => {
                println!("<details class=\"target-type\">");
                println!("<summary>Target Type</summary>");
                // highlight_to_html returns a complete <pre> element with inline styles
                println!(
                    "{}",
                    self.runner
                        .highlighter
                        .highlight_to_html(&type_def, Language::Rust)
                );
                println!("</details>");
            }
        }
        self
    }

    /// Display a custom type definition string.
    pub fn target_type_str(mut self, type_def: &str) -> Self {
        if self.skipped {
            return self;
        }
        self.ensure_header();

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", "Target Type:".bold().blue());
                println!("{}", "─".repeat(60).dimmed());
                print!(
                    "{}",
                    self.runner
                        .highlighter
                        .highlight_to_terminal(type_def, Language::Rust)
                );
                println!("{}", "─".repeat(60).dimmed());
            }
            OutputMode::Markdown => {
                println!("<details class=\"target-type\">");
                println!("<summary>Target Type</summary>");
                // highlight_to_html returns a complete <pre> element with inline styles
                println!(
                    "{}",
                    self.runner
                        .highlighter
                        .highlight_to_html(type_def, Language::Rust)
                );
                println!("</details>");
            }
        }
        self
    }

    /// Display a compiler error from raw ANSI output (e.g., from `cargo check`).
    pub fn compiler_error(mut self, ansi_output: &str) -> Self {
        if self.skipped {
            return self;
        }
        self.ensure_header();

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", "Compiler Error:".bold().red());
                println!("{ansi_output}");
            }
            OutputMode::Markdown => {
                println!("<div class=\"compiler-error\">");
                println!("<h4>Compiler Error</h4>");
                println!(
                    "<div class=\"code-block\"><pre><code>{}</code></pre></div>",
                    ansi_to_html(ansi_output)
                );
                println!("</div>");
            }
        }
        self
    }

    /// Display a successful result.
    pub fn success<'b, T: facet::Facet<'b>>(mut self, value: &'b T) -> Self {
        if self.skipped {
            return self;
        }
        self.ensure_header();

        use facet_pretty::FacetPretty;

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", "Success:".bold().green());
                println!("  {}", value.pretty());
            }
            OutputMode::Markdown => {
                let pretty_output = format!("{}", value.pretty());
                println!("<div class=\"success\">");
                println!("<h4>Success</h4>");
                println!(
                    "<div class=\"code-block\"><pre><code>{}</code></pre></div>",
                    ansi_to_html(&pretty_output)
                );
                println!("</div>");
            }
        }
        self
    }

    /// Display an error message.
    pub fn error<E: core::fmt::Display>(mut self, err: &E) -> Self {
        if self.skipped {
            return self;
        }
        self.ensure_header();

        let error_text = err.to_string();

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", "Error:".bold().red());
                println!("{error_text}");
            }
            OutputMode::Markdown => {
                println!("<div class=\"error\">");
                println!("<h4>Error</h4>");
                println!(
                    "<div class=\"code-block\"><pre><code>{}</code></pre></div>",
                    crate::highlighter::html_escape(&error_text)
                );
                println!("</div>");
            }
        }
        self
    }

    /// Display a result (either success or error).
    pub fn result<'b, T: facet::Facet<'b>, E: core::fmt::Display>(
        self,
        result: &'b Result<T, E>,
    ) -> Self {
        match result {
            Ok(value) => self.success(value),
            Err(err) => self.error(err),
        }
    }

    /// Display output with ANSI color codes, automatically converted to HTML in markdown mode.
    ///
    /// In terminal mode, the ANSI codes are printed as-is.
    /// In markdown mode, they are converted to HTML `<span>` elements with inline styles.
    pub fn ansi_output(mut self, ansi_text: &str) -> Self {
        if self.skipped {
            return self;
        }
        self.ensure_header();

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{ansi_text}");
            }
            OutputMode::Markdown => {
                println!("<div class=\"output\">");
                println!(
                    "<div class=\"code-block\"><pre><code>{}</code></pre></div>",
                    ansi_to_html(ansi_text)
                );
                println!("</div>");
            }
        }
        self
    }

    /// Finish this scenario.
    pub fn finish(mut self) {
        if self.skipped {
            return;
        }
        self.ensure_header();

        if self.runner.mode == OutputMode::Markdown {
            println!("</section>");
        }
    }
}

/// Convert inline markdown (backticks) to HTML.
fn markdown_inline_to_html(text: &str) -> String {
    let mut result = String::new();
    let chars = text.chars();
    let mut in_code = false;

    for c in chars {
        if c == '`' {
            if in_code {
                result.push_str("</code>");
                in_code = false;
            } else {
                result.push_str("<code>");
                in_code = true;
            }
        } else if c == '<' {
            result.push_str("&lt;");
        } else if c == '>' {
            result.push_str("&gt;");
        } else if c == '&' {
            result.push_str("&amp;");
        } else if c == '\n' {
            result.push_str("<br>");
        } else {
            result.push(c);
        }
    }

    if in_code {
        result.push_str("</code>");
    }

    result
}
