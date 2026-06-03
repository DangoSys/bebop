use std::collections::VecDeque;
use std::sync::mpsc::Sender;

use ratatui::style::{Color, Modifier, Style};

use crate::cli::Cli;
use crate::form::Form;

const MAX_LINES: usize = 8000;

#[derive(Debug)]
pub struct Run {
  pub text: String,
  pub style: Style,
}

impl Run {
  fn new(text: String, style: Style) -> Self {
    Self { text, style }
  }
}

#[derive(Debug)]
enum Esc {
  None,
  Esc,
  Csi(String),
}

#[derive(Debug)]
pub struct Conn {
  pub hart: u32,
  pub tx: Sender<Vec<u8>>,
}

#[derive(Debug)]
pub struct Pane {
  pub hart: u32,
  pub lines: VecDeque<Vec<Run>>,
  pub cur: Vec<Run>,
  pub input: String,
  pub input_cursor: usize,
  pub rx: u64,
  pub tx: u64,
  pub connected: bool,
  pub status: String,
  style: Style,
  esc: Esc,
}

impl Pane {
  pub fn new(hart: u32) -> Self {
    Self {
      hart,
      lines: VecDeque::new(),
      cur: Vec::new(),
      input: String::new(),
      input_cursor: 0,
      rx: 0,
      tx: 0,
      connected: false,
      status: "connecting".to_string(),
      style: Style::default(),
      esc: Esc::None,
    }
  }

  pub fn push(&mut self, data: &[u8]) {
    self.rx += data.len() as u64;
    for byte in data {
      self.push_byte(*byte);
    }
  }

  pub fn push_plain(&mut self, text: String) {
    self.lines.push_back(vec![Run::new(text, Style::default())]);
    self.truncate();
  }

  fn push_byte(&mut self, byte: u8) {
    match &mut self.esc {
      Esc::None => self.push_text_byte(byte),
      Esc::Esc => {
        self.esc = Esc::None;
        if byte == b'[' {
          self.esc = Esc::Csi(String::new());
        } else {
          self.push_escaped(0x1b);
          self.push_byte(byte);
        }
      }
      Esc::Csi(params) => {
        if byte.is_ascii_digit() || byte == b';' {
          params.push(byte as char);
        } else {
          let params = std::mem::take(params);
          self.esc = Esc::None;
          if byte == b'm' {
            self.apply_sgr(&params);
          }
        }
      }
    }
  }

  fn push_text_byte(&mut self, byte: u8) {
    match byte {
      b'\n' => self.finish(),
      b'\r' => {}
      0x08 | 0x7f => self.pop_char(),
      0x1b => self.esc = Esc::Esc,
      byte if byte.is_ascii_graphic() || byte == b' ' || byte == b'\t' => self.push_char(byte as char),
      byte => self.push_escaped(byte),
    }
  }

  fn push_char(&mut self, ch: char) {
    if let Some(run) = self.cur.last_mut().filter(|run| run.style == self.style) {
      run.text.push(ch);
    } else {
      self.cur.push(Run::new(ch.to_string(), self.style));
    }
  }

  fn push_escaped(&mut self, byte: u8) {
    for ch in format!("\\x{byte:02x}").chars() {
      self.push_char(ch);
    }
  }

  fn pop_char(&mut self) {
    if let Some(run) = self.cur.last_mut() {
      run.text.pop();
      if run.text.is_empty() {
        self.cur.pop();
      }
    }
  }

  fn apply_sgr(&mut self, params: &str) {
    let values = sgr_values(params);
    let mut i = 0;
    while i < values.len() {
      match values[i] {
        0 => self.style = Style::default(),
        1 => self.style = self.style.add_modifier(Modifier::BOLD),
        22 => self.style = self.style.remove_modifier(Modifier::BOLD | Modifier::DIM),
        30..=37 => self.style = self.style.fg(ansi_color(values[i] - 30, false)),
        39 => self.style = self.style.fg(Color::Reset),
        40..=47 => self.style = self.style.bg(ansi_color(values[i] - 40, false)),
        49 => self.style = self.style.bg(Color::Reset),
        90..=97 => self.style = self.style.fg(ansi_color(values[i] - 90, true)),
        100..=107 => self.style = self.style.bg(ansi_color(values[i] - 100, true)),
        38 | 48 if i + 2 < values.len() && values[i + 1] == 5 => {
          let color = Color::Indexed(values[i + 2] as u8);
          self.style = if values[i] == 38 {
            self.style.fg(color)
          } else {
            self.style.bg(color)
          };
          i += 2;
        }
        _ => {}
      }
      i += 1;
    }
  }

  fn finish(&mut self) {
    self.lines.push_back(std::mem::take(&mut self.cur));
    self.truncate();
  }

  fn truncate(&mut self) {
    while self.lines.len() > MAX_LINES {
      self.lines.pop_front();
    }
  }
}

fn sgr_values(params: &str) -> Vec<u16> {
  if params.is_empty() {
    return vec![0];
  }
  params
    .split(';')
    .map(|part| if part.is_empty() { Ok(0) } else { part.parse::<u16>() })
    .filter_map(Result::ok)
    .collect()
}

fn ansi_color(idx: u16, bright: bool) -> Color {
  match (idx, bright) {
    (0, false) => Color::Black,
    (1, false) => Color::Red,
    (2, false) => Color::Green,
    (3, false) => Color::Yellow,
    (4, false) => Color::Blue,
    (5, false) => Color::Magenta,
    (6, false) => Color::Cyan,
    (7, false) => Color::Gray,
    (0, true) => Color::DarkGray,
    (1, true) => Color::LightRed,
    (2, true) => Color::LightGreen,
    (3, true) => Color::LightYellow,
    (4, true) => Color::LightBlue,
    (5, true) => Color::LightMagenta,
    (6, true) => Color::LightCyan,
    (7, true) => Color::White,
    _ => Color::Reset,
  }
}

#[derive(Debug)]
pub enum Mode {
  Start,
  Session,
}

pub struct App {
  pub form: Form,
  pub mode: Mode,
  pub panes: Vec<Pane>,
  pub conns: Vec<Conn>,
  pub active: usize,
  pub grid: bool,
  pub exit: bool,
  pub session: u64,
}

impl App {
  pub fn new(cli: &Cli) -> Self {
    Self {
      form: Form::new(cli),
      mode: Mode::Start,
      panes: Vec::new(),
      conns: Vec::new(),
      active: 0,
      grid: true,
      exit: false,
      session: 0,
    }
  }

  pub fn start_session(&mut self, harts: u32) {
    self.session += 1;
    self.panes = (0..harts).map(Pane::new).collect();
    self.conns.clear();
    self.active = 0;
    self.grid = harts > 1;
    self.mode = Mode::Session;
  }

  pub fn disconnect(&mut self, msg: String) {
    self.conns.clear();
    self.panes.clear();
    self.active = 0;
    self.mode = Mode::Start;
    self.form.msg = msg;
  }

  pub fn session_done(&self) -> bool {
    matches!(self.mode, Mode::Session)
      && !self.panes.is_empty()
      && self.panes.iter().all(|p| !p.connected && p.status != "connecting")
  }

  pub fn pane(&self) -> &Pane {
    &self.panes[self.active]
  }

  pub fn pane_mut(&mut self) -> &mut Pane {
    &mut self.panes[self.active]
  }

  pub fn by_hart(&mut self, hart: u32) -> Option<&mut Pane> {
    self.panes.iter_mut().find(|pane| pane.hart == hart)
  }

  pub fn send_input(&mut self) -> Result<(), String> {
    let hart = self.pane().hart;
    let mut data = std::mem::take(&mut self.pane_mut().input).into_bytes();
    self.pane_mut().input_cursor = 0;
    data.push(b'\n');
    let len = data.len() as u64;
    let conn = self
      .conns
      .iter()
      .find(|conn| conn.hart == hart)
      .ok_or_else(|| format!("missing connection for hart {hart}"))?;
    conn
      .tx
      .send(data)
      .map_err(|e| format!("failed to queue input for hart {hart}: {e}"))?;
    self.pane_mut().tx += len;
    Ok(())
  }
}
