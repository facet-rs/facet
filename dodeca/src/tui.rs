//! TUI for dodeca build progress using ratatui
//!
//! Shows live progress for parallel build tasks with a clean terminal UI.

use color_eyre::Result;
use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
};
use std::collections::VecDeque;
use std::io::stdout;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use tokio::sync::watch;

/// Progress state for a single task
#[derive(Debug, Clone)]
pub struct TaskProgress {
    pub name: &'static str,
    pub total: usize,
    pub completed: usize,
    pub status: TaskStatus,
    pub message: Option<String>,
}

/// Status of a build task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaskStatus {
    #[default]
    Pending,
    Running,
    Done,
    Error,
}

impl TaskProgress {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            total: 0,
            completed: 0,
            status: TaskStatus::Pending,
            message: None,
        }
    }

    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.completed as f64 / self.total as f64
        }
    }

    pub fn start(&mut self, total: usize) {
        self.total = total;
        self.completed = 0;
        self.status = TaskStatus::Running;
    }

    pub fn advance(&mut self) {
        self.completed = (self.completed + 1).min(self.total);
    }

    pub fn finish(&mut self) {
        self.completed = self.total;
        self.status = TaskStatus::Done;
    }

    pub fn fail(&mut self, msg: impl Into<String>) {
        self.status = TaskStatus::Error;
        self.message = Some(msg.into());
    }
}

/// All build progress state
#[derive(Debug, Clone)]
pub struct BuildProgress {
    pub parse: TaskProgress,
    pub render: TaskProgress,
    pub sass: TaskProgress,
    pub links: TaskProgress,
    pub search: TaskProgress,
}

impl Default for BuildProgress {
    fn default() -> Self {
        Self {
            parse: TaskProgress::new("Parsing"),
            render: TaskProgress::new("Rendering"),
            sass: TaskProgress::new("Sass"),
            links: TaskProgress::new("Links"),
            search: TaskProgress::new("Search"),
        }
    }
}

/// Shared progress state for use across threads (legacy, for build mode)
pub type SharedProgress = Arc<Mutex<BuildProgress>>;

/// Create a new shared progress state (legacy, for build mode)
pub fn new_shared_progress() -> SharedProgress {
    Arc::new(Mutex::new(BuildProgress::default()))
}

// ============================================================================
// Channel-based types for serve mode (cleaner than locks)
// ============================================================================

/// Progress sender - producers call send_modify to update progress
pub type ProgressTx = watch::Sender<BuildProgress>;
/// Progress receiver - TUI reads latest progress
pub type ProgressRx = watch::Receiver<BuildProgress>;

/// Create a new progress channel
pub fn progress_channel() -> (ProgressTx, ProgressRx) {
    watch::channel(BuildProgress::default())
}

/// Server status sender
pub type ServerStatusTx = watch::Sender<ServerStatus>;
/// Server status receiver
pub type ServerStatusRx = watch::Receiver<ServerStatus>;

/// Create a new server status channel
pub fn server_status_channel() -> (ServerStatusTx, ServerStatusRx) {
    watch::channel(ServerStatus::default())
}

/// Event sender - multiple producers can clone and send
pub type EventTx = mpsc::Sender<String>;
/// Event receiver - TUI drains events
pub type EventRx = mpsc::Receiver<String>;

/// Create a new event channel
pub fn event_channel() -> (EventTx, EventRx) {
    mpsc::channel()
}

/// Helper to update progress - works with either SharedProgress or ProgressTx
pub enum ProgressReporter {
    /// Legacy mutex-based (for build command)
    Shared(SharedProgress),
    /// Channel-based (for serve mode)
    Channel(ProgressTx),
}

impl ProgressReporter {
    /// Update progress with a closure
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut BuildProgress),
    {
        match self {
            ProgressReporter::Shared(p) => {
                let mut prog = p.lock().unwrap();
                f(&mut prog);
            }
            ProgressReporter::Channel(tx) => {
                tx.send_modify(f);
            }
        }
    }
}

/// TUI application state
pub struct App {
    progress: SharedProgress,
    should_quit: bool,
}

impl App {
    pub fn new(progress: SharedProgress) -> Self {
        Self {
            progress,
            should_quit: false,
        }
    }

    /// Run the TUI event loop
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.draw(frame))?;

            // Poll for events with timeout (allows progress updates)
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                            _ => {}
                        }
                    }
                }
            }

            // Check if all tasks are done
            let progress = self.progress.lock().unwrap();
            let all_done = [
                &progress.parse,
                &progress.render,
                &progress.sass,
                &progress.links,
                &progress.search,
            ]
            .iter()
            .all(|t| matches!(t.status, TaskStatus::Done | TaskStatus::Error));

            if all_done {
                drop(progress);
                // Show final state briefly
                terminal.draw(|frame| self.draw(frame))?;
                std::thread::sleep(Duration::from_millis(500));
                self.should_quit = true;
            }
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let progress = self.progress.lock().unwrap();

        let area = frame.area();

        // Main layout
        let chunks = Layout::vertical([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Tasks
            Constraint::Length(2), // Footer
        ])
        .split(area);

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled("dodeca", Style::default().fg(Color::Cyan).bold()),
            Span::raw(" — building site"),
        ]))
        .block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(header, chunks[0]);

        // Task progress bars
        let tasks = [
            &progress.parse,
            &progress.render,
            &progress.sass,
            &progress.links,
            &progress.search,
        ];

        let task_chunks = Layout::vertical(
            tasks
                .iter()
                .map(|_| Constraint::Length(3))
                .collect::<Vec<_>>(),
        )
        .split(chunks[1]);

        for (task, chunk) in tasks.iter().zip(task_chunks.iter()) {
            let gauge = render_task_gauge(task);
            frame.render_widget(gauge, *chunk);
        }

        // Footer
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(" quit"),
        ]))
        .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(footer, chunks[2]);
    }
}

fn render_task_gauge(task: &TaskProgress) -> Gauge<'static> {
    let (color, symbol) = match task.status {
        TaskStatus::Pending => (Color::DarkGray, "○"),
        TaskStatus::Running => (Color::Cyan, "◐"),
        TaskStatus::Done => (Color::Green, "✓"),
        TaskStatus::Error => (Color::Red, "✗"),
    };

    let label = if task.total > 0 {
        format!("{} {} {}/{}", symbol, task.name, task.completed, task.total)
    } else {
        format!("{} {}", symbol, task.name)
    };

    Gauge::default()
        .block(Block::default())
        .gauge_style(Style::default().fg(color))
        .ratio(task.ratio())
        .label(label)
}

/// Initialize terminal for TUI
pub fn init_terminal() -> Result<DefaultTerminal> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let terminal = ratatui::init();
    Ok(terminal)
}

/// Restore terminal to normal state
pub fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    ratatui::restore();
    Ok(())
}

/// Run build with TUI progress display
pub async fn run_with_tui<F, Fut>(build_fn: F) -> Result<()>
where
    F: FnOnce(SharedProgress) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<()>> + Send,
{
    let progress = new_shared_progress();

    // Spawn build task
    let build_progress = progress.clone();
    let build_handle = tokio::spawn(async move { build_fn(build_progress).await });

    // Run TUI on main thread
    let mut terminal = init_terminal()?;
    let mut app = App::new(progress);

    let result = app.run(&mut terminal);

    restore_terminal()?;

    // Wait for build to complete
    build_handle.await??;

    result
}

/// Check if an IPv4 address is a private/LAN address
pub fn is_private_ip(ip: std::net::Ipv4Addr) -> bool {
    // RFC 1918 private ranges + localhost
    ip.is_loopback()           // 127.0.0.0/8
        || ip.is_private()     // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
        || ip.is_link_local() // 169.254.0.0/16
}

/// Get all LAN (private) IPv4 addresses from network interfaces
pub fn get_lan_ips() -> Vec<std::net::Ipv4Addr> {
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        interfaces
            .into_iter()
            .filter_map(|iface| {
                if let if_addrs::IfAddr::V4(addr) = iface.addr {
                    let ip = addr.ip;
                    // Include private IPs but not loopback
                    if ip.is_private() || ip.is_link_local() {
                        Some(ip)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    } else {
        vec![]
    }
}

/// Server binding mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BindMode {
    /// Local only (127.0.0.1) - shown with 💻
    #[default]
    Local,
    /// LAN interfaces (private IPs) - shown with 🏠
    Lan,
}

/// Server status for serve mode TUI
#[derive(Debug, Clone, Default)]
pub struct ServerStatus {
    pub urls: Vec<String>,
    pub is_running: bool,
    pub bind_mode: BindMode,
}

/// Command sent from TUI to server
#[derive(Debug, Clone)]
pub enum ServerCommand {
    /// Switch to LAN mode (bind to 0.0.0.0)
    GoPublic,
    /// Switch to local mode (bind to 127.0.0.1)
    GoLocal,
}

/// Serve mode TUI application state (channel-based)
pub struct ServeApp {
    progress_rx: ProgressRx,
    server_rx: ServerStatusRx,
    event_rx: EventRx,
    /// Local buffer for events (since mpsc drains)
    event_buffer: VecDeque<String>,
    command_tx: mpsc::Sender<ServerCommand>,
    show_help: bool,
    should_quit: bool,
}

/// Maximum number of events to keep in the buffer
const MAX_EVENTS: usize = 100;

impl ServeApp {
    pub fn new(
        progress_rx: ProgressRx,
        server_rx: ServerStatusRx,
        event_rx: EventRx,
        command_tx: mpsc::Sender<ServerCommand>,
    ) -> Self {
        Self {
            progress_rx,
            server_rx,
            event_rx,
            event_buffer: VecDeque::with_capacity(MAX_EVENTS),
            command_tx,
            show_help: false,
            should_quit: false,
        }
    }

    /// Run the serve TUI event loop
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            // Drain any new events into the buffer
            self.drain_events();

            terminal.draw(|frame| self.draw(frame))?;

            // Poll for events with timeout
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('c')
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                self.should_quit = true
                            }
                            KeyCode::Char('q') | KeyCode::Esc => {
                                if self.show_help {
                                    self.show_help = false;
                                } else {
                                    self.should_quit = true;
                                }
                            }
                            KeyCode::Char('?') => self.show_help = !self.show_help,
                            KeyCode::Char('p') => {
                                let current_mode = self.server_rx.borrow().bind_mode;
                                let cmd = match current_mode {
                                    BindMode::Local => ServerCommand::GoPublic,
                                    BindMode::Lan => ServerCommand::GoLocal,
                                };
                                let _ = self.command_tx.send(cmd);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Drain events from the channel into the local buffer
    fn drain_events(&mut self) {
        // Non-blocking drain of all available events
        while let Ok(event) = self.event_rx.try_recv() {
            self.event_buffer.push_back(event);
            // Keep buffer bounded
            if self.event_buffer.len() > MAX_EVENTS {
                self.event_buffer.pop_front();
            }
        }
    }

    fn draw(&self, frame: &mut Frame) {
        // Read from channels (no locks!)
        let progress = self.progress_rx.borrow();
        let server = self.server_rx.borrow();

        let area = frame.area();

        // Main layout
        let url_height = server.urls.len().max(1) as u16 + 2;
        let chunks = Layout::vertical([
            Constraint::Length(url_height), // Server URLs
            Constraint::Length(5),          // Build progress
            Constraint::Min(3),             // Events log
            Constraint::Length(1),          // Footer
        ])
        .split(area);

        // Server block with network icon and status in title
        let (network_icon, icon_color) = match server.bind_mode {
            BindMode::Local => ("💻", Color::Green),
            BindMode::Lan => ("🏠", Color::Yellow),
        };
        let status = if server.is_running {
            ("●", Color::Green)
        } else {
            ("○", Color::Yellow)
        };

        let url_lines: Vec<Line> = server
            .urls
            .iter()
            .map(|url| {
                Line::from(vec![
                    Span::styled("  → ", Style::default().fg(Color::Cyan)),
                    Span::styled(url.clone(), Style::default().fg(Color::Blue)),
                ])
            })
            .collect();

        let server_title = Line::from(vec![
            Span::raw(" Server "),
            Span::styled(network_icon, Style::default().fg(icon_color)),
            Span::raw(" "),
            Span::styled(status.0, Style::default().fg(status.1)),
        ]);
        let urls_widget = Paragraph::new(url_lines)
            .block(Block::default().title(server_title).borders(Borders::ALL));
        frame.render_widget(urls_widget, chunks[0]);

        // Build progress (compact version)
        let tasks = [
            (&progress.parse, "Parsed"),
            (&progress.render, "Rendered"),
            (&progress.sass, "Compiled"),
        ];
        let task_lines: Vec<Line> = tasks
            .iter()
            .map(|(task, done_name)| {
                let (color, symbol, name) = match task.status {
                    TaskStatus::Pending => (Color::DarkGray, "○", task.name),
                    TaskStatus::Running => (Color::Cyan, "◐", task.name),
                    TaskStatus::Done => (Color::Green, "✓", *done_name),
                    TaskStatus::Error => (Color::Red, "✗", task.name),
                };
                let label = match task.status {
                    TaskStatus::Done => format!("{} {}", symbol, name),
                    _ if task.total > 0 => {
                        format!("{} {} {}/{}", symbol, name, task.completed, task.total)
                    }
                    _ => format!("{} {}", symbol, name),
                };
                Line::from(Span::styled(label, Style::default().fg(color)))
            })
            .collect();
        let progress_widget =
            Paragraph::new(task_lines).block(Block::default().title("Build").borders(Borders::ALL));
        frame.render_widget(progress_widget, chunks[1]);

        // Events log (from local buffer)
        let max_events = (chunks[2].height.saturating_sub(2)) as usize;
        let recent_events: Vec<Line> = self
            .event_buffer
            .iter()
            .rev()
            .take(max_events)
            .rev()
            .map(|e| {
                Line::from(Span::styled(
                    e.clone(),
                    Style::default().fg(Color::DarkGray),
                ))
            })
            .collect();
        let events_widget = Paragraph::new(recent_events)
            .block(Block::default().title("Activity").borders(Borders::ALL));
        frame.render_widget(events_widget, chunks[2]);

        // Footer
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("?", Style::default().fg(Color::Yellow)),
            Span::raw(" help  "),
            Span::styled("p", Style::default().fg(Color::Yellow)),
            Span::raw(" public  "),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(" quit"),
        ]))
        .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(footer, chunks[3]);

        // Help overlay
        if self.show_help {
            self.draw_help_overlay(frame, area);
        }
    }

    fn draw_help_overlay(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        use ratatui::widgets::Clear;

        // Center the help panel
        let help_width = 40u16;
        let help_height = 10u16;
        let x = area.width.saturating_sub(help_width) / 2;
        let y = area.height.saturating_sub(help_height) / 2;
        let help_area = ratatui::layout::Rect::new(
            x,
            y,
            help_width.min(area.width),
            help_height.min(area.height),
        );

        // Clear the area behind the popup
        frame.render_widget(Clear, help_area);

        let help_text = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ?", Style::default().fg(Color::Yellow)),
                Span::raw("      Toggle this help"),
            ]),
            Line::from(vec![
                Span::styled("  p", Style::default().fg(Color::Yellow)),
                Span::raw("      Toggle public/local mode"),
            ]),
            Line::from(vec![
                Span::styled("  q", Style::default().fg(Color::Yellow)),
                Span::raw("      Quit / close panel"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+C", Style::default().fg(Color::Yellow)),
                Span::raw(" Force quit"),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  💻", Style::default().fg(Color::Green)),
                Span::raw(" = localhost only"),
            ]),
            Line::from(vec![
                Span::styled("  🏠", Style::default().fg(Color::Yellow)),
                Span::raw(" = LAN (home network)"),
            ]),
        ];

        let help_widget = Paragraph::new(help_text)
            .block(
                Block::default()
                    .title(" Help ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .style(Style::default().bg(Color::Black));

        frame.render_widget(help_widget, help_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_progress_ratio() {
        let mut task = TaskProgress::new("Test");
        assert_eq!(task.ratio(), 0.0);

        task.start(10);
        assert_eq!(task.ratio(), 0.0);

        task.advance();
        task.advance();
        assert!((task.ratio() - 0.2).abs() < f64::EPSILON);

        task.finish();
        assert_eq!(task.ratio(), 1.0);
    }
}
