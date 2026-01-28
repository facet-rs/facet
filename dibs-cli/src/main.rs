use std::fs;
use std::io::{self, IsTerminal, Write, stdout};

use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use facet::Facet;
use facet_styx::StyxFormat;
use figue as args;
use jiff::Zoned;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod config;
mod highlight;
mod lsp_extension;
mod service;
mod tui;

// Embed Styx schemas for LSP extraction via `styx extract $(which dibs)`
styx_embed::embed_outdir_file!("dibs-config.styx");
styx_embed::embed_outdir_file!("dibs-queries.styx");

/// Postgres toolkit for Rust, powered by facet reflection.
#[derive(Facet, Debug)]
struct Args {
    /// Standard CLI options (--help, --version, --completions)
    #[facet(flatten)]
    builtins: args::FigueBuiltins,

    /// Command to run
    #[facet(default, args::subcommand)]
    command: Option<Commands>,

    /// Configuration (from env vars with DIBS__ prefix, or .config/dibs.styx)
    #[facet(args::config, args::env_prefix = "DIBS")]
    config: Config,
}

// Re-export DbConfig from dibs_config for use in service module
pub use dibs_config::DbConfig;

/// Dibs configuration.
#[derive(Facet, Debug, Clone)]
struct Config {
    /// PostgreSQL connection URL.
    #[facet(args::env_alias = "DATABASE_URL", sensitive, default)]
    database_url: Option<String>,

    /// Database crate configuration.
    #[facet(default)]
    db: DbConfig,
}

impl Config {
    /// Get the database URL, or exit with an error if not set.
    fn require_database_url(&self) -> &str {
        self.database_url.as_deref().unwrap_or_else(|| {
            eprintln!("Error: DATABASE_URL is required for this command.");
            eprintln!();
            eprintln!("Set it via:");
            eprintln!("  export DATABASE_URL=postgres://user:pass@host/db");
            eprintln!("  or in .config/dibs.styx:");
            eprintln!("  {{database_url \"postgres://user:pass@host/db\"}}");
            std::process::exit(1);
        })
    }
}

/// Available commands
#[derive(Facet, Debug)]
#[repr(u8)]
enum Commands {
    /// Run pending migrations
    Migrate,
    /// Show migration status
    Status,
    /// Compare schema to database
    Diff,
    /// Generate a migration skeleton
    Generate {
        /// Migration name (e.g., "add-users-table")
        #[facet(args::positional)]
        name: String,
    },
    /// Generate a migration from schema diff
    GenerateFromDiff {
        /// Migration name (e.g., "add-users-table")
        #[facet(args::positional)]
        name: String,
    },
    /// Browse the current schema
    Schema {
        /// Output as plain text (default when not a TTY)
        #[facet(default, args::named)]
        plain: bool,

        /// Output as SQL (CREATE TABLE statements)
        #[facet(default, args::named)]
        sql: bool,
    },
    /// Run as LSP extension (invoked by Styx LSP)
    LspExtension,
}

fn main() {
    // Parse config from CLI args, environment, and config file
    let figue_config = args::builder::<Args>()
        .expect("failed to create figue builder")
        .cli(|c| c.strict())
        .env(|e| e.strict())
        .file(|f| {
            f.strict()
                .format(StyxFormat)
                .default_paths([".config/dibs.styx"])
        })
        .help(|h| {
            h.program_name("dibs")
                .description("Postgres toolkit for Rust, powered by facet reflection")
                .version(option_env!("CARGO_PKG_VERSION").unwrap_or("dev"))
        })
        .build();
    let args = args::Driver::new(figue_config).run().unwrap();

    run(args)
}

fn run(args: Args) {
    // Initialize tracing for non-LSP commands (LSP uses stdio so tracing would break it)
    if !matches!(args.command, Some(Commands::LspExtension)) {
        let _ = tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            ))
            .try_init();
    }

    let config = args.config;
    match args.command {
        Some(Commands::Migrate) => {
            run_migrate(&config);
        }
        Some(Commands::Status) => {
            run_status(&config);
        }
        Some(Commands::Diff) => {
            run_diff(&config);
        }
        Some(Commands::Generate { name }) => {
            generate_migration(&config.db, &name);
        }
        Some(Commands::GenerateFromDiff { name }) => {
            run_generate_from_diff(&config, &name);
        }
        Some(Commands::Schema { plain, sql }) => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            let schema = rt.block_on(async {
                let conn = match service::connect_to_service(&config.db).await {
                    Ok(conn) => conn,
                    Err(e) => {
                        eprintln!("Failed to connect to db service: {}", e);
                        std::process::exit(1);
                    }
                };

                match conn.client().schema().await {
                    Ok(schema_info) => schema_info_to_schema(schema_info),
                    Err(e) => {
                        eprintln!("Failed to get schema: {:?}", e);
                        std::process::exit(1);
                    }
                }
            });

            if schema.tables.is_empty() {
                println!("No tables registered.");
                println!();
                println!("Make sure your db crate has tables defined.");
                return;
            }

            if sql {
                // Output SQL CREATE statements
                println!("{}", schema.to_sql());
            } else if stdout().is_terminal() && !plain {
                // Use TUI if stdout is a TTY and --plain wasn't specified
                if let Err(e) = run_schema_tui(&schema) {
                    eprintln!("TUI error: {}", e);
                    std::process::exit(1);
                }
            } else {
                print_schema_plain(&schema);
            }
        }
        Some(Commands::LspExtension) => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(lsp_extension::run());
        }
        None => {
            // No subcommand: launch unified TUI (the default human interface)
            if stdout().is_terminal() {
                let app = tui::App::new();
                if let Err(e) = app.run(Some(&config.db)) {
                    eprintln!("TUI error: {}", e);
                    std::process::exit(1);
                }
            } else {
                // Not a TTY - just show help
                println!("dibs - Postgres toolkit for Rust");
                println!();
                println!("Run `dibs --help` for usage information.");
                println!("Run in a terminal for the interactive TUI.");
            }
        }
    }
}

/// Parse PgType from SQL type string
fn parse_pg_type(s: &str) -> dibs::PgType {
    match s.to_uppercase().as_str() {
        "SMALLINT" | "INT2" => dibs::PgType::SmallInt,
        "INTEGER" | "INT4" | "INT" => dibs::PgType::Integer,
        "BIGINT" | "INT8" => dibs::PgType::BigInt,
        "REAL" | "FLOAT4" => dibs::PgType::Real,
        "DOUBLE PRECISION" | "FLOAT8" => dibs::PgType::DoublePrecision,
        "NUMERIC" | "DECIMAL" => dibs::PgType::Numeric,
        "BOOLEAN" | "BOOL" => dibs::PgType::Boolean,
        "TEXT" | "VARCHAR" | "CHAR" | "CHARACTER VARYING" => dibs::PgType::Text,
        "BYTEA" => dibs::PgType::Bytea,
        "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" => dibs::PgType::Timestamptz,
        "DATE" => dibs::PgType::Date,
        "TIME" => dibs::PgType::Time,
        "UUID" => dibs::PgType::Uuid,
        "JSONB" => dibs::PgType::Jsonb,
        "TEXT[]" => dibs::PgType::TextArray,
        "BIGINT[]" | "INT8[]" => dibs::PgType::BigIntArray,
        "INTEGER[]" | "INT4[]" | "INT[]" => dibs::PgType::IntegerArray,
        _ => dibs::PgType::Text, // fallback
    }
}

/// Convert proto SchemaInfo to dibs::Schema
fn schema_info_to_schema(info: dibs_proto::SchemaInfo) -> dibs::Schema {
    let tables = info
        .tables
        .into_iter()
        .map(|t| dibs::Table {
            name: t.name,
            columns: t
                .columns
                .into_iter()
                .map(|c| dibs::Column {
                    name: c.name,
                    pg_type: parse_pg_type(&c.sql_type),
                    rust_type: c.rust_type,
                    nullable: c.nullable,
                    default: c.default,
                    primary_key: c.primary_key,
                    unique: c.unique,
                    auto_generated: c.auto_generated,
                    long: c.long,
                    label: c.label,
                    enum_variants: c.enum_variants,
                    doc: c.doc,
                    lang: c.lang,
                    icon: c.icon,
                    subtype: c.subtype,
                })
                .collect(),
            check_constraints: Vec::new(),
            trigger_checks: Vec::new(),
            foreign_keys: t
                .foreign_keys
                .into_iter()
                .map(|fk| dibs::ForeignKey {
                    columns: fk.columns,
                    references_table: fk.references_table,
                    references_columns: fk.references_columns,
                })
                .collect(),
            indices: t
                .indices
                .into_iter()
                .map(|idx| dibs::Index {
                    name: idx.name,
                    columns: idx
                        .columns
                        .into_iter()
                        .map(|c| dibs::IndexColumn {
                            name: c.name,
                            order: if c.order == "desc" {
                                dibs::SortOrder::Desc
                            } else {
                                dibs::SortOrder::Asc
                            },
                            nulls: match c.nulls.as_str() {
                                "first" => dibs::NullsOrder::First,
                                "last" => dibs::NullsOrder::Last,
                                _ => dibs::NullsOrder::Default,
                            },
                        })
                        .collect(),
                    unique: idx.unique,
                    where_clause: idx.where_clause,
                })
                .collect(),
            source: dibs::SourceLocation {
                file: t.source_file,
                line: t.source_line,
                column: None,
            },
            doc: t.doc,
            icon: t.icon,
        })
        .collect();

    dibs::Schema { tables }
}

/// Print schema as plain text (for piping)
fn print_schema_plain(schema: &dibs::Schema) {
    for table in &schema.tables {
        println!("TABLE {}", table.name);
        for col in &table.columns {
            let mut attrs = Vec::new();
            if col.primary_key {
                attrs.push("PK");
            }
            if col.unique {
                attrs.push("UNIQUE");
            }
            if !col.nullable {
                attrs.push("NOT NULL");
            }
            if let Some(default) = &col.default {
                attrs.push(default);
            }

            let attrs_str = if attrs.is_empty() {
                String::new()
            } else {
                format!(" [{}]", attrs.join(", "))
            };

            println!("  {} {}{}", col.name, col.pg_type, attrs_str);
        }

        for fk in &table.foreign_keys {
            println!(
                "  FK {} -> {}.{}",
                fk.columns.join(", "),
                fk.references_table,
                fk.references_columns.join(", ")
            );
        }

        for idx in &table.indices {
            let unique = if idx.unique { " UNIQUE" } else { "" };
            let cols: Vec<String> = idx
                .columns
                .iter()
                .map(|c| format!("{}{}{}", c.name, c.order.to_sql(), c.nulls.to_sql()))
                .collect();
            println!("  INDEX {} on ({}){}", idx.name, cols.join(", "), unique);
        }
        println!();
    }
}

/// Run the interactive TUI for browsing schema
fn run_schema_tui(schema: &dibs::Schema) -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut app = SchemaApp::new(schema);
    let result = app.run(&mut terminal);

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

struct SchemaApp<'a> {
    schema: &'a dibs::Schema,
    table_state: ListState,
    selected_table: usize,
    /// Which tables are expanded (showing columns)
    expanded: Vec<bool>,
    /// Current focus: left pane (tables) or right pane (details)
    focus: Focus,
    /// Selected item in details pane (for FK navigation)
    detail_selection: usize,
    /// Pending 'g' keypress for gg command
    pending_g: bool,
    /// Visible height for half-page scrolling
    visible_height: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Tables,
    Details,
}

impl<'a> SchemaApp<'a> {
    fn new(schema: &'a dibs::Schema) -> Self {
        let mut table_state = ListState::default();
        table_state.select(Some(0));
        let expanded = vec![false; schema.tables.len()];
        Self {
            schema,
            table_state,
            selected_table: 0,
            expanded,
            focus: Focus::Tables,
            detail_selection: 0,
            pending_g: false,
            visible_height: 20, // Will be updated during render
        }
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.ui(frame))?;

            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                // Handle 'g' prefix for gg command
                if self.pending_g {
                    self.pending_g = false;
                    if key.code == KeyCode::Char('g') {
                        self.go_to_first();
                        continue;
                    }
                    // If not 'g', fall through to normal handling
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Up | KeyCode::Char('k') => self.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => self.move_down(),
                    KeyCode::Left | KeyCode::Char('h') => self.focus_tables(),
                    KeyCode::Right | KeyCode::Char('l') => self.focus_details(),
                    KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
                    KeyCode::Tab => self.toggle_focus(),
                    // Vim-style navigation
                    KeyCode::Char('g') => self.pending_g = true,
                    KeyCode::Char('G') => self.go_to_last(),
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.half_page_down()
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.half_page_up()
                    }
                    _ => {}
                }
            }
        }
    }

    fn move_up(&mut self) {
        match self.focus {
            Focus::Tables => self.previous_table(),
            Focus::Details => {
                if self.detail_selection > 0 {
                    self.detail_selection -= 1;
                }
            }
        }
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Tables => self.next_table(),
            Focus::Details => {
                let max = self.detail_item_count();
                if self.detail_selection < max.saturating_sub(1) {
                    self.detail_selection += 1;
                }
            }
        }
    }

    fn focus_tables(&mut self) {
        self.focus = Focus::Tables;
    }

    fn focus_details(&mut self) {
        self.focus = Focus::Details;
        self.detail_selection = 0;
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Tables => Focus::Details,
            Focus::Details => Focus::Tables,
        };
        if self.focus == Focus::Details {
            self.detail_selection = 0;
        }
    }

    fn activate(&mut self) {
        match self.focus {
            Focus::Tables => {
                // Toggle expand/collapse
                if let Some(expanded) = self.expanded.get_mut(self.selected_table) {
                    *expanded = !*expanded;
                }
            }
            Focus::Details => {
                if let Some(table) = self.schema.tables.get(self.selected_table) {
                    let source_offset = self.detail_source_offset();

                    // Source row is index 0 (if present)
                    if source_offset > 0 && self.detail_selection == 0 {
                        // Open source in editor
                        if let Some(file) = &table.source.file {
                            let line = table.source.line.unwrap_or(1);
                            let _ = self.open_in_editor(file, line);
                        }
                        return;
                    }

                    let col_count = table.columns.len();
                    let adjusted_selection = self.detail_selection - source_offset;

                    // Jump to FK target if on a FK row
                    if adjusted_selection >= col_count {
                        let fk_idx = adjusted_selection - col_count;
                        if let Some(fk) = table.foreign_keys.get(fk_idx) {
                            // Find the target table
                            if let Some(target_idx) = self
                                .schema
                                .tables
                                .iter()
                                .position(|t| t.name == fk.references_table)
                            {
                                self.selected_table = target_idx;
                                self.table_state.select(Some(target_idx));
                                self.focus = Focus::Tables;
                                // Expand the target table
                                if let Some(expanded) = self.expanded.get_mut(target_idx) {
                                    *expanded = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn detail_item_count(&self) -> usize {
        if let Some(table) = self.schema.tables.get(self.selected_table) {
            // +1 for Source row (if present), then columns, FKs, indices
            let source_row = if table.source.is_known() { 1 } else { 0 };
            source_row + table.columns.len() + table.foreign_keys.len() + table.indices.len()
        } else {
            0
        }
    }

    /// Returns the offset for column indices based on whether source is shown
    fn detail_source_offset(&self) -> usize {
        if let Some(table) = self.schema.tables.get(self.selected_table) {
            if table.source.is_known() { 1 } else { 0 }
        } else {
            0
        }
    }

    fn next_table(&mut self) {
        if self.schema.tables.is_empty() {
            return;
        }
        self.selected_table = (self.selected_table + 1) % self.schema.tables.len();
        self.table_state.select(Some(self.selected_table));
        self.detail_selection = 0;
    }

    fn previous_table(&mut self) {
        if self.schema.tables.is_empty() {
            return;
        }
        self.selected_table = self
            .selected_table
            .checked_sub(1)
            .unwrap_or(self.schema.tables.len() - 1);
        self.table_state.select(Some(self.selected_table));
        self.detail_selection = 0;
    }

    fn go_to_first(&mut self) {
        match self.focus {
            Focus::Tables => {
                self.selected_table = 0;
                self.table_state.select(Some(0));
                self.detail_selection = 0;
            }
            Focus::Details => {
                self.detail_selection = 0;
            }
        }
    }

    fn go_to_last(&mut self) {
        match self.focus {
            Focus::Tables => {
                if !self.schema.tables.is_empty() {
                    self.selected_table = self.schema.tables.len() - 1;
                    self.table_state.select(Some(self.selected_table));
                    self.detail_selection = 0;
                }
            }
            Focus::Details => {
                let max = self.detail_item_count();
                if max > 0 {
                    self.detail_selection = max - 1;
                }
            }
        }
    }

    fn half_page_down(&mut self) {
        let half = self.visible_height / 2;
        match self.focus {
            Focus::Tables => {
                let max = self.schema.tables.len();
                if max > 0 {
                    self.selected_table = (self.selected_table + half).min(max - 1);
                    self.table_state.select(Some(self.selected_table));
                    self.detail_selection = 0;
                }
            }
            Focus::Details => {
                let max = self.detail_item_count();
                if max > 0 {
                    self.detail_selection = (self.detail_selection + half).min(max - 1);
                }
            }
        }
    }

    fn half_page_up(&mut self) {
        let half = self.visible_height / 2;
        match self.focus {
            Focus::Tables => {
                self.selected_table = self.selected_table.saturating_sub(half);
                self.table_state.select(Some(self.selected_table));
                self.detail_selection = 0;
            }
            Focus::Details => {
                self.detail_selection = self.detail_selection.saturating_sub(half);
            }
        }
    }

    fn open_in_editor(&self, file: &str, line: u32) -> io::Result<()> {
        // Restore terminal before launching editor
        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;

        // Check if we're in Zed's integrated terminal
        let in_zed = std::env::var("TERM_PROGRAM")
            .map(|v| v == "zed")
            .unwrap_or(false);

        let status = if in_zed {
            // Use Zed CLI to open in the current workspace
            std::process::Command::new("zed")
                .arg(format!("{}:{}", file, line))
                .status()
        } else {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

            // Try editor-specific line number syntax
            match editor.as_str() {
                "code" | "code-insiders" => std::process::Command::new(&editor)
                    .arg("--goto")
                    .arg(format!("{}:{}", file, line))
                    .status(),
                "subl" | "sublime" => std::process::Command::new(&editor)
                    .arg(format!("{}:{}", file, line))
                    .status(),
                "zed" => std::process::Command::new(&editor)
                    .arg(format!("{}:{}", file, line))
                    .status(),
                _ => {
                    // vim/nvim/nano/emacs style: +line
                    std::process::Command::new(&editor)
                        .arg(format!("+{}", line))
                        .arg(file)
                        .status()
                }
            }
        };

        // Re-enter TUI mode
        stdout().execute(EnterAlternateScreen)?;
        enable_raw_mode()?;

        status.map(|_| ())
    }

    fn ui(&mut self, frame: &mut Frame) {
        // Layout: header (1 line) + main area + help bar (1 line)
        let header_area = Rect {
            x: 0,
            y: 0,
            width: frame.area().width,
            height: 1,
        };

        let main_area = Rect {
            x: frame.area().x,
            y: 1,
            width: frame.area().width,
            height: frame.area().height.saturating_sub(2), // -1 for header, -1 for help
        };

        // Update visible height for half-page scrolling
        self.visible_height = main_area.height.saturating_sub(2) as usize; // -2 for borders

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled(" [dibs] ", Style::default().fg(Color::Cyan).bold()),
            Span::styled("Schema Browser", Style::default().fg(Color::White)),
            Span::raw("  "),
            Span::styled(
                format!("{} tables", self.schema.tables.len()),
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .style(Style::default().bg(Color::Black));
        frame.render_widget(header, header_area);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(main_area);

        // Left pane: table list with expand/collapse icons
        let table_items: Vec<ListItem> = self
            .schema
            .tables
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let expanded = self.expanded.get(i).copied().unwrap_or(false);
                let icon = if expanded { "▼" } else { "▶" };
                ListItem::new(format!("{} {} ({})", icon, t.name, t.columns.len()))
            })
            .collect();

        let tables_border_style = if self.focus == Focus::Tables {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let tables_list = List::new(table_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(tables_border_style)
                    .title(" Tables ")
                    .title_style(Style::default().fg(Color::Cyan).bold()),
            )
            .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White).bold())
            .highlight_symbol("› ");

        frame.render_stateful_widget(tables_list, chunks[0], &mut self.table_state);

        // Right pane: selected table details with selectable items
        if let Some(table) = self.schema.tables.get(self.selected_table) {
            let mut lines = vec![Line::from(vec![
                Span::styled("Table: ", Style::default().fg(Color::Gray)),
                Span::styled(&table.name, Style::default().fg(Color::Cyan).bold()),
            ])];

            let source_offset = self.detail_source_offset();

            // Show source location if available (selectable - index 0)
            if table.source.is_known() {
                let is_selected = self.focus == Focus::Details && self.detail_selection == 0;
                let prefix = if is_selected { "› " } else { "  " };
                lines.push(Line::from(vec![
                    Span::raw(prefix),
                    Span::styled("Source: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        table.source.to_string(),
                        if is_selected {
                            Style::default().fg(Color::Cyan).bold()
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    if is_selected {
                        Span::styled(" [Enter to open]", Style::default().fg(Color::DarkGray))
                    } else {
                        Span::raw("")
                    },
                ]));
            }

            // Show doc comment if available
            if let Some(doc) = &table.doc {
                lines.push(Line::from(vec![
                    Span::styled("  /// ", Style::default().fg(Color::Green)),
                    Span::styled(doc, Style::default().fg(Color::Green).italic()),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Columns:",
                Style::default().fg(Color::Yellow).bold(),
            )));

            for (i, col) in table.columns.iter().enumerate() {
                let is_selected =
                    self.focus == Focus::Details && self.detail_selection == i + source_offset;
                let prefix = if is_selected { "› " } else { "  " };

                let mut spans = vec![
                    Span::raw(prefix),
                    Span::styled(
                        &col.name,
                        if is_selected {
                            Style::default().fg(Color::White).bold()
                        } else {
                            Style::default().fg(Color::White)
                        },
                    ),
                    Span::raw(": "),
                    Span::styled(col.pg_type.to_string(), Style::default().fg(Color::Blue)),
                ];

                if col.primary_key {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled("PK", Style::default().fg(Color::Yellow)));
                }
                if col.unique {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled("UNIQUE", Style::default().fg(Color::Magenta)));
                }
                if !col.nullable {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled("NOT NULL", Style::default().fg(Color::Red)));
                }
                if let Some(default) = &col.default {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        format!("DEFAULT {}", default),
                        Style::default().fg(Color::Gray),
                    ));
                }

                let mut line = Line::from(spans);
                if is_selected {
                    line = line.style(Style::default().bg(Color::DarkGray));
                }
                lines.push(line);
            }

            if !table.foreign_keys.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Foreign Keys: (Enter to jump)",
                    Style::default().fg(Color::Green).bold(),
                )));

                let col_count = table.columns.len();
                for (i, fk) in table.foreign_keys.iter().enumerate() {
                    let is_selected = self.focus == Focus::Details
                        && self.detail_selection == source_offset + col_count + i;
                    let prefix = if is_selected { "› " } else { "  " };

                    let mut line = Line::from(vec![
                        Span::raw(prefix),
                        Span::styled(
                            fk.columns.join(", "),
                            if is_selected {
                                Style::default().fg(Color::White).bold()
                            } else {
                                Style::default().fg(Color::White)
                            },
                        ),
                        Span::styled(" → ", Style::default().fg(Color::Gray)),
                        Span::styled(
                            &fk.references_table,
                            Style::default().fg(Color::Cyan).underlined(),
                        ),
                        Span::raw("."),
                        Span::styled(
                            fk.references_columns.join(", "),
                            Style::default().fg(Color::White),
                        ),
                    ]);
                    if is_selected {
                        line = line.style(Style::default().bg(Color::DarkGray));
                    }
                    lines.push(line);
                }
            }

            if !table.indices.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Indices:",
                    Style::default().fg(Color::Magenta).bold(),
                )));

                let col_count = table.columns.len();
                let fk_count = table.foreign_keys.len();
                for (i, idx) in table.indices.iter().enumerate() {
                    let is_selected = self.focus == Focus::Details
                        && self.detail_selection == source_offset + col_count + fk_count + i;
                    let prefix = if is_selected { "› " } else { "  " };

                    let mut spans = vec![
                        Span::raw(prefix),
                        Span::styled(
                            &idx.name,
                            if is_selected {
                                Style::default().fg(Color::White).bold()
                            } else {
                                Style::default().fg(Color::White)
                            },
                        ),
                        Span::styled(" on ", Style::default().fg(Color::Gray)),
                        Span::styled(
                            format!(
                                "({})",
                                idx.columns
                                    .iter()
                                    .map(|c| format!(
                                        "{}{}{}",
                                        c.name,
                                        c.order.to_sql(),
                                        c.nulls.to_sql()
                                    ))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                            Style::default().fg(Color::Cyan),
                        ),
                    ];

                    if idx.unique {
                        spans.push(Span::raw(" "));
                        spans.push(Span::styled("UNIQUE", Style::default().fg(Color::Yellow)));
                    }

                    let mut line = Line::from(spans);
                    if is_selected {
                        line = line.style(Style::default().bg(Color::DarkGray));
                    }
                    lines.push(line);
                }
            }

            let details_border_style = if self.focus == Focus::Details {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let details = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(details_border_style)
                    .title(" Details ")
                    .title_style(Style::default().fg(Color::Cyan).bold()),
            );

            frame.render_widget(details, chunks[1]);
        }

        // Help bar at the bottom
        let help = Paragraph::new(Line::from(vec![
            Span::styled(" j/k ", Style::default().fg(Color::Yellow)),
            Span::raw("nav  "),
            Span::styled("gg/G ", Style::default().fg(Color::Yellow)),
            Span::raw("top/bottom  "),
            Span::styled("^D/^U ", Style::default().fg(Color::Yellow)),
            Span::raw("½page  "),
            Span::styled("Tab ", Style::default().fg(Color::Yellow)),
            Span::raw("pane  "),
            Span::styled("Enter ", Style::default().fg(Color::Yellow)),
            Span::raw("expand  "),
            Span::styled("q ", Style::default().fg(Color::Yellow)),
            Span::raw("quit"),
        ]))
        .style(Style::default().bg(Color::DarkGray));

        let help_area = Rect {
            x: 0,
            y: frame.area().height.saturating_sub(1),
            width: frame.area().width,
            height: 1,
        };
        frame.render_widget(help, help_area);
    }
}

/// Mask password in database URL for display
#[allow(dead_code)]
fn mask_password(url: &str) -> String {
    // Simple masking: replace password between :// and @
    if let Some(start) = url.find("://")
        && let Some(at) = url.find('@')
    {
        let prefix = &url[..start + 3];
        let suffix = &url[at..];
        if let Some(colon) = url[start + 3..at].find(':') {
            let user = &url[start + 3..start + 3 + colon];
            return format!("{}{}:***{}", prefix, user, suffix);
        }
    }
    url.to_string()
}

fn run_migrate(config: &Config) {
    use dibs_proto::{LogLevel, MigrateRequest};
    use owo_colors::OwoColorize as _;
    use tracing::info;

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    let database_url = config.require_database_url();
    info!(database_url = %mask_password(database_url), "Running migrations");

    rt.block_on(async {
        // Connect to the db crate via roam
        let conn = match service::connect_to_service(&config.db).await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Failed to connect to db service: {}", e);
                std::process::exit(1);
            }
        };

        let client = conn.client();

        // Create a channel for receiving log messages
        let (log_tx, mut log_rx) = roam::channel::<dibs_proto::MigrationLog>();

        // Spawn a task to print log messages as they arrive
        let log_printer = tokio::spawn(async move {
            while let Ok(Some(log)) = log_rx.recv().await {
                let prefix = match log.level {
                    LogLevel::Debug => "DEBUG".dimmed().to_string(),
                    LogLevel::Info => "INFO".blue().to_string(),
                    LogLevel::Warn => "WARN".yellow().to_string(),
                    LogLevel::Error => "ERROR".red().to_string(),
                };
                if let Some(migration) = &log.migration {
                    println!("[{}] [{}] {}", prefix, migration.cyan(), log.message);
                } else {
                    println!("[{}] {}", prefix, log.message);
                }
            }
        });

        // Call the migrate method
        let result = client
            .migrate(
                MigrateRequest {
                    database_url: database_url.to_string(),
                    migration: None, // Run all pending
                },
                log_tx,
            )
            .await;

        // Wait for log printer to finish
        let _ = log_printer.await;

        match result {
            Ok(res) => {
                if res.applied.is_empty() {
                    println!("{}", "No pending migrations.".green());
                } else {
                    println!(
                        "{}",
                        format!(
                            "Applied {} migration(s) in {}ms",
                            res.applied.len(),
                            res.total_time_ms
                        )
                        .green()
                    );
                }
            }
            Err(e) => {
                eprintln!("Migration failed: {:?}", e);
                std::process::exit(1);
            }
        }
    });
}

fn run_status(config: &Config) {
    use dibs_proto::MigrationStatusRequest;
    #[allow(unused_imports)]
    use owo_colors::OwoColorize as _;
    use tracing::info;

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    let database_url = config.require_database_url();
    info!(database_url = %mask_password(database_url), "Checking migration status");

    rt.block_on(async {
        // Connect to the db crate via roam
        let conn = match service::connect_to_service(&config.db).await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Failed to connect to db service: {}", e);
                std::process::exit(1);
            }
        };

        let client = conn.client();

        // Call the migration_status method
        let result = client
            .migration_status(MigrationStatusRequest {
                database_url: database_url.to_string(),
            })
            .await;

        match result {
            Ok(migrations) => {
                if migrations.is_empty() {
                    println!("No migrations registered.");
                } else {
                    println!("Migration status:");
                    println!();
                    for m in &migrations {
                        let status = if m.applied {
                            "✓".green().to_string()
                        } else {
                            "○".yellow().to_string()
                        };
                        println!("  {} {} - {}", status, m.version, m.name);
                    }
                    println!();
                    let applied = migrations.iter().filter(|m| m.applied).count();
                    let pending = migrations.len() - applied;
                    println!(
                        "{} applied, {} pending",
                        applied.to_string().green(),
                        if pending > 0 {
                            pending.to_string().yellow().to_string()
                        } else {
                            pending.to_string()
                        }
                    );
                }
            }
            Err(e) => {
                eprintln!("Failed to get migration status: {:?}", e);
                std::process::exit(1);
            }
        }
    });
}

fn run_diff(config: &Config) {
    use dibs_proto::DiffRequest;
    #[allow(unused_imports)]
    use owo_colors::OwoColorize as _;
    use tracing::info;

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    let database_url = config.require_database_url();
    info!(database_url = %mask_password(database_url), "Comparing schema to database");

    rt.block_on(async {
        // Connect to the db crate via roam
        let conn = match service::connect_to_service(&config.db).await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Failed to connect to db service: {}", e);
                std::process::exit(1);
            }
        };

        let client = conn.client();

        // Call the diff method
        let result = client
            .diff(DiffRequest {
                database_url: database_url.to_string(),
            })
            .await;

        match result {
            Ok(diff) => {
                if diff.table_diffs.is_empty() {
                    println!("{}", "No changes detected.".green());
                } else {
                    print_diff_result(&diff);
                }
            }
            Err(e) => {
                eprintln!("Diff failed: {:?}", e);
                std::process::exit(1);
            }
        }
    });
}

fn print_diff_result(diff: &dibs_proto::DiffResult) {
    use dibs_proto::ChangeKind;
    #[allow(unused_imports)]
    use owo_colors::OwoColorize as _;

    println!(
        "{}",
        format!(
            "Changes detected ({} tables affected):",
            diff.table_diffs.len()
        )
        .as_str()
        .yellow()
    );
    println!();

    for table_diff in &diff.table_diffs {
        println!("  {}:", table_diff.table.as_str().cyan().bold());

        for change in &table_diff.changes {
            let colored = match change.kind {
                ChangeKind::Add => format!("+ {}", change.description).green().to_string(),
                ChangeKind::Drop => format!("- {}", change.description).red().to_string(),
                ChangeKind::Alter => format!("~ {}", change.description).yellow().to_string(),
            };
            println!("    {}", colored);
        }
        println!();
    }
}

fn generate_migration(db_config: &DbConfig, name: &str) {
    let now = Zoned::now();
    let timestamp = now.strftime("%Y%m%d%H%M%S");

    // Convert name to snake_case for the module name
    let module_name = name.replace('-', "_").to_lowercase();

    // Find migrations directory from db crate
    let migrations_dir = db_config
        .crate_name
        .as_ref()
        .and_then(|crate_name| config::find_crate_path_for_watch(crate_name))
        .map(|p| p.join("src/migrations"))
        .unwrap_or_else(|| std::path::PathBuf::from("src/migrations"));

    if !migrations_dir.exists()
        && let Err(e) = fs::create_dir_all(&migrations_dir)
    {
        eprintln!("Failed to create migrations directory: {}", e);
        std::process::exit(1);
    }

    // Generate filename: m<timestamp>_<name>.rs
    let filename = format!("m{}_{}.rs", timestamp, module_name);
    let filepath = migrations_dir.join(&filename);

    if filepath.exists() {
        eprintln!("Migration file already exists: {}", filepath.display());
        std::process::exit(1);
    }

    // Generate the version string (matches the format expected by #[dibs::migration])
    let version = format!("{}-{}", timestamp, name);

    // Generate Rust migration content
    let content = format!(
        r#"//! Migration: {name}
//! Created: {created}

use dibs::{{MigrationContext, Result}};

#[dibs::migration("{version}")]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> Result<()> {{
    // Add your migration SQL here
    // ctx.execute("CREATE TABLE ...").await?;

    Ok(())
}}
"#,
        name = name,
        created = now.strftime("%Y-%m-%d %H:%M:%S %Z"),
        version = version,
    );

    let mut file = match fs::File::create(&filepath) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create migration file: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = file.write_all(content.as_bytes()) {
        eprintln!("Failed to write migration file: {}", e);
        std::process::exit(1);
    }

    println!("Created migration: {}", filepath.display());
    println!();
    println!("Don't forget to add the module to your migrations/mod.rs:");
    println!("  mod {};", filename.trim_end_matches(".rs"));
}

fn run_generate_from_diff(config: &Config, name: &str) {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(run_generate_from_diff_via_roam(config, name));
}

async fn run_generate_from_diff_via_roam(config: &Config, name: &str) {
    use dibs_proto::DiffRequest;
    #[allow(unused_imports)]
    use owo_colors::OwoColorize as _;
    use tracing::info;

    let database_url = config.require_database_url();

    info!(
        db_crate = ?config.db.crate_name,
        db_binary = ?config.db.binary,
        "Connecting to db service"
    );

    // Connect to the db crate via roam
    let conn = match service::connect_to_service(&config.db).await {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Failed to connect to db service: {}", e);
            std::process::exit(1);
        }
    };

    let client = conn.client();

    // Call generate_migration_sql method
    let result = client
        .generate_migration_sql(DiffRequest {
            database_url: database_url.to_string(),
        })
        .await;

    match result {
        Ok(sql) => {
            if sql.trim().is_empty() {
                println!("{}", "No changes detected.".green());
                println!();
                println!("Schema matches database - no migration needed.");
                return;
            }

            // Create migration file
            match create_migration_file_from_sql(&config.db, name, &sql) {
                Ok(path) => {
                    println!("{}", "Migration created successfully!".green());
                    println!();
                    println!("File: {}", path);
                }
                Err(e) => {
                    eprintln!("Failed to create migration file: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to generate migration SQL: {:?}", e);
            std::process::exit(1);
        }
    }
}

fn create_migration_file_from_sql(
    db_config: &DbConfig,
    name: &str,
    sql: &str,
) -> Result<String, std::io::Error> {
    use std::fs;
    use std::io::Write;

    let now = Zoned::now();
    let timestamp = now.strftime("%Y_%m_%d_%H%M%S");

    // Convert name to snake_case for the module name
    let module_name = name.replace('-', "_").to_lowercase();

    // Find migrations directory from db crate
    let migrations_dir = db_config
        .crate_name
        .as_ref()
        .and_then(|crate_name| config::find_crate_path_for_watch(crate_name))
        .map(|p| p.join("src/migrations"))
        .unwrap_or_else(|| std::path::PathBuf::from("src/migrations"));

    if !migrations_dir.exists() {
        fs::create_dir_all(&migrations_dir)?;
    }

    // Generate filename: m_2026_01_23_103000_name.rs
    let filename = format!("m{}_{}.rs", timestamp, module_name);
    let filepath = migrations_dir.join(&filename);

    if filepath.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("Migration file already exists: {}", filepath.display()),
        ));
    }

    // Parse SQL into ctx.execute() calls
    let sql_calls = parse_sql_to_calls(sql);

    // Generate Rust migration content
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
            mod_file.write_all(format!("\n{}", module_line).as_bytes())?;
        }
    } else {
        // Create new mod.rs
        let mut mod_file = fs::File::create(&mod_rs_path)?;
        mod_file.write_all(format!("//! Database migrations.\n\n{}\n", module_line).as_bytes())?;
    }

    Ok(filepath.display().to_string())
}

/// Parse SQL into migration function calls.
///
/// Splits SQL by statements (semicolons) and generates appropriate
/// ctx.execute() calls. Multi-line statements use raw string literals.
fn parse_sql_to_calls(sql: &str) -> String {
    let mut result = String::new();

    for chunk in split_sql_into_chunks(sql) {
        match chunk {
            SqlChunk::Comment(comment) => {
                result.push_str(&format!("    // {}\n", comment));
            }
            SqlChunk::Statement(stmt) => {
                if !stmt.trim().is_empty() {
                    result.push_str(&format_sql_call(&stmt));
                }
            }
        }
    }

    result
}

enum SqlChunk {
    Comment(String),
    Statement(String),
}

/// Split a SQL script into statements and comment lines.
///
/// This is *not* a full SQL parser, but it does correctly avoid splitting on
/// semicolons inside:
/// - single-quoted strings: `'...'`
/// - double-quoted identifiers: `"identifier"`
/// - dollar-quoted strings: `$$ ... $$` or `$tag$ ... $tag$`
fn split_sql_into_chunks(sql: &str) -> Vec<SqlChunk> {
    let mut out = Vec::new();
    let mut current = String::new();

    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut dollar_quote_tag: Option<String> = None;

    let mut i = 0usize;
    while i < sql.len() {
        // Handle exiting a dollar-quoted section.
        if let Some(tag) = dollar_quote_tag.as_deref()
            && sql[i..].starts_with(tag)
        {
            current.push_str(tag);
            i += tag.len();
            dollar_quote_tag = None;
            continue;
        }

        let ch = sql[i..]
            .chars()
            .next()
            .expect("i is always a char boundary");
        let ch_len = ch.len_utf8();

        // SQL line comments: -- ...\n
        if !in_single_quote
            && !in_double_quote
            && dollar_quote_tag.is_none()
            && ch == '-'
            && sql.as_bytes().get(i + 1) == Some(&b'-')
        {
            // Flush any pending statement first.
            if !current.trim().is_empty() {
                out.push(SqlChunk::Statement(std::mem::take(&mut current)));
            } else {
                current.clear();
            }

            let rest = &sql[i..];
            let end = rest.find('\n').map(|pos| i + pos).unwrap_or(sql.len());
            let line = sql[i..end].trim_start_matches("--").trim().to_string();
            out.push(SqlChunk::Comment(line));

            i = if end < sql.len() { end + 1 } else { end };
            continue;
        }

        // Entering a dollar-quoted section: $$ ... $$ or $tag$ ... $tag$
        if !in_single_quote && !in_double_quote && dollar_quote_tag.is_none() && ch == '$' {
            let rest = &sql[i + 1..];
            if let Some(end_rel) = rest.find('$') {
                let tag_body = &rest[..end_rel];
                if tag_body
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    let tag_end = i + 1 + end_rel + 1;
                    let tag = sql[i..tag_end].to_string();
                    current.push_str(&tag);
                    i = tag_end;
                    dollar_quote_tag = Some(tag);
                    continue;
                }
            }
        }

        // Single-quoted string handling (including '' escaping)
        if dollar_quote_tag.is_none() && !in_double_quote && ch == '\'' {
            if in_single_quote && sql.as_bytes().get(i + 1) == Some(&b'\'') {
                // Escaped single quote: ''
                current.push_str("''");
                i += 2;
                continue;
            }
            in_single_quote = !in_single_quote;
            current.push('\'');
            i += ch_len;
            continue;
        }

        // Double-quoted identifier handling (including "" escaping)
        if dollar_quote_tag.is_none() && !in_single_quote && ch == '"' {
            if in_double_quote && sql.as_bytes().get(i + 1) == Some(&b'"') {
                // Escaped double quote: ""
                current.push_str("\"\"");
                i += 2;
                continue;
            }
            in_double_quote = !in_double_quote;
            current.push('"');
            i += ch_len;
            continue;
        }

        // Statement terminator (only when not inside any quoted context)
        if !in_single_quote && !in_double_quote && dollar_quote_tag.is_none() && ch == ';' {
            current.push(';');
            out.push(SqlChunk::Statement(std::mem::take(&mut current)));
            i += ch_len;
            continue;
        }

        current.push_str(&sql[i..i + ch_len]);
        i += ch_len;
    }

    if !current.trim().is_empty() {
        out.push(SqlChunk::Statement(current));
    }

    out
}

/// Format a single SQL statement as a ctx.execute() call.
fn format_sql_call(sql: &str) -> String {
    let trimmed = sql.trim().trim_end_matches(';');

    // Single-line statements use regular string literals
    if !trimmed.contains('\n') {
        return format!(
            "    ctx.execute(\"{}\").await?;\n",
            trimmed.replace('"', "\\\"")
        );
    }

    // Multi-line statements use raw string literals
    // We need to use enough hashes to avoid conflicts with content
    let mut hash_count = 3;
    let test_pattern = "#".repeat(hash_count);
    while trimmed.contains(&format!("\"{}\"", test_pattern)) {
        hash_count += 1;
    }
    let hashes = "#".repeat(hash_count);

    format!(
        "    ctx.execute(r{hashes}\"{sql}\"{hashes}).await?;\n",
        hashes = hashes,
        sql = trimmed
    )
}

#[cfg(test)]
mod sql_split_tests {
    use super::parse_sql_to_calls;

    #[test]
    fn dollar_quoted_function_body_does_not_split_on_inner_semicolons() {
        let sql = r#"
CREATE OR REPLACE FUNCTION trgfn_test() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT (TRUE) THEN
        RAISE EXCEPTION 'nope';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_test BEFORE INSERT OR UPDATE ON "order" FOR EACH ROW EXECUTE FUNCTION trgfn_test();
"#;

        let calls = parse_sql_to_calls(sql);

        // One call for function + one call for trigger.
        assert_eq!(calls.matches("ctx.execute").count(), 2);
        assert!(calls.contains("CREATE OR REPLACE FUNCTION"));
        assert!(calls.contains("CREATE TRIGGER"));
    }
}
