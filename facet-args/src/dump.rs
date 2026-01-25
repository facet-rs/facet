use crate::{
    config_value::ConfigValue,
    provenance::{ConfigResult, FilePathStatus, FileResolution, Provenance},
    reflection::{coerce_types_from_shape, find_config_field, get_env_prefix},
};
use facet::Facet;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use unicode_width::UnicodeWidthStr;

/// Dump config with special markers for missing required fields.
#[deprecated(note = "just use dump_config_with_provenance")]
pub(crate) fn dump_config_with_missing_fields<T: Facet<'static>>(
    value: &ConfigValue,
    file_resolution: &FileResolution,
    _missing_fields: &[crate::config_value::MissingFieldInfo],
    _env_prefix: &str,
) {
    // Just show the normal dump - it already has header and sources
    dump_config_with_provenance::<T>(value, file_resolution);
}

#[deprecated(note = "provide a visitor pattern")]
pub(crate) fn collect_missing_values(
    value: &ConfigValue,
    missing: &mut Vec<crate::config_value::MissingFieldInfo>,
) {
    match value {
        ConfigValue::Missing(info) => {
            missing.push(info.clone());
        }
        ConfigValue::Object(sourced) => {
            for (_key, val) in &sourced.value {
                collect_missing_values(val, missing);
            }
        }
        ConfigValue::Array(sourced) => {
            for val in &sourced.value {
                collect_missing_values(val, missing);
            }
        }
        _ => {}
    }
}

/// A line to be printed in the config dump.
#[derive(Debug)]
struct DumpLine {
    indent: usize,
    key: String,
    value: String,
    provenance: String,
    is_header: bool,
}

/// Context for dumping configuration with provenance.
#[derive(Debug)]
struct DumpContext {
    config_field_name: &'static str,
    env_prefix: Option<&'static str>,
    max_string_length: usize,
    max_value_width: usize, // Max width for value column before wrapping
}

/// State tracking for the dump operation.
struct DumpState {
    had_truncation: bool,
}

impl DumpState {
    fn new() -> Self {
        Self {
            had_truncation: false,
        }
    }
}

impl DumpContext {
    /// Extract dump context from the shape.
    fn from_shape<T: Facet<'static>>() -> Self {
        let config_field = find_config_field(T::SHAPE);
        let (config_field_name, env_prefix) = if let Some(field) = config_field {
            (field.name, get_env_prefix(field))
        } else {
            ("settings", None)
        };

        // Check for FACET_ARGS_BLAST_IT env var to disable truncation
        let blast_it = std::env::var("FACET_ARGS_BLAST_IT")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        Self {
            config_field_name,
            env_prefix,
            max_string_length: if blast_it { usize::MAX } else { 50 },
            max_value_width: 50, // Maximum width for value column
        }
    }
}

/// Dump the ConfigValue tree with provenance information from a ConfigResult.
///
/// This is useful when using the builder API directly and want to show
/// the config dump with file resolution info.
#[deprecated(note = "just use dump_config_with_provenance")]
pub(crate) fn dump_config_from_result<T: Facet<'static>>(result: &ConfigResult<ConfigValue>) {
    dump_config_with_provenance::<T>(&result.value, &result.file_resolution);
}

/// Dump the ConfigValue tree with provenance information.
pub(crate) fn dump_config_with_provenance<T: Facet<'static>>(
    value: &ConfigValue,
    file_resolution: &FileResolution,
) {
    // Extract context from shape
    let ctx = DumpContext::from_shape::<T>();

    // Show sources
    println!("Sources:");

    // Config files - show resolution info with alignment
    if !file_resolution.paths.is_empty() {
        println!("  file:");

        // Find max path length for alignment
        let max_path_len = file_resolution
            .paths
            .iter()
            .map(|p| p.path.as_str().len())
            .max()
            .unwrap_or(0);

        for path_info in &file_resolution.paths {
            let status_label = match path_info.status {
                FilePathStatus::Picked => "  (picked)",
                FilePathStatus::NotTried => "(not tried)",
                FilePathStatus::Absent => "  (absent)",
            };

            // Calculate dots needed for alignment
            let path_str = path_info.path.as_str();
            let dots_needed = max_path_len.saturating_sub(path_str.len());
            let dots = ".".repeat(dots_needed);

            let suffix = if path_info.explicit {
                " (via --config)"
            } else {
                ""
            };

            // Color the path (purple/magenta for picked, dimmed for others)
            let colored_path = match path_info.status {
                FilePathStatus::Picked => path_str.magenta().to_string(),
                _ => path_str.dimmed().to_string(),
            };

            // Color the status
            let colored_status = match path_info.status {
                FilePathStatus::Picked => status_label.to_string(),
                _ => status_label.dimmed().to_string(),
            };

            println!("    {} {}{} {}", colored_status, colored_path, dots, suffix);
        }
    } else if file_resolution.had_explicit {
        println!("  file: (none - explicit --config not provided)");
    }

    // Environment variables - show actual prefix from shape
    if let Some(env_prefix) = ctx.env_prefix {
        println!("  env {}", format!("${}__*", env_prefix).yellow());
    }

    // CLI args - show pattern for config field overrides
    println!("  cli {}", format!("--{}.*", ctx.config_field_name).cyan());

    // Defaults
    println!("  defaults");

    println!();

    // Step 1: Coerce string values to their target types (for env vars)
    let coerced_value = coerce_types_from_shape(value, T::SHAPE);

    // Step 2: Collect all lines
    let mut lines = Vec::new();
    let mut state = DumpState::new();
    if let ConfigValue::Object(sourced) = &coerced_value
        && let facet_core::Type::User(facet_core::UserType::Struct(s)) = &T::SHAPE.ty
    {
        for field in s.fields {
            let key = field.name;
            if let Some(val) = sourced.value.get(key) {
                let is_sensitive = field.flags.contains(facet_core::FieldFlags::SENSITIVE);
                let field_shape = field.shape.get();
                collect_dump_lines(
                    val,
                    key,
                    0,
                    field_shape,
                    is_sensitive,
                    &mut lines,
                    &ctx,
                    &mut state,
                );
            }
        }
    }

    // Step 3: Calculate max widths per indent level
    let mut max_key_per_indent: HashMap<usize, usize> = HashMap::new();
    let mut max_val_per_indent: HashMap<usize, usize> = HashMap::new();

    for line in &lines {
        if !line.is_header {
            let key_width = visual_width(&line.key);
            let val_width = visual_width(&line.value);

            let key_max = max_key_per_indent.entry(line.indent).or_insert(0);
            *key_max = (*key_max).max(key_width);

            let val_max = max_val_per_indent.entry(line.indent).or_insert(0);
            *val_max = (*val_max).max(val_width);
        }
    }

    // Step 4: Print all lines with proper alignment
    for line in &lines {
        let indent_str = "  ".repeat(line.indent);

        if line.is_header {
            println!("{}{}", indent_str, line.key);
        } else {
            let key_width = visual_width(&line.key);
            let val_width = visual_width(&line.value);

            let max_key = max_key_per_indent.get(&line.indent).copied().unwrap_or(0);
            let max_val = max_val_per_indent.get(&line.indent).copied().unwrap_or(0);

            let key_padding = if key_width < max_key {
                ".".repeat(max_key - key_width)
            } else {
                String::new()
            };

            // Check if value needs wrapping
            if val_width > ctx.max_value_width {
                // Wrap value within its column
                let wrapped_lines = wrap_value(&line.value, ctx.max_value_width);

                for (i, wrapped_line) in wrapped_lines.iter().enumerate() {
                    if i == 0 {
                        // First line: show key, dots, value start, and provenance
                        let wrap_width = visual_width(wrapped_line);
                        let val_padding = if wrap_width < ctx.max_value_width {
                            ".".repeat(ctx.max_value_width - wrap_width)
                        } else {
                            String::new()
                        };
                        println!(
                            "{}{}{}  {}{} {}",
                            indent_str,
                            line.key,
                            key_padding.bright_black(),
                            wrapped_line,
                            val_padding.bright_black(),
                            line.provenance,
                        );
                    } else {
                        // Continuation lines: indent to value column
                        let continuation_indent = indent_str.len() + max_key + 2;
                        let spaces = " ".repeat(continuation_indent);
                        println!("{}{}", spaces, wrapped_line);
                    }
                }
            } else {
                // Normal single-line format with dot padding
                let val_padding = if val_width < max_val.min(ctx.max_value_width) {
                    ".".repeat(max_val.min(ctx.max_value_width) - val_width)
                } else {
                    String::new()
                };

                println!(
                    "{}{}{}  {}{} {}",
                    indent_str,
                    line.key,
                    key_padding.bright_black(),
                    line.value,
                    val_padding.bright_black(),
                    line.provenance,
                );
            }
        }
    }

    println!();

    // Show truncation notice if any values were truncated
    if state.had_truncation {
        println!();
        println!(
            "Some values were truncated. To show full values, rerun with {}=1",
            "FACET_ARGS_BLAST_IT".yellow()
        );
    }
}

/// Recursively collect lines to be printed.
#[allow(clippy::too_many_arguments)]
fn collect_dump_lines(
    value: &ConfigValue,
    path: &str,
    indent: usize,
    shape: &'static facet_core::Shape,
    is_sensitive: bool,
    lines: &mut Vec<DumpLine>,
    ctx: &DumpContext,
    state: &mut DumpState,
) {
    match value {
        ConfigValue::Object(sourced) => {
            // Add header line for this object
            if !path.is_empty() {
                lines.push(DumpLine {
                    indent,
                    key: path.to_string(),
                    value: String::new(),
                    provenance: String::new(),
                    is_header: true,
                });
            }

            // Iterate in struct field order
            if let facet_core::Type::User(facet_core::UserType::Struct(s)) = &shape.ty {
                for field in s.fields {
                    let key = field.name;
                    if let Some(val) = sourced.value.get(key) {
                        let is_sensitive = field.flags.contains(facet_core::FieldFlags::SENSITIVE);
                        let field_shape = field.shape.get();
                        collect_dump_lines(
                            val,
                            key,
                            indent + 1,
                            field_shape,
                            is_sensitive,
                            lines,
                            ctx,
                            state,
                        );
                    }
                }
            } else {
                // Fallback: iterate in insertion order
                for (key, val) in sourced.value.iter() {
                    collect_dump_lines(val, key, indent + 1, shape, false, lines, ctx, state);
                }
            }
        }
        ConfigValue::Array(sourced) => {
            // Add header for array
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: String::new(),
                provenance: String::new(),
                is_header: true,
            });

            for (i, item) in sourced.value.iter().enumerate() {
                let element_shape = shape.inner.unwrap_or(shape);
                collect_dump_lines(
                    item,
                    &format!("[{}]", i),
                    indent + 1,
                    element_shape,
                    false,
                    lines,
                    ctx,
                    state,
                );
            }
        }
        ConfigValue::String(sourced) => {
            let colored_value = if is_sensitive {
                let len = sourced.value.len();
                format!("🔒 [REDACTED ({} bytes)]", len)
                    .bright_magenta()
                    .to_string()
            } else {
                // Replace newlines with visual indicator
                let escaped = sourced.value.replace('\n', "↵");
                let (truncated, was_truncated) = truncate_middle(&escaped, ctx.max_string_length);
                if was_truncated {
                    state.had_truncation = true;
                }
                format!("\"{}\"", truncated).green().to_string()
            };
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: format_provenance(&sourced.provenance),
                is_header: false,
            });
        }
        ConfigValue::Integer(sourced) => {
            let colored_value = sourced.value.to_string().blue().to_string();
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: format_provenance(&sourced.provenance),
                is_header: false,
            });
        }
        ConfigValue::Float(sourced) => {
            let colored_value = sourced.value.to_string().bright_blue().to_string();
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: format_provenance(&sourced.provenance),
                is_header: false,
            });
        }
        ConfigValue::Bool(sourced) => {
            let colored_value = if sourced.value {
                sourced.value.to_string().green().to_string()
            } else {
                sourced.value.to_string().red().to_string()
            };
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: format_provenance(&sourced.provenance),
                is_header: false,
            });
        }
        ConfigValue::Null(sourced) => {
            let colored_value = "null".bright_black().to_string();
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: format_provenance(&sourced.provenance),
                is_header: false,
            });
        }
        ConfigValue::Enum(sourced) => {
            // Add header line showing variant name
            let variant_display = format!("{}::", sourced.value.variant).cyan().to_string();
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: variant_display,
                provenance: format_provenance(&sourced.provenance),
                is_header: true,
            });

            // Dump the enum's fields
            for (key, val) in sourced.value.fields.iter() {
                collect_dump_lines(val, key, indent + 1, shape, false, lines, ctx, state);
            }
        }
        ConfigValue::Missing(_info) => {
            // Show big red MISSING marker
            let colored_value = "❌ MISSING (required)".red().bold().to_string();
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: String::new(),
                is_header: false,
            });

            // TODO: Add help text showing CLI/env/file options
            // For now, just mark it as missing - we can enhance this later
        }
    }
}

/// Calculate visual width of a string after stripping ANSI codes.
fn visual_width(s: &str) -> usize {
    let bytes = s.as_bytes();
    let stripped = strip_ansi_escapes::strip(bytes);
    let stripped_str = core::str::from_utf8(&stripped).unwrap_or(s);
    stripped_str.width()
}

/// Truncate a string in the middle if it exceeds max_length.
/// For example: "this is a very long string" -> "this is a...g string"
/// Returns (truncated_string, was_truncated)
fn truncate_middle(s: &str, max_length: usize) -> (String, bool) {
    if s.len() <= max_length {
        return (s.to_string(), false);
    }

    // Reserve 3 chars for "..."
    if max_length < 3 {
        return ("...".to_string(), true);
    }

    let available = max_length - 3;
    let start_len = available.div_ceil(2); // Round up for start
    let end_len = available / 2;

    let start = s.chars().take(start_len).collect::<String>();
    let end = s
        .chars()
        .rev()
        .take(end_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();

    (format!("{}...{}", start, end), true)
}
/// Wrap a value string to fit within max_width, preserving ANSI color codes.
/// Returns a vector of lines with color codes reapplied to each line.
fn wrap_value(value: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;
    let mut in_ansi = false;
    let mut ansi_buffer = String::new();
    let mut active_color = String::new(); // Track the last color code

    for ch in value.chars() {
        if ch == '\x1b' {
            // Start of ANSI escape sequence
            in_ansi = true;
            ansi_buffer.push(ch);
        } else if in_ansi {
            ansi_buffer.push(ch);
            if ch == 'm' {
                // End of ANSI escape sequence
                current_line.push_str(&ansi_buffer);
                active_color = ansi_buffer.clone(); // Save this color
                ansi_buffer.clear();
                in_ansi = false;
            }
        } else {
            // Regular character
            if current_width >= max_width {
                // Need to wrap - close current line and start new one with same color
                lines.push(current_line);
                current_line = String::new();
                if !active_color.is_empty() {
                    current_line.push_str(&active_color); // Reapply color to new line
                }
                current_width = 0;
            }
            current_line.push(ch);
            current_width += 1;
        }
    }

    // Push remaining content
    if !current_line.is_empty() || !ansi_buffer.is_empty() {
        current_line.push_str(&ansi_buffer);
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Format provenance with colors.
fn format_provenance(prov: &Option<Provenance>) -> String {
    match prov {
        Some(Provenance::Cli { arg, .. }) => format!("{}", arg.cyan()),
        Some(Provenance::Env { var, .. }) => format!("{}", format!("${}", var).yellow()),
        Some(Provenance::File { file, offset, .. }) => {
            // Calculate line number from byte offset
            let line_num = calculate_line_number(&file.contents, *offset);
            // Extract just filename if full path was shown at start
            let filename = std::path::Path::new(file.path.as_str())
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file.path.as_str());
            format!("{}:{}", filename, line_num).magenta().to_string()
        }
        Some(Provenance::Default) => "DEFAULT".bright_black().to_string(),
        None => "".to_string(),
    }
}

/// Calculate line number (1-based) from byte offset in file contents.
fn calculate_line_number(contents: &str, offset: usize) -> usize {
    if offset == 0 {
        return 1;
    }

    // Count newlines before the offset
    let line_count = contents[..offset.min(contents.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count();

    line_count + 1
}
