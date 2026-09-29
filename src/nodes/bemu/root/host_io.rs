use std::{
    fs::File,
    io::ErrorKind,
    net::Shutdown,
    os::{fd::AsRawFd, unix::net::{UnixListener, UnixStream}},
    path::Path,
    sync::{atomic::{AtomicBool, Ordering}, Mutex},
    time::Duration,
};

pub(crate) struct Endpoint {
    listener: UnixListener,
    stream: Mutex<Option<UnixStream>>,
}

impl Endpoint {
    pub(crate) fn bind(directory: &Path) -> Result<Self, String> {
        let directory = File::open(directory).map_err(|e| e.to_string())?;
        let path = format!("/proc/self/fd/{}/io.sock", directory.as_raw_fd());
        let listener = UnixListener::bind(path).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Self { listener, stream: Mutex::new(None) })
    }

    pub(crate) fn accept(&self, cancelled: &AtomicBool) -> Result<UnixStream, String> {
        loop {
            if cancelled.load(Ordering::Acquire) { return Err("host I/O cancelled".into()); }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    *self.stream.lock().expect("host I/O poisoned") = Some(stream.try_clone().map_err(|e| e.to_string())?);
                    if cancelled.load(Ordering::Acquire) { self.stop(); return Err("host I/O cancelled".into()); }
                    return Ok(stream);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(1)),
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    pub(crate) fn stop(&self) {
        if let Some(stream) = self.stream.lock().expect("host I/O poisoned").take() {
            stream.shutdown(Shutdown::Both).expect("shutdown host I/O");
        }
    }
}
