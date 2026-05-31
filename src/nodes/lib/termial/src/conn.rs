use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

#[derive(Debug)]
pub enum Msg {
  Connected(u64, u32),
  Data(u64, u32, Vec<u8>),
  Closed(u64, u32),
  Error(u64, u32, String),
}

pub fn spawn(
  session: u64,
  hart: u32,
  socket: PathBuf,
  msg_tx: Sender<Msg>,
  input_rx: Receiver<Vec<u8>>,
) -> Result<(), String> {
  std::thread::Builder::new()
    .name(format!("bebop-termial-hart-{hart}"))
    .spawn(move || {
      if let Err(e) = run(session, hart, &socket, &msg_tx, input_rx) {
        let _ = msg_tx.send(Msg::Error(session, hart, e));
      }
    })
    .map(|_| ())
    .map_err(|e| format!("failed to spawn hart {hart} thread: {e}"))
}

fn run(
  session: u64,
  hart: u32,
  socket: &Path,
  msg_tx: &Sender<Msg>,
  input_rx: Receiver<Vec<u8>>,
) -> Result<(), String> {
  let mut stream =
    UnixStream::connect(socket).map_err(|e| format!("failed to connect {}: {e}", socket.display()))?;
  stream
    .write_all(format!("hart {hart}\n").as_bytes())
    .map_err(|e| format!("failed to write handshake: {e}"))?;
  stream
    .set_nonblocking(true)
    .map_err(|e| format!("failed to set socket nonblocking: {e}"))?;
  msg_tx
    .send(Msg::Connected(session, hart))
    .map_err(|e| format!("failed to report connection: {e}"))?;

  let mut buf = [0_u8; 4096];
  loop {
    drain_input(&mut stream, &input_rx)?;
    match stream.read(&mut buf) {
      Ok(0) => {
        let _ = msg_tx.send(Msg::Closed(session, hart));
        return Ok(());
      }
      Ok(n) => msg_tx
        .send(Msg::Data(session, hart, buf[..n].to_vec()))
        .map_err(|e| format!("failed to report uart data: {e}"))?,
      Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
      Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
      Err(e) => return Err(format!("failed to read uart: {e}")),
    }
  }
}

fn drain_input(stream: &mut UnixStream, input_rx: &Receiver<Vec<u8>>) -> Result<(), String> {
  loop {
    match input_rx.try_recv() {
      Ok(data) => write_input(stream, &data)?,
      Err(mpsc::TryRecvError::Empty) => return Ok(()),
      Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
    }
  }
}

fn write_input(stream: &mut UnixStream, buf: &[u8]) -> Result<(), String> {
  let out: Vec<u8> = buf
    .iter()
    .map(|byte| if *byte == b'\r' { b'\n' } else { *byte })
    .collect();
  stream
    .write_all(&out)
    .map_err(|e| format!("failed to write input to console: {e}"))
}
