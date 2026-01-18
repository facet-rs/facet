//! Unified TUI for dibs - shows schema, diff, and migrations in one interface.

use std::io::{self, stdout};
use std::time::Duration;

use arborium::Highlighter;
use arborium_theme::builtin;
use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use dibs_proto::{DiffRequest, DiffResult, MigrationInfo, MigrationStatusRequest, SchemaInfo};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs},
};

use crate::config::Config;
use crate::highlight::highlight_to_lines;
use crate::service::{self, BuildOutput, ServiceConnection};

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
            phase: AppPhase::Building,
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
            show_migration_source: false,
            source_scroll: 0,
            highlighter: Highlighter::new(),
            theme: builtin::catppuccin_mocha().clone(),
            build_output: Vec::new(),
            build_scroll: 0,
            build_auto_scroll: true,
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
            self.phase = AppPhase::Failed("No dibs.styx config found".to_string());
            self.main_loop(&mut terminal, &rt)
        };

        // Restore terminal
        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;

        result
    }

    /// Run the TUI with build phase, then main loop.
    fn run_with_build(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        rt: &tokio::runtime::Runtime,
        config: &Config,
    ) -> io::Result<()> {
        // Start the build process
        let mut build_process = match rt.block_on(service::start_service(config)) {
            Ok(bp) => bp,
            Err(e) => {
                self.phase = AppPhase::Failed(format!("Failed to start service: {}", e));
                return self.main_loop(terminal, rt);
            }
        };

        // Build phase loop - show cargo output while waiting for connection
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
            if let Some(status) = rt.block_on(build_process.check_exit()) {
                if !status.success() {
                    self.phase = AppPhase::Failed(format!(
                        "Build failed with exit code: {}",
                        status.code().unwrap_or(-1)
                    ));
                    return self.main_loop(terminal, rt);
                }
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

                    return self.main_loop(terminal, rt);
                }
                Ok(None) => {
                    // No connection yet, continue
                }
                Err(e) => {
                    self.phase = AppPhase::Failed(format!("Connection failed: {}", e));
                    return self.main_loop(terminal, rt);
                }
            }

            // Handle input (with short timeout for responsiveness)
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()?
                    && key.kind == KeyEventKind::Press
                {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
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
    }

    /// Fetch initial data after connection is established.
    async fn fetch_initial_data(&mut self) {
        if let Some(conn) = &self.conn {
            // Fetch schema
            match conn.client().schema().await {
                Ok(schema) => self.schema = Some(schema),
                Err(e) => self.error = Some(format!("Schema fetch: {:?}", e)),
            }

            // Fetch migrations if we have a database URL
            if let Some(url) = &self.database_url {
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
        }
        self.loading = None;
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
            let mut scrollbar_state = ScrollbarState::new(self.build_output.len())
                .position(self.build_scroll);
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
                    KeyCode::Char('1') if !self.show_migration_source => self.tab = Tab::Schema,
                    KeyCode::Char('2') if !self.show_migration_source => {
                        self.tab = Tab::Diff;
                        // Fetch diff if not already fetched
                        if self.diff.is_none() && self.conn.is_some() {
                            rt.block_on(self.fetch_diff());
                        }
                    }
                    KeyCode::Char('3') if !self.show_migration_source => self.tab = Tab::Migrations,
                    KeyCode::Tab if !self.show_migration_source => self.next_tab(),
                    KeyCode::BackTab if !self.show_migration_source => self.prev_tab(),
                    // Navigation
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.show_migration_source {
                            self.source_scroll = self.source_scroll.saturating_sub(1);
                        } else {
                            self.move_up();
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.show_migration_source {
                            self.source_scroll = self.source_scroll.saturating_add(1);
                        } else {
                            self.move_down();
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
                    // Enter to view migration source
                    KeyCode::Enter => {
                        if self.tab == Tab::Migrations && !self.show_migration_source {
                            self.show_migration_source = true;
                            self.source_scroll = 0;
                        } else if self.show_migration_source {
                            self.show_migration_source = false;
                            self.source_scroll = 0;
                        }
                    }
                    // Actions
                    KeyCode::Char('m') if !self.show_migration_source => {
                        // Run migrations
                        if self.tab == Tab::Migrations {
                            rt.block_on(self.run_migrations());
                        }
                    }
                    KeyCode::Char('r') if !self.show_migration_source => {
                        // Refresh
                        rt.block_on(self.refresh());
                    }
                    KeyCode::Char('g') if self.tab == Tab::Diff && !self.show_migration_source => {
                        // Generate migration from diff
                        rt.block_on(self.generate_migration());
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

    async fn generate_migration(&mut self) {
        // Check if there are any changes
        if let Some(diff) = &self.diff {
            if diff.table_diffs.is_empty() {
                self.error = Some("No changes to migrate".to_string());
                return;
            }
        } else {
            self.error = Some("No diff computed yet".to_string());
            return;
        }

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
                    match self.create_migration_file(&sql) {
                        Ok(path) => {
                            self.error = Some(format!("Created: {}", path));
                            // Refresh migrations list
                            self.refresh_migrations().await;
                        }
                        Err(e) => {
                            self.error = Some(format!("Failed to create migration: {}", e));
                        }
                    }
                }
                Err(e) => {
                    self.error = Some(format!("Failed to generate SQL: {:?}", e));
                }
            }
            self.loading = None;
        }
    }

    fn create_migration_file(&self, sql: &str) -> Result<String, std::io::Error> {
        use std::fs;
        use std::io::Write;

        let now = jiff::Zoned::now();
        let timestamp = now.strftime("%Y%m%d%H%M%S");

        // Create migrations directory if it doesn't exist
        let migrations_dir = std::path::Path::new("src/migrations");
        if !migrations_dir.exists() {
            fs::create_dir_all(migrations_dir)?;
        }

        // Generate filename
        let filename = format!("m{}_auto.rs", timestamp);
        let filepath = migrations_dir.join(&filename);

        // Generate version string
        let version = format!("{}-auto", timestamp);

        // Generate Rust migration content
        let content = format!(
            r#"//! Auto-generated migration
//! Created: {}

use dibs::{{MigrationContext, Result}};

#[dibs::migration("{}")]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> Result<()> {{
{}
    Ok(())
}}
"#,
            now.strftime("%Y-%m-%d %H:%M:%S %Z"),
            version,
            sql.lines()
                .filter(|line| !line.is_empty())
                .map(|line| {
                    if line.starts_with("--") {
                        format!("    // {}\n", &line[3..])
                    } else {
                        format!("    ctx.execute(\"{}\").await?;\n", line.replace('"', "\\\""))
                    }
                })
                .collect::<String>()
        );

        let mut file = fs::File::create(&filepath)?;
        file.write_all(content.as_bytes())?;

        Ok(filepath.display().to_string())
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

        // Clone migrations to avoid borrow issues
        let migrations = migrations.clone();

        // If showing source, split the view
        if self.show_migration_source {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .split(area);

            // Render migration list on left
            self.render_migration_list(frame, chunks[0], &migrations);

            // Render source on right
            self.render_migration_source(frame, chunks[1], &migrations);
        } else {
            // Just render the list
            self.render_migration_list(frame, area, &migrations);
        }
    }

    fn render_migration_list(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        migrations: &[MigrationInfo],
    ) {
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

            if self.tab == Tab::Migrations {
                spans.push(Span::styled("Enter ", Style::default().fg(Color::Yellow)));
                spans.push(Span::raw("view  "));
                spans.push(Span::styled("m ", Style::default().fg(Color::Yellow)));
                spans.push(Span::raw("migrate  "));
            }

            if self.tab == Tab::Diff {
                spans.push(Span::styled("g ", Style::default().fg(Color::Yellow)));
                spans.push(Span::raw("generate  "));
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
