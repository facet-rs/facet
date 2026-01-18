//! Unified TUI for dibs - shows schema, diff, and migrations in one interface.

use std::io::{self, stdout};

use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use dibs_proto::{DiffRequest, DiffResult, MigrationInfo, MigrationStatusRequest, SchemaInfo};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs},
};

use crate::config::Config;
use crate::service::{self, ServiceConnection};

/// The main unified TUI application
pub struct App {
    /// Current tab
    tab: Tab,
    /// Service connection (if available)
    conn: Option<ServiceConnection>,
    /// Database URL
    database_url: Option<String>,
    /// Schema info (fetched from service)
    schema: Option<SchemaInfo>,
    /// Diff result (fetched on demand)
    diff: Option<DiffResult>,
    /// Migration status (fetched on demand)
    migrations: Option<Vec<MigrationInfo>>,
    /// Loading state
    loading: Option<String>,
    /// Error message
    error: Option<String>,
    /// Selected table index (for schema tab)
    table_state: ListState,
    selected_table: usize,
    /// Selected migration index (for migrations tab)
    migration_state: ListState,
    selected_migration: usize,
    /// Pending 'g' for gg
    pending_g: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Schema,
    Diff,
    Migrations,
}

impl Tab {
    fn all() -> &'static [Tab] {
        &[Tab::Schema, Tab::Diff, Tab::Migrations]
    }

    fn index(self) -> usize {
        match self {
            Tab::Schema => 0,
            Tab::Diff => 1,
            Tab::Migrations => 2,
        }
    }

    fn from_index(i: usize) -> Self {
        match i {
            0 => Tab::Schema,
            1 => Tab::Diff,
            _ => Tab::Migrations,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Tab::Schema => "Schema",
            Tab::Diff => "Diff",
            Tab::Migrations => "Migrations",
        }
    }
}

impl App {
    pub fn new() -> Self {
        let mut table_state = ListState::default();
        table_state.select(Some(0));
        let mut migration_state = ListState::default();
        migration_state.select(Some(0));

        Self {
            tab: Tab::Schema,
            conn: None,
            database_url: std::env::var("DATABASE_URL").ok(),
            schema: None,
            diff: None,
            migrations: None,
            loading: Some("Connecting...".to_string()),
            error: None,
            table_state,
            selected_table: 0,
            migration_state,
            selected_migration: 0,
            pending_g: false,
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

        // Connect to service and fetch initial data
        rt.block_on(self.initialize(config));

        // Main loop
        let result = self.main_loop(&mut terminal, &rt);

        // Restore terminal
        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;

        result
    }

    async fn initialize(&mut self, config: Option<&Config>) {
        self.loading = Some("Connecting to service...".to_string());

        if let Some(cfg) = config {
            match service::connect_to_service(cfg).await {
                Ok(conn) => {
                    self.conn = Some(conn);
                    self.loading = Some("Fetching schema...".to_string());

                    // Fetch schema
                    if let Some(conn) = &self.conn {
                        match conn.client().schema().await {
                            Ok(schema) => self.schema = Some(schema),
                            Err(e) => self.error = Some(format!("Schema fetch: {:?}", e)),
                        }
                    }

                    // Fetch migrations if we have a database URL
                    if let (Some(conn), Some(url)) = (&self.conn, &self.database_url) {
                        self.loading = Some("Fetching migration status...".to_string());
                        match conn
                            .client()
                            .migration_status(MigrationStatusRequest {
                                database_url: url.clone(),
                            })
                            .await
                        {
                            Ok(migrations) => self.migrations = Some(migrations),
                            Err(e) => self.error = Some(format!("Migration status: {:?}", e)),
                        }
                    }

                    self.loading = None;
                }
                Err(e) => {
                    self.error = Some(format!("Failed to connect: {}", e));
                    self.loading = None;
                }
            }
        } else {
            self.error = Some("No dibs.styx config found".to_string());
            self.loading = None;
        }
    }

    fn main_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        rt: &tokio::runtime::Runtime,
    ) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.ui(frame))?;

            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                // Handle 'g' prefix for gg
                if self.pending_g {
                    self.pending_g = false;
                    if key.code == KeyCode::Char('g') {
                        self.go_to_first();
                        continue;
                    }
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    // Tab switching
                    KeyCode::Char('1') => self.tab = Tab::Schema,
                    KeyCode::Char('2') => {
                        self.tab = Tab::Diff;
                        // Fetch diff if not already fetched
                        if self.diff.is_none() && self.conn.is_some() {
                            rt.block_on(self.fetch_diff());
                        }
                    }
                    KeyCode::Char('3') => self.tab = Tab::Migrations,
                    KeyCode::Tab => self.next_tab(),
                    KeyCode::BackTab => self.prev_tab(),
                    // Navigation
                    KeyCode::Up | KeyCode::Char('k') => self.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => self.move_down(),
                    KeyCode::Char('g') => self.pending_g = true,
                    KeyCode::Char('G') => self.go_to_last(),
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.half_page_down()
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.half_page_up()
                    }
                    // Actions
                    KeyCode::Char('m') => {
                        // Run migrations
                        if self.tab == Tab::Migrations {
                            rt.block_on(self.run_migrations());
                        }
                    }
                    KeyCode::Char('r') => {
                        // Refresh
                        rt.block_on(self.refresh());
                    }
                    _ => {}
                }
            }
        }
    }

    async fn fetch_diff(&mut self) {
        if let (Some(conn), Some(url)) = (&self.conn, &self.database_url) {
            self.loading = Some("Computing diff...".to_string());
            match conn
                .client()
                .diff(DiffRequest {
                    database_url: url.clone(),
                })
                .await
            {
                Ok(diff) => {
                    self.diff = Some(diff);
                    self.error = None;
                }
                Err(e) => {
                    self.error = Some(format!("Diff failed: {:?}", e));
                }
            }
            self.loading = None;
        }
    }

    async fn run_migrations(&mut self) {
        if let (Some(conn), Some(url)) = (&self.conn, &self.database_url) {
            use dibs_proto::MigrateRequest;

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
                    // Refresh migrations list
                    self.refresh_migrations().await;
                    // Clear diff cache
                    self.diff = None;
                }
                Err(e) => {
                    self.error = Some(format!("Migration failed: {:?}", e));
                }
            }
            self.loading = None;
        }
    }

    async fn refresh(&mut self) {
        self.error = None;
        if let Some(conn) = &self.conn {
            self.loading = Some("Refreshing...".to_string());

            // Refresh schema
            match conn.client().schema().await {
                Ok(schema) => self.schema = Some(schema),
                Err(e) => self.error = Some(format!("Schema fetch: {:?}", e)),
            }

            // Refresh migrations
            self.refresh_migrations().await;

            // Clear cached diff
            self.diff = None;

            self.loading = None;
        }
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
                Ok(migrations) => self.migrations = Some(migrations),
                Err(e) => self.error = Some(format!("Migration status: {:?}", e)),
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
            Tab::Schema => {
                if self.schema.is_some() && self.selected_table > 0 {
                    self.selected_table -= 1;
                    self.table_state.select(Some(self.selected_table));
                }
            }
            Tab::Migrations => {
                if self.selected_migration > 0 {
                    self.selected_migration -= 1;
                    self.migration_state.select(Some(self.selected_migration));
                }
            }
            Tab::Diff => {}
        }
    }

    fn move_down(&mut self) {
        match self.tab {
            Tab::Schema => {
                if let Some(schema) = &self.schema {
                    if self.selected_table < schema.tables.len().saturating_sub(1) {
                        self.selected_table += 1;
                        self.table_state.select(Some(self.selected_table));
                    }
                }
            }
            Tab::Migrations => {
                if let Some(migrations) = &self.migrations {
                    if self.selected_migration < migrations.len().saturating_sub(1) {
                        self.selected_migration += 1;
                        self.migration_state.select(Some(self.selected_migration));
                    }
                }
            }
            Tab::Diff => {}
        }
    }

    fn go_to_first(&mut self) {
        match self.tab {
            Tab::Schema => {
                self.selected_table = 0;
                self.table_state.select(Some(0));
            }
            Tab::Migrations => {
                self.selected_migration = 0;
                self.migration_state.select(Some(0));
            }
            Tab::Diff => {}
        }
    }

    fn go_to_last(&mut self) {
        match self.tab {
            Tab::Schema => {
                if let Some(schema) = &self.schema {
                    self.selected_table = schema.tables.len().saturating_sub(1);
                    self.table_state.select(Some(self.selected_table));
                }
            }
            Tab::Migrations => {
                if let Some(migrations) = &self.migrations {
                    self.selected_migration = migrations.len().saturating_sub(1);
                    self.migration_state.select(Some(self.selected_migration));
                }
            }
            Tab::Diff => {}
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

    fn ui(&mut self, frame: &mut Frame) {
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

        // Tabs
        let tab_titles: Vec<Line> = Tab::all()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let num = format!("{}", i + 1);
                Line::from(vec![
                    Span::styled(num, Style::default().fg(Color::Yellow)),
                    Span::raw(":"),
                    Span::raw(t.name()),
                ])
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
            Tab::Schema => self.render_schema(frame, chunks[1]),
            Tab::Diff => self.render_diff(frame, chunks[1]),
            Tab::Migrations => self.render_migrations(frame, chunks[1]),
        }

        // Status bar
        let status = self.build_status_bar();
        frame.render_widget(status, chunks[2]);
    }

    fn render_schema(&mut self, frame: &mut Frame, area: Rect) {
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

            for col in &table.columns {
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(&col.name, Style::default().fg(Color::White)),
                    Span::raw(": "),
                    Span::styled(&col.sql_type, Style::default().fg(Color::Blue)),
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
                        Span::styled(" → ", Style::default().fg(Color::Gray)),
                        Span::styled(&fk.references_table, Style::default().fg(Color::Cyan)),
                        Span::raw("."),
                        Span::raw(fk.references_columns.join(", ")),
                    ]));
                }
            }

            let details = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Details ")
                    .title_style(Style::default().fg(Color::Cyan)),
            );

            frame.render_widget(details, chunks[1]);
        }
    }

    fn render_diff(&self, frame: &mut Frame, area: Rect) {
        if let Some(loading) = &self.loading {
            let p = Paragraph::new(loading.as_str())
                .block(Block::default().borders(Borders::ALL).title(" Diff "));
            frame.render_widget(p, area);
            return;
        }

        let Some(diff) = &self.diff else {
            let msg = if self.database_url.is_none() {
                "No DATABASE_URL set. Set it in .env or environment."
            } else {
                "Press '2' to load diff..."
            };
            let p =
                Paragraph::new(msg).block(Block::default().borders(Borders::ALL).title(" Diff "));
            frame.render_widget(p, area);
            return;
        };

        let mut lines = vec![];

        if diff.table_diffs.is_empty() {
            lines.push(Line::from(Span::styled(
                "✓ No changes - schema matches database",
                Style::default().fg(Color::Green),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                format!(
                    "Changes detected ({} tables affected):",
                    diff.table_diffs.len()
                ),
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(""));

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
                .title(" Diff (Schema vs Database) ")
                .title_style(Style::default().fg(Color::Cyan)),
        );
        frame.render_widget(p, area);
    }

    fn render_migrations(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(loading) = &self.loading {
            let p = Paragraph::new(loading.as_str())
                .block(Block::default().borders(Borders::ALL).title(" Migrations "));
            frame.render_widget(p, area);
            return;
        }

        let Some(migrations) = &self.migrations else {
            let msg = if self.database_url.is_none() {
                "No DATABASE_URL set. Set it in .env or environment."
            } else {
                "No migration info available"
            };
            let p = Paragraph::new(msg)
                .block(Block::default().borders(Borders::ALL).title(" Migrations "));
            frame.render_widget(p, area);
            return;
        };

        if migrations.is_empty() {
            let p = Paragraph::new("No migrations registered.")
                .block(Block::default().borders(Borders::ALL).title(" Migrations "));
            frame.render_widget(p, area);
            return;
        }

        let items: Vec<ListItem> = migrations
            .iter()
            .map(|m| {
                let status = if m.applied { "✓" } else { "○" };
                let style = if m.applied {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(status, style),
                    Span::raw(" "),
                    Span::raw(&m.version),
                    Span::raw(" - "),
                    Span::raw(&m.name),
                ]))
            })
            .collect();

        let applied = migrations.iter().filter(|m| m.applied).count();
        let pending = migrations.len() - applied;

        let title = format!(" Migrations ({} applied, {} pending) ", applied, pending);

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

    fn build_status_bar(&self) -> Paragraph<'static> {
        let mut spans = vec![
            Span::styled(" Tab ", Style::default().fg(Color::Yellow)),
            Span::raw("switch  "),
            Span::styled("j/k ", Style::default().fg(Color::Yellow)),
            Span::raw("nav  "),
            Span::styled("r ", Style::default().fg(Color::Yellow)),
            Span::raw("refresh  "),
        ];

        if self.tab == Tab::Migrations {
            spans.push(Span::styled("m ", Style::default().fg(Color::Yellow)));
            spans.push(Span::raw("migrate  "));
        }

        spans.push(Span::styled("q ", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw("quit"));

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
    if let Some(at) = url.find('@') {
        if let Some(colon) = url[..at].rfind(':') {
            if let Some(slash) = url[..colon].rfind('/') {
                let prefix = &url[..slash + 1];
                let user_start = slash + 1;
                if let Some(user_colon) = url[user_start..colon].find(':') {
                    let user = &url[user_start..user_start + user_colon];
                    let suffix = &url[at..];
                    return format!("{}{}:***{}", prefix, user, suffix);
                }
            }
        }
    }
    url.to_string()
}
