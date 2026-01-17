use std::io::{self, IsTerminal, stdout};

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

// Example table definitions for testing

#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "tenants")]
struct Tenant {
    #[facet(dibs::pk)]
    id: i64,

    #[facet(dibs::unique)]
    slug: String,

    #[facet(dibs::index)]
    name: String,

    #[facet(dibs::default = "now()")]
    created_at: i64,
}

#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "users")]
#[facet(dibs::composite_index(columns = "tenant_id,email"))]
struct User {
    #[facet(dibs::pk)]
    id: i64,

    #[facet(dibs::unique)]
    email: String,

    #[facet(dibs::index)]
    name: String,

    bio: Option<String>,

    #[facet(dibs::fk = "tenants.id", dibs::index)]
    tenant_id: i64,

    #[facet(dibs::default = "now()", dibs::index = "idx_users_created")]
    created_at: i64,
}

#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "posts")]
#[facet(dibs::composite_index(
    name = "idx_posts_tenant_published",
    columns = "tenant_id,published"
))]
struct Post {
    #[facet(dibs::pk)]
    id: i64,

    #[facet(dibs::index)]
    title: String,

    body: String,

    published: bool,

    #[facet(dibs::fk = "users.id", dibs::index)]
    author_id: i64,

    #[facet(dibs::fk = "tenants.id", dibs::index)]
    tenant_id: i64,

    #[facet(dibs::default = "now()")]
    created_at: i64,

    updated_at: Option<i64>,
}

#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "comments")]
struct Comment {
    #[facet(dibs::pk)]
    id: i64,

    body: String,

    #[facet(dibs::fk = "posts.id", dibs::index)]
    post_id: i64,

    #[facet(dibs::fk = "users.id", dibs::index)]
    author_id: i64,

    #[facet(dibs::default = "now()")]
    created_at: i64,
}

#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "tags")]
struct Tag {
    #[facet(dibs::pk)]
    id: i64,

    #[facet(dibs::unique)]
    name: String,

    #[facet(dibs::fk = "tenants.id", dibs::index)]
    tenant_id: i64,
}

#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "post_tags")]
#[facet(dibs::composite_index(name = "idx_post_tags_unique", columns = "post_id,tag_id"))]
struct PostTag {
    #[facet(dibs::pk)]
    id: i64,

    #[facet(dibs::fk = "posts.id", dibs::index)]
    post_id: i64,

    #[facet(dibs::fk = "tags.id", dibs::index)]
    tag_id: i64,
}

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
    Migrate {
        /// Database connection URL
        #[facet(default, args::named)]
        database_url: Option<String>,
    },
    /// Show migration status
    Status {
        /// Database connection URL
        #[facet(default, args::named)]
        database_url: Option<String>,
    },
    /// Compare schema to database
    Diff {
        /// Database connection URL
        #[facet(default, args::named)]
        database_url: Option<String>,
    },
    /// Generate a migration skeleton
    Generate {
        /// Migration name (e.g., "add-users-table")
        #[facet(args::positional)]
        name: String,
    },
    /// Browse the current schema
    Schema {
        /// Database connection URL (not yet used)
        #[facet(default, args::named)]
        #[allow(dead_code)]
        database_url: Option<String>,

        /// Output as plain text (default when not a TTY)
        #[facet(default, args::named)]
        plain: bool,

        /// Output as SQL (CREATE TABLE statements)
        #[facet(default, args::named)]
        sql: bool,
    },
}

fn main() {
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
        Some(Commands::Migrate { database_url }) => {
            println!("dibs migrate");
            if let Some(url) = database_url {
                println!("  database: {}", mask_password(&url));
            }
            println!("  (not yet implemented)");
        }
        Some(Commands::Status { database_url }) => {
            println!("dibs status");
            if let Some(url) = database_url {
                println!("  database: {}", mask_password(&url));
            }
            println!("  (not yet implemented)");
        }
        Some(Commands::Diff { database_url }) => {
            println!("dibs diff");
            if let Some(url) = database_url {
                println!("  database: {}", mask_password(&url));
            }
            println!("  (not yet implemented)");
        }
        Some(Commands::Generate { name }) => {
            let date = Zoned::now().date();
            println!("dibs generate {}", name);
            println!("  Would create: migrations/{}-{}.rs", date, name);
            println!("  (not yet implemented)");
        }
        Some(Commands::Schema {
            database_url: _,
            plain,
            sql,
        }) => {
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
            let config = args::HelpConfig {
                program_name: Some("dibs".to_string()),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                ..Default::default()
            };
            print!("{}", args::generate_help::<Cli>(&config));
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
                // Jump to FK target if on a FK row
                if let Some(table) = self.schema.tables.get(self.selected_table) {
                    let col_count = table.columns.len();
                    // Detail items: columns first, then FKs
                    if self.detail_selection >= col_count {
                        let fk_idx = self.detail_selection - col_count;
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
            table.columns.len() + table.foreign_keys.len() + table.indices.len()
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
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Table: ", Style::default().fg(Color::Gray)),
                    Span::styled(&table.name, Style::default().fg(Color::Cyan).bold()),
                ]),
                Line::from(""),
                Line::from(Span::styled(
                    "Columns:",
                    Style::default().fg(Color::Yellow).bold(),
                )),
            ];

            for (i, col) in table.columns.iter().enumerate() {
                let is_selected = self.focus == Focus::Details && self.detail_selection == i;
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
                    let is_selected =
                        self.focus == Focus::Details && self.detail_selection == col_count + i;
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
                        && self.detail_selection == col_count + fk_count + i;
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
