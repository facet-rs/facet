//! TUI for exploring facet compile-time metrics

use std::{
    fs,
    io::{self, stdout},
    path::PathBuf,
    process::Command,
};

use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use facet::Facet;
use ratatui::{
    prelude::*,
    widgets::{
        Bar, BarChart, BarGroup, Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState,
    },
};

#[derive(Debug, Default, Facet)]
struct Metrics {
    timestamp: String,
    commit: String,
    branch: String,
    experiment: String,
    compile_secs: f64,
    bin_unstripped: u64,
    bin_stripped: u64,
    llvm_lines: u64,
    llvm_copies: u64,
    type_sizes_total: u64,
    selfprof: SelfProfileMetrics,
}

#[derive(Debug, Default, Facet)]
struct SelfProfileMetrics {
    llvm_module_optimize_ms: u64,
    llvm_module_codegen_ms: u64,
    llvm_lto_optimize_ms: u64,
    llvm_thin_lto_ms: u64,
    typeck_ms: u64,
    mir_borrowck_ms: u64,
    expand_proc_macro_ms: u64,
    eval_to_allocation_raw_ms: u64,
    codegen_module_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Timestamp,
    Commit,
    Branch,
    Experiment,
    CompileSecs,
    BinUnstripped,
    BinStripped,
    LlvmLines,
    LlvmCopies,
    TypeSizesTotal,
    LlvmOptimize,
    LlvmCodegen,
    LlvmLto,
    Typeck,
    MirBorrowck,
    ProcMacro,
    ConstEval,
    Codegen,
}

impl Column {
    fn all() -> &'static [Column] {
        &[
            Column::Timestamp,
            Column::Commit,
            Column::Branch,
            Column::Experiment,
            Column::CompileSecs,
            Column::BinUnstripped,
            Column::BinStripped,
            Column::LlvmLines,
            Column::LlvmCopies,
            Column::TypeSizesTotal,
            Column::LlvmOptimize,
            Column::LlvmCodegen,
            Column::LlvmLto,
            Column::Typeck,
            Column::MirBorrowck,
            Column::ProcMacro,
            Column::ConstEval,
            Column::Codegen,
        ]
    }

    fn numeric_columns() -> &'static [Column] {
        &[
            Column::CompileSecs,
            Column::BinUnstripped,
            Column::BinStripped,
            Column::LlvmLines,
            Column::LlvmCopies,
            Column::TypeSizesTotal,
            Column::LlvmOptimize,
            Column::LlvmCodegen,
            Column::LlvmLto,
            Column::Typeck,
            Column::MirBorrowck,
            Column::ProcMacro,
            Column::ConstEval,
            Column::Codegen,
        ]
    }

    fn name(&self) -> &'static str {
        match self {
            Column::Timestamp => "Time",
            Column::Commit => "Commit",
            Column::Branch => "Branch",
            Column::Experiment => "Experiment",
            Column::CompileSecs => "Compile(s)",
            Column::BinUnstripped => "Bin(KB)",
            Column::BinStripped => "Strip(KB)",
            Column::LlvmLines => "LLVM Lines",
            Column::LlvmCopies => "LLVM Copies",
            Column::TypeSizesTotal => "Types(B)",
            Column::LlvmOptimize => "LLVM Opt(ms)",
            Column::LlvmCodegen => "LLVM CG(ms)",
            Column::LlvmLto => "LLVM LTO(ms)",
            Column::Typeck => "Typeck(ms)",
            Column::MirBorrowck => "Borrowck(ms)",
            Column::ProcMacro => "ProcMacro(ms)",
            Column::ConstEval => "ConstEval(ms)",
            Column::Codegen => "Codegen(ms)",
        }
    }

    fn value(&self, m: &Metrics) -> String {
        match self {
            Column::Timestamp => {
                // Extract just time part HH:MM
                if let Some(t_pos) = m.timestamp.find('T') {
                    let time_part = &m.timestamp[t_pos + 1..];
                    time_part.chars().take(5).collect()
                } else {
                    m.timestamp.clone()
                }
            }
            Column::Commit => m.commit.chars().take(8).collect(),
            Column::Branch => m.branch.chars().take(15).collect(),
            Column::Experiment => m.experiment.chars().take(18).collect(),
            Column::CompileSecs => format!("{:.2}", m.compile_secs),
            Column::BinUnstripped => format!("{}", m.bin_unstripped / 1024),
            Column::BinStripped => format!("{}", m.bin_stripped / 1024),
            Column::LlvmLines => format!("{}", m.llvm_lines),
            Column::LlvmCopies => format!("{}", m.llvm_copies),
            Column::TypeSizesTotal => format!("{}", m.type_sizes_total),
            Column::LlvmOptimize => format!("{}", m.selfprof.llvm_module_optimize_ms),
            Column::LlvmCodegen => format!("{}", m.selfprof.llvm_module_codegen_ms),
            Column::LlvmLto => format!("{}", m.selfprof.llvm_lto_optimize_ms),
            Column::Typeck => format!("{}", m.selfprof.typeck_ms),
            Column::MirBorrowck => format!("{}", m.selfprof.mir_borrowck_ms),
            Column::ProcMacro => format!("{}", m.selfprof.expand_proc_macro_ms),
            Column::ConstEval => format!("{}", m.selfprof.eval_to_allocation_raw_ms),
            Column::Codegen => format!("{}", m.selfprof.codegen_module_ms),
        }
    }

    fn numeric_value(&self, m: &Metrics) -> Option<u64> {
        match self {
            Column::Timestamp | Column::Commit | Column::Branch | Column::Experiment => None,
            Column::CompileSecs => Some((m.compile_secs * 1000.0) as u64),
            Column::BinUnstripped => Some(m.bin_unstripped),
            Column::BinStripped => Some(m.bin_stripped),
            Column::LlvmLines => Some(m.llvm_lines),
            Column::LlvmCopies => Some(m.llvm_copies),
            Column::TypeSizesTotal => Some(m.type_sizes_total),
            Column::LlvmOptimize => Some(m.selfprof.llvm_module_optimize_ms),
            Column::LlvmCodegen => Some(m.selfprof.llvm_module_codegen_ms),
            Column::LlvmLto => Some(m.selfprof.llvm_lto_optimize_ms),
            Column::Typeck => Some(m.selfprof.typeck_ms),
            Column::MirBorrowck => Some(m.selfprof.mir_borrowck_ms),
            Column::ProcMacro => Some(m.selfprof.expand_proc_macro_ms),
            Column::ConstEval => Some(m.selfprof.eval_to_allocation_raw_ms),
            Column::Codegen => Some(m.selfprof.codegen_module_ms),
        }
    }
}

struct App {
    metrics: Vec<Metrics>,
    table_state: TableState,
    selected_for_diff: Option<usize>,
    visible_columns: Vec<Column>,
    show_column_picker: bool,
    column_picker_state: usize,
    graph_column_idx: usize,    // Index into numeric_columns()
    graph_scroll_offset: usize, // Horizontal scroll offset for bar chart
    should_quit: bool,
}

impl App {
    fn new(metrics: Vec<Metrics>) -> Self {
        let mut table_state = TableState::default();
        if !metrics.is_empty() {
            table_state.select(Some(0));
        }
        Self {
            metrics,
            table_state,
            selected_for_diff: None,
            visible_columns: vec![
                Column::Experiment,
                Column::Commit,
                Column::CompileSecs,
                Column::BinStripped,
                Column::LlvmLines,
                Column::TypeSizesTotal,
                Column::Typeck,
                Column::ConstEval,
            ],
            show_column_picker: false,
            column_picker_state: 0,
            graph_column_idx: 0, // Start with CompileSecs
            graph_scroll_offset: 0,
            should_quit: false,
        }
    }

    fn current_graph_column(&self) -> Column {
        Column::numeric_columns()[self.graph_column_idx]
    }

    fn next_graph_column(&mut self) {
        self.graph_column_idx = (self.graph_column_idx + 1) % Column::numeric_columns().len();
    }

    fn prev_graph_column(&mut self) {
        if self.graph_column_idx == 0 {
            self.graph_column_idx = Column::numeric_columns().len() - 1;
        } else {
            self.graph_column_idx -= 1;
        }
    }

    fn next(&mut self) {
        if self.metrics.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => (i + 1) % self.metrics.len(),
            None => 0,
        };
        self.table_state.select(Some(i));
        self.ensure_selected_visible();
    }

    fn previous(&mut self) {
        if self.metrics.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.metrics.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
        self.ensure_selected_visible();
    }

    /// Ensure the selected row is visible in the bar chart by adjusting scroll offset
    fn ensure_selected_visible(&mut self) {
        if let Some(selected) = self.table_state.selected() {
            // We'll calculate visible bars in render_graph, but here we ensure
            // the selected item is within a reasonable window
            const VISIBLE_BARS: usize = 8; // approximate number of visible bars
            if selected < self.graph_scroll_offset {
                self.graph_scroll_offset = selected;
            } else if selected >= self.graph_scroll_offset + VISIBLE_BARS {
                self.graph_scroll_offset = selected.saturating_sub(VISIBLE_BARS - 1);
            }
        }
    }

    fn toggle_diff_selection(&mut self) {
        if let Some(current) = self.table_state.selected() {
            if self.selected_for_diff == Some(current) {
                self.selected_for_diff = None;
            } else {
                self.selected_for_diff = Some(current);
            }
        }
    }

    fn toggle_column(&mut self, col: Column) {
        if let Some(pos) = self.visible_columns.iter().position(|c| *c == col) {
            self.visible_columns.remove(pos);
        } else {
            self.visible_columns.push(col);
        }
    }
}

fn load_metrics(reports_dir: &std::path::Path) -> Vec<Metrics> {
    let metrics_path = reports_dir.join("metrics.jsonl");
    if !metrics_path.exists() {
        return Vec::new();
    }

    let content = fs::read_to_string(&metrics_path).unwrap_or_default();
    content
        .lines()
        .filter_map(|line| facet_json::from_str::<Metrics>(line).ok())
        .collect()
}

fn workspace_root() -> PathBuf {
    let output = Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()
        .expect("Failed to run cargo locate-project");

    let path = String::from_utf8(output.stdout).expect("Invalid UTF-8");
    PathBuf::from(path.trim())
        .parent()
        .expect("No parent directory")
        .to_path_buf()
}

fn main() -> io::Result<()> {
    let workspace = workspace_root();
    let reports_dir = workspace.join("reports");
    let metrics = load_metrics(&reports_dir);

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut app = App::new(metrics);

    while !app.should_quit {
        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if app.show_column_picker {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('c') => app.show_column_picker = false,
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.column_picker_state > 0 {
                            app.column_picker_state -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if app.column_picker_state < Column::all().len() - 1 {
                            app.column_picker_state += 1;
                        }
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        let col = Column::all()[app.column_picker_state];
                        app.toggle_column(col);
                    }
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true
                    }
                    KeyCode::Up | KeyCode::Char('k') => app.previous(),
                    KeyCode::Down | KeyCode::Char('j') => app.next(),
                    KeyCode::Left | KeyCode::Char('h') => app.prev_graph_column(),
                    KeyCode::Right | KeyCode::Char('l') => app.next_graph_column(),
                    KeyCode::Enter | KeyCode::Char(' ') => app.toggle_diff_selection(),
                    KeyCode::Char('c') => app.show_column_picker = true,
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // Table
            Constraint::Percentage(45), // Graph
            Constraint::Length(2),      // Help
        ])
        .split(f.area());

    render_table(f, app, chunks[0]);
    render_graph(f, app, chunks[1]);
    render_help(f, app, chunks[2]);

    // Column picker popup
    if app.show_column_picker {
        render_column_picker(f, app);
    }
}

fn render_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header_cells: Vec<Cell> = app
        .visible_columns
        .iter()
        .map(|c| Cell::from(c.name()).style(Style::default().bold()))
        .collect();
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows: Vec<Row> = app
        .metrics
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let is_diff_base = app.selected_for_diff == Some(idx);

            let cells: Vec<Cell> = app
                .visible_columns
                .iter()
                .map(|col| {
                    let value = col.value(m);

                    // Show diff if we have a base selected and this is a different row
                    if let Some(base_idx) = app.selected_for_diff
                        && base_idx != idx
                        && let (Some(base_val), Some(curr_val)) = (
                            col.numeric_value(&app.metrics[base_idx]),
                            col.numeric_value(m),
                        )
                    {
                        let diff = curr_val as i64 - base_val as i64;
                        let pct = if base_val != 0 {
                            (diff as f64 / base_val as f64) * 100.0
                        } else {
                            0.0
                        };
                        if diff > 0 {
                            return Cell::from(format!("{value} +{pct:.1}%"))
                                .style(Style::default().fg(Color::Red));
                        } else if diff < 0 {
                            return Cell::from(format!("{value} {pct:.1}%"))
                                .style(Style::default().fg(Color::Green));
                        }
                    }

                    Cell::from(value)
                })
                .collect();

            let style = if is_diff_base {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };

            Row::new(cells).style(style)
        })
        .collect();

    // Calculate column widths based on actual content
    let widths: Vec<Constraint> = app
        .visible_columns
        .iter()
        .map(|col| {
            // Start with header width
            let header_width = col.name().len();
            // Find max content width
            let max_content_width = app
                .metrics
                .iter()
                .map(|m| col.value(m).len())
                .max()
                .unwrap_or(0);
            // Use the larger of header or content, with some padding
            let w = header_width.max(max_content_width).min(40) as u16 + 2;
            Constraint::Length(w)
        })
        .collect();

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Metrics "))
        .row_highlight_style(Style::default().bg(Color::Blue).fg(Color::White));

    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_graph(f: &mut Frame, app: &App, area: Rect) {
    let col = app.current_graph_column();
    let selected_row = app.table_state.selected();

    // Collect data for bar chart
    let data: Vec<(&str, u64)> = app
        .metrics
        .iter()
        .map(|m| {
            let label = m.experiment.as_str();
            let value = col.numeric_value(m).unwrap_or(0);
            (label, value)
        })
        .collect();

    if data.is_empty() {
        let empty = Paragraph::new("No data").block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} (◄► to change) ", col.name())),
        );
        f.render_widget(empty, area);
        return;
    }

    // Calculate how many bars can fit
    let bar_width: u16 = 14;
    let bar_gap: u16 = 1;
    let available_width = area.width.saturating_sub(2); // account for borders
    let bars_per_screen = (available_width / (bar_width + bar_gap)).max(1) as usize;

    // Apply scroll offset - show a window of bars
    let start_idx = app.graph_scroll_offset;
    let end_idx = (start_idx + bars_per_screen).min(data.len());
    let visible_data = &data[start_idx..end_idx];

    // Find max for scaling (use global max for consistent scale)
    let max_val = data.iter().map(|(_, v)| *v).max().unwrap_or(1);

    // Create bars with highlighting
    let bars: Vec<Bar> = visible_data
        .iter()
        .enumerate()
        .map(|(visible_idx, (label, value))| {
            let actual_idx = start_idx + visible_idx;
            let is_selected = selected_row == Some(actual_idx);
            let is_diff_base = app.selected_for_diff == Some(actual_idx);

            let style = if is_selected {
                Style::default().fg(Color::Yellow)
            } else if is_diff_base {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Blue)
            };

            let value_style = if is_selected {
                Style::default().fg(Color::Yellow).bold()
            } else {
                Style::default()
            };

            // Truncate label to fit
            let short_label: String = label.chars().take(12).collect();

            Bar::default()
                .value(*value)
                .label(Line::from(short_label))
                .style(style)
                .value_style(value_style)
        })
        .collect();

    // Show scroll indicators in title
    let scroll_info = if data.len() > bars_per_screen {
        format!(" [{}-{}/{}] ", start_idx + 1, end_idx, data.len())
    } else {
        String::new()
    };

    let bar_chart = BarChart::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} (◄► to change){scroll_info}", col.name())),
        )
        .data(BarGroup::default().bars(&bars))
        .bar_width(bar_width)
        .bar_gap(bar_gap)
        .max(max_val);

    f.render_widget(bar_chart, area);
}

fn render_help(f: &mut Frame, app: &App, area: Rect) {
    let help_text =
        " ↑↓/jk: select row | ←→/hl: change column | Space: set diff base | c: columns | q: quit ";

    let status = if let Some(base_idx) = app.selected_for_diff {
        format!("│ Base: {} ", app.metrics[base_idx].experiment)
    } else {
        String::new()
    };

    let text = format!("{help_text}{status}");
    let help = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, area);
}

fn render_column_picker(f: &mut Frame, app: &App) {
    let popup_area = centered_rect(40, 80, f.area());
    f.render_widget(Clear, popup_area);

    let items: Vec<Row> = Column::all()
        .iter()
        .enumerate()
        .map(|(idx, col)| {
            let enabled = app.visible_columns.contains(col);
            let marker = if enabled { "[x]" } else { "[ ]" };
            let style = if idx == app.column_picker_state {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else {
                Style::default()
            };
            Row::new(vec![Cell::from(marker), Cell::from(col.name())]).style(style)
        })
        .collect();

    let table = Table::new(items, [Constraint::Length(4), Constraint::Min(10)]).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Columns (Space: toggle, Esc: close) "),
    );
    f.render_widget(table, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
