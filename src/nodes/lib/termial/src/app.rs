use std::collections::VecDeque;
use std::sync::mpsc::Sender;

use crate::cli::Cli;
use crate::form::Form;

const MAX_LINES: usize = 8000;

#[derive(Debug)]
pub struct Conn {
  pub hart: u32,
  pub tx: Sender<Vec<u8>>,
}

#[derive(Debug)]
pub struct Pane {
  pub hart: u32,
  pub lines: VecDeque<String>,
  pub cur: String,
  pub input: String,
  pub rx: u64,
  pub tx: u64,
  pub connected: bool,
  pub status: String,
}

impl Pane {
  pub fn new(hart: u32) -> Self {
    Self {
      hart,
      lines: VecDeque::new(),
      cur: String::new(),
      input: String::new(),
      rx: 0,
      tx: 0,
      connected: false,
      status: "connecting".to_string(),
    }
  }

  pub fn push(&mut self, data: &[u8]) {
    self.rx += data.len() as u64;
    for byte in data {
      match *byte {
        b'\n' => self.finish(),
        b'\r' => {}
        0x08 | 0x7f => {
          self.cur.pop();
        }
        byte if byte.is_ascii_graphic() || byte == b' ' || byte == b'\t' => {
          self.cur.push(byte as char);
        }
        byte => self.cur.push_str(&format!("\\x{byte:02x}")),
      }
    }
  }

  fn finish(&mut self) {
    self.lines.push_back(std::mem::take(&mut self.cur));
    while self.lines.len() > MAX_LINES {
      self.lines.pop_front();
    }
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
