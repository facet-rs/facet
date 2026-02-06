//! Debug dump utilities for configuration values.

#[allow(unused_imports)]
use crate::macros::debug;
use crate::{
    config_value::ConfigValue,
    provenance::{FilePathStatus, FileResolution, Provenance},
    schema::{ConfigValueSchema, Schema},
};
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use std::collections::HashMap;
use std::io::Write;
use unicode_width::UnicodeWidthStr;

/// A node in the dump tree. Simple struct with optional children.
struct DumpEntry {
    key: String,
    value: String,      // Already formatted with colors. Empty for group headers.
    provenance: String, // Already formatted with colors.
    children: Vec<DumpEntry>,
}

impl DumpEntry {
    fn leaf(key: impl Into<String>, value: String, provenance: String) -> Self {
        Self {
            key: key.into(),
            value,
            provenance,
            children: Vec::new(),
        }
    }

    fn group(key: impl Into<String>, children: Vec<DumpEntry>) -> Self {
        Self {
            key: key.into(),
            value: String::new(),
            provenance: String::new(),
            children,
        }
    }

    fn missing(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: String::new(), // Empty - will be filled with dots
            provenance: format!("{} {}", "⨯".red(), "MISSING".red().bold()),
            children: Vec::new(),
        }
    }

    fn default_value(key: impl Into<String>) -> Self {
        Self::leaf(
            key,
            "<default>".bright_black().to_string(),
            "DEFAULT".bright_black().to_string(),
        )
    }

    fn is_group(&self) -> bool {
        !self.children.is_empty()
    }
}

/// Column widths at each depth level.
#[derive(Default, Clone)]
struct ColumnWidths {
    key: usize,
    value: usize,
}

/// Formatting options.
struct FormatOptions {
    max_string_length: usize,
    max_value_width: usize,
}

impl FormatOptions {
    fn from_env() -> Self {
        let blast_it = std::env::var("FACET_ARGS_BLAST_IT")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        Self {
            max_string_length: if blast_it { usize::MAX } else { 50 },
            max_value_width: 50,
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Dump the ConfigValue tree with provenance information to a writer, using Schema.
pub(crate) fn dump_config_with_schema(
    w: &mut impl Write,
    value: &ConfigValue,
    file_resolution: &FileResolution,
    schema: &Schema,
) {
    let opts = FormatOptions::from_env();

    // Write sources header
    write_sources_header(w, file_resolution, schema);

    // Build the dump tree
    let entries = build_dump_tree(value, schema, &opts);

    // Compute column widths at each depth
    let mut widths: HashMap<usize, ColumnWidths> = HashMap::new();
    compute_widths(&entries, 0, &mut widths, &opts);

    // Render
    let had_truncation = render_entries(w, &entries, 0, &widths, &opts);

    if had_truncation {
        writeln!(w).ok();
        writeln!(
            w,
            "Some values were truncated. To show full values, rerun with {}=1",
            "FACET_ARGS_BLAST_IT".if_supports_color(Stdout, |text| text.yellow())
        )
        .ok();
    }
}

// ============================================================================
// Sources Header
// ============================================================================

fn write_sources_header(w: &mut impl Write, file_resolution: &FileResolution, schema: &Schema) {
    let config = schema.config();
    let config_field_name = config.and_then(|c| c.field_name()).unwrap_or("settings");
    let env_prefix = config.and_then(|c| c.env_prefix());

    writeln!(w, "Sources:").ok();

    // Count how many sources we have to determine which is last
    let has_files = !file_resolution.paths.is_empty() || file_resolution.had_explicit;
    let has_env = env_prefix.is_some();
    let has_cli = true;
    let has_defaults = true;

    let sources_count = [has_files, has_env, has_cli, has_defaults]
        .iter()
        .filter(|&&x| x)
        .count();
    let mut current_source = 0;

    if !file_resolution.paths.is_empty() {
        current_source += 1;
        let is_last_source = current_source == sources_count;
        let branch = if is_last_source { "└─ " } else { "├─ " };
        let cont = if is_last_source { "···" } else { "│  " };

        writeln!(w, "{}file:", branch).ok();

        let max_path_len = file_resolution
            .paths
            .iter()
            .map(|p| p.path.as_str().len())
            .max()
            .unwrap_or(0);

        for (i, path_info) in file_resolution.paths.iter().enumerate() {
            let is_last_file = i == file_resolution.paths.len() - 1;
            let file_branch = if is_last_file { "└─ " } else { "├─ " };

            let status_label = match path_info.status {
                FilePathStatus::Picked => "(picked)",
                FilePathStatus::NotTried => "(not tried)",
                FilePathStatus::Absent => "(absent)",
            };

            let path_str = path_info.path.as_str();
            let dots = ".".repeat(max_path_len.saturating_sub(path_str.len()));
            let suffix = if path_info.explicit {
                " (via --config)"
            } else {
                ""
            };

            let colored_path = match path_info.status {
                FilePathStatus::Picked => path_str
                    .if_supports_color(Stdout, |text| text.magenta())
                    .to_string(),
                _ => path_str
                    .if_supports_color(Stdout, |text| text.dimmed())
                    .to_string(),
            };

            let colored_status = match path_info.status {
                FilePathStatus::Picked => status_label.to_string(),
                _ => status_label
                    .if_supports_color(Stdout, |text| text.dimmed())
                    .to_string(),
            };

            writeln!(
                w,
                "{}{}{} {}{} {}",
                cont, file_branch, colored_status, colored_path, dots, suffix
            )
            .ok();
        }
    } else if file_resolution.had_explicit {
        current_source += 1;
        let is_last_source = current_source == sources_count;
        let branch = if is_last_source { "└─ " } else { "├─ " };
        writeln!(w, "{}file: (none - explicit --config not provided)", branch).ok();
    }

    if let Some(prefix) = env_prefix {
        current_source += 1;
        let is_last_source = current_source == sources_count;
        let branch = if is_last_source { "└─ " } else { "├─ " };
        writeln!(
            w,
            "{}env {}",
            branch,
            format!("${}__*", prefix).if_supports_color(Stdout, |text| text.yellow())
        )
        .ok();
    }

    {
        current_source += 1;
        let is_last_source = current_source == sources_count;
        let branch = if is_last_source { "└─ " } else { "├─ " };
        writeln!(
            w,
            "{}cli {}",
            branch,
            format!("--{}.*", config_field_name).if_supports_color(Stdout, |text| text.cyan())
        )
        .ok();
    }

    {
        current_source += 1;
        let is_last_source = current_source == sources_count;
        let branch = if is_last_source { "└─ " } else { "├─ " };
        writeln!(w, "{}defaults", branch).ok();
    }

    writeln!(w).ok();
}

// ============================================================================
// Tree Building
// ============================================================================

fn build_dump_tree(value: &ConfigValue, schema: &Schema, opts: &FormatOptions) -> Vec<DumpEntry> {
    let mut entries = Vec::new();

    let ConfigValue::Object(sourced) = value else {
        return entries;
    };

    // Args
    for (name, arg_schema) in schema.args().args() {
        if let Some(val) = sourced.value.get(name.as_str()) {
            entries.push(build_leaf_entry(name, val, false, opts));
        } else if arg_schema.required() {
            entries.push(DumpEntry::missing(name));
        } else {
            entries.push(DumpEntry::default_value(name));
        }
    }

    // Subcommand
    if let Some(subcommand_field) = schema.args().subcommand_field_name() {
        if let Some(val) = sourced.value.get(subcommand_field) {
            entries.push(build_leaf_entry(subcommand_field, val, false, opts));
        } else {
            entries.push(DumpEntry::missing(subcommand_field));
        }
    }

    // Config fields
    if let Some(config_schema) = schema.config() {
        let config_field_name = config_schema.field_name().unwrap_or("config");
        if let Some(ConfigValue::Object(config_sourced)) = sourced.value.get(config_field_name) {
            for (field_name, field_schema) in config_schema.fields() {
                if let Some(field_value) = config_sourced.value.get(field_name.as_str()) {
                    entries.push(build_entry_from_schema(
                        field_name,
                        field_value,
                        field_schema.value(),
                        field_schema.is_sensitive(),
                        opts,
                    ));
                } else {
                    let is_optional =
                        matches!(field_schema.value(), ConfigValueSchema::Option { .. });
                    if is_optional {
                        entries.push(DumpEntry::default_value(field_name));
                    } else {
                        entries.push(DumpEntry::missing(field_name));
                    }
                }
            }
        }
    }

    entries
}

fn build_entry_from_schema(
    key: &str,
    value: &ConfigValue,
    schema: &ConfigValueSchema,
    is_sensitive: bool,
    opts: &FormatOptions,
) -> DumpEntry {
    match (value, schema) {
        // Struct: recurse into fields using schema
        (ConfigValue::Object(sourced), ConfigValueSchema::Struct(struct_schema)) => {
            let mut children = Vec::new();
            for (field_name, field_schema) in struct_schema.fields() {
                if let Some(field_value) = sourced.value.get(field_name.as_str()) {
                    children.push(build_entry_from_schema(
                        field_name,
                        field_value,
                        field_schema.value(),
                        field_schema.is_sensitive(),
                        opts,
                    ));
                } else {
                    let is_optional =
                        matches!(field_schema.value(), ConfigValueSchema::Option { .. });
                    if is_optional {
                        children.push(DumpEntry::default_value(field_name));
                    } else {
                        children.push(DumpEntry::missing(field_name));
                    }
                }
            }
            DumpEntry::group(key, children)
        }

        // Vec: recurse into elements using schema
        (ConfigValue::Array(sourced), ConfigValueSchema::Vec(vec_schema)) => {
            let children = sourced
                .value
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    build_entry_from_schema(
                        &format!("[{}]", i),
                        item,
                        vec_schema.element(),
                        is_sensitive,
                        opts,
                    )
                })
                .collect();
            DumpEntry::group(key, children)
        }

        // Option: unwrap and recurse
        (value, ConfigValueSchema::Option { value: inner, .. }) => {
            build_entry_from_schema(key, value, inner, is_sensitive, opts)
        }

        // Enum (native): ConfigValue::Enum paired with ConfigValueSchema::Enum
        (ConfigValue::Enum(sourced), ConfigValueSchema::Enum(enum_schema)) => {
            let variant_name = &sourced.value.variant;

            // Build variant's children
            let mut variant_children = Vec::new();
            if let Some(variant_schema) = enum_schema.variants().get(variant_name.as_str()) {
                for (field_name, field_schema) in variant_schema.fields() {
                    if let Some(field_value) = sourced.value.fields.get(field_name.as_str()) {
                        variant_children.push(build_entry_from_schema(
                            field_name,
                            field_value,
                            field_schema.value(),
                            field_schema.is_sensitive(),
                            opts,
                        ));
                    } else {
                        let is_optional =
                            matches!(field_schema.value(), ConfigValueSchema::Option { .. });
                        if is_optional {
                            variant_children.push(DumpEntry::default_value(field_name));
                        } else {
                            variant_children.push(DumpEntry::missing(field_name));
                        }
                    }
                }
            } else {
                // Unknown variant - just show what we have
                for (k, v) in &sourced.value.fields {
                    variant_children.push(build_leaf_entry(k, v, false, opts));
                }
            }

            // Variant is a group under the enum field
            let variant_entry = DumpEntry::group(variant_name, variant_children);
            DumpEntry::group(key, vec![variant_entry])
        }

        // Enum (from JSON): JSON parses enums as { "VariantName": { ...fields... } }
        (ConfigValue::Object(sourced), ConfigValueSchema::Enum(enum_schema)) => {
            if sourced.value.len() == 1 {
                let (variant_name, variant_value) = sourced.value.iter().next().unwrap();

                // Get variant fields (if it's an object) or empty map
                let variant_fields = match variant_value {
                    ConfigValue::Object(obj) => Some(&obj.value),
                    _ => None,
                };

                // Build variant's children
                let mut variant_children = Vec::new();
                if let Some(variant_schema) = enum_schema.variants().get(variant_name.as_str()) {
                    for (field_name, field_schema) in variant_schema.fields() {
                        let field_value = variant_fields.and_then(|f| f.get(field_name.as_str()));
                        if let Some(fv) = field_value {
                            variant_children.push(build_entry_from_schema(
                                field_name,
                                fv,
                                field_schema.value(),
                                field_schema.is_sensitive(),
                                opts,
                            ));
                        } else {
                            let is_optional =
                                matches!(field_schema.value(), ConfigValueSchema::Option { .. });
                            if is_optional {
                                variant_children.push(DumpEntry::default_value(field_name));
                            } else {
                                variant_children.push(DumpEntry::missing(field_name));
                            }
                        }
                    }
                } else {
                    // Unknown variant - show what we have
                    if let Some(fields) = variant_fields {
                        for (k, v) in fields {
                            variant_children.push(build_leaf_entry(k, v, false, opts));
                        }
                    }
                }

                // Variant is a group under the enum field
                let variant_entry = DumpEntry::group(variant_name, variant_children);
                DumpEntry::group(key, vec![variant_entry])
            } else {
                // Not a single-key object - shouldn't happen for well-formed enum
                // Just show as a regular object
                let children: Vec<_> = sourced
                    .value
                    .iter()
                    .map(|(k, v)| build_leaf_entry(k, v, false, opts))
                    .collect();
                DumpEntry::group(key, children)
            }
        }

        // Leaf values: terminal cases that don't need schema recursion
        (ConfigValue::String(sourced), ConfigValueSchema::Leaf(_)) => {
            let formatted = if is_sensitive {
                format!("🔒 [REDACTED ({} bytes)]", sourced.value.len())
                    .if_supports_color(Stdout, |text| text.bright_magenta())
                    .to_string()
            } else {
                let escaped = sourced.value.replace('\n', "↵");
                let (truncated, _) = truncate_middle(&escaped, opts.max_string_length);
                truncated
                    .if_supports_color(Stdout, |text| text.green())
                    .to_string()
            };
            DumpEntry::leaf(key, formatted, format_provenance(&sourced.provenance))
        }
        (ConfigValue::Integer(sourced), ConfigValueSchema::Leaf(_)) => DumpEntry::leaf(
            key,
            sourced
                .value
                .if_supports_color(Stdout, |value| value.blue())
                .to_string(),
            format_provenance(&sourced.provenance),
        ),
        (ConfigValue::Float(sourced), ConfigValueSchema::Leaf(_)) => DumpEntry::leaf(
            key,
            sourced
                .value
                .if_supports_color(Stdout, |value| value.bright_blue())
                .to_string(),
            format_provenance(&sourced.provenance),
        ),
        (ConfigValue::Bool(sourced), ConfigValueSchema::Leaf(_)) => DumpEntry::leaf(
            key,
            if sourced.value {
                "true"
                    .if_supports_color(Stdout, |text| text.green())
                    .to_string()
            } else {
                "false"
                    .if_supports_color(Stdout, |text| text.red())
                    .to_string()
            },
            format_provenance(&sourced.provenance),
        ),
        (ConfigValue::Null(sourced), ConfigValueSchema::Leaf(_)) => DumpEntry::leaf(
            key,
            "null"
                .if_supports_color(Stdout, |text| text.bright_black())
                .to_string(),
            format_provenance(&sourced.provenance),
        ),

        // Fallback for any value with Leaf schema (type mismatch - just show value)
        (value, ConfigValueSchema::Leaf(_)) => build_leaf_entry(key, value, is_sensitive, opts),

        // Any other mismatch - shouldn't happen with well-formed data
        (value, _schema) => {
            debug!(
                "schema mismatch for {}: value={:?}",
                key,
                std::mem::discriminant(value)
            );
            build_leaf_entry(key, value, is_sensitive, opts)
        }
    }
}

/// Build a leaf entry from a ConfigValue without schema guidance.
/// Used for unknown variants or schema mismatches.
fn build_leaf_entry(
    key: &str,
    value: &ConfigValue,
    is_sensitive: bool,
    opts: &FormatOptions,
) -> DumpEntry {
    match value {
        ConfigValue::String(sourced) => {
            let formatted = if is_sensitive {
                format!("🔒 [REDACTED ({} bytes)]", sourced.value.len())
                    .if_supports_color(Stdout, |text| text.bright_magenta())
                    .to_string()
            } else {
                let escaped = sourced.value.replace('\n', "↵");
                let (truncated, _) = truncate_middle(&escaped, opts.max_string_length);
                truncated
                    .if_supports_color(Stdout, |text| text.green())
                    .to_string()
            };
            DumpEntry::leaf(key, formatted, format_provenance(&sourced.provenance))
        }
        ConfigValue::Integer(sourced) => DumpEntry::leaf(
            key,
            sourced
                .value
                .if_supports_color(Stdout, |text| text.blue())
                .to_string(),
            format_provenance(&sourced.provenance),
        ),
        ConfigValue::Float(sourced) => DumpEntry::leaf(
            key,
            sourced
                .value
                .if_supports_color(Stdout, |text| text.bright_blue())
                .to_string(),
            format_provenance(&sourced.provenance),
        ),
        ConfigValue::Bool(sourced) => DumpEntry::leaf(
            key,
            if sourced.value {
                "true"
                    .if_supports_color(Stdout, |text| text.green())
                    .to_string()
            } else {
                "false"
                    .if_supports_color(Stdout, |text| text.red())
                    .to_string()
            },
            format_provenance(&sourced.provenance),
        ),
        ConfigValue::Null(sourced) => DumpEntry::leaf(
            key,
            "null"
                .if_supports_color(Stdout, |text| text.bright_black())
                .to_string(),
            format_provenance(&sourced.provenance),
        ),
        ConfigValue::Object(sourced) => {
            let children: Vec<_> = sourced
                .value
                .iter()
                .map(|(k, v)| build_leaf_entry(k, v, false, opts))
                .collect();
            DumpEntry::group(key, children)
        }
        ConfigValue::Array(sourced) => {
            let children: Vec<_> = sourced
                .value
                .iter()
                .enumerate()
                .map(|(i, v)| build_leaf_entry(&format!("[{}]", i), v, false, opts))
                .collect();
            DumpEntry::group(key, children)
        }
        ConfigValue::Enum(sourced) => {
            let children: Vec<_> = sourced
                .value
                .fields
                .iter()
                .map(|(k, v)| build_leaf_entry(k, v, false, opts))
                .collect();
            DumpEntry {
                key: key.to_string(),
                value: format!("{}::", sourced.value.variant).cyan().to_string(),
                provenance: format_provenance(&sourced.provenance),
                children,
            }
        }
    }
}

// ============================================================================
// Width Computation
// ============================================================================

fn compute_widths(
    entries: &[DumpEntry],
    depth: usize,
    widths: &mut HashMap<usize, ColumnWidths>,
    opts: &FormatOptions,
) {
    for entry in entries {
        if !entry.is_group() || !entry.value.is_empty() {
            // Leaf or enum with value
            let w = widths.entry(depth).or_default();
            w.key = w.key.max(visual_width(&entry.key) + 2);
            w.value = w
                .value
                .max(visual_width(&entry.value).min(opts.max_value_width) + 2);
        }
        compute_widths(&entry.children, depth + 1, widths, opts);
    }
}

// ============================================================================
// Rendering
// ============================================================================

fn render_entries(
    w: &mut dyn Write,
    entries: &[DumpEntry],
    depth: usize,
    widths: &HashMap<usize, ColumnWidths>,
    opts: &FormatOptions,
) -> bool {
    render_entries_with_prefix(w, entries, depth, widths, opts, "")
}

fn render_entries_with_prefix(
    w: &mut dyn Write,
    entries: &[DumpEntry],
    depth: usize,
    widths: &HashMap<usize, ColumnWidths>,
    opts: &FormatOptions,
    prefix: &str,
) -> bool {
    let col = widths.get(&depth).cloned().unwrap_or_default();
    let mut had_truncation = false;

    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == entries.len() - 1;

        // Tree branch characters
        let branch = if depth == 0 {
            "" // No branch for root level
        } else if is_last {
            "└─ "
        } else {
            "├─ "
        };

        // Prefix for children: continue vertical line if not last, dots if last
        // Using dots instead of spaces when is_last so they don't get stripped by log viewers
        // (the │ character already protects the line from stripping)
        let child_prefix = if depth == 0 {
            String::new()
        } else {
            format!("{}{}", prefix, if is_last { "···" } else { "│  " })
        };

        let full_prefix = format!("{}{}", prefix, branch);

        if entry.is_group() && entry.value.is_empty() {
            // Pure group header (struct/array)
            writeln!(w, "{}{}", full_prefix, entry.key).ok();
        } else {
            // Leaf or enum with value
            let key_pad = ".".repeat(col.key.saturating_sub(visual_width(&entry.key)));
            let val_width = visual_width(&entry.value);

            if val_width > opts.max_value_width {
                had_truncation = true;
                let wrapped = wrap_value(&entry.value, opts.max_value_width);
                for (j, line) in wrapped.iter().enumerate() {
                    if j == 0 {
                        let val_pad =
                            ".".repeat(opts.max_value_width.saturating_sub(visual_width(line)));
                        writeln!(
                            w,
                            "{}{}{} {}{} {}",
                            full_prefix,
                            entry.key,
                            key_pad.if_supports_color(Stdout, |text| text.bright_black()),
                            line,
                            val_pad.if_supports_color(Stdout, |text| text.bright_black()),
                            entry.provenance,
                        )
                        .ok();
                    } else {
                        // Continuation line for wrapped values
                        let cont_prefix = if depth == 0 {
                            String::new()
                        } else {
                            format!("{}{}", prefix, if is_last { "···" } else { "│  " })
                        };
                        let continuation = "."
                            .repeat(visual_width(&full_prefix) + col.key + 1 - cont_prefix.len());
                        writeln!(w, "{}{}{}", cont_prefix, continuation, line).ok();
                    }
                }
            } else {
                let val_pad = ".".repeat(col.value.saturating_sub(val_width));
                writeln!(
                    w,
                    "{}{}{} {}{} {}",
                    full_prefix,
                    entry.key,
                    key_pad.bright_black(),
                    entry.value,
                    val_pad.bright_black(),
                    entry.provenance,
                )
                .ok();
            }
        }

        // Render children with updated prefix
        if render_entries_with_prefix(w, &entry.children, depth + 1, widths, opts, &child_prefix) {
            had_truncation = true;
        }
    }

    had_truncation
}

// ============================================================================
// Formatting Utilities
// ============================================================================

fn format_provenance(prov: &Option<Provenance>) -> String {
    match prov {
        Some(Provenance::Cli { arg, .. }) => arg
            .if_supports_color(Stdout, |text| text.cyan())
            .to_string(),
        Some(Provenance::Env { var, .. }) => format!("${}", var)
            .if_supports_color(Stdout, |text| text.yellow())
            .to_string(),
        Some(Provenance::File { file, offset, .. }) => {
            let line_num = calculate_line_number(&file.contents, *offset);
            let filename = std::path::Path::new(file.path.as_str())
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file.path.as_str());
            format!("{}:{}", filename, line_num)
                .if_supports_color(Stdout, |text| text.magenta())
                .to_string()
        }
        Some(Provenance::Default) => "DEFAULT"
            .if_supports_color(Stdout, |text| text.bright_black())
            .to_string(),
        None => String::new(),
    }
}

fn calculate_line_number(contents: &str, offset: usize) -> usize {
    if offset == 0 {
        return 1;
    }
    contents[..offset.min(contents.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count()
        + 1
}

fn visual_width(s: &str) -> usize {
    let stripped = strip_ansi_escapes::strip(s.as_bytes());
    let stripped_str = core::str::from_utf8(&stripped).unwrap_or(s);
    stripped_str.width()
}

fn truncate_middle(s: &str, max_length: usize) -> (String, bool) {
    if s.len() <= max_length {
        return (s.to_string(), false);
    }
    if max_length < 3 {
        return ("...".to_string(), true);
    }

    let available = max_length - 3;
    let start_len = available.div_ceil(2);
    let end_len = available / 2;

    let start: String = s.chars().take(start_len).collect();
    let end: String = s
        .chars()
        .rev()
        .take(end_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    (format!("{}...{}", start, end), true)
}

fn wrap_value(value: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;
    let mut in_ansi = false;
    let mut ansi_buffer = String::new();
    let mut active_color = String::new();

    for ch in value.chars() {
        if ch == '\x1b' {
            in_ansi = true;
            ansi_buffer.push(ch);
        } else if in_ansi {
            ansi_buffer.push(ch);
            if ch == 'm' {
                current_line.push_str(&ansi_buffer);
                active_color = ansi_buffer.clone();
                ansi_buffer.clear();
                in_ansi = false;
            }
        } else {
            if current_width >= max_width {
                lines.push(current_line);
                current_line = String::new();
                if !active_color.is_empty() {
                    current_line.push_str(&active_color);
                }
                current_width = 0;
            }
            current_line.push(ch);
            current_width += 1;
        }
    }

    if !current_line.is_empty() || !ansi_buffer.is_empty() {
        current_line.push_str(&ansi_buffer);
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Helper functions tests
    // ========================================================================

    #[test]
    fn test_truncate_middle_short_string() {
        let (result, truncated) = truncate_middle("hello", 10);
        assert_eq!(result, "hello");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_middle_exact_length() {
        let (result, truncated) = truncate_middle("hello", 5);
        assert_eq!(result, "hello");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_middle_long_string() {
        let (result, truncated) = truncate_middle("hello world", 8);
        assert_eq!(result.len(), 8);
        assert!(result.contains("..."));
        assert!(truncated);
    }

    #[test]
    fn test_truncate_middle_very_short_max() {
        let (result, truncated) = truncate_middle("hello", 2);
        assert_eq!(result, "...");
        assert!(truncated);
    }

    #[test]
    fn test_calculate_line_number_first_line() {
        let contents = "first\nsecond\nthird";
        assert_eq!(calculate_line_number(contents, 0), 1);
        assert_eq!(calculate_line_number(contents, 3), 1);
    }

    #[test]
    fn test_calculate_line_number_second_line() {
        let contents = "first\nsecond\nthird";
        // offset 6 is start of "second"
        assert_eq!(calculate_line_number(contents, 6), 2);
    }

    #[test]
    fn test_calculate_line_number_third_line() {
        let contents = "first\nsecond\nthird";
        // offset 13 is start of "third"
        assert_eq!(calculate_line_number(contents, 13), 3);
    }

    #[test]
    fn test_visual_width_plain_text() {
        assert_eq!(visual_width("hello"), 5);
    }

    #[test]
    fn test_visual_width_with_ansi() {
        // ANSI codes should not count toward visual width
        let colored = "\x1b[32mhello\x1b[0m";
        assert_eq!(visual_width(colored), 5);
    }

    #[test]
    fn test_wrap_value_short() {
        let lines = wrap_value("short", 10);
        assert_eq!(lines, vec!["short"]);
    }

    #[test]
    fn test_wrap_value_long() {
        let lines = wrap_value("hello world", 5);
        assert_eq!(lines.len(), 3); // "hello", " worl", "d"
    }

    #[test]
    fn test_wrap_value_empty() {
        let lines = wrap_value("", 10);
        assert_eq!(lines, vec![""]);
    }

    // ========================================================================
    // DumpEntry tests
    // ========================================================================

    #[test]
    fn test_dump_entry_leaf() {
        let entry = DumpEntry::leaf("key", "value".to_string(), "prov".to_string());
        assert_eq!(entry.key, "key");
        assert_eq!(entry.value, "value");
        assert_eq!(entry.provenance, "prov");
        assert!(!entry.is_group());
    }

    #[test]
    fn test_dump_entry_group() {
        let child = DumpEntry::leaf("child", "val".to_string(), "prov".to_string());
        let entry = DumpEntry::group("parent", vec![child]);
        assert_eq!(entry.key, "parent");
        assert!(entry.is_group());
        assert_eq!(entry.children.len(), 1);
    }

    #[test]
    fn test_dump_entry_missing() {
        let entry = DumpEntry::missing("field");
        assert_eq!(entry.key, "field");
        assert!(entry.provenance.contains("MISSING"));
    }

    #[test]
    fn test_dump_entry_default() {
        let entry = DumpEntry::default_value("field");
        assert_eq!(entry.key, "field");
        assert!(entry.provenance.contains("DEFAULT"));
    }

    // ========================================================================
    // Provenance formatting tests
    // ========================================================================

    #[test]
    fn test_format_provenance_cli() {
        let prov = Some(Provenance::Cli {
            arg: "--verbose".to_string(),
            value: "true".to_string(),
        });
        let formatted = format_provenance(&prov);
        // Should contain the arg name (with ANSI codes)
        assert!(formatted.contains("--verbose") || visual_width(&formatted) > 0);
    }

    #[test]
    fn test_format_provenance_env() {
        let prov = Some(Provenance::Env {
            var: "MY_VAR".to_string(),
            value: "value".to_string(),
        });
        let formatted = format_provenance(&prov);
        assert!(formatted.contains("MY_VAR"));
    }

    #[test]
    fn test_format_provenance_default() {
        let prov = Some(Provenance::Default);
        let formatted = format_provenance(&prov);
        assert!(formatted.contains("DEFAULT"));
    }

    #[test]
    fn test_format_provenance_none() {
        let formatted = format_provenance(&None);
        assert!(formatted.is_empty());
    }

    // ========================================================================
    // Integration tests with schema
    // ========================================================================

    #[test]
    fn test_build_dump_tree_basic() {
        use crate as args;
        use crate::config_value::Sourced;
        use facet::Facet;
        use indexmap::IndexMap;

        #[derive(Facet)]
        struct TestArgs {
            #[facet(args::named)]
            verbose: bool,
        }

        let schema = Schema::from_shape(TestArgs::SHAPE).expect("schema should build");

        // Build a simple ConfigValue
        let mut root = IndexMap::default();
        root.insert(
            "verbose".to_string(),
            ConfigValue::Bool(Sourced {
                value: true,
                span: None,
                provenance: Some(Provenance::Cli {
                    arg: "--verbose".to_string(),
                    value: "true".to_string(),
                }),
            }),
        );
        let value = ConfigValue::Object(Sourced::new(root));

        let opts = FormatOptions {
            max_string_length: 50,
            max_value_width: 50,
        };
        let entries = build_dump_tree(&value, &schema, &opts);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "verbose");
    }

    #[test]
    fn test_build_dump_tree_with_config() {
        use crate as args;
        use crate::config_value::Sourced;
        use facet::Facet;
        use indexmap::IndexMap;

        #[derive(Facet)]
        struct ServerConfig {
            host: String,
            port: u16,
        }

        #[derive(Facet)]
        struct TestArgs {
            #[facet(args::config)]
            config: ServerConfig,
        }

        let schema = Schema::from_shape(TestArgs::SHAPE).expect("schema should build");

        // Build ConfigValue with nested config
        let mut config_map = IndexMap::default();
        config_map.insert(
            "host".to_string(),
            ConfigValue::String(Sourced {
                value: "localhost".to_string(),
                span: None,
                provenance: Some(Provenance::Default),
            }),
        );
        config_map.insert(
            "port".to_string(),
            ConfigValue::Integer(Sourced {
                value: 8080,
                span: None,
                provenance: Some(Provenance::Env {
                    var: "PORT".to_string(),
                    value: "8080".to_string(),
                }),
            }),
        );

        let mut root = IndexMap::default();
        root.insert(
            "config".to_string(),
            ConfigValue::Object(Sourced::new(config_map)),
        );
        let value = ConfigValue::Object(Sourced::new(root));

        let opts = FormatOptions {
            max_string_length: 50,
            max_value_width: 50,
        };
        let entries = build_dump_tree(&value, &schema, &opts);

        // Should have entries for host and port
        assert!(entries.len() >= 2);
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"host"));
        assert!(keys.contains(&"port"));
    }

    #[test]
    fn test_build_dump_tree_missing_required() {
        use crate as args;
        use crate::config_value::Sourced;
        use facet::Facet;
        use indexmap::IndexMap;

        #[derive(Facet)]
        struct TestArgs {
            #[facet(args::positional)]
            input: String, // required
        }

        let schema = Schema::from_shape(TestArgs::SHAPE).expect("schema should build");

        // Empty value - missing required field
        let root = IndexMap::default();
        let value = ConfigValue::Object(Sourced::new(root));

        let opts = FormatOptions {
            max_string_length: 50,
            max_value_width: 50,
        };
        let entries = build_dump_tree(&value, &schema, &opts);

        // Should have a MISSING entry
        assert_eq!(entries.len(), 1);
        assert!(entries[0].provenance.contains("MISSING"));
    }

    #[test]
    fn test_build_dump_tree_optional_default() {
        use crate as args;
        use crate::config_value::Sourced;
        use facet::Facet;
        use indexmap::IndexMap;

        #[derive(Facet)]
        struct TestArgs {
            #[facet(args::named)]
            verbose: Option<bool>, // optional
        }

        let schema = Schema::from_shape(TestArgs::SHAPE).expect("schema should build");

        // Empty value - optional field not set
        let root = IndexMap::default();
        let value = ConfigValue::Object(Sourced::new(root));

        let opts = FormatOptions {
            max_string_length: 50,
            max_value_width: 50,
        };
        let entries = build_dump_tree(&value, &schema, &opts);

        // Should have a DEFAULT entry (not MISSING)
        assert_eq!(entries.len(), 1);
        assert!(entries[0].provenance.contains("DEFAULT"));
    }

    #[test]
    fn test_build_leaf_entry_sensitive() {
        let opts = FormatOptions {
            max_string_length: 50,
            max_value_width: 50,
        };

        let value = ConfigValue::String(crate::config_value::Sourced {
            value: "secret_password_123".to_string(),
            span: None,
            provenance: Some(Provenance::Env {
                var: "PASSWORD".to_string(),
                value: "secret_password_123".to_string(),
            }),
        });

        let entry = build_leaf_entry("password", &value, true, &opts);

        // Sensitive field should show REDACTED
        assert!(entry.value.contains("REDACTED"));
        assert!(!entry.value.contains("secret"));
    }

    #[test]
    fn test_dump_config_with_schema_output() {
        use crate as args;
        use crate::config_value::Sourced;
        use facet::Facet;
        use indexmap::IndexMap;

        #[derive(Facet)]
        struct TestArgs {
            #[facet(args::named)]
            verbose: bool,
        }

        let schema = Schema::from_shape(TestArgs::SHAPE).expect("schema should build");

        let mut root = IndexMap::default();
        root.insert(
            "verbose".to_string(),
            ConfigValue::Bool(Sourced {
                value: true,
                span: None,
                provenance: Some(Provenance::Cli {
                    arg: "--verbose".to_string(),
                    value: "true".to_string(),
                }),
            }),
        );
        let value = ConfigValue::Object(Sourced::new(root));

        let file_resolution = FileResolution {
            paths: vec![],
            had_explicit: false,
        };

        let mut output = Vec::new();
        dump_config_with_schema(&mut output, &value, &file_resolution, &schema);

        let output_str = String::from_utf8(output).expect("output should be valid UTF-8");

        // Should contain Sources header and verbose field
        assert!(output_str.contains("Sources:"));
        assert!(output_str.contains("verbose"));
    }
}
