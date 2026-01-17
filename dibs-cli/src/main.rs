use std::io::{self, IsTerminal, stdout};

use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use facet::Facet;
use facet_args as args;
use jiff::Zoned;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

// Example table definition for testing
#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "users")]
struct User {
    #[facet(dibs::pk)]
    id: i64,

    #[facet(dibs::unique)]
    email: String,

    name: String,

    bio: Option<String>,

    #[facet(dibs::fk = "tenants.id")]
    tenant_id: i64,
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
        /// Database connection URL
        #[facet(default, args::named)]
        database_url: Option<String>,

        /// Output as plain text (default when not a TTY)
        #[facet(default, args::named)]
        plain: bool,
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
        }) => {
            let schema = dibs::Schema::collect();

            if schema.tables.is_empty() {
                println!("No tables registered.");
                println!();
                println!("Define tables using #[facet(dibs::table = \"name\")] on Facet structs.");
                return;
            }

            // Use TUI if stdout is a TTY and --plain wasn't specified
            if stdout().is_terminal() && !plain {
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
}

impl<'a> SchemaApp<'a> {
    fn new(schema: &'a dibs::Schema) -> Self {
        let mut table_state = ListState::default();
        table_state.select(Some(0));
        Self {
            schema,
            table_state,
            selected_table: 0,
        }
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.ui(frame))?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Up | KeyCode::Char('k') => self.previous_table(),
                        KeyCode::Down | KeyCode::Char('j') => self.next_table(),
                        _ => {}
                    }
                }
            }
        }
    }

    fn next_table(&mut self) {
        if self.schema.tables.is_empty() {
            return;
        }
        self.selected_table = (self.selected_table + 1) % self.schema.tables.len();
        self.table_state.select(Some(self.selected_table));
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
    }

    fn ui(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(frame.area());

        // Left pane: table list
        let table_items: Vec<ListItem> = self
            .schema
            .tables
            .iter()
            .map(|t| ListItem::new(format!("{} ({})", t.name, t.columns.len())))
            .collect();

        let tables_list = List::new(table_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Tables ")
                    .title_style(Style::default().fg(Color::Cyan).bold()),
            )
            .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White).bold())
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(tables_list, chunks[0], &mut self.table_state);

        // Right pane: selected table details
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

            for col in &table.columns {
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(&col.name, Style::default().fg(Color::White)),
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

                lines.push(Line::from(spans));
            }

            if !table.foreign_keys.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Foreign Keys:",
                    Style::default().fg(Color::Green).bold(),
                )));

                for fk in &table.foreign_keys {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(fk.columns.join(", "), Style::default().fg(Color::White)),
                        Span::styled(" -> ", Style::default().fg(Color::Gray)),
                        Span::styled(&fk.references_table, Style::default().fg(Color::Cyan)),
                        Span::raw("."),
                        Span::styled(
                            fk.references_columns.join(", "),
                            Style::default().fg(Color::White),
                        ),
                    ]));
                }
            }

            let details = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Details ")
                    .title_style(Style::default().fg(Color::Cyan).bold()),
            );

            frame.render_widget(details, chunks[1]);
        }

        // Help bar at the bottom
        let help = Paragraph::new(Line::from(vec![
            Span::styled(" j/↓ ", Style::default().fg(Color::Yellow)),
            Span::raw("down  "),
            Span::styled(" k/↑ ", Style::default().fg(Color::Yellow)),
            Span::raw("up  "),
            Span::styled(" q/Esc ", Style::default().fg(Color::Yellow)),
            Span::raw("quit"),
        ]))
        .style(Style::default().bg(Color::DarkGray));

        // Create a small area at the bottom for the help bar
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
    if let Some(start) = url.find("://") {
        if let Some(at) = url.find('@') {
            let prefix = &url[..start + 3];
            let suffix = &url[at..];
            if let Some(colon) = url[start + 3..at].find(':') {
                let user = &url[start + 3..start + 3 + colon];
                return format!("{}{}:***{}", prefix, user, suffix);
            }
        }
    }
    url.to_string()
}
