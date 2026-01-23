//! Unified TUI for dibs - shows schema, diff, and migrations in one interface.

use std::io::{self, stdout};
use std::sync::mpsc;
use std::time::Duration;

use arborium::Highlighter;
use arborium_theme::builtin;
use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use dibs_proto::{
    DibsError, DiffRequest, DiffResult, MigrationInfo, MigrationStatusRequest, SchemaInfo, SqlError,
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::{
    prelude::*,
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Tabs,
    },
};
use roam::session::{CallError, RoamError};

use crate::config::Config;
use crate::highlight::highlight_to_lines;
use crate::service::{self, BuildOutput, ServiceConnection};

/// Parse SQL into migration function calls.
///
/// Splits SQL by statements (semicolons) and generates appropriate
/// ctx.execute() calls. Multi-line statements use raw string literals.
fn parse_sql_to_calls(sql: &str) -> String {
    let mut result = String::new();
    let mut current_statement = String::new();

    for line in sql.lines() {
        let trimmed = line.trim();

        // Handle SQL comments - convert to Rust comments
        if trimmed.starts_with("--") {
            // Flush any pending statement first
            if !current_statement.trim().is_empty() {
                result.push_str(&format_sql_call(&current_statement));
                current_statement.clear();
            }
            // Add as Rust comment
            result.push_str(&format!(
                "    // {}\n",
                trimmed.trim_start_matches("--").trim()
            ));
            continue;
        }

        // Skip empty lines between statements
        if trimmed.is_empty() && current_statement.trim().is_empty() {
            continue;
        }

        // Add line to current statement
        if !current_statement.is_empty() {
            current_statement.push('\n');
        }
        current_statement.push_str(line);

        // Check if statement is complete (ends with semicolon)
        if trimmed.ends_with(';') {
            result.push_str(&format_sql_call(&current_statement));
            current_statement.clear();
        }
    }

    // Handle any remaining statement without trailing semicolon
    if !current_statement.trim().is_empty() {
        result.push_str(&format_sql_call(&current_statement));
    }

    result
}

/// Format a single SQL statement as a ctx.execute() call.
fn format_sql_call(sql: &str) -> String {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Remove trailing semicolon for the execute call (postgres doesn't need it)
    let sql_clean = trimmed.trim_end_matches(';').trim();

    // Check if it's a single line or multi-line
    if sql_clean.contains('\n') {
        // Multi-line: use raw string literal
        format!("    ctx.execute(r#\"\n{}\n\"#).await?;\n", sql_clean)
    } else {
        // Single line: use regular string
        format!(
            "    ctx.execute(\"{}\").await?;\n",
            sql_clean.replace('"', "\\\"")
        )
    }
}

/// Format a SqlError with ariadne for nice display.
fn format_sql_error(err: &SqlError) -> String {
    use ariadne::{Config, Label, Report, ReportKind, Source};

    let mut result = String::new();

    // If we have caller location, try to render the Rust source with ariadne
    if let Some(caller) = &err.caller {
        if let Some((file_path, line, _col)) = parse_caller_location(caller) {
            // Try to resolve the path and read the source file
            if let Some(resolved_path) = resolve_source_path(&file_path)
                && let Ok(source) = std::fs::read_to_string(&resolved_path)
            {
                // Calculate byte offset for the line
                if let Some(byte_offset) = line_to_byte_offset(&source, line) {
                    let mut output = Vec::new();

                    let builder = Report::build(
                        ReportKind::Error,
                        (&file_path, byte_offset..byte_offset + 1),
                    )
                    .with_message(&err.message)
                    .with_config(Config::default().with_color(false))
                    .with_label(
                        Label::new((&file_path, byte_offset..byte_offset + 1))
                            .with_message(&err.message),
                    );

                    let report = builder.finish();
                    report
                        .write((&file_path, Source::from(&source)), &mut output)
                        .ok();

                    result.push_str(&String::from_utf8_lossy(&output));

                    // Add hint/detail if available
                    if let Some(hint) = &err.hint {
                        result.push_str(&format!("\nHint: {}", hint));
                    }
                    if let Some(detail) = &err.detail {
                        result.push_str(&format!("\nDetail: {}", detail));
                    }

                    return result;
                }
            }
        }
        // Fallback: just show the location
        result.push_str(&format!("At: {}\n\n", caller));
    }

    // If we have SQL, render with ariadne (with or without position)
    if let Some(sql) = &err.sql {
        // Determine position - use provided position or default to start of SQL
        let pos = err.position.map(|p| p as usize).unwrap_or(1);
        // ariadne uses 0-indexed byte offsets, postgres gives 1-indexed
        let pos = pos.saturating_sub(1);
        // Clamp position to valid range
        let pos = pos.min(sql.len().saturating_sub(1));

        let mut output = Vec::new();

        let mut builder = Report::build(ReportKind::Error, ("sql", pos..pos.saturating_add(1)))
            .with_message(&err.message)
            .with_config(Config::default().with_color(false));

        // Add label at the error position (only if we have a specific position)
        if err.position.is_some() {
            builder = builder.with_label(
                Label::new(("sql", pos..pos.saturating_add(1))).with_message(&err.message),
            );
        }

        // Add hint if available
        if let Some(hint) = &err.hint {
            builder = builder.with_help(hint);
        }

        // Add detail as a note if available
        if let Some(detail) = &err.detail {
            builder = builder.with_note(detail);
        }

        let report = builder.finish();

        // Write to string
        report.write(("sql", Source::from(sql)), &mut output).ok();

        result.push_str(&String::from_utf8_lossy(&output));
    } else {
        // No SQL context, just format the message
        result.push_str(&err.message);
        if let Some(detail) = &err.detail {
            result.push_str(&format!("\nDetail: {}", detail));
        }
        if let Some(hint) = &err.hint {
            result.push_str(&format!("\nHint: {}", hint));
        }
    }

    result
}

/// Parse a caller location string like "path/to/file.rs:14:5" into (path, line, col)
fn parse_caller_location(caller: &str) -> Option<(String, usize, usize)> {
    // Format is "file:line:col"
    let parts: Vec<&str> = caller.rsplitn(3, ':').collect();
    if parts.len() == 3 {
        let col: usize = parts[0].parse().ok()?;
        let line: usize = parts[1].parse().ok()?;
        let file = parts[2].to_string();
        Some((file, line, col))
    } else {
        None
    }
}

/// Convert a 1-indexed line number to a byte offset in the source
fn line_to_byte_offset(source: &str, line: usize) -> Option<usize> {
    if line == 0 {
        return None;
    }
    let mut current_line = 1;
    for (i, c) in source.char_indices() {
        if current_line == line {
            return Some(i);
        }
        if c == '\n' {
            current_line += 1;
        }
    }
    // If we're looking for the last line and it doesn't end with newline
    if current_line == line {
        return Some(source.len().saturating_sub(1));
    }
    None
}

/// Resolve a potentially relative file path to an absolute path.
/// Handles the case where file!() returns workspace-relative paths.
fn resolve_source_path(file_path: &str) -> Option<String> {
    let path = std::path::Path::new(file_path);

    // If already absolute and exists, use it
    if path.is_absolute() {
        if path.exists() {
            return Some(file_path.to_string());
        }
        return None;
    }

    // Try from current directory
    if let Ok(cwd) = std::env::current_dir() {
        let candidate = cwd.join(path);
        if candidate.exists() {
            return Some(candidate.display().to_string());
        }

        // Walk up the directory tree looking for a match
        // This handles the case where cwd is inside the workspace
        let mut dir = cwd.as_path();
        while let Some(parent) = dir.parent() {
            let candidate = parent.join(path);
            if candidate.exists() {
                return Some(candidate.display().to_string());
            }
            dir = parent;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_caller_location_valid() {
        let result = parse_caller_location("src/lib.rs:42:5");
        assert_eq!(result, Some(("src/lib.rs".to_string(), 42, 5)));
    }

    #[test]
    fn test_parse_caller_location_deep_path() {
        let result = parse_caller_location("examples/my-app-db/src/migrations/m2026_01_19.rs:14:5");
        assert_eq!(
            result,
            Some((
                "examples/my-app-db/src/migrations/m2026_01_19.rs".to_string(),
                14,
                5
            ))
        );
    }

    #[test]
    fn test_parse_caller_location_invalid() {
        assert_eq!(parse_caller_location("no-colons"), None);
        assert_eq!(parse_caller_location("file:notanumber:5"), None);
        assert_eq!(parse_caller_location("file:5:notanumber"), None);
    }

    #[test]
    fn test_line_to_byte_offset() {
        let source = "line1\nline2\nline3\n";
        assert_eq!(line_to_byte_offset(source, 1), Some(0)); // start of line1
        assert_eq!(line_to_byte_offset(source, 2), Some(6)); // start of line2
        assert_eq!(line_to_byte_offset(source, 3), Some(12)); // start of line3
        assert_eq!(line_to_byte_offset(source, 0), None); // invalid line
    }

    #[test]
    fn test_line_to_byte_offset_no_trailing_newline() {
        let source = "line1\nline2";
        assert_eq!(line_to_byte_offset(source, 1), Some(0));
        assert_eq!(line_to_byte_offset(source, 2), Some(6));
    }

    #[test]
    fn test_resolve_source_path_finds_file_in_parent() {
        // This test verifies the walk-up-the-tree logic
        // We can't easily test this without setting up a temp directory structure
        // but at least verify it doesn't panic on non-existent paths
        let result = resolve_source_path("definitely/does/not/exist.rs");
        assert_eq!(result, None);
    }

    #[test]
    fn test_error_pointer_padding() {
        // The error display format is:
        // "> 1234 code_here()"
        //   ^--^  = 2 chars for "> " prefix
        //      ^--^ = 5 chars for line number format "{:4} " (4 digits + space)
        // Total prefix = 7 chars before code content starts

        // For column 1 (first char of code), we need 7 spaces before ^
        // padding = 7 + col.saturating_sub(1) = 7 + 0 = 7
        assert_eq!(7 + 1_usize.saturating_sub(1), 7);

        // For column 5, we need 7 + 4 = 11 spaces before ^
        // padding = 7 + col.saturating_sub(1) = 7 + 4 = 11
        assert_eq!(7 + 5_usize.saturating_sub(1), 11);

        // For column 48 (e.g., where ? is at end of line), we need 7 + 47 = 54 spaces
        assert_eq!(7 + 48_usize.saturating_sub(1), 54);
    }

    #[test]
    fn test_effective_col_calculation() {
        // We point to the first non-whitespace char, not the track_caller col
        // (track_caller col points to `?` at end of line, which isn't helpful)

        // Line with 4 spaces of indentation
        let line = "    ctx.execute(\"...\").await?;";
        let leading_ws = line.chars().take_while(|c| c.is_whitespace()).count();
        let effective_col = leading_ws + 1; // 1-indexed
        assert_eq!(effective_col, 5); // points to 'c' in ctx

        // Line with no indentation
        let line = "ctx.execute(\"...\").await?;";
        let leading_ws = line.chars().take_while(|c| c.is_whitespace()).count();
        let effective_col = leading_ws + 1;
        assert_eq!(effective_col, 1); // points to 'c' in ctx

        // Line with tabs (each tab counts as 1 char)
        let line = "\t\tctx.execute(\"...\").await?;";
        let leading_ws = line.chars().take_while(|c| c.is_whitespace()).count();
        let effective_col = leading_ws + 1;
        assert_eq!(effective_col, 3); // points to 'c' in ctx
    }
}

/// The main unified TUI application
pub struct App {
    /// Current phase of the app
    phase: AppPhase,
    /// Current tab (when in Connected phase)
    tab: Tab,
    /// Service connection (if available)
    conn: Option<ServiceConnection>,
    /// Database URL
    database_url: Option<String>,
    /// Schema info (fetched from service)
    schema: Option<SchemaInfo>,
    /// Diff state
    diff: DiffState,
    /// Migration status (fetched on demand)
    migrations: Option<Vec<MigrationInfo>>,
    /// Loading state (for schema/migrations)
    loading: Option<String>,
    /// Error message
    error: Option<String>,
    /// Selected table index (for Rust tab)
    table_state: ListState,
    selected_table: usize,
    /// Selected migration index (for Postgres tab)
    migration_state: ListState,
    selected_migration: usize,
    /// Pending 'g' for gg
    pending_g: bool,
    /// Whether to show migration source viewer
    show_migration_source: bool,
    /// Scroll position in migration source viewer
    source_scroll: u16,
    /// Syntax highlighter
    highlighter: Highlighter,
    /// Theme for syntax highlighting
    theme: arborium_theme::Theme,
    /// Build output lines (during build phase)
    build_output: Vec<BuildOutput>,
    /// Scroll position in build output
    build_scroll: usize,
    /// Auto-scroll build output
    build_auto_scroll: bool,
    /// Whether showing migration name input dialog
    show_migration_dialog: bool,
    /// Migration name being entered
    migration_name_input: String,
    /// Cursor position in migration name input
    migration_name_cursor: usize,
    /// Whether showing error modal (for long errors)
    show_error_modal: bool,
    /// Error lines for modal display (pre-highlighted)
    error_modal_lines: Vec<Line<'static>>,
    /// Which pane has focus in Rust tab (0 = table list, 1 = details)
    schema_focus: usize,
    /// Selected item index in details pane (columns then foreign keys)
    details_selection: usize,
    /// Scroll position in schema details pane
    details_scroll: u16,
    /// Flag to trigger a rebuild
    needs_rebuild: bool,
    /// Postgres tab mode (HasPending or AllApplied)
    postgres_mode: PostgresMode,
    /// Postgres tab selection state
    postgres_selection: PostgresSelection,
    /// Whether we're currently rebuilding (for spinner display)
    rebuilding: bool,
    /// File watcher for auto-rebuild on source changes
    file_watcher_rx: Option<std::sync::mpsc::Receiver<()>>,
    /// Pending migration to apply and commit after rebuild (path, name)
    pending_migration_commit: Option<(String, String)>,
}

/// The current phase of the application.
enum AppPhase {
    /// Building/starting the service
    Building,
    /// Connected and ready
    Connected,
    /// Failed to connect
    Failed(String),
}

/// State of the diff computation.
enum DiffState {
    /// No DATABASE_URL configured
    NoDatabaseUrl,
    /// Not yet loaded
    NotLoaded,
    /// Currently loading
    Loading,
    /// Successfully loaded
    Loaded(DiffResult),
    /// Failed to load
    Error(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Rust,
    Postgres,
}

impl Tab {
    fn all() -> &'static [Tab] {
        &[Tab::Rust, Tab::Postgres]
    }

    fn index(self) -> usize {
        match self {
            Tab::Rust => 0,
            Tab::Postgres => 1,
        }
    }

    fn from_index(i: usize) -> Self {
        match i {
            0 => Tab::Rust,
            _ => Tab::Postgres,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Tab::Rust => "Rust",
            Tab::Postgres => "Postgres",
        }
    }
}

/// The current mode of the Postgres tab.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PostgresMode {
    /// Has unapplied migrations - blocks diff/generation
    HasPending,
    /// All caught up - can generate new migrations
    AllApplied,
}

/// Selection state within the Postgres tab.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PostgresSelection {
    /// Selected a migration by index
    Migration(usize),
    /// Selected the virtual "New Changes" item
    NewChanges,
}

impl App {
    pub fn new() -> Self {
        let mut table_state = ListState::default();
        table_state.select(Some(0));
        let mut migration_state = ListState::default();
        migration_state.select(Some(0));

        let database_url = std::env::var("DATABASE_URL").ok();
        let diff = if database_url.is_some() {
            DiffState::NotLoaded
        } else {
            DiffState::NoDatabaseUrl
        };

        Self {
            phase: AppPhase::Building,
            tab: Tab::Rust,
            conn: None,
            database_url,
            schema: None,
            diff,
            migrations: None,
            loading: Some("Connecting...".to_string()),
            error: None,
            table_state,
            selected_table: 0,
            migration_state,
            selected_migration: 0,
            pending_g: false,
            show_migration_source: false,
            source_scroll: 0,
            highlighter: Highlighter::new(),
            theme: builtin::catppuccin_mocha().clone(),
            build_output: Vec::new(),
            build_scroll: 0,
            build_auto_scroll: true,
            show_migration_dialog: false,
            migration_name_input: String::new(),
            migration_name_cursor: 0,
            show_error_modal: false,
            error_modal_lines: Vec::new(),
            schema_focus: 0,
            details_selection: 0,
            details_scroll: 0,
            needs_rebuild: false,
            postgres_mode: PostgresMode::AllApplied,
            postgres_selection: PostgresSelection::NewChanges,
            rebuilding: false,
            file_watcher_rx: None,
            pending_migration_commit: None,
        }
    }

    /// Set up file watcher for auto-rebuild on source changes.
    /// Watches the crate's src/ directory for .rs file changes.
    fn setup_file_watcher(&mut self, config: &Config) -> Option<RecommendedWatcher> {
        let (tx, rx) = mpsc::channel::<()>();
        self.file_watcher_rx = Some(rx);

        // Find the crate path to watch
        let watch_path = if let Some(crate_name) = &config.db.crate_name {
            // Find the crate using cargo metadata
            if let Some(path) = crate::config::find_crate_path_for_watch(crate_name) {
                path.join("src")
            } else {
                std::path::PathBuf::from("src")
            }
        } else {
            std::path::PathBuf::from("src")
        };

        // Create a debounced watcher
        let mut last_event = std::time::Instant::now();
        let debounce_duration = Duration::from_millis(500);

        let watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    // Only trigger on .rs file modifications
                    let is_rs_change = event
                        .paths
                        .iter()
                        .any(|p| p.extension().map(|e| e == "rs").unwrap_or(false));

                    if is_rs_change
                        && matches!(
                            event.kind,
                            notify::EventKind::Modify(_)
                                | notify::EventKind::Create(_)
                                | notify::EventKind::Remove(_)
                        )
                    {
                        // Debounce: only send if enough time has passed
                        let now = std::time::Instant::now();
                        if now.duration_since(last_event) > debounce_duration {
                            last_event = now;
                            let _ = tx.send(());
                        }
                    }
                }
            });

        match watcher {
            Ok(mut w) => {
                if watch_path.exists()
                    && let Err(e) = w.watch(&watch_path, RecursiveMode::Recursive)
                {
                    eprintln!("Warning: Failed to watch {}: {}", watch_path.display(), e);
                    return None;
                }
                Some(w)
            }
            Err(e) => {
                eprintln!("Warning: Failed to create file watcher: {}", e);
                None
            }
        }
    }

    /// Check for file change notifications (non-blocking).
    fn check_file_changes(&mut self) -> bool {
        if let Some(rx) = &self.file_watcher_rx {
            // Drain all pending notifications and return true if any
            let mut changed = false;
            while rx.try_recv().is_ok() {
                changed = true;
            }
            changed
        } else {
            false
        }
    }

    /// Run the TUI
    pub fn run(mut self, config: Option<&Config>) -> io::Result<()> {
        // Set up terminal
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

        // Create a tokio runtime for async operations
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        // If we have config, start building with TUI visible
        let result = if let Some(cfg) = config {
            self.run_with_build(&mut terminal, &rt, cfg)
        } else {
            self.phase = AppPhase::Failed("No .config/dibs.styx config found".to_string());
            self.main_loop(&mut terminal, &rt)
        };

        // Restore terminal
        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;

        result
    }

    /// Run the TUI with build phase, then main loop.
    /// Supports rebuilding via the 'R' key or automatic file watching.
    fn run_with_build(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        rt: &tokio::runtime::Runtime,
        config: &Config,
    ) -> io::Result<()> {
        // Set up file watcher for auto-rebuild (keep watcher alive in scope)
        let _watcher = self.setup_file_watcher(config);

        loop {
            // Reset state for fresh build
            self.phase = AppPhase::Building;
            self.conn = None;
            self.build_output.clear();
            self.build_scroll = 0;
            self.build_auto_scroll = true;
            self.needs_rebuild = false;
            self.rebuilding = false;
            self.error = None;

            // Start the build process
            let mut build_process = match rt.block_on(service::start_service(config)) {
                Ok(bp) => bp,
                Err(e) => {
                    self.phase = AppPhase::Failed(format!("Failed to start service: {}", e));
                    self.main_loop(terminal, rt)?;
                    if self.needs_rebuild {
                        continue;
                    }
                    return Ok(());
                }
            };

            // Build phase loop - show cargo output while waiting for connection
            let build_result = self.build_loop(terminal, rt, &mut build_process);

            match build_result {
                Ok(true) => {
                    // Connected, run main loop
                    let result = self.main_loop(terminal, rt);
                    if self.needs_rebuild {
                        // User requested rebuild or file changed, loop again
                        continue;
                    }
                    return result;
                }
                Ok(false) => {
                    // User quit during build
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Build phase loop - returns Ok(true) if connected, Ok(false) if user quit.
    fn build_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        rt: &tokio::runtime::Runtime,
        build_process: &mut service::BuildProcess,
    ) -> io::Result<bool> {
        loop {
            // Draw the build UI
            terminal.draw(|frame| self.render_build_phase(frame))?;

            // Poll for build output (non-blocking)
            while let Some(line) = build_process.try_read_line() {
                self.build_output.push(line);
                if self.build_auto_scroll {
                    self.build_scroll = self.build_output.len().saturating_sub(1);
                }
            }

            // Check if the child process exited with an error
            if let Some(status) = rt.block_on(build_process.check_exit())
                && !status.success()
            {
                self.phase = AppPhase::Failed(format!(
                    "Build failed with exit code: {}",
                    status.code().unwrap_or(-1)
                ));
                // Stay in failed state, let main_loop handle it
                return Ok(true);
            }

            // Try to accept a connection
            match rt.block_on(build_process.try_accept()) {
                Ok(Some(conn)) => {
                    // Connected! Transition to main phase
                    self.conn = Some(conn);
                    self.phase = AppPhase::Connected;
                    self.loading = Some("Fetching schema...".to_string());

                    // Fetch initial data
                    rt.block_on(self.fetch_initial_data());

                    return Ok(true);
                }
                Ok(None) => {
                    // No connection yet, continue
                }
                Err(e) => {
                    self.phase = AppPhase::Failed(format!("Connection failed: {}", e));
                    return Ok(true);
                }
            }

            // Handle input (with short timeout for responsiveness)
            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(false),
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.build_auto_scroll = false;
                        self.build_scroll = self.build_scroll.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.build_scroll < self.build_output.len().saturating_sub(1) {
                            self.build_scroll += 1;
                        } else {
                            self.build_auto_scroll = true;
                        }
                    }
                    KeyCode::Char('G') => {
                        self.build_scroll = self.build_output.len().saturating_sub(1);
                        self.build_auto_scroll = true;
                    }
                    KeyCode::Char('g') => {
                        self.build_scroll = 0;
                        self.build_auto_scroll = false;
                    }
                    _ => {}
                }
            }
        }
    }

    /// Fetch initial data after connection is established.
    async fn fetch_initial_data(&mut self) {
        // Clone what we need to avoid borrow conflicts
        let Some(conn) = &self.conn else { return };
        let client = conn.client().clone();
        let database_url = self.database_url.clone();

        // Check if we have a pending migration to apply and commit
        let pending_commit = self.pending_migration_commit.take();

        // Fetch schema
        match client.schema().await {
            Ok(schema) => self.schema = Some(schema),
            Err(e) => self.show_error(format!("Schema fetch: {:?}", e)),
        }

        // Fetch migrations and diff if we have a database URL
        if let Some(url) = database_url.clone() {
            self.loading = Some("Fetching migration status...".to_string());
            match client
                .migration_status(MigrationStatusRequest {
                    database_url: url.clone(),
                })
                .await
            {
                Ok(migrations) => {
                    self.update_postgres_mode(&migrations);
                    self.migrations = Some(migrations);
                }
                Err(e) => self.show_error(format!("Migration status: {:?}", e)),
            }

            // Also fetch diff
            self.diff = DiffState::Loading;
            match client.diff(DiffRequest { database_url: url }).await {
                Ok(diff) => self.diff = DiffState::Loaded(diff),
                Err(e) => self.diff = DiffState::Error(format!("{:?}", e)),
            }
        }
        self.loading = None;

        // If we have a pending migration commit, apply and commit it now
        if let Some((path, name)) = pending_commit {
            self.apply_and_commit_migration(&path, &name).await;
        }
    }

    /// Apply pending migrations and commit to git.
    async fn apply_and_commit_migration(&mut self, path: &str, name: &str) {
        self.loading = Some("Applying migration...".to_string());

        // Apply migrations
        if let (Some(conn), Some(url)) = (&self.conn, &self.database_url) {
            use dibs_proto::MigrateRequest;

            let (log_tx, mut log_rx) = roam::channel::<dibs_proto::MigrationLog>();

            let client = conn.client().clone();
            let url = url.clone();

            let result = client
                .migrate(
                    MigrateRequest {
                        database_url: url,
                        migration: None,
                    },
                    log_tx,
                )
                .await;

            // Drain any remaining logs
            while let Ok(Some(_)) = log_rx.recv().await {}

            match result {
                Ok(res) => {
                    if !res.applied.is_empty() {
                        // Commit the migration
                        self.loading = Some("Committing...".to_string());
                        if let Err(e) = self.git_commit(name) {
                            self.show_error(format!("Migration applied but commit failed: {}", e));
                        } else {
                            self.error = Some(format!(
                                "Applied {} migration(s), committed: {}",
                                res.applied.len(),
                                name
                            ));
                        }
                    } else {
                        // No migrations applied (maybe already applied?)
                        // Still commit the new file
                        self.loading = Some("Committing...".to_string());
                        if let Err(e) = self.git_commit(name) {
                            self.show_error(format!("File created but commit failed: {}", e));
                        } else {
                            self.error = Some(format!("Created and committed: {}", path));
                        }
                    }
                    // Refresh migrations list and diff
                    self.refresh_migrations().await;
                    self.refresh_diff().await;
                }
                Err(e) => {
                    self.show_migration_error(&e);
                    // Migration failed - unstage the file
                    let _ = self.git_reset(path);
                }
            }
        }

        self.loading = None;
    }

    /// Commit staged changes with a migration message.
    fn git_commit(&self, name: &str) -> Result<(), String> {
        use std::process::Command;

        let message = format!("migration: {}", name);

        let output = Command::new("git")
            .args(["commit", "-m", &message])
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git commit failed: {}", stderr));
        }

        Ok(())
    }

    /// Unstage a file (git reset).
    fn git_reset(&self, path: &str) -> Result<(), String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["reset", "HEAD", "--", path])
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git reset failed: {}", stderr));
        }

        Ok(())
    }

    /// Update postgres_mode based on migration status.
    fn update_postgres_mode(&mut self, migrations: &[MigrationInfo]) {
        let pending_count = migrations.iter().filter(|m| !m.applied).count();
        if pending_count > 0 {
            self.postgres_mode = PostgresMode::HasPending;
            // Select first pending migration
            if let Some(first_pending) = migrations.iter().position(|m| !m.applied) {
                self.postgres_selection = PostgresSelection::Migration(first_pending);
                self.selected_migration = first_pending;
                self.migration_state.select(Some(first_pending));
            }
        } else {
            self.postgres_mode = PostgresMode::AllApplied;
            // Default to NewChanges in AllApplied mode
            self.postgres_selection = PostgresSelection::NewChanges;
            self.migration_state.select(None);
        }
    }

    /// Render the build phase UI.
    fn render_build_phase(&self, frame: &mut Frame) {
        let area = frame.area();

        // Layout: header + build output + status bar
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Build output
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled(" dibs ", Style::default().fg(Color::Cyan).bold()),
            Span::styled("Building...", Style::default().fg(Color::Yellow)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        frame.render_widget(header, chunks[0]);

        // Build output
        let visible_height = chunks[1].height.saturating_sub(2) as usize; // -2 for borders
        let start = self.build_scroll.saturating_sub(visible_height / 2);
        let lines: Vec<Line> = self
            .build_output
            .iter()
            .skip(start)
            .take(visible_height)
            .map(|output| {
                let style = if output.is_stderr {
                    // Cargo uses stderr for most output, color based on content
                    if output.text.contains("Compiling") {
                        Style::default().fg(Color::Green)
                    } else if output.text.contains("warning") {
                        Style::default().fg(Color::Yellow)
                    } else if output.text.contains("error") {
                        Style::default().fg(Color::Red)
                    } else if output.text.contains("Finished") {
                        Style::default().fg(Color::Cyan)
                    } else if output.text.contains("Running") {
                        Style::default().fg(Color::Magenta)
                    } else {
                        Style::default().fg(Color::White)
                    }
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(&output.text, style))
            })
            .collect();

        let output_block = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Build Output ")
                .title_style(Style::default().fg(Color::Cyan)),
        );
        frame.render_widget(output_block, chunks[1]);

        // Scrollbar
        if self.build_output.len() > visible_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None);
            let mut scrollbar_state =
                ScrollbarState::new(self.build_output.len()).position(self.build_scroll);
            frame.render_stateful_widget(
                scrollbar,
                chunks[1].inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }

        // Status bar
        let status = Paragraph::new(Line::from(vec![
            Span::styled(" j/k ", Style::default().fg(Color::Yellow)),
            Span::raw("scroll  "),
            Span::styled("g/G ", Style::default().fg(Color::Yellow)),
            Span::raw("top/bottom  "),
            Span::styled("q ", Style::default().fg(Color::Yellow)),
            Span::raw("quit  "),
            Span::raw("│  "),
            Span::styled(
                format!("{} lines", self.build_output.len()),
                Style::default().fg(Color::DarkGray),
            ),
            if self.build_auto_scroll {
                Span::styled("  [auto-scroll]", Style::default().fg(Color::DarkGray))
            } else {
                Span::raw("")
            },
        ]))
        .style(Style::default().bg(Color::DarkGray));
        frame.render_widget(status, chunks[2]);
    }

    fn main_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        rt: &tokio::runtime::Runtime,
    ) -> io::Result<()> {
        loop {
            // Check for file changes (triggers auto-rebuild)
            if self.check_file_changes() {
                self.needs_rebuild = true;
                return Ok(());
            }

            terminal.draw(|frame| self.ui(frame))?;

            // Use poll to allow checking file changes periodically
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }

            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                // Handle 'g' prefix for gg
                // Handle error modal
                if self.show_error_modal {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                            self.show_error_modal = false;
                            self.error_modal_lines.clear();
                        }
                        _ => {}
                    }
                    continue;
                }

                // Handle migration name dialog input
                if self.show_migration_dialog {
                    match key.code {
                        KeyCode::Esc => {
                            self.show_migration_dialog = false;
                            self.migration_name_input.clear();
                            self.migration_name_cursor = 0;
                        }
                        KeyCode::Enter => {
                            let name = if self.migration_name_input.is_empty() {
                                // Auto-generate a descriptive name based on the diff
                                self.suggest_migration_name()
                            } else {
                                self.migration_name_input.clone()
                            };
                            self.show_migration_dialog = false;
                            rt.block_on(self.generate_migration_with_name(&name));
                            self.migration_name_input.clear();
                            self.migration_name_cursor = 0;
                            // If migration was created, needs_rebuild is set - exit to trigger rebuild
                            if self.needs_rebuild {
                                return Ok(());
                            }
                        }
                        KeyCode::Backspace => {
                            if self.migration_name_cursor > 0 {
                                self.migration_name_cursor -= 1;
                                self.migration_name_input.remove(self.migration_name_cursor);
                            }
                        }
                        KeyCode::Delete => {
                            if self.migration_name_cursor < self.migration_name_input.len() {
                                self.migration_name_input.remove(self.migration_name_cursor);
                            }
                        }
                        KeyCode::Left => {
                            self.migration_name_cursor =
                                self.migration_name_cursor.saturating_sub(1);
                        }
                        KeyCode::Right => {
                            if self.migration_name_cursor < self.migration_name_input.len() {
                                self.migration_name_cursor += 1;
                            }
                        }
                        KeyCode::Home => {
                            self.migration_name_cursor = 0;
                        }
                        KeyCode::End => {
                            self.migration_name_cursor = self.migration_name_input.len();
                        }
                        KeyCode::Char(c) => {
                            // Only allow valid migration name chars
                            if c.is_alphanumeric() || c == '-' || c == '_' {
                                self.migration_name_input
                                    .insert(self.migration_name_cursor, c);
                                self.migration_name_cursor += 1;
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                if self.pending_g {
                    self.pending_g = false;
                    if key.code == KeyCode::Char('g') {
                        self.go_to_first();
                        continue;
                    }
                }

                match key.code {
                    KeyCode::Char('q') => {
                        // Close source viewer first, or quit
                        if self.show_migration_source {
                            self.show_migration_source = false;
                            self.source_scroll = 0;
                        } else {
                            return Ok(());
                        }
                    }
                    KeyCode::Esc => {
                        // Close source viewer or quit
                        if self.show_migration_source {
                            self.show_migration_source = false;
                            self.source_scroll = 0;
                        } else {
                            return Ok(());
                        }
                    }
                    // Tab switching (only when not viewing source)
                    KeyCode::Char('1') if !self.show_migration_source => {
                        self.tab = Tab::Rust;
                        self.schema_focus = 0;
                    }
                    KeyCode::Char('2') if !self.show_migration_source => {
                        self.tab = Tab::Postgres;
                    }
                    KeyCode::Tab if !self.show_migration_source => {
                        // In Rust tab, Tab cycles between panes
                        if self.tab == Tab::Rust {
                            self.schema_focus = (self.schema_focus + 1) % 2;
                        } else {
                            self.next_tab();
                        }
                    }
                    KeyCode::BackTab if !self.show_migration_source => {
                        // In Rust tab, Shift+Tab cycles between panes in reverse
                        if self.tab == Tab::Rust {
                            self.schema_focus = (self.schema_focus + 1) % 2;
                        } else {
                            self.prev_tab();
                        }
                    }
                    // Navigation
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.show_migration_source {
                            self.source_scroll = self.source_scroll.saturating_sub(1);
                        } else if self.tab == Tab::Rust && self.schema_focus == 1 {
                            self.details_selection_up();
                        } else if self.tab == Tab::Postgres {
                            self.postgres_move_up();
                        } else {
                            self.move_up();
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.show_migration_source {
                            self.source_scroll = self.source_scroll.saturating_add(1);
                        } else if self.tab == Tab::Rust && self.schema_focus == 1 {
                            self.details_selection_down();
                        } else if self.tab == Tab::Postgres {
                            self.postgres_move_down();
                        } else {
                            self.move_down();
                        }
                    }
                    KeyCode::Char('g')
                        if self.tab == Tab::Postgres
                            && self.postgres_mode == PostgresMode::AllApplied
                            && !self.show_migration_source =>
                    {
                        // Generate migration if there are changes
                        if let DiffState::Loaded(diff) = &self.diff {
                            if !diff.table_diffs.is_empty() {
                                // Start with empty input - user can leave empty for auto-generated name
                                self.migration_name_input.clear();
                                self.migration_name_cursor = 0;
                                self.show_migration_dialog = true;
                            } else {
                                self.error = Some("No changes to migrate".to_string());
                            }
                        } else {
                            self.error =
                                Some("No diff computed yet - press 'r' to refresh".to_string());
                        }
                    }
                    KeyCode::Char('g') if !self.show_migration_source => self.pending_g = true,
                    KeyCode::Char('G') if !self.show_migration_source => self.go_to_last(),
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if self.show_migration_source {
                            self.source_scroll = self.source_scroll.saturating_add(10);
                        } else {
                            self.half_page_down();
                        }
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if self.show_migration_source {
                            self.source_scroll = self.source_scroll.saturating_sub(10);
                        } else {
                            self.half_page_up();
                        }
                    }
                    // Enter to view migration source or follow FK
                    KeyCode::Enter => {
                        if self.tab == Tab::Rust && self.schema_focus == 1 {
                            self.follow_foreign_key();
                        } else if self.tab == Tab::Postgres && !self.show_migration_source {
                            // Only show source if a migration is selected (not NewChanges)
                            if let PostgresSelection::Migration(_) = self.postgres_selection {
                                self.show_migration_source = true;
                                self.source_scroll = 0;
                            }
                        } else if self.show_migration_source {
                            self.show_migration_source = false;
                            self.source_scroll = 0;
                        }
                    }
                    // Actions
                    KeyCode::Char('m') if !self.show_migration_source => {
                        // Run migrations (only in Postgres tab with pending migrations)
                        if self.tab == Tab::Postgres
                            && self.postgres_mode == PostgresMode::HasPending
                        {
                            rt.block_on(self.run_migrations());
                        }
                    }
                    KeyCode::Char('d') if !self.show_migration_source => {
                        // Delete migration (only if not committed)
                        if self.tab == Tab::Postgres
                            && let PostgresSelection::Migration(idx) = self.postgres_selection
                        {
                            self.selected_migration = idx;
                            self.delete_selected_migration();
                        }
                    }
                    KeyCode::Char('r') if !self.show_migration_source => {
                        // Refresh
                        rt.block_on(self.refresh());
                    }
                    KeyCode::Char('R') if !self.show_migration_source => {
                        // Rebuild - restart the service
                        self.needs_rebuild = true;
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }

    /// Show an error - uses modal for long errors, status bar for short ones.
    fn show_error(&mut self, msg: String) {
        // Use modal for multi-line errors or errors longer than 60 chars
        if msg.contains('\n') || msg.len() > 60 {
            self.error_modal_lines = msg.lines().map(|l| Line::from(l.to_string())).collect();
            self.show_error_modal = true;
            self.error = Some("Error occurred - see details".to_string());
        } else {
            self.error = Some(msg);
        }
    }

    /// Show an error with pre-highlighted lines.
    fn show_error_lines(&mut self, lines: Vec<Line<'static>>) {
        self.error_modal_lines = lines;
        self.show_error_modal = true;
        self.error = Some("Error occurred - see details".to_string());
    }

    /// Show a migration error with syntax-highlighted source code.
    fn show_migration_error(&mut self, err: &CallError<DibsError>) {
        // Try to extract SqlError from the nested error
        let sql_err = match err {
            CallError::Roam(RoamError::User(DibsError::MigrationFailed(e))) => e,
            _ => {
                self.show_error(format!("{}", err));
                return;
            }
        };

        let mut lines: Vec<Line<'static>> = Vec::new();

        // If we have caller location, show highlighted source
        if let Some(caller) = &sql_err.caller
            && let Some((file_path, line_num, col)) = parse_caller_location(caller)
            && let Some(resolved_path) = resolve_source_path(&file_path)
            && let Ok(source) = std::fs::read_to_string(&resolved_path)
        {
            // Error header
            lines.push(Line::from(vec![
                Span::styled("Error: ", Style::default().fg(Color::Red).bold()),
                Span::styled(sql_err.message.clone(), Style::default().fg(Color::White)),
            ]));

            // File location
            lines.push(Line::from(vec![
                Span::styled("   --> ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{}:{}:{}", file_path, line_num, col),
                    Style::default().fg(Color::White),
                ),
            ]));

            lines.push(Line::from(""));

            // Get highlighted source lines
            let highlighted =
                highlight_to_lines(&mut self.highlighter, &self.theme, "rust", &source);

            // Get the source lines for finding leading whitespace
            let source_lines: Vec<&str> = source.lines().collect();

            // Show context: 2 lines before, error line, 2 lines after
            let start = line_num.saturating_sub(3);
            let end = (line_num + 2).min(highlighted.len());

            for (i, hl_line) in highlighted.iter().enumerate() {
                let display_line = i + 1; // 1-indexed
                if display_line >= start && display_line <= end {
                    let line_num_str = format!("{:4} ", display_line);
                    let is_error_line = display_line == line_num;

                    let mut spans = vec![Span::styled(
                        line_num_str,
                        Style::default().fg(Color::DarkGray),
                    )];

                    if is_error_line {
                        spans.insert(
                            0,
                            Span::styled("> ", Style::default().fg(Color::Red).bold()),
                        );
                    } else {
                        spans.insert(0, Span::styled("  ", Style::default()));
                    }

                    // Add the highlighted content
                    spans.extend(hl_line.spans.iter().cloned());

                    lines.push(Line::from(spans));

                    // Add error pointer under the error line
                    if is_error_line {
                        // Point to first non-whitespace char instead of track_caller col
                        // (track_caller col points to `?` which isn't helpful)
                        let effective_col = source_lines
                            .get(line_num - 1)
                            .map(|line| {
                                line.chars().take_while(|c| c.is_whitespace()).count() + 1 // 1-indexed
                            })
                            .unwrap_or(col);

                        // Prefix is: "> " (2) + "{:4} " line number (5) = 7 chars
                        let padding = " ".repeat(7 + effective_col.saturating_sub(1));
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{}^", padding),
                                Style::default().fg(Color::Red).bold(),
                            ),
                            Span::styled(
                                format!(" {}", sql_err.message),
                                Style::default().fg(Color::Red),
                            ),
                        ]));
                    }
                }
            }

            // Add hint/detail
            if let Some(hint) = &sql_err.hint {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Hint: ", Style::default().fg(Color::Cyan).bold()),
                    Span::styled(hint.clone(), Style::default().fg(Color::White)),
                ]));
            }
            if let Some(detail) = &sql_err.detail {
                lines.push(Line::from(vec![
                    Span::styled("Detail: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(detail.clone(), Style::default().fg(Color::White)),
                ]));
            }

            self.show_error_lines(lines);
            return;
        }

        // Fallback to plain text
        self.show_error(format_sql_error(sql_err));
    }

    /// Suggest a migration name based on the current diff.
    fn suggest_migration_name(&self) -> String {
        let DiffState::Loaded(diff) = &self.diff else {
            return "schema-update".to_string();
        };

        // Collect all changes by type
        let mut new_tables: Vec<&str> = Vec::new();
        let mut dropped_tables: Vec<&str> = Vec::new();
        let mut new_columns: Vec<(&str, &str)> = Vec::new(); // (table, column)
        let mut dropped_columns: Vec<(&str, &str)> = Vec::new();
        let mut type_changes: Vec<(&str, &str, &str)> = Vec::new(); // (table, column, new_type)
        let mut _other_changes = false;

        for td in &diff.table_diffs {
            for change in &td.changes {
                let desc = &change.description;
                if desc.starts_with("+ table") {
                    new_tables.push(&td.table);
                } else if desc.starts_with("- table") {
                    dropped_tables.push(&td.table);
                } else if desc.starts_with("+ ") {
                    // Adding a column: "+ column_name: TYPE"
                    if let Some(col) = desc.strip_prefix("+ ").and_then(|s| s.split(':').next()) {
                        new_columns.push((&td.table, col.trim()));
                    }
                } else if desc.starts_with("- ") {
                    // Dropping a column
                    if let Some(col) = desc.strip_prefix("- ") {
                        dropped_columns.push((&td.table, col.trim()));
                    }
                } else if desc.starts_with("~ ") {
                    // Type or other change: "~ column: OLD -> NEW"
                    if let Some(rest) = desc.strip_prefix("~ ")
                        && let Some((col, change_desc)) = rest.split_once(':')
                    {
                        if let Some((_, new_type)) = change_desc.split_once(" -> ") {
                            type_changes.push((&td.table, col.trim(), new_type.trim()));
                        } else {
                            _other_changes = true;
                        }
                    }
                } else {
                    _other_changes = true;
                }
            }
        }

        // Generate name based on what we found
        if !new_tables.is_empty()
            && dropped_tables.is_empty()
            && new_columns.is_empty()
            && type_changes.is_empty()
        {
            if new_tables.len() == 1 {
                return format!("create-{}", new_tables[0]);
            } else {
                return "create-tables".to_string();
            }
        }

        if !dropped_tables.is_empty()
            && new_tables.is_empty()
            && dropped_columns.is_empty()
            && type_changes.is_empty()
        {
            if dropped_tables.len() == 1 {
                return format!("drop-{}", dropped_tables[0]);
            } else {
                return "drop-tables".to_string();
            }
        }

        if !new_columns.is_empty() && type_changes.is_empty() && dropped_columns.is_empty() {
            // Check if all columns have the same name
            let col_names: std::collections::HashSet<&str> =
                new_columns.iter().map(|(_, c)| *c).collect();
            if col_names.len() == 1 {
                let col = col_names.into_iter().next().unwrap();
                return format!("add-{}", col.replace('_', "-"));
            } else if new_columns.len() == 1 {
                return format!(
                    "add-{}-to-{}",
                    new_columns[0].1.replace('_', "-"),
                    new_columns[0].0
                );
            }
        }

        if !type_changes.is_empty()
            && new_columns.is_empty()
            && dropped_columns.is_empty()
            && new_tables.is_empty()
        {
            // Check if all type changes are for the same column name
            let col_names: std::collections::HashSet<&str> =
                type_changes.iter().map(|(_, c, _)| *c).collect();
            let new_types: std::collections::HashSet<&str> =
                type_changes.iter().map(|(_, _, t)| *t).collect();

            if col_names.len() == 1 && new_types.len() == 1 {
                let col = col_names.into_iter().next().unwrap();
                let new_type = new_types
                    .into_iter()
                    .next()
                    .unwrap()
                    .to_lowercase()
                    .replace(' ', "-");
                return format!("{}-to-{}", col.replace('_', "-"), new_type);
            } else if col_names.len() == 1 {
                let col = col_names.into_iter().next().unwrap();
                return format!("change-{}-type", col.replace('_', "-"));
            }
        }

        if !dropped_columns.is_empty() && new_columns.is_empty() && type_changes.is_empty() {
            let col_names: std::collections::HashSet<&str> =
                dropped_columns.iter().map(|(_, c)| *c).collect();
            if col_names.len() == 1 {
                let col = col_names.into_iter().next().unwrap();
                return format!("drop-{}", col.replace('_', "-"));
            }
        }

        // Fallback: describe what tables are affected
        let affected_tables: std::collections::HashSet<&str> = diff
            .table_diffs
            .iter()
            .map(|td| td.table.as_str())
            .collect();
        if affected_tables.len() == 1 {
            let table = affected_tables.into_iter().next().unwrap();
            return format!("alter-{}", table);
        }

        "schema-update".to_string()
    }

    async fn generate_migration_with_name(&mut self, name: &str) {
        if let (Some(conn), Some(url)) = (&self.conn, &self.database_url) {
            self.loading = Some("Generating migration...".to_string());

            // Get migration SQL from service
            match conn
                .client()
                .generate_migration_sql(DiffRequest {
                    database_url: url.clone(),
                })
                .await
            {
                Ok(sql) => {
                    // Generate migration file
                    match self.create_migration_file(name, &sql) {
                        Ok(path) => {
                            // Stage the migration file with git
                            if let Err(e) = self.git_add(&path) {
                                self.show_error(format!("Failed to stage file: {}", e));
                                self.loading = None;
                                return;
                            }

                            // Also stage mod.rs if it exists in the same directory
                            if let Some(parent) = std::path::Path::new(&path).parent() {
                                let mod_rs = parent.join("mod.rs");
                                if mod_rs.exists() {
                                    let _ = self.git_add(&mod_rs.display().to_string());
                                }
                            }

                            // Store pending commit info for after rebuild
                            self.pending_migration_commit = Some((path.clone(), name.to_string()));

                            self.error = Some(format!("Created: {} (rebuilding...)", path));
                            // Automatically rebuild to pick up the new migration
                            self.needs_rebuild = true;
                        }
                        Err(e) => {
                            self.show_error(format!("Failed to create migration: {}", e));
                        }
                    }
                }
                Err(e) => {
                    self.show_error(format!("Failed to generate SQL: {:?}", e));
                }
            }
            self.loading = None;
        }
    }

    /// Stage a file with git add.
    fn git_add(&self, path: &str) -> Result<(), String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["add", path])
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git add failed: {}", stderr));
        }

        Ok(())
    }

    fn create_migration_file(&self, name: &str, sql: &str) -> Result<String, std::io::Error> {
        use std::fs;
        use std::io::Write;

        let now = jiff::Zoned::now();
        // Human-readable timestamp: m_2026_01_18_173711
        let timestamp = now.strftime("%Y_%m_%d_%H%M%S");

        // Convert name to snake_case for the module name
        let module_name = name.replace('-', "_").to_lowercase();

        // Find migrations directory from config
        let (cfg, config_path) = crate::config::load().unwrap_or_else(|_| {
            (
                crate::config::Config::default(),
                std::path::PathBuf::from("."),
            )
        });
        let project_root = config_path
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(std::path::Path::new("."));
        let migrations_dir = crate::config::find_migrations_dir(&cfg, project_root);
        if !migrations_dir.exists() {
            fs::create_dir_all(&migrations_dir)?;
        }

        // Generate filename: m_2026_01_18_173711_name.rs
        let filename = format!("m{}_{}.rs", timestamp, module_name);
        let filepath = migrations_dir.join(&filename);

        // Generate Rust migration content
        // Version is derived from filename automatically by the macro
        // Split SQL by statements (semicolons), not by lines
        let sql_calls = parse_sql_to_calls(sql);

        let content = format!(
            r#"//! Migration: {name}
//! Created: {created}

use dibs::{{MigrationContext, MigrationResult}};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {{
{sql_calls}
    Ok(())
}}
"#,
            name = name,
            created = now.strftime("%Y-%m-%d %H:%M:%S %Z"),
            sql_calls = sql_calls,
        );

        let mut file = fs::File::create(&filepath)?;
        file.write_all(content.as_bytes())?;

        // Add to mod.rs
        let mod_rs_path = migrations_dir.join("mod.rs");
        let module_line = format!("mod m{}_{};", timestamp, module_name);

        if mod_rs_path.exists() {
            // Read existing mod.rs and append
            let existing = fs::read_to_string(&mod_rs_path)?;
            if !existing.contains(&module_line) {
                let mut mod_file = fs::OpenOptions::new().append(true).open(&mod_rs_path)?;
                writeln!(mod_file, "{}", module_line)?;
            }
        } else {
            // Create new mod.rs
            let mut mod_file = fs::File::create(&mod_rs_path)?;
            writeln!(mod_file, "//! Database migrations.")?;
            writeln!(mod_file)?;
            writeln!(mod_file, "{}", module_line)?;
        }

        Ok(filepath.display().to_string())
    }

    /// Delete the currently selected migration (only if not committed to git).
    fn delete_selected_migration(&mut self) {
        let Some(migrations) = &self.migrations else {
            return;
        };

        let Some(migration) = migrations.get(self.selected_migration) else {
            return;
        };

        // Can't delete applied migrations
        if migration.applied {
            self.error = Some("Cannot delete applied migration".to_string());
            return;
        }

        let Some(source_file) = &migration.source_file else {
            self.error = Some("Migration has no source file".to_string());
            return;
        };

        let path = std::path::Path::new(source_file);
        if !path.exists() {
            self.error = Some("Migration file not found".to_string());
            return;
        }

        // Check if file is committed in git
        if self.is_file_committed(path) {
            self.show_error(format!(
                "Cannot delete committed migration!\n\n\
                 File: {}\n\n\
                 This migration has been committed to git.\n\
                 Deleting it could cause issues for other developers.\n\n\
                 If you really need to remove it, use git manually.",
                source_file
            ));
            return;
        }

        // Safe to delete - file is not committed
        match self.delete_migration_file(path) {
            Ok(()) => {
                self.error = Some(format!("Deleted: {}", source_file));
                // Trigger rebuild to pick up the change
                self.needs_rebuild = true;
            }
            Err(e) => {
                self.show_error(format!("Failed to delete migration: {}", e));
            }
        }
    }

    /// Check if a file is committed in git (not untracked, not just staged).
    fn is_file_committed(&self, path: &std::path::Path) -> bool {
        use std::process::Command;

        // Use git status --porcelain to check file status
        // If file shows up in output, it's either untracked (??) or modified
        // If it doesn't show up, it's committed and unchanged
        let output = Command::new("git")
            .args(["status", "--porcelain", "--"])
            .arg(path)
            .output();

        match output {
            Ok(output) => {
                let status = String::from_utf8_lossy(&output.stdout);
                let trimmed = status.trim();

                if trimmed.is_empty() {
                    // File is tracked and unchanged = committed
                    true
                } else if trimmed.starts_with("??") {
                    // Untracked = not committed
                    false
                } else if trimmed.starts_with("A ") {
                    // Staged but not committed
                    false
                } else {
                    // Modified, deleted, etc. - file exists in git history
                    true
                }
            }
            Err(_) => {
                // If git fails, assume it's committed to be safe
                true
            }
        }
    }

    /// Delete a migration file and remove it from mod.rs.
    fn delete_migration_file(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        use std::fs;

        // Extract module name from filename (e.g., m2026_01_18_185242_name.rs -> m2026_01_18_185242_name)
        let module_name = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid filename")
        })?;

        // Delete the migration file
        fs::remove_file(path)?;

        // Remove from mod.rs
        let mod_rs_path = path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("mod.rs");
        if mod_rs_path.exists() {
            let content = fs::read_to_string(&mod_rs_path)?;
            let module_line = format!("mod {};", module_name);

            // Filter out the line that declares this module
            let new_content: String = content
                .lines()
                .filter(|line| !line.trim().starts_with(&module_line))
                .collect::<Vec<_>>()
                .join("\n");

            // Add trailing newline if there was content
            let new_content = if new_content.is_empty() {
                new_content
            } else {
                format!("{}\n", new_content)
            };

            fs::write(&mod_rs_path, new_content)?;
        }

        Ok(())
    }

    async fn run_migrations(&mut self) {
        if let (Some(conn), Some(url)) = (&self.conn, &self.database_url) {
            use dibs_proto::MigrateRequest;

            // Safety check: refuse to run if migration files are newer than the binary
            if let Some(stale_file) = conn.check_migrations_stale() {
                self.show_error(format!(
                    "Migration files changed since build!\n\n\
                     Stale file: {}\n\n\
                     Press R to rebuild.",
                    stale_file.display()
                ));
                return;
            }

            self.loading = Some("Running migrations...".to_string());

            let (log_tx, mut log_rx) = roam::channel::<dibs_proto::MigrationLog>();

            // We can't easily show streaming logs in TUI without more complex async handling
            // For now, just run and show result
            let client = conn.client().clone();
            let url = url.clone();

            let result = client
                .migrate(
                    MigrateRequest {
                        database_url: url,
                        migration: None,
                    },
                    log_tx,
                )
                .await;

            // Drain any remaining logs
            while let Ok(Some(_)) = log_rx.recv().await {}

            match result {
                Ok(res) => {
                    if res.applied.is_empty() {
                        self.error = None;
                    } else {
                        self.error = Some(format!("Applied {} migration(s)", res.applied.len()));
                    }
                    // Refresh migrations list and diff
                    self.refresh_migrations().await;
                    self.refresh_diff().await;
                }
                Err(e) => {
                    self.show_migration_error(&e);
                }
            }
            self.loading = None;
        }
    }

    async fn refresh(&mut self) {
        self.error = None;
        let Some(conn) = &self.conn else { return };
        let client = conn.client().clone();

        self.loading = Some("Refreshing...".to_string());

        // Refresh schema
        match client.schema().await {
            Ok(schema) => self.schema = Some(schema),
            Err(e) => self.show_error(format!("Schema fetch: {:?}", e)),
        }

        // Refresh migrations and diff if we have a database URL
        if let Some(url) = self.database_url.clone() {
            self.refresh_migrations().await;

            // Refresh diff
            self.diff = DiffState::Loading;
            match client.diff(DiffRequest { database_url: url }).await {
                Ok(diff) => self.diff = DiffState::Loaded(diff),
                Err(e) => self.diff = DiffState::Error(format!("{:?}", e)),
            }
        }

        self.loading = None;
    }

    async fn refresh_migrations(&mut self) {
        if let (Some(conn), Some(url)) = (&self.conn, &self.database_url) {
            match conn
                .client()
                .migration_status(MigrationStatusRequest {
                    database_url: url.clone(),
                })
                .await
            {
                Ok(migrations) => {
                    self.update_postgres_mode(&migrations);
                    self.migrations = Some(migrations);
                }
                Err(e) => self.show_error(format!("Migration status: {:?}", e)),
            }
        }
    }

    async fn refresh_diff(&mut self) {
        if let (Some(conn), Some(url)) = (&self.conn, &self.database_url) {
            self.diff = DiffState::Loading;
            match conn
                .client()
                .diff(DiffRequest {
                    database_url: url.clone(),
                })
                .await
            {
                Ok(diff) => self.diff = DiffState::Loaded(diff),
                Err(e) => self.diff = DiffState::Error(format!("{:?}", e)),
            }
        }
    }

    fn next_tab(&mut self) {
        let i = self.tab.index();
        self.tab = Tab::from_index((i + 1) % Tab::all().len());
    }

    fn prev_tab(&mut self) {
        let i = self.tab.index();
        self.tab = Tab::from_index((i + Tab::all().len() - 1) % Tab::all().len());
    }

    fn move_up(&mut self) {
        match self.tab {
            Tab::Rust => {
                if self.schema.is_some() && self.selected_table > 0 {
                    self.selected_table -= 1;
                    self.table_state.select(Some(self.selected_table));
                    self.details_selection = 0;
                    self.details_scroll = 0;
                }
            }
            Tab::Postgres => {
                self.postgres_move_up();
            }
        }
    }

    fn move_down(&mut self) {
        match self.tab {
            Tab::Rust => {
                if let Some(schema) = &self.schema
                    && self.selected_table < schema.tables.len().saturating_sub(1)
                {
                    self.selected_table += 1;
                    self.table_state.select(Some(self.selected_table));
                    self.details_selection = 0;
                    self.details_scroll = 0;
                }
            }
            Tab::Postgres => {
                self.postgres_move_down();
            }
        }
    }

    /// Move up in Postgres tab - through migrations then to NewChanges
    fn postgres_move_up(&mut self) {
        match self.postgres_selection {
            PostgresSelection::Migration(idx) => {
                if idx > 0 {
                    self.postgres_selection = PostgresSelection::Migration(idx - 1);
                    self.selected_migration = idx - 1;
                    self.migration_state.select(Some(idx - 1));
                }
            }
            PostgresSelection::NewChanges => {
                // Move to last migration if any
                if let Some(migrations) = &self.migrations
                    && !migrations.is_empty()
                {
                    let last_idx = migrations.len() - 1;
                    self.postgres_selection = PostgresSelection::Migration(last_idx);
                    self.selected_migration = last_idx;
                    self.migration_state.select(Some(last_idx));
                }
            }
        }
    }

    /// Move down in Postgres tab - through migrations then to NewChanges
    fn postgres_move_down(&mut self) {
        let migration_count = self.migrations.as_ref().map(|m| m.len()).unwrap_or(0);

        match self.postgres_selection {
            PostgresSelection::Migration(idx) => {
                if idx + 1 < migration_count {
                    self.postgres_selection = PostgresSelection::Migration(idx + 1);
                    self.selected_migration = idx + 1;
                    self.migration_state.select(Some(idx + 1));
                } else if self.postgres_mode == PostgresMode::AllApplied {
                    // Move to NewChanges
                    self.postgres_selection = PostgresSelection::NewChanges;
                    self.migration_state.select(None);
                }
            }
            PostgresSelection::NewChanges => {
                // Already at bottom, do nothing
            }
        }
    }

    fn go_to_first(&mut self) {
        match self.tab {
            Tab::Rust => {
                self.selected_table = 0;
                self.table_state.select(Some(0));
            }
            Tab::Postgres => {
                if let Some(migrations) = &self.migrations
                    && !migrations.is_empty()
                {
                    self.postgres_selection = PostgresSelection::Migration(0);
                    self.selected_migration = 0;
                    self.migration_state.select(Some(0));
                }
            }
        }
    }

    fn go_to_last(&mut self) {
        match self.tab {
            Tab::Rust => {
                if let Some(schema) = &self.schema {
                    self.selected_table = schema.tables.len().saturating_sub(1);
                    self.table_state.select(Some(self.selected_table));
                }
            }
            Tab::Postgres => {
                if self.postgres_mode == PostgresMode::AllApplied {
                    self.postgres_selection = PostgresSelection::NewChanges;
                    self.migration_state.select(None);
                } else if let Some(migrations) = &self.migrations {
                    let last_idx = migrations.len().saturating_sub(1);
                    self.postgres_selection = PostgresSelection::Migration(last_idx);
                    self.selected_migration = last_idx;
                    self.migration_state.select(Some(last_idx));
                }
            }
        }
    }

    fn half_page_down(&mut self) {
        // Simplified for now
        for _ in 0..10 {
            self.move_down();
        }
    }

    fn half_page_up(&mut self) {
        for _ in 0..10 {
            self.move_up();
        }
    }

    /// Get the total number of selectable items in the details pane.
    /// This is columns + foreign keys for the current table.
    fn details_item_count(&self) -> usize {
        let Some(schema) = &self.schema else { return 0 };
        let Some(table) = schema.tables.get(self.selected_table) else {
            return 0;
        };
        table.columns.len() + table.foreign_keys.len()
    }

    fn details_selection_up(&mut self) {
        if self.details_selection > 0 {
            self.details_selection -= 1;
            // Auto-scroll to keep selection visible
            // Each item takes roughly 1-2 lines, selection line ~ details_selection + header_lines
            let header_lines = 6; // approx lines before columns
            let selection_line = header_lines + self.details_selection;
            if (selection_line as u16) < self.details_scroll {
                self.details_scroll = selection_line as u16;
            }
        }
    }

    fn details_selection_down(&mut self) {
        let max = self.details_item_count().saturating_sub(1);
        if self.details_selection < max {
            self.details_selection += 1;
            // Auto-scroll to keep selection visible (rough estimate)
            let header_lines = 6;
            let selection_line = (header_lines + self.details_selection) as u16;
            // Assume ~20 visible lines in details pane
            if selection_line > self.details_scroll + 15 {
                self.details_scroll = selection_line.saturating_sub(15);
            }
        }
    }

    /// Follow a foreign key reference - jump to the referenced table.
    fn follow_foreign_key(&mut self) {
        let Some(schema) = &self.schema else { return };
        let Some(table) = schema.tables.get(self.selected_table) else {
            return;
        };

        let col_count = table.columns.len();

        // Check if selection is on a foreign key
        if self.details_selection >= col_count {
            let fk_idx = self.details_selection - col_count;
            if let Some(fk) = table.foreign_keys.get(fk_idx) {
                // Find the referenced table
                let target_table = &fk.references_table;
                if let Some(idx) = schema.tables.iter().position(|t| &t.name == target_table) {
                    self.selected_table = idx;
                    self.table_state.select(Some(idx));
                    self.details_selection = 0;
                    self.details_scroll = 0;
                    // Switch focus back to table list briefly, then to details
                    // to give feedback that we jumped
                }
            }
        }
    }

    fn ui(&mut self, frame: &mut Frame) {
        // Handle failed phase - show build output with error
        if let AppPhase::Failed(ref msg) = self.phase {
            self.render_failed_phase(frame, msg.clone());
            return;
        }

        let area = frame.area();

        // Layout: tabs (1) + main content + status bar (1)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Tabs
                Constraint::Min(0),    // Content
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Count indicators for tabs
        let diff_count = if let DiffState::Loaded(diff) = &self.diff {
            diff.table_diffs
                .iter()
                .map(|td| td.changes.len())
                .sum::<usize>()
        } else {
            0
        };
        let pending_migrations = self
            .migrations
            .as_ref()
            .map(|m| m.iter().filter(|m| !m.applied).count())
            .unwrap_or(0);

        // Tabs
        let tab_titles: Vec<Line> = Tab::all()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let num = format!("{}", i + 1);
                let mut spans = vec![
                    Span::styled(num, Style::default().fg(Color::Yellow)),
                    Span::raw(":"),
                    Span::raw(t.name()),
                ];

                // Add count indicators for Postgres tab
                if *t == Tab::Postgres {
                    if pending_migrations > 0 {
                        spans.push(Span::styled(
                            format!(" ({} pending)", pending_migrations),
                            Style::default().fg(Color::Red),
                        ));
                    } else if diff_count > 0 {
                        spans.push(Span::styled(
                            format!(" ({} changes)", diff_count),
                            Style::default().fg(Color::Magenta),
                        ));
                    }
                }

                Line::from(spans)
            })
            .collect();

        let tabs = Tabs::new(tab_titles)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" dibs ")
                    .title_style(Style::default().fg(Color::Cyan).bold()),
            )
            .select(self.tab.index())
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Cyan).bold());

        frame.render_widget(tabs, chunks[0]);

        // Content area
        match self.tab {
            Tab::Rust => self.render_rust_tab(frame, chunks[1]),
            Tab::Postgres => self.render_postgres_tab(frame, chunks[1]),
        }

        // Status bar
        let status = self.build_status_bar();
        frame.render_widget(status, chunks[2]);

        // Render migration name dialog as overlay
        if self.show_migration_dialog {
            self.render_migration_dialog(frame, area);
        }

        // Render error modal as overlay
        if self.show_error_modal {
            self.render_error_modal(frame, area);
        }
    }

    /// Render the error modal as a centered overlay.
    fn render_error_modal(&self, frame: &mut Frame, area: Rect) {
        use ratatui::widgets::Clear;

        // Calculate dialog size based on content
        let max_line_len = self
            .error_modal_lines
            .iter()
            .map(|l| l.width())
            .max()
            .unwrap_or(20);
        let dialog_width = (max_line_len as u16 + 4)
            .min(area.width.saturating_sub(4))
            .max(40);
        let dialog_height =
            (self.error_modal_lines.len() as u16 + 5).min(area.height.saturating_sub(4));

        let x = (area.width.saturating_sub(dialog_width)) / 2;
        let y = (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

        // Clear the background
        frame.render_widget(Clear, dialog_area);

        // Dialog box with error styling
        let dialog = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .title(" Error ")
            .title_style(Style::default().fg(Color::Red).bold());
        frame.render_widget(dialog, dialog_area);

        // Inner area for content
        let inner = dialog_area.inner(Margin {
            vertical: 1,
            horizontal: 1,
        });

        // Error text (scrollable if needed)
        let content_height = inner.height.saturating_sub(2); // Reserve space for help
        let error_lines: Vec<Line> = self
            .error_modal_lines
            .iter()
            .take(content_height as usize)
            .cloned()
            .collect();

        let content_area = Rect::new(inner.x, inner.y, inner.width, content_height);
        let error_text = Paragraph::new(error_lines);
        frame.render_widget(error_text, content_area);

        // Help text at bottom
        let help_area = Rect::new(inner.x, inner.y + content_height + 1, inner.width, 1);
        let help = Paragraph::new(Line::from(vec![
            Span::styled("Press ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::styled(" or ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::styled(" to close", Style::default().fg(Color::DarkGray)),
        ]));
        frame.render_widget(help, help_area);
    }

    /// Render the migration name input dialog as a centered overlay.
    fn render_migration_dialog(&self, frame: &mut Frame, area: Rect) {
        use ratatui::widgets::Clear;

        // Center a dialog box
        let dialog_width = 50u16.min(area.width.saturating_sub(4));
        let dialog_height = 7u16;

        let x = (area.width.saturating_sub(dialog_width)) / 2;
        let y = (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

        // Clear the background
        frame.render_widget(Clear, dialog_area);

        // Dialog content
        let inner_chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1), // Label
                Constraint::Length(1), // Spacing
                Constraint::Length(1), // Input
                Constraint::Length(1), // Help
            ])
            .split(dialog_area);

        // Dialog box
        let dialog = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Generate Migration ")
            .title_style(Style::default().fg(Color::Cyan).bold());
        frame.render_widget(dialog, dialog_area);

        // Label
        let label = Paragraph::new("Migration name:").style(Style::default().fg(Color::White));
        frame.render_widget(label, inner_chunks[0]);

        // Input field with cursor
        let input_text = if self.migration_name_input.is_empty() {
            Span::styled(
                "(leave empty for autogenerated)",
                Style::default().fg(Color::DarkGray),
            )
        } else {
            Span::styled(
                &self.migration_name_input,
                Style::default().fg(Color::White),
            )
        };
        let input = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Yellow)),
            input_text,
        ]));
        frame.render_widget(input, inner_chunks[2]);

        // Set cursor position
        if !self.migration_name_input.is_empty() || self.show_migration_dialog {
            frame.set_cursor_position((
                inner_chunks[2].x + 2 + self.migration_name_cursor as u16,
                inner_chunks[2].y,
            ));
        }

        // Help text
        let help = Paragraph::new("Enter: confirm  Esc: cancel")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(help, inner_chunks[3]);
    }

    /// Render the failed phase - shows build output with error message.
    fn render_failed_phase(&self, frame: &mut Frame, error_msg: String) {
        let area = frame.area();

        // Layout: header + error + build output + status bar
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Length(3), // Error
                Constraint::Min(0),    // Build output
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled(" dibs ", Style::default().fg(Color::Cyan).bold()),
            Span::styled("Build Failed", Style::default().fg(Color::Red).bold()),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        );
        frame.render_widget(header, chunks[0]);

        // Error message
        let error = Paragraph::new(error_msg)
            .style(Style::default().fg(Color::Red))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Error ")
                    .title_style(Style::default().fg(Color::Red)),
            );
        frame.render_widget(error, chunks[1]);

        // Build output (scrollable, show last lines)
        let visible_height = chunks[2].height.saturating_sub(2) as usize;
        let start = self.build_output.len().saturating_sub(visible_height);
        let lines: Vec<Line> = self
            .build_output
            .iter()
            .skip(start)
            .take(visible_height)
            .map(|output| {
                let style = if output.is_stderr {
                    if output.text.contains("error") {
                        Style::default().fg(Color::Red)
                    } else if output.text.contains("warning") {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::White)
                    }
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(&output.text, style))
            })
            .collect();

        let output_block = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Build Output ")
                .title_style(Style::default().fg(Color::Cyan)),
        );
        frame.render_widget(output_block, chunks[2]);

        // Status bar
        let status = Paragraph::new(Line::from(vec![
            Span::styled(" q ", Style::default().fg(Color::Yellow)),
            Span::raw("quit"),
        ]))
        .style(Style::default().bg(Color::DarkGray));
        frame.render_widget(status, chunks[3]);
    }

    fn render_rust_tab(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(loading) = &self.loading {
            let p = Paragraph::new(loading.as_str())
                .block(Block::default().borders(Borders::ALL).title(" Schema "));
            frame.render_widget(p, area);
            return;
        }

        let Some(schema) = &self.schema else {
            let p = Paragraph::new("No schema available")
                .block(Block::default().borders(Borders::ALL).title(" Schema "));
            frame.render_widget(p, area);
            return;
        };

        // Split into table list and details
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(area);

        // Border styles based on focus
        let list_border = if self.schema_focus == 0 {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let details_border = if self.schema_focus == 1 {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Table list
        let items: Vec<ListItem> = schema
            .tables
            .iter()
            .map(|t| ListItem::new(format!("{} ({})", t.name, t.columns.len())))
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(list_border)
                    .title(" Tables ")
                    .title_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White).bold())
            .highlight_symbol("› ");

        frame.render_stateful_widget(list, chunks[0], &mut self.table_state);

        // Details
        if let Some(table) = schema.tables.get(self.selected_table) {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Table: ", Style::default().fg(Color::Gray)),
                    Span::styled(&table.name, Style::default().fg(Color::Cyan).bold()),
                ]),
                Line::from(""),
            ];

            // Source location
            if let (Some(file), Some(line)) = (&table.source_file, table.source_line) {
                lines.push(Line::from(vec![
                    Span::styled("Source: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        format!("{}:{}", file, line),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }

            // Doc comment
            if let Some(doc) = &table.doc {
                lines.push(Line::from(vec![
                    Span::styled("/// ", Style::default().fg(Color::Green)),
                    Span::styled(doc, Style::default().fg(Color::Green).italic()),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Columns:",
                Style::default().fg(Color::Yellow).bold(),
            )));

            let is_focused = self.schema_focus == 1;

            for (col_idx, col) in table.columns.iter().enumerate() {
                let is_selected = is_focused && self.details_selection == col_idx;

                // Column doc comment (if any)
                if let Some(doc) = &col.doc {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled("/// ", Style::default().fg(Color::Green)),
                        Span::styled(doc, Style::default().fg(Color::Green).italic()),
                    ]));
                }

                let highlight_style = if is_selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                let prefix = if is_selected { "› " } else { "  " };
                let mut spans = vec![
                    Span::styled(prefix, highlight_style),
                    Span::styled(&col.name, highlight_style.fg(Color::White)),
                    Span::styled(": ", highlight_style),
                    Span::styled(&col.sql_type, highlight_style.fg(Color::Blue)),
                ];

                if col.primary_key {
                    spans.push(Span::styled(" ", highlight_style));
                    spans.push(Span::styled("PK", highlight_style.fg(Color::Yellow)));
                }
                if col.unique {
                    spans.push(Span::styled(" ", highlight_style));
                    spans.push(Span::styled("UNIQUE", highlight_style.fg(Color::Magenta)));
                }
                if !col.nullable {
                    spans.push(Span::styled(" ", highlight_style));
                    spans.push(Span::styled("NOT NULL", highlight_style.fg(Color::Red)));
                }
                if let Some(default) = &col.default {
                    spans.push(Span::styled(" ", highlight_style));
                    spans.push(Span::styled(
                        format!("DEFAULT {}", default),
                        highlight_style.fg(Color::Gray),
                    ));
                }

                lines.push(Line::from(spans));
            }

            if !table.foreign_keys.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Foreign Keys:",
                    Style::default().fg(Color::Green).bold(),
                )));

                let col_count = table.columns.len();
                for (fk_idx, fk) in table.foreign_keys.iter().enumerate() {
                    let is_selected = is_focused && self.details_selection == col_count + fk_idx;
                    let highlight_style = if is_selected {
                        Style::default().bg(Color::DarkGray)
                    } else {
                        Style::default()
                    };
                    let prefix = if is_selected { "› " } else { "  " };

                    lines.push(Line::from(vec![
                        Span::styled(prefix, highlight_style),
                        Span::styled(fk.columns.join(", "), highlight_style.fg(Color::White)),
                        Span::styled(" → ", highlight_style.fg(Color::Gray)),
                        Span::styled(&fk.references_table, highlight_style.fg(Color::Cyan)),
                        Span::styled(".", highlight_style),
                        Span::styled(fk.references_columns.join(", "), highlight_style),
                    ]));
                }
            }

            let details = Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(details_border)
                        .title(" Details ")
                        .title_style(Style::default().fg(Color::Cyan)),
                )
                .scroll((self.details_scroll, 0));

            frame.render_widget(details, chunks[1]);
        }
    }

    /// Render the Postgres tab - shows different UI based on mode
    fn render_postgres_tab(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(loading) = &self.loading {
            let p = Paragraph::new(loading.as_str())
                .block(Block::default().borders(Borders::ALL).title(" Postgres "));
            frame.render_widget(p, area);
            return;
        }

        // Handle no database URL
        if self.database_url.is_none() {
            let p = Paragraph::new("No DATABASE_URL set. Set it in .env or environment.")
                .block(Block::default().borders(Borders::ALL).title(" Postgres "));
            frame.render_widget(p, area);
            return;
        }

        // Clone migrations to avoid borrow issues
        let migrations = self.migrations.clone().unwrap_or_default();

        // If showing source, split the view
        if self.show_migration_source {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .split(area);

            // Render migration list on left
            self.render_postgres_list(frame, chunks[0], &migrations);

            // Render source on right
            self.render_migration_source(frame, chunks[1], &migrations);
        } else {
            // Split: migrations list on left, right panel content depends on mode
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(area);

            // Render migration list on left
            self.render_postgres_list(frame, chunks[0], &migrations);

            // Render right panel based on mode and selection
            match self.postgres_mode {
                PostgresMode::HasPending => {
                    self.render_postgres_action_required(frame, chunks[1], &migrations);
                }
                PostgresMode::AllApplied => {
                    self.render_postgres_changes(frame, chunks[1]);
                }
            }
        }
    }

    /// Render the migration list with "New Changes" item in AllApplied mode
    fn render_postgres_list(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        migrations: &[MigrationInfo],
    ) {
        let mut items: Vec<ListItem> = migrations
            .iter()
            .enumerate()
            .map(|(idx, m)| {
                let status = if m.applied { "✓" } else { "○" };
                let status_style = if m.applied {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                };
                let is_selected =
                    matches!(self.postgres_selection, PostgresSelection::Migration(i) if i == idx);
                let highlight = if is_selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(status, status_style),
                    Span::styled(" ", highlight),
                    Span::styled(&m.version, highlight),
                    Span::styled(" ", highlight),
                    Span::styled(&m.name, highlight),
                ]))
            })
            .collect();

        // In AllApplied mode, add "New Changes" item
        if self.postgres_mode == PostgresMode::AllApplied {
            let change_count = if let DiffState::Loaded(diff) = &self.diff {
                diff.table_diffs
                    .iter()
                    .map(|td| td.changes.len())
                    .sum::<usize>()
            } else {
                0
            };

            let is_selected = matches!(self.postgres_selection, PostgresSelection::NewChanges);
            let style = if is_selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::Magenta)
                    .bold()
            } else {
                Style::default().fg(Color::Magenta)
            };

            let indicator = if change_count > 0 {
                format!("+ New Changes ({})", change_count)
            } else {
                "+ New Changes".to_string()
            };

            items.push(ListItem::new(Line::from(vec![Span::styled(
                indicator, style,
            )])));
        }

        let title = " Migrations ";

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .title_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White).bold())
            .highlight_symbol("› ");

        frame.render_stateful_widget(list, area, &mut self.migration_state);
    }

    /// Render "Action Required" panel when there are pending migrations
    fn render_postgres_action_required(
        &self,
        frame: &mut Frame,
        area: Rect,
        migrations: &[MigrationInfo],
    ) {
        let pending_count = migrations.iter().filter(|m| !m.applied).count();

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {} pending migration(s)", pending_count),
                Style::default().fg(Color::Yellow).bold(),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Press ", Style::default().fg(Color::White)),
                Span::styled(
                    " m ",
                    Style::default().fg(Color::Black).bg(Color::Yellow).bold(),
                ),
                Span::styled(" to apply all", Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Cannot generate new migrations",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  until pending ones are applied.",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        // Show which migrations are pending
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Pending:",
            Style::default().fg(Color::Yellow),
        )));
        for m in migrations.iter().filter(|m| !m.applied) {
            lines.push(Line::from(vec![
                Span::styled("    ○ ", Style::default().fg(Color::Yellow)),
                Span::raw(&m.version),
                Span::styled(" - ", Style::default().fg(Color::DarkGray)),
                Span::raw(&m.name),
            ]));
        }

        let p = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Action Required ")
                .title_style(Style::default().fg(Color::Yellow)),
        );
        frame.render_widget(p, area);
    }

    /// Render "Schema Changes" panel when all migrations are applied
    fn render_postgres_changes(&self, frame: &mut Frame, area: Rect) {
        let diff = match &self.diff {
            DiffState::NoDatabaseUrl => {
                let p = Paragraph::new("No DATABASE_URL set.").block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Schema Changes "),
                );
                frame.render_widget(p, area);
                return;
            }
            DiffState::NotLoaded | DiffState::Loading => {
                let p = Paragraph::new("Loading diff...").block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Schema Changes "),
                );
                frame.render_widget(p, area);
                return;
            }
            DiffState::Error(err) => {
                let p = Paragraph::new(format!("Error: {}\n\nPress 'r' to retry", err))
                    .style(Style::default().fg(Color::Red))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Schema Changes "),
                    );
                frame.render_widget(p, area);
                return;
            }
            DiffState::Loaded(diff) => diff,
        };

        let mut lines = vec![];

        if diff.table_diffs.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  ✓ No changes",
                Style::default().fg(Color::Green),
            )));
            lines.push(Line::from(Span::styled(
                "  Schema matches database",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            // Call to action
            lines.push(Line::from(vec![
                Span::styled("  Press ", Style::default().fg(Color::White)),
                Span::styled(
                    " g ",
                    Style::default().fg(Color::Black).bg(Color::Yellow).bold(),
                ),
                Span::styled(" to generate", Style::default().fg(Color::White)),
            ]));
            lines.push(Line::from(""));

            // Show changes
            for td in &diff.table_diffs {
                lines.push(Line::from(Span::styled(
                    format!("{}:", td.table),
                    Style::default().fg(Color::Cyan).bold(),
                )));

                for change in &td.changes {
                    let style = match change.kind {
                        dibs_proto::ChangeKind::Add => Style::default().fg(Color::Green),
                        dibs_proto::ChangeKind::Drop => Style::default().fg(Color::Red),
                        dibs_proto::ChangeKind::Alter => Style::default().fg(Color::Yellow),
                    };
                    lines.push(Line::from(Span::styled(
                        format!("  {}", change.description),
                        style,
                    )));
                }
                lines.push(Line::from(""));
            }
        }

        let p = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Schema Changes ")
                .title_style(Style::default().fg(Color::Cyan)),
        );
        frame.render_widget(p, area);
    }

    fn render_migration_source(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        migrations: &[MigrationInfo],
    ) {
        let Some(migration) = migrations.get(self.selected_migration) else {
            let p = Paragraph::new("No migration selected")
                .block(Block::default().borders(Borders::ALL).title(" Source "));
            frame.render_widget(p, area);
            return;
        };

        let title = format!(" {} ", migration.version);

        let lines: Vec<Line<'static>> = if let Some(source) = &migration.source {
            // Highlight the Rust source
            highlight_to_lines(&mut self.highlighter, &self.theme, "rust", source)
        } else {
            vec![Line::from(Span::styled(
                "Source not available",
                Style::default().fg(Color::DarkGray),
            ))]
        };

        let p = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .title_style(Style::default().fg(Color::Cyan)),
            )
            .scroll((self.source_scroll, 0));

        frame.render_widget(p, area);
    }

    fn build_status_bar(&self) -> Paragraph<'static> {
        let mut spans = vec![];

        if self.show_migration_source {
            // Source viewer mode
            spans.push(Span::styled(" j/k ", Style::default().fg(Color::Yellow)));
            spans.push(Span::raw("scroll  "));
            spans.push(Span::styled("^D/^U ", Style::default().fg(Color::Yellow)));
            spans.push(Span::raw("½page  "));
            spans.push(Span::styled(
                "Enter/Esc ",
                Style::default().fg(Color::Yellow),
            ));
            spans.push(Span::raw("close  "));
        } else {
            spans.push(Span::styled(" Tab ", Style::default().fg(Color::Yellow)));
            spans.push(Span::raw("switch  "));
            spans.push(Span::styled("j/k ", Style::default().fg(Color::Yellow)));
            spans.push(Span::raw("nav  "));
            spans.push(Span::styled("r ", Style::default().fg(Color::Yellow)));
            spans.push(Span::raw("refresh  "));
            spans.push(Span::styled("R ", Style::default().fg(Color::Yellow)));
            spans.push(Span::raw("rebuild  "));

            if self.tab == Tab::Postgres {
                if self.postgres_mode == PostgresMode::HasPending {
                    spans.push(Span::styled("m ", Style::default().fg(Color::Yellow)));
                    spans.push(Span::raw("apply all  "));
                } else {
                    spans.push(Span::styled("g ", Style::default().fg(Color::Yellow)));
                    spans.push(Span::raw("generate  "));
                }
                if let PostgresSelection::Migration(_) = self.postgres_selection {
                    spans.push(Span::styled("Enter ", Style::default().fg(Color::Yellow)));
                    spans.push(Span::raw("view  "));
                    spans.push(Span::styled("d ", Style::default().fg(Color::Yellow)));
                    spans.push(Span::raw("delete  "));
                }
            }

            spans.push(Span::styled("q ", Style::default().fg(Color::Yellow)));
            spans.push(Span::raw("quit"));
        }

        // Show error or db url
        if let Some(err) = &self.error {
            spans.push(Span::raw("  │  "));
            spans.push(Span::styled(err.clone(), Style::default().fg(Color::Red)));
        } else if let Some(url) = &self.database_url {
            spans.push(Span::raw("  │  "));
            spans.push(Span::styled(
                format!("DB: {}", mask_db_url(url)),
                Style::default().fg(Color::DarkGray),
            ));
        }

        Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::DarkGray))
    }
}

fn mask_db_url(url: &str) -> String {
    // Mask password in URL
    if let Some(at) = url.find('@')
        && let Some(colon) = url[..at].rfind(':')
        && let Some(slash) = url[..colon].rfind('/')
    {
        let prefix = &url[..slash + 1];
        let user_start = slash + 1;
        if let Some(user_colon) = url[user_start..colon].find(':') {
            let user = &url[user_start..user_start + user_colon];
            let suffix = &url[at..];
            return format!("{}{}:***{}", prefix, user, suffix);
        }
    }
    url.to_string()
}
