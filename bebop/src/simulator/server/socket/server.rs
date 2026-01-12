use std::io::Result;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::thread;

pub fn accept_connection_async(port: u16, name: &str) -> Result<(TcpListener, Receiver<TcpStream>)> {
  let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
  // println!("Socket server listening on 127.0.0.1:{} ({})", port, name);

  let listener_clone = listener.try_clone()?;
  let (tx, rx) = mpsc::channel();
  let name_owned = name.to_string();

  thread::spawn(move || {
    // println!("Waiting for {} connection on {}...", name_owned, port);
    match listener_clone.accept() {
      Ok((stream, addr)) => {
        // println!("{} Connected: {}", name_owned, addr);
        let _ = tx.send(stream);
      },
      Err(e) => {
        eprintln!("{} accept error: {}", name_owned, e);
      },
    }
  });

  Ok((listener, rx))
}
