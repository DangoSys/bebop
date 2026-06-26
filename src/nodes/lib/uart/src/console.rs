use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

type ConsoleClients = Arc<Mutex<HashMap<u32, Vec<UnixStream>>>>;
type RxHandler = Arc<dyn Fn(u32, u8) + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy)]
pub struct UartTx {
    pub hart_id: u32,
    pub byte: u8,
}

pub struct ConsoleConfig {
    pub thread_prefix: String,
    pub uart_log_dir: Option<PathBuf>,
    pub rx_log_path: Option<PathBuf>,
    pub display: Option<File>,
}

impl ConsoleConfig {
    pub fn new(thread_prefix: impl Into<String>) -> Self {
        Self {
            thread_prefix: thread_prefix.into(),
            uart_log_dir: None,
            rx_log_path: None,
            display: None,
        }
    }
}

pub struct ConsoleServer {
    socket_path: PathBuf,
    path_file: PathBuf,
    uart_log_dir: Option<PathBuf>,
    uart_logs: Mutex<HashMap<u32, BufWriter<File>>>,
    display: Option<Mutex<BufWriter<File>>>,
    stop: Arc<AtomicBool>,
    clients: ConsoleClients,
    tx_sender: mpsc::Sender<UartTx>,
    handles: Vec<JoinHandle<()>>,
}

impl ConsoleServer {
    pub fn start(
        log_dir: &Path,
        config: ConsoleConfig,
        rx_handler: impl Fn(u32, u8) + Send + Sync + 'static,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(log_dir)
            .map_err(|e| format!("failed to create console log dir {}: {e}", log_dir.display()))?;
        if let Some(uart_log_dir) = config.uart_log_dir.as_ref() {
            std::fs::create_dir_all(uart_log_dir)
                .map_err(|e| format!("failed to create UART log dir {}: {e}", uart_log_dir.display()))?;
        }

        let path_file = log_dir.join("console.sock.path");
        let socket_path = std::env::temp_dir().join(format!("bebop-console-{}.sock", std::process::id()));
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)
                .map_err(|e| format!("failed to remove stale console socket {}: {e}", socket_path.display()))?;
        }

        let listener = UnixListener::bind(&socket_path)
            .map_err(|e| format!("failed to bind console socket {}: {e}", socket_path.display()))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("failed to set console socket nonblocking: {e}"))?;
        std::fs::write(&path_file, format!("{}\n", socket_path.display()))
            .map_err(|e| format!("failed to write console socket path file {}: {e}", path_file.display()))?;

        let stop = Arc::new(AtomicBool::new(false));
        let clients: ConsoleClients = Arc::new(Mutex::new(HashMap::new()));
        let (tx_sender, tx_receiver) = mpsc::channel::<UartTx>();
        let rx_log = config
            .rx_log_path
            .as_ref()
            .map(|path| File::create(path).map(BufWriter::new))
            .transpose()
            .map_err(|e| format!("failed to create console RX log: {e}"))?
            .map(|file| Arc::new(Mutex::new(file)));
        let rx_handler: RxHandler = Arc::new(rx_handler);

        let accept_stop = stop.clone();
        let accept_clients = clients.clone();
        let accept_rx_log = rx_log.clone();
        let accept_rx_handler = rx_handler.clone();
        let accept_thread_name = format!("{}-console-accept", config.thread_prefix);
        let accept_handle = std::thread::Builder::new()
            .name(accept_thread_name)
            .spawn(move || {
                while !accept_stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _addr)) => {
                            if let Err(e) = Self::spawn_client(
                                stream,
                                accept_clients.clone(),
                                accept_stop.clone(),
                                accept_rx_log.clone(),
                                accept_rx_handler.clone(),
                            ) {
                                eprintln!("failed to start console client: {e}");
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        Err(e) => {
                            eprintln!("console accept failed: {e}");
                            std::thread::sleep(Duration::from_millis(200));
                        }
                    }
                }
            })
            .map_err(|e| format!("failed to spawn console accept thread: {e}"))?;

        let tx_stop = stop.clone();
        let tx_clients = clients.clone();
        let tx_thread_name = format!("{}-console-tx", config.thread_prefix);
        let tx_handle = std::thread::Builder::new()
            .name(tx_thread_name)
            .spawn(move || {
                while !tx_stop.load(Ordering::Relaxed) {
                    match tx_receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(msg) => Self::broadcast_to_clients(&tx_clients, msg.hart_id, msg.byte),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|e| format!("failed to spawn console tx thread: {e}"))?;

        Ok(Self {
            socket_path,
            path_file,
            uart_log_dir: config.uart_log_dir,
            uart_logs: Mutex::new(HashMap::new()),
            display: config.display.map(BufWriter::new).map(Mutex::new),
            stop,
            clients,
            tx_sender,
            handles: vec![accept_handle, tx_handle],
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn uart_log_dir(&self) -> Option<&Path> {
        self.uart_log_dir.as_deref()
    }

    pub fn tx_sender(&self) -> mpsc::Sender<UartTx> {
        self.tx_sender.clone()
    }

    pub fn send_tx(&self, hart_id: u32, byte: u8) {
        self.write_uart_log(hart_id, byte);
        self.write_display(hart_id, byte);
        Self::broadcast_to_clients(&self.clients, hart_id, byte);
    }

    fn spawn_client(
        stream: UnixStream,
        clients: ConsoleClients,
        stop: Arc<AtomicBool>,
        rx_log: Option<Arc<Mutex<BufWriter<File>>>>,
        rx_handler: RxHandler,
    ) -> Result<(), String> {
        let mut reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|e| format!("failed to clone console stream for reader: {e}"))?,
        );
        let mut header = String::new();
        if reader
            .read_line(&mut header)
            .map_err(|e| format!("failed to read console handshake: {e}"))?
            == 0
        {
            return Err("console client closed before handshake".to_string());
        }

        let hart_id = parse_console_handshake(&header)?;
        let write_stream = stream
            .try_clone()
            .map_err(|e| format!("failed to clone console stream for writer: {e}"))?;
        clients.lock().unwrap().entry(hart_id).or_default().push(write_stream);
        Self::log_rx(&rx_log, format_args!("connect hart={hart_id}\n"));

        std::thread::Builder::new()
            .name(format!("console-rx-hart-{hart_id}"))
            .spawn(move || {
                let mut input = reader;
                let mut buf = [0_u8; 256];

                while !stop.load(Ordering::Relaxed) {
                    match input.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            Self::log_rx(&rx_log, format_args!("rx hart={hart_id} bytes={:?}\n", &buf[..n]));
                            for byte in &buf[..n] {
                                rx_handler(hart_id, *byte);
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| format!("failed to spawn console rx thread: {e}"))?;

        Ok(())
    }

    fn broadcast_to_clients(clients: &ConsoleClients, hart_id: u32, byte: u8) {
        let mut guard = clients.lock().unwrap();
        let Some(streams) = guard.get_mut(&hart_id) else {
            return;
        };

        streams.retain_mut(|stream| stream.write_all(&[byte]).and_then(|_| stream.flush()).is_ok());
    }

    fn write_uart_log(&self, hart_id: u32, byte: u8) {
        let Some(uart_log_dir) = self.uart_log_dir.as_ref() else {
            return;
        };

        let mut logs = self.uart_logs.lock().unwrap();
        let writer = logs.entry(hart_id).or_insert_with(|| {
            let path = uart_log_dir.join(format!("hart-{hart_id}.log"));
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .unwrap_or_else(|e| panic!("failed to open UART log {}: {e}", path.display()));
            BufWriter::new(file)
        });
        writer
            .write_all(&[byte])
            .unwrap_or_else(|e| panic!("failed to write UART log for hart {hart_id}: {e}"));
        writer
            .flush()
            .unwrap_or_else(|e| panic!("failed to flush UART log for hart {hart_id}: {e}"));
    }

    fn write_display(&self, hart_id: u32, byte: u8) {
        if hart_id != 0 {
            return;
        }

        let Some(display) = self.display.as_ref() else {
            return;
        };
        let mut display = display.lock().unwrap();
        display
            .write_all(&[byte])
            .unwrap_or_else(|e| panic!("failed to write hart 0 display output: {e}"));
        if byte == b'\n' {
            display
                .flush()
                .unwrap_or_else(|e| panic!("failed to flush hart 0 display output: {e}"));
        }
    }

    fn log_rx(log: &Option<Arc<Mutex<BufWriter<File>>>>, args: std::fmt::Arguments<'_>) {
        let Some(log) = log.as_ref() else {
            return;
        };
        let mut log = log.lock().unwrap();
        log.write_fmt(args)
            .unwrap_or_else(|e| panic!("failed to write console RX log: {e}"));
        log.flush()
            .unwrap_or_else(|e| panic!("failed to flush console RX log: {e}"));
    }
}

impl Drop for ConsoleServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file(&self.path_file);
    }
}

fn parse_console_handshake(line: &str) -> Result<u32, String> {
    let line = line.trim_end_matches(['\r', '\n']);
    let (cmd, value) = line
        .split_once(' ')
        .ok_or_else(|| "console handshake must be `hart <id>`".to_string())?;
    if cmd != "hart" {
        return Err(format!("console handshake command must be `hart`, got `{cmd}`"));
    }
    value
        .parse::<u32>()
        .map_err(|e| format!("invalid console hart id `{value}`: {e}"))
}
