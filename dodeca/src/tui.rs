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
use std::io::stdout;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

/// Shared progress state for use across threads
pub type SharedProgress = Arc<Mutex<BuildProgress>>;

/// Create a new shared progress state
pub fn new_shared_progress() -> SharedProgress {
    Arc::new(Mutex::new(BuildProgress::default()))
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
        TaskStatus::Done => (Color::Green, "●"),
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
