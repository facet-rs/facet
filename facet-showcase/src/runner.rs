//! Showcase runner - the main API for creating showcases.

use crate::highlighter::{Highlighter, Language, ansi_to_html, html_escape};
use crate::output::OutputMode;
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme};
use owo_colors::OwoColorize;

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
}

impl ShowcaseRunner {
    /// Create a new showcase runner with the given title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            slug: None,
            mode: OutputMode::from_env(),
            highlighter: Highlighter::new(),
            primary_language: Language::Json,
            scenario_count: 0,
        }
    }

    /// Set the URL slug for Zola (overrides the default derived from filename).
    pub fn slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = Some(slug.into());
        self
    }

    /// Set the primary language for this showcase.
    pub fn language(mut self, lang: Language) -> Self {
        self.primary_language = lang;
        self
    }

    /// Add KDL syntax support from a directory.
    pub fn with_kdl_syntaxes(mut self, syntax_dir: &str) -> Self {
        self.highlighter = std::mem::take(&mut self.highlighter).with_kdl_syntaxes(syntax_dir);
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
                    println!("slug = \"{}\"", slug);
                }
                println!("+++");
                println!();
                println!("<div class=\"showcase\">");
            }
        }
    }

    /// Start a new scenario.
    pub fn scenario(&mut self, name: impl Into<String>) -> Scenario<'_> {
        self.scenario_count += 1;
        Scenario::new(self, name.into())
    }

    /// Finish the showcase and print footer.
    pub fn footer(&self) {
        match self.mode {
            OutputMode::Terminal => {
                println!();
                self.print_box("END OF SHOWCASE", "green");
            }
            OutputMode::Markdown => {
                println!("</div>");
            }
        }
    }

    /// Get a reference to the highlighter.
    pub fn highlighter(&self) -> &Highlighter {
        &self.highlighter
    }

    /// Get the output mode.
    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Get the primary language.
    pub fn primary_language(&self) -> Language {
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
}

impl<'a> Scenario<'a> {
    fn new(runner: &'a mut ShowcaseRunner, name: String) -> Self {
        Self {
            runner,
            name,
            description: None,
            printed_header: false,
        }
    }

    /// Set a description for this scenario.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Print the scenario header (called automatically on first content).
    fn ensure_header(&mut self) {
        if self.printed_header {
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
                println!();
                println!("## {}", self.name);
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
                // highlight_to_html returns a complete <pre> element with inline styles
                println!("{}", self.runner.highlighter.highlight_to_html(code, lang));
                println!("</div>");
            }
        }
        self
    }

    /// Display the target type definition using facet-pretty.
    pub fn target_type<T: facet::Facet<'static>>(mut self) -> Self {
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
                println!("<div class=\"target-type\">");
                println!("<h4>Target Type</h4>");
                // highlight_to_html returns a complete <pre> element with inline styles
                println!(
                    "{}",
                    self.runner
                        .highlighter
                        .highlight_to_html(&type_def, Language::Rust)
                );
                println!("</div>");
            }
        }
        self
    }

    /// Display a custom type definition string.
    pub fn target_type_str(mut self, type_def: &str) -> Self {
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
                println!("<div class=\"target-type\">");
                println!("<h4>Target Type</h4>");
                // highlight_to_html returns a complete <pre> element with inline styles
                println!(
                    "{}",
                    self.runner
                        .highlighter
                        .highlight_to_html(type_def, Language::Rust)
                );
                println!("</div>");
            }
        }
        self
    }

    /// Display an error using miette's graphical reporter.
    pub fn error(mut self, err: &dyn Diagnostic) -> Self {
        self.ensure_header();

        match self.runner.mode {
            OutputMode::Terminal => {
                println!();
                println!("{}", "Error:".bold().red());

                let mut output = String::new();
                let highlighter = self
                    .runner
                    .highlighter
                    .build_miette_highlighter(self.runner.primary_language);
                let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
                    .with_syntax_highlighting(highlighter);
                handler.render_report(&mut output, err).unwrap();
                println!("{output}");
            }
            OutputMode::Markdown => {
                // Render the error with ANSI colors, then convert to HTML
                let mut output = String::new();
                let highlighter = self
                    .runner
                    .highlighter
                    .build_miette_highlighter(self.runner.primary_language);
                let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
                    .with_syntax_highlighting(highlighter);
                handler.render_report(&mut output, err).unwrap();

                println!("<div class=\"error\">");
                println!("<h4>Error</h4>");
                println!("<pre><code>{}</code></pre>", ansi_to_html(&output));
                println!("</div>");
            }
        }
        self
    }

    /// Display a successful result.
    pub fn success<'b, T: facet::Facet<'b>>(mut self, value: &'b T) -> Self {
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
                println!("<pre><code>{}</code></pre>", ansi_to_html(&pretty_output));
                println!("</div>");
            }
        }
        self
    }

    /// Display a result (either success or error).
    pub fn result<'b, T: facet::Facet<'b>, E: Diagnostic>(self, result: &'b Result<T, E>) -> Self {
        match result {
            Ok(value) => self.success(value),
            Err(err) => self.error(err),
        }
    }

    /// Finish this scenario.
    pub fn finish(mut self) {
        self.ensure_header();

        if self.runner.mode == OutputMode::Markdown {
            println!("</section>");
        }
    }
}

/// Convert inline markdown (backticks) to HTML.
fn markdown_inline_to_html(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    let mut in_code = false;

    while let Some(c) = chars.next() {
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
