//! Help text generation for command-line interfaces.
//!
//! This module provides utilities to generate help text from Schema,
//! including doc comments, field names, and attribute information.

use crate::missing::normalize_program_name;
use crate::schema::{
    ArgLevelSchema, ArgSchema, ConfigFieldGroupSchema, ConfigFieldSchema, ConfigStructSchema,
    ConfigValueSchema, Docs, Schema, Subcommand,
};
use facet_core::Facet;
use heck::ToKebabCase;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::string::String;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec::Vec;

const DEFAULT_CONFIG_FILE_EXTENSIONS: &[&str] = &["json"];

/// Generate help text for a Facet type.
///
/// This is a convenience function that builds a Schema internally.
/// If you already have a Schema, use `generate_help_for_subcommand` instead.
pub fn generate_help<T: Facet<'static>>(config: &HelpConfig) -> String {
    generate_help_for_shape(T::SHAPE, config)
}

/// Generate help text from a Shape.
///
/// This is a convenience function that builds a Schema internally.
/// If you already have a Schema, use `generate_help_for_subcommand` instead.
pub fn generate_help_for_shape(shape: &'static facet_core::Shape, config: &HelpConfig) -> String {
    let schema = match Schema::from_shape(shape) {
        Ok(s) => s,
        Err(_) => {
            // Fall back to a minimal help message
            let program_name = config
                .program_name
                .clone()
                .or_else(|| {
                    std::env::args()
                        .next()
                        .map(|path| normalize_program_name(&path))
                })
                .unwrap_or_else(|| "program".to_string());
            return format!(
                "{}\n\n(Schema could not be built for this type)\n",
                program_name
            );
        }
    };

    generate_help_for_subcommand(&schema, &[], config)
}

/// Generate HTML help for a Facet type.
///
/// This uses the same schema data as terminal help, but renders it as a
/// standalone HTML document suitable for writing to disk or sharing.
pub fn generate_html_help<T: Facet<'static>>(config: &HelpConfig) -> String {
    generate_html_help_for_shape(T::SHAPE, config)
}

/// Generate HTML help from a Shape.
pub fn generate_html_help_for_shape(
    shape: &'static facet_core::Shape,
    config: &HelpConfig,
) -> String {
    let schema = match Schema::from_shape(shape) {
        Ok(s) => s,
        Err(_) => {
            let program_name = resolve_program_name(config);
            return render_minimal_html_help(
                &program_name,
                config.version.as_deref(),
                "(Schema could not be built for this type)",
            );
        }
    };

    generate_html_help_for_subcommand(&schema, &[], config)
}

/// Generate HTML help for a specific subcommand path from a Schema.
pub fn generate_html_help_for_subcommand(
    schema: &Schema,
    subcommand_path: &[String],
    config: &HelpConfig,
) -> String {
    generate_html_help_for_subcommand_with_config_formats(
        schema,
        subcommand_path,
        config,
        DEFAULT_CONFIG_FILE_EXTENSIONS,
    )
}

pub(crate) fn generate_root_html_help_with_config_formats_and_anchor(
    schema: &Schema,
    config: &HelpConfig,
    config_file_extensions: &[&str],
    initial_anchor: Option<&str>,
) -> String {
    let program_name = resolve_program_name(config);
    render_html_help_document(HtmlHelpDocument {
        command: &program_name,
        version: config.version.as_deref(),
        docs: schema.docs(),
        description: config.description.as_deref(),
        args: schema.args(),
        config_roots: schema.configs(),
        config_file_extensions,
        initial_anchor,
    })
}

pub(crate) fn html_help_anchor_for_subcommand_path(
    schema: &Schema,
    subcommand_path: &[String],
) -> Option<String> {
    if subcommand_path.is_empty() {
        return None;
    }

    let mut current_args = schema.args();
    let mut anchor_path = Vec::new();

    for name in subcommand_path {
        let sub = current_args
            .subcommands()
            .values()
            .find(|sub| sub.effective_name() == name)?;
        anchor_path.push(sub.cli_name());
        current_args = sub.args();
    }

    Some(command_heading_id(&anchor_path))
}

pub(crate) fn generate_html_help_for_subcommand_with_config_formats(
    schema: &Schema,
    subcommand_path: &[String],
    config: &HelpConfig,
    config_file_extensions: &[&str],
) -> String {
    let program_name = resolve_program_name(config);

    if subcommand_path.is_empty() {
        return generate_root_html_help_with_config_formats_and_anchor(
            schema,
            config,
            config_file_extensions,
            None,
        );
    }

    let mut current_args = schema.args();
    let mut command_path = vec![program_name.clone()];
    let mut final_sub: Option<&Subcommand> = None;

    for name in subcommand_path {
        let Some(sub) = current_args
            .subcommands()
            .values()
            .find(|sub| sub.effective_name() == name)
        else {
            return render_html_help_document(HtmlHelpDocument {
                command: &program_name,
                version: config.version.as_deref(),
                docs: schema.docs(),
                description: config.description.as_deref(),
                args: schema.args(),
                config_roots: schema.configs(),
                config_file_extensions,
                initial_anchor: None,
            });
        };

        command_path.push(sub.cli_name().to_string());
        current_args = sub.args();
        final_sub = Some(sub);
    }

    render_html_help_document(HtmlHelpDocument {
        command: &command_path.join(" "),
        version: None,
        docs: final_sub
            .map(Subcommand::docs)
            .unwrap_or_else(|| schema.docs()),
        description: config.description.as_deref(),
        args: current_args,
        config_roots: &[],
        config_file_extensions: DEFAULT_CONFIG_FILE_EXTENSIONS,
        initial_anchor: None,
    })
}

/// Write an HTML help document to a unique file under the system temp directory.
///
/// The directory is intentionally not deleted when this function returns because
/// browsers may read the file after the process exits.
pub fn write_html_help_to_temp_file(html: &str) -> io::Result<PathBuf> {
    let path = html_help_temp_dir().join("index.html");
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("could not build HTML help path"))?;
    fs::create_dir_all(dir)?;
    fs::write(&path, html)?;
    Ok(path)
}

/// Open an HTML help file in the user's default browser.
pub fn open_html_help_file(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    let status = open_file_command(path).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "browser opener exited with status {status}"
        )))
    }
}

/// Configuration for help text generation.
#[derive(Debug, Clone)]
pub struct HelpConfig {
    /// Program name (defaults to executable name)
    pub program_name: Option<String>,
    /// Program version
    pub version: Option<String>,
    /// Additional description to show after the auto-generated one
    pub description: Option<String>,
    /// Width for wrapping text (0 = no wrapping)
    pub width: usize,
}

impl Default for HelpConfig {
    fn default() -> Self {
        Self {
            program_name: None,
            version: None,
            description: None,
            width: 80,
        }
    }
}

/// Generate help text for a specific subcommand path from a Schema.
///
/// `subcommand_path` is a list of subcommand names (e.g., `["repo", "clone"]` for `myapp repo clone --help`).
/// This navigates through the schema to find the target subcommand and generates help for it.
pub fn generate_help_for_subcommand(
    schema: &Schema,
    subcommand_path: &[String],
    config: &HelpConfig,
) -> String {
    generate_help_for_subcommand_with_config_formats(
        schema,
        subcommand_path,
        config,
        DEFAULT_CONFIG_FILE_EXTENSIONS,
    )
}

pub(crate) fn generate_help_for_subcommand_with_config_formats(
    schema: &Schema,
    subcommand_path: &[String],
    config: &HelpConfig,
    config_file_extensions: &[&str],
) -> String {
    let program_name = config
        .program_name
        .clone()
        .or_else(|| {
            std::env::args()
                .next()
                .map(|path| normalize_program_name(&path))
        })
        .unwrap_or_else(|| "program".to_string());

    if subcommand_path.is_empty() {
        return generate_help_from_schema(schema, &program_name, config, config_file_extensions);
    }

    // Navigate to the subcommand
    let mut current_args = schema.args();
    let mut command_path = vec![program_name.clone()];

    for name in subcommand_path {
        // The path contains effective names (e.g., "Clone", "rm") from ConfigValue.
        // Look up by effective_name since that's what's stored in the path.
        let sub = current_args
            .subcommands()
            .values()
            .find(|s| s.effective_name() == name);

        if let Some(sub) = sub {
            command_path.push(sub.cli_name().to_string());
            current_args = sub.args();
        } else {
            // Subcommand not found, fall back to root help
            return generate_help_from_schema(
                schema,
                &program_name,
                config,
                config_file_extensions,
            );
        }
    }

    // Find the final subcommand to get its docs
    let mut final_sub: Option<&Subcommand> = None;
    let mut args = schema.args();

    for name in subcommand_path {
        let sub = args
            .subcommands()
            .values()
            .find(|s| s.effective_name() == name);
        if let Some(sub) = sub {
            final_sub = Some(sub);
            args = sub.args();
        }
    }

    generate_help_for_subcommand_level(current_args, final_sub, &command_path.join(" "), config)
}

/// Generate help from a built Schema.
fn generate_help_from_schema(
    schema: &Schema,
    program_name: &str,
    config: &HelpConfig,
    config_file_extensions: &[&str],
) -> String {
    let mut out = String::new();

    // Program name and version
    if let Some(version) = &config.version {
        out.push_str(&format!("{program_name} {version}\n"));
    } else {
        out.push_str(&format!("{program_name}\n"));
    }

    // Type doc comment from schema
    if let Some(summary) = schema.docs().summary() {
        out.push('\n');
        out.push_str(summary.trim());
        out.push('\n');
    }
    if let Some(details) = schema.docs().details() {
        for line in details.lines() {
            out.push_str(line.trim());
            out.push('\n');
        }
    }

    // Additional description
    if let Some(desc) = &config.description {
        out.push('\n');
        out.push_str(desc);
        out.push('\n');
    }

    out.push('\n');

    generate_arg_level_help(
        &mut out,
        schema.args(),
        schema.configs(),
        program_name,
        config,
        config_file_extensions,
    );

    out
}

/// Generate help for a subcommand level.
fn generate_help_for_subcommand_level(
    args: &ArgLevelSchema,
    subcommand: Option<&Subcommand>,
    full_command: &str,
    config: &HelpConfig,
) -> String {
    let mut out = String::new();

    // Header with full command
    out.push_str(&format!("{full_command}\n"));

    // Doc comment for the subcommand
    if let Some(sub) = subcommand {
        if let Some(summary) = sub.docs().summary() {
            out.push('\n');
            out.push_str(summary.trim());
            out.push('\n');
        }
        if let Some(details) = sub.docs().details() {
            for line in details.lines() {
                out.push_str(line.trim());
                out.push('\n');
            }
        }
    }

    // Additional description from config
    if let Some(desc) = &config.description {
        out.push('\n');
        out.push_str(desc);
        out.push('\n');
    }

    out.push('\n');

    generate_arg_level_help(
        &mut out,
        args,
        &[],
        full_command,
        config,
        DEFAULT_CONFIG_FILE_EXTENSIONS,
    );

    out
}

fn resolve_program_name(config: &HelpConfig) -> String {
    config
        .program_name
        .clone()
        .or_else(|| {
            std::env::args()
                .next()
                .map(|path| normalize_program_name(&path))
        })
        .unwrap_or_else(|| "program".to_string())
}

fn html_help_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    std::env::temp_dir()
        .join("figue-help-tabs")
        .join(format!("{}-{nanos}", std::process::id()))
}

#[cfg(target_os = "macos")]
fn open_file_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg(path);
    command
}

#[cfg(target_os = "windows")]
fn open_file_command(path: &Path) -> Command {
    let mut command = Command::new("cmd");
    command.arg("/C").arg("start").arg("").arg(path);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_file_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
}

struct HtmlHelpDocument<'a> {
    command: &'a str,
    version: Option<&'a str>,
    docs: &'a crate::schema::Docs,
    description: Option<&'a str>,
    args: &'a ArgLevelSchema,
    config_roots: &'a [ConfigStructSchema],
    config_file_extensions: &'a [&'a str],
    initial_anchor: Option<&'a str>,
}

fn render_minimal_html_help(command: &str, version: Option<&str>, message: &str) -> String {
    let docs = crate::schema::Docs::default();
    let args = ArgLevelSchema::default();
    render_html_help_document(HtmlHelpDocument {
        command,
        version,
        docs: &docs,
        description: Some(message),
        args: &args,
        config_roots: &[],
        config_file_extensions: DEFAULT_CONFIG_FILE_EXTENSIONS,
        initial_anchor: None,
    })
}

fn render_html_help_document(doc: HtmlHelpDocument<'_>) -> String {
    let title = match doc.version {
        Some(version) => format!("{} {}", doc.command, version),
        None => doc.command.to_string(),
    };

    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("  <meta charset=\"utf-8\">\n");
    out.push_str("  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("  <title>");
    push_escaped(&mut out, &title);
    out.push_str("</title>\n");
    out.push_str("  <style>\n");
    out.push_str(
        "    @font-face { font-family: 'Inter Variable'; font-style: normal; font-display: swap; font-weight: 100 900; src: url(https://cdn.jsdelivr.net/fontsource/fonts/inter:vf@latest/latin-wght-normal.woff2) format('woff2-variations'); unicode-range: U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD; }\n",
    );
    out.push_str(
        "    @font-face { font-family: 'Maple Mono'; font-style: normal; font-display: swap; font-weight: 400; src: url(https://cdn.jsdelivr.net/fontsource/fonts/maple-mono@latest/maple-mono-latin-400-normal.woff2) format('woff2'), url(https://cdn.jsdelivr.net/fontsource/fonts/maple-mono@latest/maple-mono-latin-400-normal.woff) format('woff'); }\n",
    );
    out.push_str(
        "    :root { color-scheme: light dark; interpolate-size: allow-keywords; --bg: light-dark(#f8f7f3, #171819); --fg: light-dark(#202124, #f1efe7); --muted: light-dark(#68645d, #b8b2a7); --panel: light-dark(#ffffff, #212325); --line: light-dark(#dedbd2, #34373a); --accent: light-dark(#0f766e, #5eead4); --code: light-dark(#f0eee7, #2b2d2f); --soft: light-dark(#edf7f5, #183532); --value-bg: light-dark(#fff4d6, #3a2f18); --value-fg: light-dark(#8a4b0b, #fbbf24); --env-bg: light-dark(#e9f7ef, #143321); --env-fg: light-dark(#166534, #86efac); --search-bg: light-dark(#fde68a, #43330c); --search-fg: light-dark(#111827, #fff7d6); --search-current-bg: light-dark(#f59e0b, #b45309); --search-current-fg: light-dark(#111827, #fff7d6); }\n",
    );
    out.push_str(
        "    * { box-sizing: border-box; } html { scroll-behavior: smooth; } body { margin: 0; background: var(--bg); color: var(--fg); font: 16px/1.62 \"Inter Variable\", Inter, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif; text-rendering: optimizeLegibility; }\n",
    );
    out.push_str(
        "    .topbar { position: sticky; top: 0; z-index: 10; border-bottom: 1px solid var(--line); background: color-mix(in srgb, var(--bg) 92%, transparent); backdrop-filter: blur(12px); }\n",
    );
    out.push_str(
        "    .topbar-inner { width: min(1180px, calc(100% - 32px)); margin: 0 auto; padding: 10px 0 6px; display: grid; grid-template-columns: minmax(160px, auto) minmax(240px, 1fr); gap: 16px; align-items: center; } .topbar-title { font-weight: 750; font-size: 1rem; white-space: nowrap; }\n",
    );
    out.push_str(
        "    .breadcrumbs { width: min(1180px, calc(100% - 32px)); margin: 0 auto; padding: 0 0 9px; color: var(--muted); font-size: .82rem; overflow-x: auto; scrollbar-width: thin; } .breadcrumbs ol { display: flex; align-items: center; gap: 0; min-height: 1.45rem; margin: 0; padding: 0; list-style: none; white-space: nowrap; } .breadcrumbs li { display: inline-flex; align-items: center; min-width: 0; } .breadcrumbs li + li::before { content: \"/\"; margin: 0 5px; color: color-mix(in srgb, var(--muted) 62%, transparent); } .breadcrumbs a { display: inline-block; max-width: 26ch; overflow: hidden; text-overflow: ellipsis; color: var(--muted); text-decoration: none; border-radius: 4px; padding: 1px 4px; } .breadcrumbs a:hover { color: var(--accent); background: var(--soft); } .breadcrumbs li:last-child a { color: var(--fg); font-weight: 650; }\n",
    );
    out.push_str(
        "    .layout { width: min(1180px, calc(100% - 32px)); margin: 28px auto 72px; display: grid; grid-template-columns: 180px minmax(0, 1fr); gap: 28px; align-items: start; } main { min-width: 0; } .side-nav { position: sticky; top: 76px; max-height: calc(100vh - 92px); overflow-y: auto; display: flex; flex-direction: column; gap: 3px; padding-left: 12px; padding-right: 8px; border-left: 2px solid var(--line); font-size: .9rem; scrollbar-width: thin; } .side-nav-title { margin: 0 0 8px; color: var(--muted); font-size: .72rem; font-weight: 750; text-transform: uppercase; letter-spacing: .08em; } .side-nav a { color: var(--muted); text-decoration: none; padding: 4px 0; } .side-nav a.nav-section { color: var(--fg); font-weight: 650; } .side-nav a.nav-command { padding-left: 12px; font-size: .86rem; } .side-nav a:hover { color: var(--accent); }\n",
    );
    out.push_str(
        "    header { max-width: 74ch; border-bottom: 1px solid var(--line); padding-bottom: 28px; margin-bottom: 30px; } h1 { margin: 0 0 8px; font-size: clamp(2rem, 4vw, 2.45rem); line-height: 1.08; letter-spacing: 0; } h2 { margin: 34px 0 14px; font-size: 1.02rem; line-height: 1.2; text-transform: uppercase; letter-spacing: .08em; color: var(--muted); scroll-margin-top: 118px; } h3 { margin: 20px 0 10px; font-size: 1.02rem; line-height: 1.35; scroll-margin-top: 118px; } section, details.schema-node, details.command-card, .config-schema-panel, .schema-node { scroll-margin-top: 118px; }\n",
    );
    out.push_str(
        "    p { margin: 0 0 .85rem; max-width: 72ch; } a { color: var(--accent); text-underline-offset: 2px; } .summary { font-size: 1.12rem; line-height: 1.55; color: var(--fg); } .description, .details, .meta, .name-meta { color: var(--muted); } .details p + p { margin-top: .95rem; } .meta, .name-meta { font-size: .9rem; line-height: 1.45; } .name-meta { display: block; margin-top: 7px; } .intro-copy { position: relative; max-height: 13.5rem; overflow: hidden; } .intro-copy.is-expanded { max-height: none; } .intro-copy:not(.is-expanded)::after { content: \"\"; position: absolute; inset: auto 0 0; height: 48px; background: linear-gradient(transparent, var(--bg)); pointer-events: none; } .intro-toggle { margin-top: 4px; padding: 6px 10px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--fg); font: inherit; cursor: pointer; } .markdown-body > :first-child { margin-top: 0; } .markdown-body p { margin-bottom: .85rem; } .markdown-body ul, .markdown-body ol { margin: 0 0 .9rem 1.4rem; padding: 0; } .markdown-body li { margin: .25rem 0; }\n",
    );
    out.push_str(
        "    .usage { display: block; padding: 14px 16px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); overflow-x: auto; }\n",
    );
    out.push_str(
        "    .search { width: 100%; padding: 10px 12px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--fg); font: inherit; }\n",
    );
    out.push_str(
        "    .search-hit { background: var(--search-bg); color: var(--search-fg); border-radius: 3px; box-shadow: 0 0 0 1px color-mix(in srgb, var(--search-bg) 70%, transparent); } .search-current { background: var(--search-current-bg); color: var(--search-current-fg); box-shadow: 0 0 0 2px color-mix(in srgb, var(--search-current-bg) 55%, transparent); }\n",
    );
    out.push_str(
        "    table { width: 100%; border-collapse: collapse; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; overflow: hidden; } th, td { padding: 10px 12px; border-bottom: 1px solid var(--line); text-align: left; vertical-align: top; } tr:last-child td { border-bottom: 0; } th { color: var(--muted); font-weight: 600; font-size: .88rem; }\n",
    );
    out.push_str(
        "    code { font: .93em/1.45 \"Maple Mono\", ui-monospace, SFMono-Regular, Consolas, \"Liberation Mono\", monospace; background: var(--code); color: var(--fg); border-radius: 4px; padding: 2px 5px; white-space: nowrap; vertical-align: .03em; } code.value-token { background: var(--value-bg); color: var(--value-fg); } code.env-token { background: var(--env-bg); color: var(--env-fg); } .names { width: 32%; } .empty { color: var(--muted); } .badge { display: inline-block; margin: 2px 4px 2px 0; padding: 2px 7px; border-radius: 999px; background: var(--soft); color: var(--accent); font-size: .82rem; font-weight: 600; }\n",
    );
    out.push_str(
        "    details.schema-node, details.command-card { margin: 10px 0; padding: 10px 12px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); } details.inline-schema { margin-top: 12px; } details.schema-node details.schema-node { margin-left: 18px; background: transparent; } summary { cursor: pointer; font-weight: 650; list-style: none; } summary::-webkit-details-marker { display: none; } details > summary::before { content: \"+\"; display: inline-grid; place-items: center; width: 1.1em; margin-right: 4px; color: var(--accent); font-weight: 750; } details[open] > summary::before { content: \"-\"; } details::details-content { block-size: 0; overflow: clip; transition: block-size .22s ease, content-visibility .22s allow-discrete; } details[open]::details-content { block-size: auto; } @media (prefers-reduced-motion: reduce) { details::details-content { transition: none; } } .node-body, .command-body { margin-top: 8px; padding-left: 2px; } .node-grid { display: grid; grid-template-columns: minmax(120px, 1fr) minmax(100px, auto); gap: 8px 16px; align-items: start; } .config-field { display: grid; grid-template-columns: minmax(0, 34%) minmax(0, 1fr); gap: 12px 20px; padding: 10px 0; border-bottom: 1px solid var(--line); } .config-field:last-child { border-bottom: 0; } .config-name, .config-desc { min-width: 0; } .config-name code { white-space: normal; overflow-wrap: anywhere; word-break: break-word; } .config-desc p:last-child { margin-bottom: 0; } .override-details { margin-top: 8px; color: var(--muted); font-size: .9rem; } .override-details summary { font-weight: 600; } .override-grid { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 5px 8px; align-items: center; margin-top: 6px; } .override-grid code { min-width: 0; white-space: normal; overflow-wrap: anywhere; word-break: break-word; } .copy-button { width: 1.55rem; height: 1.55rem; padding: 0; display: inline-grid; place-items: center; border: 1px solid var(--line); border-radius: 5px; background: var(--panel); color: var(--muted); cursor: pointer; } .copy-button svg { width: .9rem; height: .9rem; stroke: currentColor; } .copy-button:hover { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 45%, var(--line)); } .copy-button.is-copied { color: var(--accent); background: var(--soft); } .override-label { color: var(--muted); font-weight: 650; display: inline-flex; align-items: center; gap: 6px; } .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0,0,0,0); white-space: nowrap; border: 0; } .layer-examples { display: grid; gap: 4px; }\n",
    );
    out.push_str(
        "    .config-schema-panel { margin-top: 16px; padding: 14px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); }\n",
    );
    out.push_str(
        "    @media (max-width: 820px) { .topbar-inner { grid-template-columns: 1fr; } .layout { display: block; } .side-nav { display: none; } .names { width: 40%; } .config-field { grid-template-columns: 1fr; } }\n",
    );
    out.push_str("  </style>\n</head>\n<body>\n");
    out.push_str("<div class=\"topbar\"><div class=\"topbar-inner\"><div class=\"topbar-title\">");
    push_escaped(&mut out, doc.command);
    out.push_str("</div><input class=\"search\" type=\"search\" placeholder=\"Search flags, commands, config paths, defaults (Cmd/Ctrl-K)\" aria-label=\"Search help\"></div><nav class=\"breadcrumbs\" aria-label=\"Breadcrumb\"><ol data-breadcrumbs><li><a href=\"#top\">");
    push_escaped(&mut out, doc.command);
    out.push_str("</a></li></ol></nav></div>\n");
    out.push_str("<div class=\"layout\">\n");
    render_html_side_nav(&mut out, doc.args, doc.config_roots);
    out.push_str("<main id=\"top\">\n<header>\n");
    out.push_str("  <h1>");
    push_escaped(&mut out, doc.command);
    out.push_str("</h1>\n");

    if let Some(version) = doc.version {
        out.push_str("  <p class=\"description\">Version ");
        push_escaped(&mut out, version);
        out.push_str("</p>\n");
    }

    render_html_intro(&mut out, doc.docs, doc.description);

    out.push_str("</header>\n");
    render_html_usage(&mut out, doc.args, doc.config_roots, doc.command);
    render_html_command_sections(&mut out, doc.args, doc.command);
    render_html_layer_summary(&mut out, doc.config_roots, doc.config_file_extensions);
    render_html_arg_sections(
        &mut out,
        doc.args,
        doc.config_roots,
        doc.config_file_extensions,
    );
    if let Some(initial_anchor) = doc.initial_anchor {
        out.push_str("<script>window.FIGUE_INITIAL_ANCHOR = ");
        push_json_string(&mut out, initial_anchor);
        out.push_str(";</script>\n");
    }
    render_html_search_script(&mut out);
    out.push_str("</main>\n</div>\n</body>\n</html>\n");
    out
}

fn render_html_side_nav(
    out: &mut String,
    args: &ArgLevelSchema,
    config_roots: &[ConfigStructSchema],
) {
    out.push_str("<nav class=\"side-nav\" aria-label=\"Sections\">\n");
    out.push_str("  <p class=\"side-nav-title\">On This Page</p>\n");
    out.push_str("  <a class=\"nav-section\" href=\"#usage-heading\">Usage</a>\n");
    if args.has_subcommands() {
        out.push_str("  <a class=\"nav-section\" href=\"#commands-heading\">Commands</a>\n");
        for sub in args.subcommands().values() {
            out.push_str("  <a class=\"nav-command\" href=\"#");
            push_escaped(out, &command_heading_id(&[sub.cli_name()]));
            out.push_str("\">");
            push_escaped(out, sub.cli_name());
            out.push_str("</a>\n");
        }
    }
    if !config_roots.is_empty() {
        out.push_str("  <a class=\"nav-section\" href=\"#layers-heading\">Configuration</a>\n");
        for config_root in config_roots {
            let Some(name) = config_root.field_name() else {
                continue;
            };
            out.push_str("  <a class=\"nav-command\" href=\"#");
            push_escaped(out, &config_schema_heading_id(name));
            out.push_str("\">Config fields for ");
            push_escaped(out, name);
            out.push_str("</a>\n");
        }
    }
    out.push_str("  <a class=\"nav-section\" href=\"#options-heading\">Top-Level Options</a>\n");
    out.push_str("</nav>\n");
}

fn render_html_intro(out: &mut String, docs: &crate::schema::Docs, description: Option<&str>) {
    if docs.summary().is_none() && docs.details().is_none() && description.is_none() {
        return;
    }

    let mut markdown = String::new();
    if let Some(summary) = docs.summary() {
        markdown.push_str(summary.trim());
        markdown.push_str("\n\n");
    }

    if let Some(details) = docs.details() {
        markdown.push_str(details.trim());
        markdown.push_str("\n\n");
    }

    if let Some(description) = description {
        markdown.push_str(description.trim());
        markdown.push('\n');
    }

    out.push_str("  <div class=\"intro-copy markdown-body\" data-collapsible-intro data-markdown-target=\"intro\"></div>\n");
    out.push_str("  <script type=\"text/plain\" data-markdown-source=\"intro\">");
    push_script_text(out, &markdown);
    out.push_str("</script>\n");
    out.push_str(
        "  <button class=\"intro-toggle\" type=\"button\" data-intro-toggle>Show description</button>\n",
    );
}

fn render_html_usage(
    out: &mut String,
    args: &ArgLevelSchema,
    config_roots: &[ConfigStructSchema],
    command: &str,
) {
    out.push_str(
        "<section aria-labelledby=\"usage-heading\" data-breadcrumb-label=\"Usage\" data-breadcrumb-anchor=\"usage-heading\">\n",
    );
    out.push_str("  <h2 id=\"usage-heading\">Usage</h2>\n");
    out.push_str("  <code class=\"usage\">");
    push_escaped(out, command);

    let has_flags = args
        .args()
        .iter()
        .any(|(_, arg)| !arg.kind().is_positional())
        || !config_roots.is_empty();
    if has_flags {
        out.push_str(" [OPTIONS]");
    }

    for (_, arg) in args
        .args()
        .iter()
        .filter(|(_, arg)| arg.kind().is_positional())
    {
        let name = arg.name().to_uppercase();
        if arg.required() {
            out.push(' ');
            push_escaped(out, &format!("<{name}>"));
        } else {
            out.push(' ');
            push_escaped(out, &format!("[{name}]"));
        }
    }

    if args.has_subcommands() {
        if args.subcommand_optional() {
            out.push_str(" [COMMAND]");
        } else {
            push_escaped(out, " <COMMAND>");
        }
    }

    out.push_str("</code>\n</section>\n");
}

fn render_html_command_sections(out: &mut String, args: &ArgLevelSchema, command: &str) {
    if !args.has_subcommands() {
        return;
    }

    out.push_str(
        "<section aria-labelledby=\"commands-heading\" data-breadcrumb-label=\"Commands\" data-breadcrumb-anchor=\"commands-heading\">\n",
    );
    out.push_str("  <h2 id=\"commands-heading\">Commands</h2>\n");
    for sub in args.subcommands().values() {
        render_html_command_card(out, sub, &[sub.cli_name()], command);
    }
    out.push_str("</section>\n");
}

fn render_html_command_card(out: &mut String, sub: &Subcommand, path: &[&str], root_command: &str) {
    let heading_id = command_heading_id(path);
    out.push_str("<details class=\"command-card search-item\" data-breadcrumb-label=\"");
    push_escaped(out, sub.cli_name());
    out.push_str("\" data-breadcrumb-anchor=\"");
    push_escaped(out, &heading_id);
    out.push_str("\">\n<summary id=\"");
    push_escaped(out, &heading_id);
    out.push_str("\"><code>");
    push_escaped(out, sub.cli_name());
    out.push_str("</code>");
    if let Some(summary) = sub.docs().summary() {
        out.push_str(" <span class=\"meta\">");
        push_markdown(out, summary.trim());
        out.push_str("</span>");
    }
    out.push_str("</summary>\n<div class=\"command-body\">\n");
    out.push_str("<p class=\"meta\">Usage <code>");
    push_escaped(out, root_command);
    for segment in path {
        out.push(' ');
        push_escaped(out, segment);
    }
    render_html_command_usage_suffix(out, sub.args());
    out.push_str("</code></p>\n");

    render_html_arg_level_tables(out, sub.args(), "Arguments", "Options");

    if sub.args().has_subcommands() {
        out.push_str("<h3>Subcommands</h3>\n");
        for nested in sub.args().subcommands().values() {
            let mut nested_path = path.to_vec();
            nested_path.push(nested.cli_name());
            render_html_command_card(out, nested, &nested_path, root_command);
        }
    }

    out.push_str("</div>\n</details>\n");
}

fn render_html_command_usage_suffix(out: &mut String, args: &ArgLevelSchema) {
    let has_flags = args
        .args()
        .iter()
        .any(|(_, arg)| !arg.kind().is_positional());
    if has_flags {
        out.push_str(" [OPTIONS]");
    }

    for (_, arg) in args
        .args()
        .iter()
        .filter(|(_, arg)| arg.kind().is_positional())
    {
        let name = arg.name().to_uppercase();
        if arg.required() {
            out.push(' ');
            push_escaped(out, &format!("<{name}>"));
        } else {
            out.push(' ');
            push_escaped(out, &format!("[{name}]"));
        }
    }

    if args.has_subcommands() {
        if args.subcommand_optional() {
            out.push_str(" [COMMAND]");
        } else {
            push_escaped(out, " <COMMAND>");
        }
    }
}

fn render_html_arg_level_tables(
    out: &mut String,
    args: &ArgLevelSchema,
    positionals_title: &str,
    flags_title: &str,
) {
    let positionals: Vec<&ArgSchema> = args
        .args()
        .iter()
        .filter_map(|(_, arg)| arg.kind().is_positional().then_some(arg))
        .collect();
    let flags: Vec<&ArgSchema> = args
        .args()
        .iter()
        .filter_map(|(_, arg)| (!arg.kind().is_positional()).then_some(arg))
        .collect();

    if !positionals.is_empty() {
        out.push_str("<h3 data-breadcrumb-label=\"");
        push_escaped(out, positionals_title);
        out.push_str("\">");
        push_escaped(out, positionals_title);
        out.push_str("</h3>\n<table>\n  <thead><tr><th>Name</th><th>Description</th></tr></thead>\n  <tbody>\n");
        for arg in positionals {
            render_html_arg_row(out, arg);
        }
        out.push_str("  </tbody>\n</table>\n");
    }

    if !flags.is_empty() {
        out.push_str("<h3 data-breadcrumb-label=\"");
        push_escaped(out, flags_title);
        out.push_str("\">");
        push_escaped(out, flags_title);
        out.push_str("</h3>\n<table>\n  <thead><tr><th>Name</th><th>Description</th></tr></thead>\n  <tbody>\n");
        for arg in flags {
            render_html_arg_row(out, arg);
        }
        out.push_str("  </tbody>\n</table>\n");
    }
}

fn render_html_layer_summary(
    out: &mut String,
    config_roots: &[ConfigStructSchema],
    config_file_extensions: &[&str],
) {
    if config_roots.is_empty() {
        return;
    }

    out.push_str("<section aria-labelledby=\"layers-heading\" class=\"search-item\" data-breadcrumb-label=\"Configuration\" data-breadcrumb-anchor=\"layers-heading\">\n");
    out.push_str("  <h2 id=\"layers-heading\">Configuration</h2>\n");
    out.push_str("  <p>When a field is set in more than one place, precedence is <span class=\"badge\">CLI</span> &gt; <span class=\"badge\">Environment</span> &gt; <span class=\"badge\">Config file</span> &gt; <span class=\"badge\">Defaults</span>.</p>\n");
    out.push_str("  <p class=\"description\">Supported file formats: ");
    push_escaped(out, &config_file_extension_list(config_file_extensions));
    out.push_str(".</p>\n</section>\n");
}

fn render_html_arg_sections(
    out: &mut String,
    args: &ArgLevelSchema,
    config_roots: &[ConfigStructSchema],
    config_file_extensions: &[&str],
) {
    let positionals: Vec<&ArgSchema> = args
        .args()
        .iter()
        .filter_map(|(_, arg)| arg.kind().is_positional().then_some(arg))
        .collect();
    let flags: Vec<&ArgSchema> = args
        .args()
        .iter()
        .filter_map(|(_, arg)| (!arg.kind().is_positional()).then_some(arg))
        .collect();

    if !positionals.is_empty() {
        render_html_table_start(out, "arguments", "Arguments");
        for arg in positionals {
            render_html_arg_row(out, arg);
        }
        out.push_str("  </tbody>\n</table>\n</section>\n");
    }

    if !flags.is_empty() || !config_roots.is_empty() {
        render_html_table_start(out, "options", "Top-Level Options");
        for arg in flags {
            render_html_arg_row(out, arg);
        }
        for config_root in config_roots {
            render_html_config_rows(out, config_root, config_file_extensions);
        }
        out.push_str("  </tbody>\n</table>\n");
        for config_root in config_roots {
            render_html_config_schema_panel(out, config_root);
        }
        out.push_str("</section>\n");
    }
}

fn render_html_table_start(out: &mut String, id: &str, title: &str) {
    out.push_str("<section aria-labelledby=\"");
    push_escaped(out, id);
    out.push_str("-heading\" data-breadcrumb-label=\"");
    push_escaped(out, title);
    out.push_str("\" data-breadcrumb-anchor=\"");
    push_escaped(out, id);
    out.push_str("-heading\">\n  <h2 id=\"");
    push_escaped(out, id);
    out.push_str("-heading\">");
    push_escaped(out, title);
    out.push_str(
        "</h2>\n<table>\n  <thead><tr><th>Name</th><th>Description</th></tr></thead>\n  <tbody>\n",
    );
}

fn render_html_arg_row(out: &mut String, arg: &ArgSchema) {
    out.push_str("    <tr class=\"search-item\"><td class=\"names\">");
    for (idx, part) in arg_help_names(arg).iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        render_code_tokens(out, part);
    }
    render_arg_name_meta(out, arg);
    out.push_str("</td><td>");
    if arg.docs().summary().is_some() || arg.docs().details().is_some() {
        render_html_docs(out, arg.docs());
    } else {
        out.push_str("<span class=\"empty\">No description.</span>");
    }
    if arg.kind().is_counted() {
        out.push_str("<p class=\"description\">can be repeated</p>");
    }
    out.push_str("</td></tr>\n");
}

fn render_arg_name_meta(out: &mut String, arg: &ArgSchema) {
    let hide_false_bool_default = arg.value().inner_if_option().is_bool()
        && arg.default().map(config_value_summary).as_deref() == Some("false");
    let has_enum_values = arg.value().inner_if_option().enum_variants().is_some();

    if hide_false_bool_default && !has_enum_values {
        return;
    }

    out.push_str("<span class=\"name-meta\">");
    if let Some(default) = arg.default().filter(|_| !hide_false_bool_default) {
        out.push_str("Default ");
        out.push_str("<code>");
        push_escaped(out, &config_value_summary(default));
        out.push_str("</code>");
    } else if arg.required() {
        out.push_str("Required");
    } else if !arg.kind().is_positional() && !arg.kind().is_counted() && !arg.value().is_bool() {
        out.push_str("Optional value");
    } else {
        out.push_str("Optional");
    }
    if let Some(variants) = arg.value().inner_if_option().enum_variants() {
        out.push_str("<br>Values ");
        for variant in variants {
            out.push_str("<code class=\"value-token\">");
            push_escaped(out, variant);
            out.push_str("</code> ");
        }
    }
    out.push_str("</span>");
}

fn arg_help_names(arg: &ArgSchema) -> Vec<String> {
    if arg.kind().is_positional() {
        return vec![format!("<{}>", arg.name().to_uppercase())];
    }

    let mut names = Vec::new();
    if let Some(c) = arg.kind().short() {
        names.push(format!("-{c},"));
    }

    let is_bool = arg.value().inner_if_option().is_bool();
    let is_counted = arg.kind().is_counted();
    let mut long = if is_bool {
        if arg.default().map(config_value_summary).as_deref() == Some("false") {
            format!("--{}", arg.name().to_kebab_case())
        } else {
            format!("--[no-]{}", arg.name().to_kebab_case())
        }
    } else {
        format!("--{}", arg.name().to_kebab_case())
    };

    if !is_counted && !arg.value().is_bool() {
        let placeholder = if let Some(label) = arg.label() {
            Some(label.to_uppercase())
        } else if arg.value().inner_if_option().enum_variants().is_some() {
            None
        } else {
            Some(arg.value().type_identifier().to_uppercase())
        };
        if let Some(placeholder) = placeholder {
            long.push_str(&format!(" <{placeholder}>"));
        }
    }
    names.push(long);
    names
}

fn render_code_tokens(out: &mut String, text: &str) {
    for (idx, token) in text.split_whitespace().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        if token.starts_with('-') {
            out.push_str("<code>");
        } else {
            out.push_str("<code class=\"value-token\">");
        }
        push_escaped(out, token);
        out.push_str("</code>");
    }
}

fn command_heading_id(path: &[&str]) -> String {
    let mut id = "command".to_string();
    for segment in path {
        id.push('-');
        id.push_str(&segment.to_kebab_case());
    }
    id
}

fn config_schema_heading_id(root_name: &str) -> String {
    format!("config-fields-{}", root_name.to_kebab_case())
}

fn config_node_id(path: &str) -> String {
    let mut id = String::from("config-node-");
    let mut last_was_dash = true;
    for c in path.chars() {
        let next = if c.is_ascii_alphanumeric() {
            Some(c.to_ascii_lowercase())
        } else if matches!(c, '-' | '_' | '.' | '<' | '>') {
            Some('-')
        } else {
            None
        };

        let Some(next) = next else {
            continue;
        };
        if next == '-' {
            if last_was_dash {
                continue;
            }
            last_was_dash = true;
        } else {
            last_was_dash = false;
        }
        id.push(next);
    }
    id.trim_end_matches('-').to_string()
}

fn config_file_example(root_name: &str, extensions: &[&str]) -> String {
    let extension = extensions
        .iter()
        .copied()
        .find(|extension| *extension == "jsonc")
        .or_else(|| extensions.first().copied())
        .unwrap_or("json");
    format!("{}.{}", root_name.to_kebab_case(), extension)
}

fn render_html_config_rows(
    out: &mut String,
    config_root: &ConfigStructSchema,
    config_file_extensions: &[&str],
) {
    let Some(name) = config_root.field_name() else {
        return;
    };

    let config_flag = format!("--{}", name.to_kebab_case());
    out.push_str("    <tr class=\"search-item\"><td class=\"names\">");
    render_code_tokens(
        out,
        &format!(
            "{config_flag} {}",
            config_file_example(name, config_file_extensions)
        ),
    );
    out.push_str("</td><td>");
    push_markdown(
        out,
        config_root
            .docs()
            .summary()
            .unwrap_or("Load configuration values from a file."),
    );
    out.push_str("<p class=\"description\">");
    push_escaped(out, &format_config_file_extensions(config_file_extensions));
    out.push_str("</p><p class=\"description\"><a href=\"#");
    push_escaped(out, &config_schema_heading_id(name));
    out.push_str("\">View config fields.</a></p>");
    out.push_str("</td></tr>\n");
}

fn render_html_config_schema_panel(out: &mut String, config_root: &ConfigStructSchema) {
    let Some(root_path) = config_root.field_name() else {
        return;
    };

    out.push_str("<div class=\"config-schema-panel search-item\" data-breadcrumb-label=\"");
    push_escaped(out, &format!("Config fields for {root_path}"));
    out.push_str("\" data-breadcrumb-anchor=\"");
    push_escaped(out, &config_schema_heading_id(root_path));
    out.push_str("\">\n");
    out.push_str("<h3 id=\"");
    push_escaped(out, &config_schema_heading_id(root_path));
    out.push_str("\">Config fields for <code>");
    push_escaped(out, root_path);
    out.push_str("</code></h3>\n<div class=\"node-body\">\n");

    if let Some(env_prefix) = config_root.env_prefix() {
        out.push_str("<p class=\"meta\">Environment prefix: <code class=\"env-token\">");
        push_escaped(out, env_prefix);
        out.push_str("</code></p>\n");
    }

    render_html_config_struct_children(out, config_root, root_path, 1, config_root.env_prefix());

    out.push_str("</div>\n</div>\n");
}

fn render_html_config_struct_node(
    out: &mut String,
    config_struct: &ConfigStructSchema,
    display_name: Option<&str>,
    path: &str,
    depth: usize,
    env_prefix: Option<&str>,
) {
    let name = display_name.unwrap_or("config");
    let node_id = config_node_id(path);
    out.push_str("<details id=\"");
    push_escaped(out, &node_id);
    out.push_str("\" class=\"schema-node search-item\" data-breadcrumb-label=\"");
    push_escaped(out, name);
    out.push_str("\" data-breadcrumb-anchor=\"");
    push_escaped(out, &node_id);
    out.push_str("\">\n<summary><code>");
    push_escaped(out, name);
    out.push_str("</code> <span class=\"meta\">struct ");
    push_escaped(out, config_struct.shape().type_identifier);
    out.push_str("</span></summary>\n<div class=\"node-body\">\n");

    if let Some(summary) = config_struct.docs().summary() {
        out.push_str("<p>");
        push_markdown(out, summary.trim());
        out.push_str("</p>\n");
    }

    if depth == 0
        && let Some(env_prefix) = config_struct.env_prefix()
    {
        out.push_str("<p class=\"meta\">Environment prefix: <code class=\"env-token\">");
        push_escaped(out, env_prefix);
        out.push_str("</code></p>\n");
    }

    render_html_config_struct_children(out, config_struct, path, depth + 1, env_prefix);

    out.push_str("</div>\n</details>\n");
}

fn render_html_config_struct_children(
    out: &mut String,
    config_struct: &ConfigStructSchema,
    path: &str,
    depth: usize,
    env_prefix: Option<&str>,
) {
    render_html_config_children(
        out,
        config_struct.fields(),
        config_struct.field_groups(),
        path,
        path,
        depth,
        env_prefix,
    );
}

fn render_html_config_group_children(
    out: &mut String,
    group: &ConfigFieldGroupSchema,
    config_path: &str,
    group_path: &str,
    depth: usize,
    env_prefix: Option<&str>,
) {
    render_html_config_children(
        out,
        group.fields(),
        group.field_groups(),
        config_path,
        group_path,
        depth,
        env_prefix,
    );
}

fn render_html_config_children(
    out: &mut String,
    fields: &indexmap::IndexMap<String, ConfigFieldSchema, std::hash::RandomState>,
    groups: &[ConfigFieldGroupSchema],
    config_path: &str,
    group_path: &str,
    depth: usize,
    env_prefix: Option<&str>,
) {
    for (field_name, field) in fields {
        if config_field_is_grouped(groups, field_name) {
            continue;
        }
        let child_path = join_path(config_path, field_name);
        render_html_config_field_node(out, field_name, &child_path, field, depth, env_prefix);
    }

    for group in groups {
        let child_group_path = join_path(&join_path(group_path, "__group"), group.name());
        render_html_config_field_group(
            out,
            group,
            config_path,
            &child_group_path,
            depth,
            env_prefix,
        );
    }
}

fn config_field_is_grouped(groups: &[ConfigFieldGroupSchema], field_name: &str) -> bool {
    groups
        .iter()
        .any(|group| group.fields().contains_key(field_name))
}

fn render_html_config_field_group(
    out: &mut String,
    group: &ConfigFieldGroupSchema,
    config_path: &str,
    group_path: &str,
    depth: usize,
    env_prefix: Option<&str>,
) {
    let group_id = config_node_id(group_path);
    out.push_str("<details id=\"");
    push_escaped(out, &group_id);
    out.push_str("\" class=\"schema-node search-item\" data-breadcrumb-label=\"");
    push_escaped(out, group.name());
    out.push_str("\" data-breadcrumb-anchor=\"");
    push_escaped(out, &group_id);
    out.push_str("\">\n<summary><code>");
    push_escaped(out, group.name());
    out.push_str(
        "</code> <span class=\"meta\">group</span></summary>\n<div class=\"node-body\">\n",
    );
    render_html_docs(out, group.docs());
    render_html_config_group_children(out, group, config_path, group_path, depth + 1, env_prefix);
    out.push_str("</div>\n</details>\n");
}

fn render_html_config_field_node(
    out: &mut String,
    field_name: &str,
    path: &str,
    field: &ConfigFieldSchema,
    depth: usize,
    env_prefix: Option<&str>,
) {
    match field.value().inner_if_option() {
        ConfigValueSchema::Struct(config_struct) => {
            render_html_config_struct_node(
                out,
                config_struct,
                Some(field_name),
                path,
                depth,
                env_prefix,
            );
        }
        ConfigValueSchema::Vec(vec_schema) => {
            let node_id = config_node_id(path);
            out.push_str("<details id=\"");
            push_escaped(out, &node_id);
            out.push_str("\" class=\"schema-node search-item\" data-breadcrumb-label=\"");
            push_escaped(out, field_name);
            out.push_str("\" data-breadcrumb-anchor=\"");
            push_escaped(out, &node_id);
            out.push_str("\">\n<summary><code>");
            push_escaped(out, field_name);
            out.push_str("</code> <span class=\"meta\">list</span>");
            render_default_summary(out, field.default());
            out.push_str("</summary>\n<div class=\"node-body\">\n");
            render_config_field_docs(out, field);
            render_config_override_details(out, path, env_prefix);
            let item_path = join_path(path, "<INDEX>");
            render_html_config_value_node(
                out,
                "<INDEX>",
                &item_path,
                vec_schema.element(),
                depth,
                env_prefix,
            );
            out.push_str("</div>\n</details>\n");
        }
        ConfigValueSchema::Enum(enum_schema) => {
            let node_id = config_node_id(path);
            out.push_str("<details id=\"");
            push_escaped(out, &node_id);
            out.push_str("\" class=\"schema-node search-item\" data-breadcrumb-label=\"");
            push_escaped(out, field_name);
            out.push_str("\" data-breadcrumb-anchor=\"");
            push_escaped(out, &node_id);
            out.push_str("\">\n<summary><code>");
            push_escaped(out, field_name);
            out.push_str("</code> <span class=\"meta\">enum ");
            push_escaped(out, field.value().type_identifier());
            out.push_str("</span>");
            render_default_summary(out, field.default());
            out.push_str("</summary>\n<div class=\"node-body\">\n");
            render_config_field_docs(out, field);
            render_config_override_details(out, path, env_prefix);
            for (variant_name, variant) in enum_schema.variants() {
                let variant_path = join_path(path, variant_name);
                let variant_id = config_node_id(&variant_path);
                out.push_str("<details id=\"");
                push_escaped(out, &variant_id);
                out.push_str("\" class=\"schema-node search-item\" data-breadcrumb-label=\"");
                push_escaped(out, variant_name);
                out.push_str("\" data-breadcrumb-anchor=\"");
                push_escaped(out, &variant_id);
                out.push_str("\">\n<summary><code>");
                push_escaped(out, variant_name);
                out.push_str("</code> <span class=\"meta\">variant</span></summary>\n<div class=\"node-body\">\n");
                if let Some(summary) = variant.docs().summary() {
                    out.push_str("<p>");
                    push_markdown(out, summary.trim());
                    out.push_str("</p>\n");
                }
                if variant.fields().is_empty() {
                    out.push_str("<p class=\"meta\">Unit variant.</p>\n");
                }
                for (variant_field_name, variant_field) in variant.fields() {
                    let variant_path = join_path(&variant_path, variant_field_name);
                    render_html_config_field_node(
                        out,
                        variant_field_name,
                        &variant_path,
                        variant_field,
                        depth + 1,
                        env_prefix,
                    );
                }
                out.push_str("</div>\n</details>\n");
            }
            out.push_str("</div>\n</details>\n");
        }
        ConfigValueSchema::Leaf(_) => {
            render_html_config_leaf_node(out, field_name, path, field, env_prefix)
        }
        ConfigValueSchema::Option { .. } => unreachable!("inner_if_option removes Option wrappers"),
    }
}

fn render_html_config_value_node(
    out: &mut String,
    name: &str,
    path: &str,
    value: &ConfigValueSchema,
    depth: usize,
    env_prefix: Option<&str>,
) {
    match value.inner_if_option() {
        ConfigValueSchema::Struct(config_struct) => {
            render_html_config_struct_node(
                out,
                config_struct,
                Some(name),
                path,
                depth + 1,
                env_prefix,
            );
        }
        ConfigValueSchema::Vec(vec_schema) => {
            let item_path = join_path(path, "<INDEX>");
            render_html_config_value_node(
                out,
                "<INDEX>",
                &item_path,
                vec_schema.element(),
                depth + 1,
                env_prefix,
            );
        }
        ConfigValueSchema::Enum(enum_schema) => {
            let node_id = config_node_id(path);
            out.push_str("<details id=\"");
            push_escaped(out, &node_id);
            out.push_str("\" class=\"schema-node search-item\" data-breadcrumb-label=\"");
            push_escaped(out, name);
            out.push_str("\" data-breadcrumb-anchor=\"");
            push_escaped(out, &node_id);
            out.push_str("\">\n<summary><code>");
            push_escaped(out, name);
            out.push_str("</code> <span class=\"meta\">enum ");
            push_escaped(out, value.type_identifier());
            out.push_str("</span></summary>\n<div class=\"node-body\">\n");
            render_config_override_details(out, path, env_prefix);
            for variant_name in enum_schema.variants().keys() {
                out.push_str("<p><code>");
                push_escaped(out, variant_name);
                out.push_str("</code></p>\n");
            }
            out.push_str("</div>\n</details>\n");
        }
        ConfigValueSchema::Leaf(_) => {
            let node_id = config_node_id(path);
            out.push_str("<div id=\"");
            push_escaped(out, &node_id);
            out.push_str("\" class=\"schema-node search-item node-grid\" data-breadcrumb-label=\"");
            push_escaped(out, name);
            out.push_str("\" data-breadcrumb-anchor=\"");
            push_escaped(out, &node_id);
            out.push_str("\"><div><code>");
            push_escaped(out, name);
            out.push_str("</code> <span class=\"meta\">");
            push_escaped(out, value.type_identifier());
            out.push_str("</span>");
            render_config_override_details(out, path, env_prefix);
            out.push_str("</div></div>\n");
        }
        ConfigValueSchema::Option { .. } => unreachable!("inner_if_option removes Option wrappers"),
    }
}

fn render_html_config_leaf_node(
    out: &mut String,
    field_name: &str,
    path: &str,
    field: &ConfigFieldSchema,
    env_prefix: Option<&str>,
) {
    let node_id = config_node_id(path);
    out.push_str("<div id=\"");
    push_escaped(out, &node_id);
    out.push_str("\" class=\"schema-node search-item config-field\" data-breadcrumb-label=\"");
    push_escaped(out, field_name);
    out.push_str("\" data-breadcrumb-anchor=\"");
    push_escaped(out, &node_id);
    out.push_str("\"><div class=\"config-name\"><code>");
    push_escaped(out, field_name);
    out.push_str("</code> <span class=\"meta\">");
    push_escaped(out, field.value().type_identifier());
    out.push_str("</span>");
    render_config_default_meta(out, field.value(), field.default());
    out.push_str("</div><div class=\"config-desc\">");
    render_config_field_docs(out, field);
    render_config_override_details(out, path, env_prefix);
    out.push_str("</div></div>\n");
}

fn render_config_field_docs(out: &mut String, field: &ConfigFieldSchema) {
    if field.docs().summary().is_some() || field.docs().details().is_some() {
        render_html_docs(out, field.docs());
    }
}

fn render_config_override_details(out: &mut String, path: &str, env_prefix: Option<&str>) {
    out.push_str("<details class=\"override-details\"><summary>Overrides</summary>\n");
    out.push_str("<div class=\"override-grid\">\n");
    let cli_flag = format!("--{path}");
    out.push_str("<span class=\"override-label\">");
    render_copy_button(out, &cli_flag);
    out.push_str("CLI</span><code>");
    push_escaped(out, &cli_flag);
    out.push_str("</code>\n");
    if let Some(env_prefix) = env_prefix {
        let env_name = env_override_name(env_prefix, path);
        out.push_str("<span class=\"override-label\">");
        render_copy_button(out, &env_name);
        out.push_str("Env</span><code class=\"env-token\">");
        push_escaped(out, &env_name);
        out.push_str("</code>\n");
    }
    out.push_str("</div>\n</details>\n");
}

fn render_copy_button(out: &mut String, value: &str) {
    out.push_str("<button class=\"copy-button\" type=\"button\" data-copy=\"");
    push_escaped(out, value);
    out.push_str("\" aria-label=\"Copy ");
    push_escaped(out, value);
    out.push_str("\"><svg viewBox=\"0 0 24 24\" fill=\"none\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><rect x=\"9\" y=\"9\" width=\"11\" height=\"11\" rx=\"2\"></rect><path d=\"M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1\"></path></svg><span class=\"sr-only\">Copy ");
    push_escaped(out, value);
    out.push_str("</span></button>");
}

fn render_config_default_meta(
    out: &mut String,
    value: &ConfigValueSchema,
    default: Option<&crate::config_value::ConfigValue>,
) {
    let false_bool_default =
        value.is_bool() && default.map(config_value_summary).as_deref() == Some("false");
    if false_bool_default || (default.is_none() && value.is_bool()) {
        return;
    }

    out.push_str("<span class=\"name-meta\">");
    if let Some(default) = default {
        out.push_str("Default ");
        out.push_str("<code class=\"value-token\">");
        push_escaped(out, &config_value_summary(default));
        out.push_str("</code>");
    } else if value.is_option() {
        out.push_str("Optional");
    } else {
        out.push_str("Required");
    }
    out.push_str("</span>");
}

fn env_override_name(env_prefix: &str, path: &str) -> String {
    let mut parts = path.split('.');
    let _root = parts.next();
    let env_path = parts
        .map(|segment| {
            segment
                .replace('-', "_")
                .replace("<INDEX>", "INDEX")
                .to_uppercase()
        })
        .collect::<Vec<_>>()
        .join("__");
    if env_path.is_empty() {
        env_prefix.to_string()
    } else {
        format!("{env_prefix}__{env_path}")
    }
}

fn render_default_summary(out: &mut String, default: Option<&crate::config_value::ConfigValue>) {
    if let Some(default) = default {
        out.push_str(" <span class=\"meta\">default ");
        out.push_str("<code>");
        push_escaped(out, &config_value_summary(default));
        out.push_str("</code></span>");
    }
}

fn config_value_summary(value: &crate::config_value::ConfigValue) -> String {
    match value {
        crate::config_value::ConfigValue::Null(_) => "null".to_string(),
        crate::config_value::ConfigValue::Bool(s) => s.value.to_string(),
        crate::config_value::ConfigValue::Integer(s) => s.value.to_string(),
        crate::config_value::ConfigValue::Float(s) => s.value.to_string(),
        crate::config_value::ConfigValue::String(s) => s.value.clone(),
        crate::config_value::ConfigValue::Array(s) => {
            format!("list[{}]", s.value.len())
        }
        crate::config_value::ConfigValue::Object(s) => {
            format!("object{{{} fields}}", s.value.len())
        }
        crate::config_value::ConfigValue::Enum(s) => s.value.variant.clone(),
    }
}

fn join_path(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_kebab_case()
    } else {
        format!("{prefix}.{}", segment.to_kebab_case())
    }
}

fn render_html_search_script(out: &mut String) {
    out.push_str(
        r##"<script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js"></script>
<script>
for (const source of document.querySelectorAll('[data-markdown-source]')) {
  const key = source.getAttribute('data-markdown-source');
  const target = document.querySelector(`[data-markdown-target="${key}"]`);
  if (!target) {
    continue;
  }
  if (window.marked) {
    target.innerHTML = marked.parse(source.textContent);
  } else {
    target.textContent = source.textContent;
  }
}

const searchInput = document.querySelector('.search');
const searchableRoot = document.querySelector('main');
const intro = document.querySelector('[data-collapsible-intro]');
const introToggle = document.querySelector('[data-intro-toggle]');
const breadcrumbList = document.querySelector('[data-breadcrumbs]');
const breadcrumbHome = document.querySelector('.topbar-title')?.textContent?.trim() || document.title || 'Home';
let searchHits = [];
let currentHit = -1;
let revealingSearchHit = false;
let breadcrumbFrame = 0;

introToggle?.addEventListener('click', () => {
  intro?.classList.toggle('is-expanded');
  introToggle.textContent = intro?.classList.contains('is-expanded')
    ? 'Collapse description'
    : 'Show description';
});

document.addEventListener('keydown', event => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault();
    searchInput?.focus();
    searchInput?.select();
  }
});

searchInput?.addEventListener('input', () => {
  updateSearch(searchInput.value);
});

searchInput?.addEventListener('keydown', event => {
  if (event.key === 'Enter') {
    event.preventDefault();
    moveSearch(event.shiftKey ? -1 : 1);
  }
});

document.addEventListener('scroll', scheduleBreadcrumbUpdate, { passive: true });
window.addEventListener('resize', scheduleBreadcrumbUpdate);
breadcrumbList?.addEventListener('click', event => {
  const link = event.target.closest('a[href^="#"]');
  if (!link) {
    return;
  }
  const id = decodeURIComponent(link.getAttribute('href').slice(1));
  const target = document.getElementById(id);
  if (!target) {
    return;
  }
  event.preventDefault();
  revealBreadcrumbTarget(target);
  target.scrollIntoView({ behavior: 'smooth', block: 'start', inline: 'nearest' });
  history.replaceState(null, '', `#${encodeURIComponent(id)}`);
});

for (const details of document.querySelectorAll('details')) {
  details.addEventListener('toggle', () => {
    scheduleBreadcrumbUpdate();
    if (!details.open || revealingSearchHit) {
      return;
    }
    window.setTimeout(() => {
      keepOpenedDetailsComfortable(details);
    }, 280);
  });
}

revealInitialAnchor();
scheduleBreadcrumbUpdate();

function revealInitialAnchor() {
  const initialAnchor = window.FIGUE_INITIAL_ANCHOR || (window.location.hash ? decodeURIComponent(window.location.hash.slice(1)) : '');
  if (!initialAnchor) {
    return;
  }
  const target = document.getElementById(initialAnchor);
  if (!target) {
    return;
  }
  revealBreadcrumbTarget(target);
  requestAnimationFrame(() => {
    target.scrollIntoView({ behavior: 'smooth', block: 'start', inline: 'nearest' });
    history.replaceState(null, '', `#${encodeURIComponent(initialAnchor)}`);
  });
}

function scheduleBreadcrumbUpdate() {
  if (breadcrumbFrame) {
    return;
  }
  breadcrumbFrame = requestAnimationFrame(() => {
    breadcrumbFrame = 0;
    updateBreadcrumbs();
  });
}

function updateBreadcrumbs() {
  if (!breadcrumbList) {
    return;
  }

  const active = currentBreadcrumbTarget();
  const trail = breadcrumbTrail(active);
  breadcrumbList.replaceChildren();
  appendBreadcrumbCrumb(breadcrumbList, breadcrumbHome, 'top');
  for (const item of trail) {
    appendBreadcrumbCrumb(
      breadcrumbList,
      item.getAttribute('data-breadcrumb-label'),
      item.getAttribute('data-breadcrumb-anchor')
    );
  }
}

function currentBreadcrumbTarget() {
  const items = [...document.querySelectorAll('[data-breadcrumb-label][data-breadcrumb-anchor]')]
    .filter(item => item.getClientRects().length > 0);
  if (items.length === 0) {
    return null;
  }

  const topbarBottom = document.querySelector('.topbar')?.getBoundingClientRect().bottom || 0;
  const activationLine = topbarBottom + 16;
  let active = items[0];
  for (const item of items) {
    const rect = item.getBoundingClientRect();
    if (rect.top <= activationLine) {
      active = item;
    } else {
      break;
    }
  }
  return active;
}

function breadcrumbTrail(active) {
  const trail = [];
  let node = active;
  while (node) {
    if (
      node.matches?.('[data-breadcrumb-label][data-breadcrumb-anchor]') &&
      node.getAttribute('data-breadcrumb-label') &&
      node.getAttribute('data-breadcrumb-anchor')
    ) {
      trail.push(node);
    }
    node = node.parentElement?.closest('[data-breadcrumb-label][data-breadcrumb-anchor]');
  }
  return trail.reverse();
}

function appendBreadcrumbCrumb(list, label, anchor) {
  if (!label || !anchor) {
    return;
  }
  const li = document.createElement('li');
  const link = document.createElement('a');
  link.href = `#${anchor}`;
  link.textContent = label;
  li.append(link);
  list.append(li);
}

function revealBreadcrumbTarget(target) {
  revealingSearchHit = true;
  let details = target.closest('details');
  while (details) {
    details.open = true;
    details = details.parentElement?.closest('details');
  }
  requestAnimationFrame(() => {
    revealingSearchHit = false;
    scheduleBreadcrumbUpdate();
  });
}

function keepOpenedDetailsComfortable(details) {
  const rect = details.getBoundingClientRect();
  const topbarBottom = document.querySelector('.topbar')?.getBoundingClientRect().bottom || 0;
  const padding = 40;
  const visibleTop = topbarBottom + padding;
  const visibleBottom = window.innerHeight - padding;

  if (rect.top >= visibleTop && rect.bottom <= visibleBottom) {
    return;
  }

  const capacity = visibleBottom - visibleTop;
  let delta = 0;

  if (rect.height > capacity) {
    delta = rect.top - visibleTop;
  } else if (rect.top < visibleTop) {
    delta = rect.top - visibleTop;
  } else if (rect.bottom > visibleBottom) {
    delta = rect.bottom - visibleBottom;
  }

  if (Math.abs(delta) > 1) {
    window.scrollBy({ top: delta, behavior: 'smooth' });
  }
}

document.addEventListener('click', async event => {
  const button = event.target.closest('[data-copy]');
  if (!button) {
    return;
  }
  event.preventDefault();
  const value = button.getAttribute('data-copy');
  if (!value) {
    return;
  }
  try {
    await navigator.clipboard.writeText(value);
    button.classList.add('is-copied');
    button.setAttribute('aria-label', `Copied ${value}`);
    window.setTimeout(() => {
      button.classList.remove('is-copied');
      button.setAttribute('aria-label', `Copy ${value}`);
    }, 1200);
  } catch {
    button.setAttribute('aria-label', `Failed to copy ${value}`);
    window.setTimeout(() => {
      button.setAttribute('aria-label', `Copy ${value}`);
    }, 1200);
  }
});

function updateSearch(rawQuery) {
  clearSearchHighlights();
  const query = rawQuery.trim();
  if (!query || !searchableRoot) {
    return;
  }

  const pattern = new RegExp(escapeRegExp(query), 'gi');
  const walker = document.createTreeWalker(
    searchableRoot,
    NodeFilter.SHOW_TEXT,
    {
      acceptNode(node) {
        if (!node.nodeValue.trim()) {
          return NodeFilter.FILTER_REJECT;
        }
        const parent = node.parentElement;
        if (!parent || parent.closest('header, script, style, textarea, input, .override-details, mark.search-hit')) {
          return NodeFilter.FILTER_REJECT;
        }
        pattern.lastIndex = 0;
        return pattern.test(node.nodeValue)
          ? NodeFilter.FILTER_ACCEPT
          : NodeFilter.FILTER_REJECT;
      },
    }
  );

  const nodes = [];
  while (walker.nextNode()) {
    nodes.push(walker.currentNode);
  }

  for (const node of nodes) {
    highlightTextNode(node, pattern);
  }

  if (searchHits.length > 0) {
    currentHit = 0;
    scrollToSearchHit(currentHit);
  }
}

function clearSearchHighlights() {
  for (const mark of Array.from(document.querySelectorAll('mark.search-hit'))) {
    mark.replaceWith(document.createTextNode(mark.textContent));
  }
  searchableRoot?.normalize();
  searchHits = [];
  currentHit = -1;
}

function highlightTextNode(node, pattern) {
  pattern.lastIndex = 0;
  const text = node.nodeValue;
  const fragment = document.createDocumentFragment();
  let cursor = 0;
  let match;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > cursor) {
      fragment.append(document.createTextNode(text.slice(cursor, match.index)));
    }
    const mark = document.createElement('mark');
    mark.className = 'search-hit';
    mark.textContent = match[0];
    fragment.append(mark);
    searchHits.push(mark);
    cursor = match.index + match[0].length;
  }

  if (cursor < text.length) {
    fragment.append(document.createTextNode(text.slice(cursor)));
  }
  node.replaceWith(fragment);
}

function moveSearch(delta) {
  if (searchHits.length === 0) {
    updateSearch(searchInput?.value || '');
  }
  if (searchHits.length === 0) {
    return;
  }
  currentHit = (currentHit + delta + searchHits.length) % searchHits.length;
  scrollToSearchHit(currentHit);
}

function scrollToSearchHit(index) {
  const hit = searchHits[index];
  if (!hit) {
    return;
  }
  for (const mark of searchHits) {
    mark.classList.remove('search-current');
  }
  hit.classList.add('search-current');
  revealSearchHit(hit);
  hit.scrollIntoView({ behavior: 'smooth', block: 'center', inline: 'nearest' });
}

function revealSearchHit(hit) {
  revealingSearchHit = true;
  let details = hit.parentElement?.closest('details');
  while (details) {
    details.open = true;
    details = details.parentElement?.closest('details');
  }
  requestAnimationFrame(() => {
    revealingSearchHit = false;
  });
  const collapsedIntro = hit.closest('[data-collapsible-intro]');
  if (collapsedIntro && !collapsedIntro.classList.contains('is-expanded')) {
    collapsedIntro.classList.add('is-expanded');
    if (introToggle) {
      introToggle.textContent = 'Collapse description';
    }
  }
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
</script>
"##,
    );
}

fn render_html_docs(out: &mut String, docs: &Docs) {
    let mut text = String::new();
    if let Some(summary) = docs.summary() {
        text.push_str(summary.trim());
    }
    if let Some(details) = docs.details() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(details.trim());
    }

    for paragraph in text.split("\n\n") {
        let paragraph = paragraph
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if paragraph.is_empty() {
            continue;
        }
        out.push_str("<p>");
        push_markdown(out, &paragraph);
        out.push_str("</p>\n");
    }
}

fn push_markdown(out: &mut String, text: &str) {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '`'
            && let Some(end) = chars[i + 1..].iter().position(|c| *c == '`')
        {
            out.push_str("<code>");
            push_escaped(out, &chars[i + 1..i + 1 + end].iter().collect::<String>());
            out.push_str("</code>");
            i += end + 2;
        } else if chars[i] == '*'
            && chars.get(i + 1) == Some(&'*')
            && let Some(end) = chars[i + 2..]
                .windows(2)
                .position(|window| window == ['*', '*'])
        {
            out.push_str("<strong>");
            push_escaped(out, &chars[i + 2..i + 2 + end].iter().collect::<String>());
            out.push_str("</strong>");
            i += end + 4;
        } else if chars[i] == '['
            && let Some(label_end) = chars[i + 1..].iter().position(|c| *c == ']')
        {
            let after_label = i + 1 + label_end + 1;
            if chars.get(after_label) == Some(&'(')
                && let Some(url_end) = chars[after_label + 1..].iter().position(|c| *c == ')')
            {
                let label = chars[i + 1..i + 1 + label_end].iter().collect::<String>();
                let url = chars[after_label + 1..after_label + 1 + url_end]
                    .iter()
                    .collect::<String>();
                out.push_str("<a href=\"");
                push_escaped(out, &url);
                out.push_str("\">");
                push_escaped(out, &label);
                out.push_str("</a>");
                i = after_label + url_end + 2;
                continue;
            }
            push_escaped(out, &chars[i].to_string());
            i += 1;
        } else {
            push_escaped(out, &chars[i].to_string());
            i += 1;
        }
    }
}

fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
}

fn push_script_text(out: &mut String, text: &str) {
    out.push_str(&text.replace("</script", "<\\/script"));
}

fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
}

/// Wrap `text` into lines of at most `max_width - indent.len()` characters,
/// prefixing each line with `indent`. When `max_width` is 0 the text is
/// returned on a single line (no wrapping).
fn wrap_text(text: &str, indent: &str, max_width: usize) -> String {
    let available = if max_width == 0 || max_width <= indent.len() {
        // No wrapping or degenerate case – just one line.
        let mut s = indent.to_string();
        s.push_str(text);
        return s;
    } else {
        max_width - indent.len()
    };

    let mut result = String::new();
    let mut line = String::new();

    for word in text.split_whitespace() {
        if line.is_empty() {
            line.push_str(word);
        } else if line.len() + 1 + word.len() <= available {
            line.push(' ');
            line.push_str(word);
        } else {
            result.push_str(indent);
            result.push_str(&line);
            result.push('\n');
            line.clear();
            line.push_str(word);
        }
    }

    if !line.is_empty() {
        result.push_str(indent);
        result.push_str(&line);
    }

    result
}

/// Generate help output for an argument level (args + subcommands).
fn generate_arg_level_help(
    out: &mut String,
    args: &ArgLevelSchema,
    config_roots: &[ConfigStructSchema],
    program_name: &str,
    config: &HelpConfig,
    config_file_extensions: &[&str],
) {
    // Separate positionals and named flags
    let mut positionals: Vec<&ArgSchema> = Vec::new();
    let mut flags: Vec<&ArgSchema> = Vec::new();

    for (_name, arg) in args.args().iter() {
        if arg.kind().is_positional() {
            positionals.push(arg);
        } else {
            flags.push(arg);
        }
    }

    // Usage line
    out.push_str(&format!("{}:\n    ", "USAGE".yellow().bold()));
    out.push_str(program_name);

    if !flags.is_empty() || !config_roots.is_empty() {
        out.push_str(" [OPTIONS]");
    }

    for pos in &positionals {
        let name = pos.name().to_uppercase();
        if pos.required() {
            out.push_str(&format!(" <{name}>"));
        } else {
            out.push_str(&format!(" [{name}]"));
        }
    }

    if args.has_subcommands() {
        if args.subcommand_optional() {
            out.push_str(" [COMMAND]");
        } else {
            out.push_str(" <COMMAND>");
        }
    }

    out.push_str("\n\n");

    // Positional arguments
    if !positionals.is_empty() {
        out.push_str(&format!("{}:\n", "ARGUMENTS".yellow().bold()));
        for arg in &positionals {
            write_arg_help(out, arg, config);
        }
        out.push('\n');
    }

    // Options
    if !flags.is_empty() || !config_roots.is_empty() {
        out.push_str(&format!("{}:\n", "OPTIONS".yellow().bold()));
        for arg in &flags {
            write_arg_help(out, arg, config);
        }
        for config_root in config_roots {
            write_config_help(out, config_root, config, config_file_extensions);
        }
        out.push('\n');
    }

    // Subcommands
    if args.has_subcommands() {
        out.push_str(&format!("{}:\n", "COMMANDS".yellow().bold()));
        for sub in args.subcommands().values() {
            write_subcommand_help(out, sub, config);
        }
        out.push('\n');
    }
}

/// Write help for a config root.
fn write_config_help(
    out: &mut String,
    config_root: &ConfigStructSchema,
    config: &HelpConfig,
    config_file_extensions: &[&str],
) {
    let Some(name) = config_root.field_name() else {
        return;
    };
    let cli_name = name.to_kebab_case();
    let config_flag = format!("--{cli_name}");

    out.push_str("        ");
    out.push_str(&format!(
        "{} <FILE>",
        config_flag.if_supports_color(Stdout, |text| text.green())
    ));
    out.push('\n');
    let file_help = config_root
        .docs()
        .summary()
        .unwrap_or("Load configuration values from a file.");
    out.push_str(&wrap_text(file_help, "            ", config.width));
    out.push('\n');
    out.push_str(&wrap_text(
        &format_config_file_extensions(config_file_extensions),
        "            ",
        config.width,
    ));
    out.push('\n');

    for item in config_override_help_items(&config_flag, config_root) {
        write_config_override_help(out, &item, config);
    }
}

fn format_config_file_extensions(extensions: &[&str]) -> String {
    let formatted = config_file_extension_list(extensions);
    if formatted.is_empty() {
        "No config file formats are registered.".to_string()
    } else {
        format!("Supported file formats: {formatted}.")
    }
}

fn config_file_extension_list(extensions: &[&str]) -> String {
    let mut unique = Vec::new();
    for extension in extensions {
        let extension = extension.trim_start_matches('.');
        if !extension.is_empty()
            && !unique
                .iter()
                .any(|existing: &&str| existing.eq_ignore_ascii_case(extension))
        {
            unique.push(extension);
        }
    }

    if unique.is_empty() {
        return String::new();
    }

    unique
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect::<Vec<_>>()
        .join(", ")
}

struct ConfigOverrideHelpItem {
    flag: String,
    placeholder: String,
    help: Option<String>,
}

fn write_config_override_help(
    out: &mut String,
    item: &ConfigOverrideHelpItem,
    config: &HelpConfig,
) {
    out.push_str("        ");
    out.push_str(&format!(
        "{} <{}>",
        item.flag.if_supports_color(Stdout, |text| text.green()),
        item.placeholder
    ));
    out.push('\n');

    if let Some(help) = &item.help {
        out.push_str(&wrap_text(help, "            ", config.width));
        out.push('\n');
    }
}

fn config_override_help_items(
    config_flag: &str,
    config_root: &ConfigStructSchema,
) -> Vec<ConfigOverrideHelpItem> {
    let mut items = Vec::new();
    collect_config_struct_overrides(config_flag, config_root, Vec::new(), &mut items);
    items
}

fn collect_config_struct_overrides(
    config_flag: &str,
    config_struct: &ConfigStructSchema,
    path: Vec<String>,
    items: &mut Vec<ConfigOverrideHelpItem>,
) {
    for (field_name, field) in config_struct.fields() {
        let mut field_path = path.clone();
        field_path.push(field_name.clone());
        collect_config_field_override(config_flag, field_path, field, items);
    }
}

fn collect_config_field_override(
    config_flag: &str,
    path: Vec<String>,
    field: &ConfigFieldSchema,
    items: &mut Vec<ConfigOverrideHelpItem>,
) {
    let help = field.docs().summary().map(str::to_string);
    collect_config_value_overrides(config_flag, path, field.value(), help, items);
}

fn collect_config_value_overrides(
    config_flag: &str,
    path: Vec<String>,
    value: &ConfigValueSchema,
    help: Option<String>,
    items: &mut Vec<ConfigOverrideHelpItem>,
) {
    match value.inner_if_option() {
        ConfigValueSchema::Struct(config_struct) => {
            collect_config_struct_overrides(config_flag, config_struct, path, items);
        }
        ConfigValueSchema::Vec(vec_schema) => {
            let mut element_path = path;
            element_path.push("<INDEX>".to_string());
            collect_config_value_overrides(
                config_flag,
                element_path,
                vec_schema.element(),
                help,
                items,
            );
        }
        ConfigValueSchema::Enum(enum_schema) => {
            let variants: Vec<&str> = enum_schema.variants().keys().map(String::as_str).collect();
            if enum_schema
                .variants()
                .values()
                .any(|variant| variant.fields().is_empty())
            {
                items.push(ConfigOverrideHelpItem {
                    flag: config_override_flag(config_flag, &path),
                    placeholder: variants.join(","),
                    help: help.clone(),
                });
            }

            for (variant_name, variant_schema) in enum_schema.variants() {
                let mut variant_path = path.clone();
                variant_path.push(variant_name.clone());
                for (field_name, field) in variant_schema.fields() {
                    let mut field_path = variant_path.clone();
                    field_path.push(field_name.clone());
                    collect_config_field_override(config_flag, field_path, field, items);
                }
            }
        }
        ConfigValueSchema::Leaf(_) => {
            items.push(ConfigOverrideHelpItem {
                flag: config_override_flag(config_flag, &path),
                placeholder: value.type_identifier().to_uppercase(),
                help,
            });
        }
        ConfigValueSchema::Option { .. } => unreachable!("inner_if_option removes Option wrappers"),
    }
}

fn config_override_flag(config_flag: &str, path: &[String]) -> String {
    format!("{config_flag}.{}", path.join("."))
}

/// Write help for a single argument.
fn write_arg_help(out: &mut String, arg: &ArgSchema, config: &HelpConfig) {
    out.push_str("    ");

    let is_positional = arg.kind().is_positional();

    // Short flag (or spacing for alignment)
    if let Some(c) = arg.kind().short() {
        out.push_str(&format!(
            "{}, ",
            format!("-{c}").if_supports_color(Stdout, |text| text.green())
        ));
    } else {
        // Add spacing to align with flags that have short options
        out.push_str("    ");
    }

    // Long flag or positional name
    let name = arg.name();
    let is_counted = arg.kind().is_counted();

    if is_positional {
        out.push_str(&format!(
            "{}",
            format!("<{}>", name.to_uppercase()).if_supports_color(Stdout, |text| text.green())
        ));
    } else {
        let is_bool = arg.value().inner_if_option().is_bool();
        let flag_str = if is_bool {
            format!("--[no-]{}", name.to_kebab_case())
        } else {
            format!("--{}", name.to_kebab_case())
        };
        out.push_str(&format!(
            "{}",
            flag_str.if_supports_color(Stdout, |text| text.green())
        ));

        // Show value placeholder for non-bool, non-counted types
        if !is_counted && !arg.value().is_bool() {
            let placeholder = if let Some(desc) = arg.label() {
                desc.to_uppercase()
            } else if let Some(variants) = arg.value().inner_if_option().enum_variants() {
                variants.join(",")
            } else {
                arg.value().type_identifier().to_uppercase()
            };
            out.push_str(&format!(" <{}>", placeholder));

            // Append the default value or required text
            if let Some(default) = arg.default() {
                out.push_str(&format!(" [Default: `{}`]", config_value_summary(default)));
            } else if arg.required() {
                out.push_str(" [Required]")
            }
        }
    }

    // Doc comment
    const DOC_INDENT: &str = "            ";
    if let Some(summary) = arg.docs().summary() {
        out.push('\n');
        out.push_str(&wrap_text(summary.trim(), DOC_INDENT, config.width));
    }

    if is_counted {
        out.push('\n');
        out.push_str(&wrap_text("[can be repeated]", DOC_INDENT, config.width));
    }

    out.push('\n');
}

/// Write help for a subcommand.
fn write_subcommand_help(out: &mut String, sub: &Subcommand, config: &HelpConfig) {
    out.push_str("    ");

    out.push_str(&format!(
        "{}",
        sub.cli_name()
            .if_supports_color(Stdout, |text| text.green())
    ));

    // Doc comment
    if let Some(summary) = sub.docs().summary() {
        out.push('\n');
        out.push_str(&wrap_text(summary.trim(), "            ", config.width));
    }

    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use figue_attrs as args;

    /// Common arguments that can be flattened into other structs
    #[derive(Facet)]
    struct CommonArgs {
        /// Enable verbose output
        #[facet(args::named, crate::short = 'v')]
        verbose: bool,

        /// Enable quiet mode
        #[facet(args::named, crate::short = 'q')]
        quiet: bool,
    }

    /// Args struct with flattened common args
    #[derive(Facet)]
    struct ArgsWithFlatten {
        /// Input file
        #[facet(args::positional)]
        input: String,

        /// Common options
        #[facet(flatten)]
        common: CommonArgs,
    }

    #[test]
    fn test_flatten_args_appear_in_help() {
        let schema = Schema::from_shape(ArgsWithFlatten::SHAPE).unwrap();
        let help = generate_help_for_subcommand(&schema, &[], &HelpConfig::default());

        // Flattened fields should appear at top level
        assert!(
            help.contains("--[no-]verbose"),
            "help should contain --[no-]verbose from flattened CommonArgs"
        );
        assert!(help.contains("-v"), "help should contain -v short flag");
        assert!(
            help.contains("--[no-]quiet"),
            "help should contain --[no-]quiet from flattened CommonArgs"
        );
        assert!(help.contains("-q"), "help should contain -q short flag");

        // The flattened field name 'common' should NOT appear as a flag
        assert!(
            !help.contains("--common"),
            "help should not show --common as a flag"
        );
    }

    #[test]
    fn test_flatten_docs_preserved() {
        let schema = Schema::from_shape(ArgsWithFlatten::SHAPE).unwrap();
        let help = generate_help_for_subcommand(&schema, &[], &HelpConfig::default());

        // Doc comments from flattened fields should be present
        assert!(
            help.contains("verbose output"),
            "help should contain verbose field doc"
        );
        assert!(
            help.contains("quiet mode"),
            "help should contain quiet field doc"
        );
    }

    /// Arguments for the serve subcommand
    #[derive(Facet)]
    struct ServeArgs {
        /// Port to serve on
        #[facet(args::named)]
        port: u16,

        /// Host to bind to
        #[facet(args::named)]
        host: String,
    }

    /// Top-level command with tuple variant subcommand
    #[derive(Facet)]
    struct TupleVariantArgs {
        /// Subcommand to run
        #[facet(args::subcommand)]
        command: Option<TupleVariantCommand>,
    }

    /// Command enum with tuple variant
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum TupleVariantCommand {
        /// Start the server
        Serve(ServeArgs),
    }

    #[test]
    fn test_label_overrides_placeholder() {
        #[derive(Facet)]
        struct TDArgs {
            /// Input path
            #[facet(args::named, args::label = "PATH")]
            input: std::path::PathBuf,
        }
        let schema = Schema::from_shape(TDArgs::SHAPE).unwrap();
        let help = generate_help_for_subcommand(&schema, &[], &HelpConfig::default());
        // Only assert on the placeholder to avoid issues with ANSI color codes around the flag name
        assert!(
            help.contains("<PATH>"),
            "help should use custom label placeholder"
        );
    }

    #[test]
    fn test_text_help_shows_required_and_default_meta() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Args {
            /// A required value.
            #[facet(args::named)]
            required: String,

            /// A value with a default.
            #[facet(args::named, default = "standard")]
            mode: String,

            /// An optional value.
            #[facet(args::named)]
            maybe: Option<String>,
        }

        let schema = Schema::from_shape(Args::SHAPE).unwrap();
        let help = generate_help_for_subcommand(&schema, &[], &HelpConfig::default());
        let help = strip_ansi_escapes::strip_str(&help);

        assert!(
            help.contains("--required <STRING> [Required]"),
            "help should show required marker: {help}"
        );
        assert!(
            help.contains("--mode <STRING> [Default: `standard`]"),
            "help should show default marker: {help}"
        );
        assert!(
            help.contains("--maybe <STRING>"),
            "help should show optional argument placeholder: {help}"
        );
        assert!(
            !help.contains("--maybe <STRING> [Required]"),
            "help should not mark optional arguments as required: {help}"
        );
    }

    #[test]
    fn test_tuple_variant_fields_not_shown_as_option() {
        let schema = Schema::from_shape(TupleVariantArgs::SHAPE).unwrap();
        // Path contains effective names (e.g., "Serve" not "serve")
        let help =
            generate_help_for_subcommand(&schema, &["Serve".to_string()], &HelpConfig::default());

        // The inner struct's fields should appear
        assert!(
            help.contains("--port"),
            "help should contain --port from ServeArgs"
        );
        assert!(
            help.contains("--host"),
            "help should contain --host from ServeArgs"
        );

        // The tuple field "0" should NOT appear as --0
        assert!(
            !help.contains("--0"),
            "help should NOT show --0 for tuple variant wrapper field"
        );
        assert!(
            !help.contains("SERVEARGS"),
            "help should NOT show SERVEARGS as an option value"
        );
    }

    #[test]
    fn test_config_roots_appear_in_help() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Args {
            /// Session configuration
            #[facet(args::config, args::env_prefix = "BEE", rename = "cfg")]
            cfg: SessionConfig,

            /// Evaluation configuration
            #[facet(args::config, args::env_prefix = "BEE_EVAL", rename = "eval")]
            eval: EvalConfig,
        }

        #[derive(Facet)]
        struct SessionConfig {
            /// Session hostname
            #[facet(default = "localhost")]
            host: String,

            /// Labels attached to this session
            tags: Vec<String>,

            server: ServerConfig,
        }

        #[derive(Facet)]
        struct ServerConfig {
            /// Server port
            #[facet(default = 8080)]
            port: u16,
        }

        #[derive(Facet)]
        struct EvalConfig {
            /// Number of evaluation samples
            #[facet(default = 10)]
            samples: u32,
        }

        let schema = Schema::from_shape(Args::SHAPE).unwrap();
        let help = generate_help_for_subcommand(&schema, &[], &HelpConfig::default());
        let help = strip_ansi_escapes::strip_str(&help);

        assert!(help.contains("--cfg <FILE>"));
        assert!(help.contains("Session configuration"));
        assert!(help.contains("Supported file formats: .json."));
        assert!(help.contains("--cfg.host <STRING>"));
        assert!(help.contains("Session hostname"));
        assert!(help.contains("--cfg.tags.<INDEX> <STRING>"));
        assert!(help.contains("Labels attached to this session"));
        assert!(help.contains("--cfg.server.port <U16>"));
        assert!(help.contains("Server port"));
        assert!(help.contains("--eval <FILE>"));
        assert!(help.contains("Evaluation configuration"));
        assert!(help.contains("--eval.samples <U32>"));
        assert!(help.contains("Number of evaluation samples"));
        assert!(!help.contains("--cfg.<KEY>"));
        assert!(!help.contains("--eval.<KEY>"));
    }

    #[test]
    fn test_html_help_contains_schema_sections() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Args {
            /// Input <path> to process
            #[facet(args::positional)]
            input: String,

            /// Enable verbose output
            #[facet(args::named, args::short = 'v')]
            verbose: bool,
        }

        let config = HelpConfig {
            program_name: Some("tool".to_string()),
            version: Some("1.2.3".to_string()),
            ..Default::default()
        };
        let html = generate_html_help::<Args>(&config);

        assert!(html.contains("<h1>tool</h1>"));
        assert!(html.contains("Version 1.2.3"));
        assert!(html.contains("&lt;INPUT&gt;"));
        assert!(html.contains("Input &lt;path&gt; to process"));
        assert!(html.contains("--[no-]verbose"));
        assert!(html.contains("class=\"breadcrumbs\""));
        assert!(html.contains("data-breadcrumbs"));
        assert!(html.contains("function updateBreadcrumbs()"));
        assert!(html.contains("<nav class=\"side-nav\""));
        assert!(html.contains("max-height: calc(100vh - 92px); overflow-y: auto;"));
        assert!(html.contains("On This Page"));
        assert!(html.contains("Top-Level Options"));
        assert!(html.contains("data-collapsible-intro"));
        assert!(!html.contains("<th>Default</th>"));
    }

    #[test]
    fn test_html_help_config_schema_search_defaults_and_markdown() {
        #[derive(Facet)]
        struct Args {
            /// **Application** settings from all layers.
            #[facet(args::config, args::env_prefix = "APP")]
            settings: Settings,
        }

        #[derive(Facet)]
        struct Settings {
            /// Nested server settings.
            server: Server,
        }

        #[derive(Facet)]
        struct Server {
            /// Port with a `default` value.
            #[facet(default = 8080)]
            port: u16,
        }

        let config = HelpConfig {
            program_name: Some("tool".to_string()),
            ..Default::default()
        };
        let html = generate_html_help::<Args>(&config);

        assert!(html.contains("class=\"search\""));
        assert!(html.contains("metaKey || event.ctrlKey"));
        assert!(html.contains("mark.search-hit"));
        assert!(html.contains("scrollIntoView"));
        assert!(!html.contains("item.hidden"));
        assert!(html.contains("marked.min.js"));
        assert!(html.contains("<strong>Application</strong> settings from all layers."));
        assert!(html.contains("precedence is <span class=\"badge\">CLI</span> &gt;"));
        assert!(html.contains("data-copy=\"--settings.server.port\""));
        assert!(html.contains("<code class=\"env-token\">APP__SERVER__PORT</code>"));
        assert!(html.contains("data-copy=\"APP__SERVER__PORT\""));
        assert!(!html.contains(
            "<code>--settings.server.port</code><code class=\"value-token\">&lt;U16&gt;</code>"
        ));
        assert!(!html.contains(
            "<code class=\"env-token\">APP__SERVER__PORT</code><code class=\"value-token\">=...</code>"
        ));
        assert!(
            html.contains(
                "<code>--settings</code> <code class=\"value-token\">settings.json</code>"
            )
        );
        assert!(html.contains("View config fields."));
        assert!(html.contains("--settings.server.port"));
        assert!(html.contains("Config fields for <code>settings</code>"));
        assert!(html.contains("config-schema-panel"));
        assert!(!html.contains("data-collapsible-schema"));
        assert!(!html.contains("Show all config fields"));
        assert!(html.contains("Default <code class=\"value-token\">8080</code>"));
        assert!(!html.contains("config-schema-heading"));
        assert!(!html.contains("--settings.server.port &lt;U16&gt;</code></td><td>"));
        assert!(html.contains("data-breadcrumb-label=\"Config fields for settings\""));
        assert!(html.contains("data-breadcrumb-label=\"server\""));
        assert!(html.contains(".config-name code { white-space: normal;"));
        assert!(html.contains(
            ".override-grid { display: grid; grid-template-columns: max-content minmax(0, 1fr);"
        ));
    }

    #[test]
    fn test_html_help_config_meta_matches_implicit_defaults() {
        #[derive(Facet)]
        struct Args {
            #[facet(args::config)]
            settings: Settings,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Settings {
            /// Enables the feature.
            #[facet(default)]
            enabled: bool,

            /// Optional queue size.
            queue_size: Option<usize>,

            /// Implicitly false when omitted.
            implicit_flag: bool,
        }

        let html = generate_html_help::<Args>(&HelpConfig::default());

        assert!(!html.contains("Default <code class=\"value-token\">false</code>"));
        assert!(!html.contains("<code>implicit_flag</code> <span class=\"meta\">bool</span><span class=\"name-meta\">Required</span>"));
        assert!(html.contains("<code>queue_size</code> <span class=\"meta\">usize</span><span class=\"name-meta\">Optional</span>"));
    }

    #[test]
    fn test_html_help_arg_rows_include_wrapped_doc_details() {
        #[derive(Facet)]
        struct Args {
            /// Output path for the alignment JSONL (one
            /// object per line).
            #[facet(args::named)]
            out: Option<String>,
        }

        let html = generate_html_help::<Args>(&HelpConfig::default());

        assert!(html.contains("<p>Output path for the alignment JSONL (one object per line).</p>"));
    }

    #[test]
    fn test_html_help_groups_flattened_config_fields() {
        #[derive(Facet)]
        struct Args {
            #[facet(args::config)]
            settings: Settings,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Settings {
            /// Runtime tuning controls.
            #[facet(flatten)]
            runtime: Runtime,

            /// Service host.
            host: String,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Runtime {
            /// Worker count.
            #[facet(default = 4)]
            workers: usize,
        }

        let html = generate_html_help::<Args>(&HelpConfig::default());

        assert!(html.contains("data-breadcrumb-label=\"runtime\""));
        assert!(html.contains("<span class=\"meta\">group</span>"));
        assert!(html.contains("<p>Runtime tuning controls.</p>"));
        assert!(html.contains("data-breadcrumb-label=\"workers\""));
        assert!(html.contains("data-copy=\"--settings.workers\""));
        assert!(!html.contains("--settings.runtime.workers"));
    }

    #[test]
    fn test_html_help_renders_command_specific_options() {
        #[derive(Facet)]
        struct Args {
            #[facet(args::subcommand)]
            command: Command,
        }

        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum Command {
            /// Start the service.
            Serve {
                /// Bind port.
                #[facet(args::named, default = 8080)]
                port: u16,
            },
        }

        let config = HelpConfig {
            program_name: Some("tool".to_string()),
            ..Default::default()
        };
        let html = generate_html_help::<Args>(&config);

        assert!(html.contains("<h2 id=\"commands-heading\">Commands</h2>"));
        assert!(html.contains("id=\"command-serve\""));
        assert!(html.contains("Usage <code>tool serve [OPTIONS]</code>"));
        assert!(
            html.contains("<code>--port</code> <code class=\"value-token\">&lt;U16&gt;</code>")
        );
        assert!(html.contains("Default <code>8080</code>"));
    }

    #[test]
    fn test_html_help_formats_false_bool_and_enum_values() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Args {
            /// Enable feature.
            #[facet(args::named, default)]
            feature: bool,

            /// Shell to generate.
            #[facet(args::named)]
            shell: Option<Shell>,
        }

        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum Shell {
            Bash,
            Zsh,
            Fish,
        }

        let html = generate_html_help::<Args>(&HelpConfig::default());

        assert!(html.contains("<code>--feature</code>"));
        assert!(!html.contains("--[no-]feature"));
        assert!(!html.contains("Default <code>false</code>"));
        assert!(html.contains("<code>--shell</code>"));
        assert!(html.contains("Optional value<br>Values"));
        assert!(!html.contains("--shell &lt;bash,zsh,fish&gt;"));
        assert!(html.contains("<br>Values <code class=\"value-token\">bash</code>"));
    }

    #[test]
    fn test_write_html_help_to_temp_file_keeps_file() {
        let path = write_html_help_to_temp_file("<!doctype html><title>help</title>")
            .expect("HTML help should be written");

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("index.html")
        );
        assert!(path.exists());
        assert_eq!(
            std::fs::read_to_string(path).expect("HTML help should be readable"),
            "<!doctype html><title>help</title>"
        );
    }

    #[test]
    fn test_long_doc_comment_wraps() {
        #[derive(Facet)]
        struct LongDocArgs {
            /// This is a very long description that should definitely be wrapped because it exceeds the default width of eighty columns by quite a bit
            #[facet(args::named)]
            output: String,
        }

        let schema = Schema::from_shape(LongDocArgs::SHAPE).unwrap();
        let config = HelpConfig {
            width: 80,
            ..Default::default()
        };
        let help = generate_help_for_subcommand(&schema, &[], &config);
        eprintln!("{help}");

        // Every doc-comment line should fit within 80 columns
        for line in help.lines() {
            // Strip ANSI escape codes before measuring length
            let plain: String = line
                .chars()
                .fold((String::new(), false), |(mut s, in_esc), c| {
                    if in_esc {
                        if c == 'm' { (s, false) } else { (s, true) }
                    } else if c == '\x1b' {
                        (s, true)
                    } else {
                        s.push(c);
                        (s, false)
                    }
                })
                .0;
            assert!(
                plain.len() <= 80,
                "line exceeds 80 columns ({} chars): {:?}",
                plain.len(),
                plain
            );
        }
    }
}
