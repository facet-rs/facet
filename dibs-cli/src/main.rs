use std::fs;
use std::io::{self, IsTerminal, Write, stdout};
use std::path::Path;

use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use facet::Facet;
use facet_args as args;
use jiff::Zoned;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

mod config;
mod highlight;
mod service;
mod tables;
mod tui;

/// Postgres toolkit for Rust, powered by facet reflection.
#[derive(Facet, Debug)]
struct Cli {
    /// Show version information
    #[facet(args::named, args::short = 'V')]
    version: bool,

    /// Command to run
    #[facet(default, args::subcommand)]
    command: Option<Commands>,
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
    /// Browse the current schema
    Schema {
        /// Output as plain text (default when not a TTY)
        #[facet(default, args::named)]
        plain: bool,

        /// Output as SQL (CREATE TABLE statements)
        #[facet(default, args::named)]
        sql: bool,
    },
}

fn main() {
    // Load .env file if present (silently ignore if not found)
    let _ = dotenvy::dotenv();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let result: Result<Cli, _> = args::from_slice(&args_ref);

    match result {
        Ok(cli) => run(cli),
        Err(err) if err.is_help_request() => {
            print!("{}", err.help_text().unwrap_or(""));
        }
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) {
    if cli.version {
        println!("dibs {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    match cli.command {
        Some(Commands::Migrate) => {
            run_migrate();
        }
        Some(Commands::Status) => {
            run_status();
        }
        Some(Commands::Diff) => {
            run_diff();
        }
        Some(Commands::Generate { name }) => {
            generate_migration(&name);
        }
        Some(Commands::Schema { plain, sql }) => {
            let schema = dibs::Schema::collect();

            if schema.tables.is_empty() {
                println!("No tables registered.");
                println!();
                println!("Define tables using #[facet(dibs::table = \"name\")] on Facet structs.");
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
        None => {
            // No subcommand: launch unified TUI (the default human interface)
            if stdout().is_terminal() {
                // Try to load config for roam connection
                let config = config::Config::load().ok();
                let app = tui::App::new();
                if let Err(e) = app.run(config.as_ref().map(|(c, _)| c)) {
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
            println!(
                "  INDEX {} on ({}){}",
                idx.name,
                idx.columns.join(", "),
                unique
            );
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
                            format!("({})", idx.columns.join(", ")),
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

fn run_migrate() {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        eprintln!("Error: DATABASE_URL environment variable not set.");
        eprintln!();
        eprintln!("Set it via:");
        eprintln!("  export DATABASE_URL=postgres://user:pass@host/db");
        std::process::exit(1);
    });

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    // Try to load dibs.toml - if present, use roam
    if let Ok((cfg, config_path)) = config::Config::load() {
        rt.block_on(run_migrate_via_roam(&cfg, &config_path, &url));
    } else {
        // No dibs.toml - use local migration runner
        rt.block_on(run_migrate_local(&url));
    }
}

async fn run_migrate_via_roam(cfg: &config::Config, config_path: &Path, database_url: &str) {
    use dibs_proto::{LogLevel, MigrateRequest};
    use owo_colors::OwoColorize as _;

    println!(
        "{}",
        format!("Using config: {}", config_path.display())
            .as_str()
            .dimmed()
    );

    // Connect to the db crate via roam
    let conn = match service::connect_to_service(cfg).await {
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
}

async fn run_migrate_local(database_url: &str) {
    #[allow(unused_imports)]
    use owo_colors::OwoColorize as _;

    // Connect to database
    let (mut client, connection) =
        match tokio_postgres::connect(database_url, tokio_postgres::NoTls).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to database: {}", e);
                std::process::exit(1);
            }
        };

    // Spawn connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Database connection error: {}", e);
        }
    });

    // Run migrations
    let mut runner = dibs::MigrationRunner::new(&mut client);

    match runner.migrate().await {
        Ok(applied) => {
            if applied.is_empty() {
                println!("{}", "No pending migrations.".green());
            } else {
                for version in &applied {
                    println!("  {} {}", "Applied".green(), version);
                }
                println!(
                    "{}",
                    format!("Applied {} migration(s)", applied.len()).green()
                );
            }
        }
        Err(e) => {
            eprintln!("Migration failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_status() {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        eprintln!("Error: DATABASE_URL environment variable not set.");
        eprintln!();
        eprintln!("Set it via:");
        eprintln!("  export DATABASE_URL=postgres://user:pass@host/db");
        std::process::exit(1);
    });

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    // Try to load dibs.toml - if present, use roam
    if let Ok((cfg, config_path)) = config::Config::load() {
        rt.block_on(run_status_via_roam(&cfg, &config_path, &url));
    } else {
        // No dibs.toml - use local migration status
        rt.block_on(run_status_local(&url));
    }
}

async fn run_status_via_roam(cfg: &config::Config, config_path: &Path, database_url: &str) {
    use dibs_proto::MigrationStatusRequest;
    use owo_colors::OwoColorize as _;

    println!(
        "{}",
        format!("Using config: {}", config_path.display())
            .as_str()
            .dimmed()
    );

    // Connect to the db crate via roam
    let conn = match service::connect_to_service(cfg).await {
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
}

async fn run_status_local(database_url: &str) {
    #[allow(unused_imports)]
    use owo_colors::OwoColorize as _;

    // Connect to database
    let (mut client, connection) =
        match tokio_postgres::connect(database_url, tokio_postgres::NoTls).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to database: {}", e);
                std::process::exit(1);
            }
        };

    // Spawn connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Database connection error: {}", e);
        }
    });

    // Get migration status
    let runner = dibs::MigrationRunner::new(&mut client);

    match runner.status().await {
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
            eprintln!("Failed to get migration status: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_diff() {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        eprintln!("Error: DATABASE_URL environment variable not set.");
        eprintln!();
        eprintln!("Set it via:");
        eprintln!("  export DATABASE_URL=postgres://user:pass@host/db");
        std::process::exit(1);
    });

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    // Try to load dibs.toml - if present, use roam to call the db crate
    if let Ok((cfg, config_path)) = config::Config::load() {
        rt.block_on(run_diff_via_roam(&cfg, &config_path, &url));
    } else {
        // No dibs.toml - use local schema collection (legacy mode)
        rt.block_on(run_diff_local(&url));
    }
}

async fn run_diff_via_roam(cfg: &config::Config, config_path: &Path, database_url: &str) {
    use dibs_proto::DiffRequest;
    use owo_colors::OwoColorize as _;

    println!(
        "{}",
        format!("Using config: {}", config_path.display())
            .as_str()
            .dimmed()
    );

    // Connect to the db crate via roam
    let conn = match service::connect_to_service(cfg).await {
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

async fn run_diff_local(database_url: &str) {
    // Collect Rust schema
    let rust_schema = dibs::Schema::collect();

    if rust_schema.tables.is_empty() {
        eprintln!("No tables registered in Rust schema.");
        eprintln!();
        eprintln!("Define tables using #[facet(dibs::table = \"name\")] on Facet structs.");
        std::process::exit(1);
    }

    // Connect to database and introspect
    let result = async {
        let (client, connection) =
            tokio_postgres::connect(database_url, tokio_postgres::NoTls).await?;

        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Database connection error: {}", e);
            }
        });

        // Introspect database schema
        let db_schema = dibs::Schema::from_database(&client).await?;

        Ok::<_, dibs::Error>(db_schema)
    }
    .await;

    let db_schema = match result {
        Ok(schema) => schema,
        Err(e) => {
            eprintln!("Failed to introspect database: {}", e);
            std::process::exit(1);
        }
    };

    // Compute diff
    let diff = rust_schema.diff(&db_schema);

    if diff.is_empty() {
        #[allow(unused_imports)]
        use owo_colors::OwoColorize as _;
        println!("{}", "No changes detected.".green());
        println!();
        println!(
            "Rust schema ({} tables) matches database.",
            rust_schema.tables.len()
        );
    } else {
        print_diff(&diff);
    }
}

fn print_diff(diff: &dibs::SchemaDiff) {
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
            let formatted = format!("{}", change);
            let colored = if formatted.starts_with('+') {
                formatted.as_str().green().to_string()
            } else if formatted.starts_with('-') {
                formatted.as_str().red().to_string()
            } else if formatted.starts_with('~') {
                formatted.as_str().yellow().to_string()
            } else {
                formatted
            };
            println!("    {}", colored);
        }
        println!();
    }
}

fn generate_migration(name: &str) {
    let now = Zoned::now();
    let timestamp = now.strftime("%Y%m%d%H%M%S");

    // Convert name to snake_case for the module name
    let module_name = name.replace('-', "_").to_lowercase();

    // Create migrations directory if it doesn't exist
    let migrations_dir = Path::new("src/migrations");
    if !migrations_dir.exists()
        && let Err(e) = fs::create_dir_all(migrations_dir)
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
