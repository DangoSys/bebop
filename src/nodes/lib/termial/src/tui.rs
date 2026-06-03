use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::app::{App, Conn, Mode, Pane, Run};
use crate::cli::Cli;
use crate::conn::{self, Msg};

const TICK: Duration = Duration::from_millis(33);

pub fn run(cli: Cli) -> Result<(), String> {
  let mut app = App::new(&cli);
  let (msg_tx, msg_rx) = mpsc::channel();
  let mut tui = Tui::enter()?;
  let res = loop_ui(&mut tui.terminal, &mut app, &msg_tx, &msg_rx);
  drop(tui);
  res
}

fn loop_ui(
  term: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
  app: &mut App,
  msg_tx: &mpsc::Sender<Msg>,
  msg_rx: &mpsc::Receiver<Msg>,
) -> Result<(), String> {
  let mut last = Instant::now();
  while !app.exit {
    drain(app, msg_rx);
    if app.session_done() {
      app.disconnect("disconnected: simulation ended".to_string());
    }
    term
      .draw(|f| draw(f, app))
      .map_err(|e| format!("failed to draw UI: {e}"))?;
    let timeout = TICK.saturating_sub(last.elapsed());
    if event::poll(timeout).map_err(|e| format!("failed to poll terminal event: {e}"))? {
      let ev = event::read().map_err(|e| format!("failed to read terminal event: {e}"))?;
      key(app, msg_tx, ev)?;
    }
    if last.elapsed() >= TICK {
      last = Instant::now();
    }
  }
  Ok(())
}

fn drain(app: &mut App, msg_rx: &mpsc::Receiver<Msg>) {
  while let Ok(msg) = msg_rx.try_recv() {
    match msg {
      Msg::Connected(s, h) if s == app.session => set_status(app, h, true, "connected"),
      Msg::Closed(s, h) if s == app.session => set_status(app, h, false, "closed"),
      Msg::Error(s, h, e) if s == app.session => {
        if let Some(p) = app.by_hart(h) {
          p.connected = false;
          p.status = e;
        }
      }
      Msg::Data(s, h, data) if s == app.session => {
        if let Some(p) = app.by_hart(h) {
          p.connected = true;
          p.status = "connected".to_string();
          p.push(&data);
        }
      }
      _ => {}
    }
  }
}

fn set_status(app: &mut App, hart: u32, connected: bool, status: &str) {
  if let Some(p) = app.by_hart(hart) {
    p.connected = connected;
    p.status = status.to_string();
  }
}

fn key(app: &mut App, msg_tx: &mpsc::Sender<Msg>, ev: Event) -> Result<(), String> {
  let Event::Key(k) = ev else { return Ok(()) };
  if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('c')) {
    app.exit = true;
    return Ok(());
  }
  match app.mode {
    Mode::Start => start_key(app, msg_tx, k),
    Mode::Session => session_key(app, k),
  }
}

fn start_key(app: &mut App, msg_tx: &mpsc::Sender<Msg>, k: KeyEvent) -> Result<(), String> {
  match k {
    KeyEvent { code: KeyCode::Tab, .. }
    | KeyEvent {
      code: KeyCode::Down, ..
    } => {
      app.form.active = (app.form.active + 1) % app.form.fields.len();
    }
    KeyEvent {
      code: KeyCode::BackTab, ..
    }
    | KeyEvent { code: KeyCode::Up, .. } => {
      app.form.active = (app.form.active + app.form.fields.len() - 1) % app.form.fields.len();
    }
    KeyEvent {
      code: KeyCode::Enter, ..
    } => {
      if let Err(e) = connect(app, msg_tx) {
        if matches!(app.mode, Mode::Session) {
          app.disconnect(format!("connect failed: {e}"));
        } else {
          app.form.msg = format!("connect failed: {e}");
        }
      }
    }
    KeyEvent {
      code: KeyCode::Backspace,
      ..
    } => {
      app.form.fields[app.form.active].pop();
    }
    KeyEvent {
      code: KeyCode::Char(c),
      modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
      ..
    } => {
      app.form.fields[app.form.active].push(c);
    }
    _ => {}
  }
  Ok(())
}

fn connect(app: &mut App, msg_tx: &mpsc::Sender<Msg>) -> Result<(), String> {
  let log_dir = form_log_dir(app)?;
  let socket = log_socket(&log_dir)?;
  let harts = form_u32(app, 1, "harts")?;
  if harts == 0 {
    return Err("harts must be greater than 0".to_string());
  }
  app.start_session(harts);
  load_uart_logs(app, &log_dir)?;
  for p in &app.panes {
    let (tx, rx) = mpsc::channel();
    conn::spawn(app.session, p.hart, socket.clone(), msg_tx.clone(), rx)?;
    app.conns.push(Conn { hart: p.hart, tx });
  }
  Ok(())
}

fn form_log_dir(app: &App) -> Result<PathBuf, String> {
  let log_dir = app.form.fields[0].trim();
  if log_dir.is_empty() {
    Err("log dir is required".to_string())
  } else {
    Ok(PathBuf::from(log_dir))
  }
}

fn log_socket(dir: &Path) -> Result<PathBuf, String> {
  let file = dir.join("console.sock.path");
  let path = std::fs::read_to_string(&file)
    .map_err(|e| format!("failed to read console socket path {}: {e}", file.display()))?;
  let path = path.trim_end_matches(['\r', '\n']);
  if path.is_empty() {
    return Err(format!("console socket path file {} is empty", file.display()));
  }
  Ok(PathBuf::from(path))
}

fn load_uart_logs(app: &mut App, log_dir: &Path) -> Result<(), String> {
  let uart_dir = log_dir.join("uart");
  for p in &mut app.panes {
    let path = uart_dir.join(format!("hart-{}.log", p.hart));
    match std::fs::read(&path) {
      Ok(data) => p.push(&data),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
      Err(e) => return Err(format!("failed to read UART log {}: {e}", path.display())),
    }
  }
  Ok(())
}

fn form_u32(app: &App, idx: usize, name: &str) -> Result<u32, String> {
  let value = app.form.fields[idx].trim();
  value
    .parse::<u32>()
    .map_err(|e| format!("invalid {name} value `{value}`: {e}"))
}

fn session_key(app: &mut App, k: KeyEvent) -> Result<(), String> {
  match k {
    KeyEvent { code: KeyCode::Tab, .. } => app.active = (app.active + 1) % app.panes.len(),
    KeyEvent {
      code: KeyCode::BackTab, ..
    } => app.active = (app.active + app.panes.len() - 1) % app.panes.len(),
    KeyEvent {
      code: KeyCode::Left, ..
    } => move_input_cursor(app, -1),
    KeyEvent {
      code: KeyCode::Right, ..
    } => move_input_cursor(app, 1),
    KeyEvent {
      code: KeyCode::Home, ..
    } => app.pane_mut().input_cursor = 0,
    KeyEvent { code: KeyCode::End, .. } => app.pane_mut().input_cursor = app.pane().input.len(),
    KeyEvent { code: KeyCode::Up, .. } => move_row(app, -1),
    KeyEvent {
      code: KeyCode::Down, ..
    } => move_row(app, 1),
    KeyEvent {
      code: KeyCode::Enter, ..
    } => send_input(app)?,
    KeyEvent { code: KeyCode::Esc, .. } => app.disconnect("disconnected by user".to_string()),
    KeyEvent {
      code: KeyCode::Backspace,
      ..
    } => {
      backspace_input(app);
    }
    KeyEvent {
      code: KeyCode::Delete, ..
    } => {
      delete_input(app);
    }
    KeyEvent {
      code: KeyCode::Char('f'),
      modifiers: KeyModifiers::NONE,
      ..
    } => app.grid = !app.grid,
    KeyEvent {
      code: KeyCode::Char(c),
      modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
      ..
    } => {
      insert_input(app, c);
    }
    _ => {}
  }
  Ok(())
}

fn move_input_cursor(app: &mut App, dir: isize) {
  let p = app.pane_mut();
  if dir < 0 {
    p.input_cursor = prev_char_boundary(&p.input, p.input_cursor);
  } else {
    p.input_cursor = next_char_boundary(&p.input, p.input_cursor);
  }
}

fn insert_input(app: &mut App, ch: char) {
  let p = app.pane_mut();
  p.input.insert(p.input_cursor, ch);
  p.input_cursor += ch.len_utf8();
}

fn backspace_input(app: &mut App) {
  let p = app.pane_mut();
  if p.input_cursor > 0 {
    p.input_cursor = prev_char_boundary(&p.input, p.input_cursor);
    p.input.remove(p.input_cursor);
  }
}

fn delete_input(app: &mut App) {
  let p = app.pane_mut();
  if p.input_cursor < p.input.len() {
    p.input.remove(p.input_cursor);
  }
}

fn prev_char_boundary(s: &str, cursor: usize) -> usize {
  s[..cursor].char_indices().last().map(|(idx, _)| idx).unwrap_or(0)
}

fn next_char_boundary(s: &str, cursor: usize) -> usize {
  s[cursor..]
    .char_indices()
    .nth(1)
    .map(|(idx, _)| cursor + idx)
    .unwrap_or(s.len())
}

fn send_input(app: &mut App) -> Result<(), String> {
  let input = app.pane().input.clone();
  app.send_input()?;
  if !input.is_empty() {
    app.pane_mut().push_plain(format!("tx> {input}"));
  }
  Ok(())
}

fn move_row(app: &mut App, dir: isize) {
  let next = app.active as isize + dir * cols(app.panes.len()) as isize;
  if (0..app.panes.len() as isize).contains(&next) {
    app.active = next as usize;
  }
}

fn draw(f: &mut Frame<'_>, app: &App) {
  match app.mode {
    Mode::Start => draw_start(f, app),
    Mode::Session => draw_session(f, app),
  }
}

fn draw_start(f: &mut Frame<'_>, app: &App) {
  let area = centered(f.area(), 80, 14);
  let inner = Block::default().title(" connect ").borders(Borders::ALL).inner(area);
  f.render_widget(Block::default().title(" connect ").borders(Borders::ALL), area);
  let rows = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(1),
      Constraint::Length(1),
      Constraint::Length(4),
      Constraint::Length(3),
      Constraint::Length(1),
    ])
    .split(inner);
  f.render_widget(
    Paragraph::new(Line::from(Span::styled(
      "bebop-termial",
      Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    ))),
    rows[0],
  );
  f.render_widget(Paragraph::new(app.form.msg.clone()).wrap(Wrap { trim: false }), rows[1]);
  field(f, rows[2], app, 0, "log dir");
  field(f, rows[3], app, 1, "harts");
  f.render_widget(
    Paragraph::new("tab/up/down switch  enter connect  ctrl-c quit"),
    rows[4],
  );
}

fn field(f: &mut Frame<'_>, area: Rect, app: &App, idx: usize, label: &str) {
  let active = idx == app.form.active;
  let color = if active { Color::Yellow } else { Color::Gray };
  let title = if active {
    format!(" > {label} ")
  } else {
    format!("   {label} ")
  };
  let block = Block::default()
    .title(title)
    .borders(Borders::ALL)
    .border_style(Style::default().fg(color));
  f.render_widget(
    Paragraph::new(app.form.fields[idx].clone())
      .block(block)
      .wrap(Wrap { trim: false }),
    area,
  );
}

fn draw_session(f: &mut Frame<'_>, app: &App) {
  let a = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(1), Constraint::Min(3), Constraint::Length(3)])
    .split(f.area());
  tabs(f, a[0], app);
  if app.grid {
    grid(f, a[1], app);
  } else {
    pane(f, a[1], app.pane(), true);
  }
  input(f, a[2], app);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
  let v = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Min(0),
      Constraint::Length(h.min(area.height)),
      Constraint::Min(0),
    ])
    .split(area);
  Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
      Constraint::Min(0),
      Constraint::Length(w.min(area.width)),
      Constraint::Min(0),
    ])
    .split(v[1])[1]
}

fn tabs(f: &mut Frame<'_>, area: Rect, app: &App) {
  let mut spans = vec![Span::styled(
    " bebop-termial ",
    Style::default().fg(Color::Black).bg(Color::Cyan),
  )];
  for (idx, p) in app.panes.iter().enumerate() {
    let color = if idx == app.active {
      Color::Yellow
    } else if p.connected {
      Color::Green
    } else {
      Color::Red
    };
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
      format!("hart{}", p.hart),
      Style::default().fg(color).add_modifier(Modifier::BOLD),
    ));
  }
  spans.push(Span::raw(
    "  tab switch  left/right edit  f grid  enter send  esc disconnect  ctrl-c quit",
  ));
  f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn grid(f: &mut Frame<'_>, area: Rect, app: &App) {
  let cols = cols(app.panes.len());
  let rows = app.panes.len().div_ceil(cols);
  let rs = Layout::default()
    .direction(Direction::Vertical)
    .constraints(vec![Constraint::Ratio(1, rows as u32); rows])
    .split(area);
  for r in 0..rows {
    let cs = Layout::default()
      .direction(Direction::Horizontal)
      .constraints(vec![Constraint::Ratio(1, cols as u32); cols])
      .split(rs[r]);
    for c in 0..cols {
      let idx = r * cols + c;
      if let Some(p) = app.panes.get(idx) {
        pane(f, cs[c], p, idx == app.active);
      }
    }
  }
}

fn cols(n: usize) -> usize {
  if n <= 2 {
    n.max(1)
  } else {
    (n as f64).sqrt().ceil() as usize
  }
}

fn pane(f: &mut Frame<'_>, area: Rect, p: &Pane, active: bool) {
  let color = if active {
    Color::Yellow
  } else if p.connected {
    Color::Green
  } else {
    Color::Red
  };
  let title = format!(" hart{}  {}  rx:{} tx:{} ", p.hart, p.status, p.rx, p.tx);
  let rows = area.height.saturating_sub(2) as usize;
  let cap = rows.saturating_sub((!p.cur.is_empty()) as usize);
  let mut lines: Vec<Line<'static>> = p.lines.iter().rev().take(cap).map(|runs| runs_line(runs)).collect();
  lines.reverse();
  if !p.cur.is_empty() && lines.len() < rows {
    lines.push(runs_line(&p.cur));
  }
  let block = Block::default()
    .title(title)
    .borders(Borders::ALL)
    .border_style(Style::default().fg(color));
  f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), area);
}

fn runs_line(runs: &[Run]) -> Line<'static> {
  Line::from(
    runs
      .iter()
      .map(|run| Span::styled(run.text.clone(), run.style))
      .collect::<Vec<_>>(),
  )
}

fn input(f: &mut Frame<'_>, area: Rect, app: &App) {
  let p = app.pane();
  let has_input = !p.input.is_empty();
  let text = if has_input {
    p.input.clone()
  } else {
    "type command and press enter".to_string()
  };
  let block = Block::default()
    .title(format!(" input hart{} ", p.hart))
    .borders(Borders::ALL)
    .border_style(Style::default().fg(Color::Yellow));
  let inner = block.inner(area);
  f.render_widget(Paragraph::new(text).block(block), area);
  if has_input {
    let x = inner.x + (p.input[..p.input_cursor].chars().count() as u16).min(inner.width.saturating_sub(1));
    let y = inner.y;
    f.set_cursor_position((x, y));
  }
}

struct Tui {
  terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}

impl Tui {
  fn enter() -> Result<Self, String> {
    terminal::enable_raw_mode().map_err(|e| format!("failed to enable raw mode: {e}"))?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| format!("failed to enter alt screen: {e}"))?;
    let terminal =
      Terminal::new(CrosstermBackend::new(stdout)).map_err(|e| format!("failed to create terminal: {e}"))?;
    Ok(Self { terminal })
  }
}

impl Drop for Tui {
  fn drop(&mut self) {
    let _ = terminal::disable_raw_mode();
    let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    let _ = self.terminal.show_cursor();
  }
}
