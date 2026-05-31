use snafu::{FromString, Whatever};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{collections::HashMap, path::Path};

use crate::{mmio, sim, trace};

#[derive(Debug, Clone)]
pub struct VerilatorCli {
    pub elf: PathBuf,
    pub log_dir: PathBuf,
    pub fst_dir: PathBuf,
    pub wave: bool,
    pub itrace: bool,
    pub mtrace: bool,
    pub pmctrace: bool,
    pub ctrace: bool,
    pub banktrace: bool,
}

pub fn run(cli: VerilatorCli) -> Result<(), Whatever> {
    let config = VerilatorConfig::parse(cli)?;
    config.run()
}

#[derive(Debug, Clone)]
struct VerilatorConfig {
    elf: PathBuf,
    log: PathBuf,
    fst: PathBuf,
    wave: bool,
    stdout: Option<PathBuf>,
    stderr: Option<PathBuf>,
    trace_config: trace::TraceConfig,
    coverage: bool,
    mem_base: u64,
    mem_size: usize,
}

impl VerilatorConfig {
    fn parse(cli: VerilatorCli) -> Result<Self, Whatever> {
        // Check for coverage env var
        let coverage = std::env::var("BEBOP_VERILATOR_COVERAGE")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        // Default memory config (can be made configurable)
        let mem_base = 0x8000_0000;
        let mem_size = 256 * 1024 * 1024; // 256MB

        // Generate file paths from directories
        let log = cli.log_dir.join("bdb.ndjson");
        let stdout = Some(cli.log_dir.join("stdout.log"));
        let stderr = Some(cli.log_dir.join("stderr.log"));
        let fst = cli.fst_dir.join("waveform.fst");

        Ok(Self {
            elf: cli.elf,
            log,
            fst,
            wave: cli.wave,
            stdout,
            stderr,
            trace_config: trace::TraceConfig {
                itrace: cli.itrace,
                mtrace: cli.mtrace,
                pmctrace: cli.pmctrace,
                ctrace: cli.ctrace,
                banktrace: cli.banktrace,
            },
            coverage,
            mem_base,
            mem_size,
        })
    }

    fn run(self) -> Result<(), Whatever> {
        // Setup Ctrl-C handler
        sim::setup_ctrlc_handler();

        // Create fst directory if needed
        if self.wave {
            if let Some(parent) = self.fst.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| Whatever::without_source(format!("Failed to create fst directory: {}", e)))?;
            }
        }
        if let Some(parent) = self.log.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Whatever::without_source(format!("Failed to create log directory: {}", e)))?;
        }

        // Initialize trace logging
        trace::init_trace(&self.log, self.trace_config.clone())
            .map_err(|e| Whatever::without_source(format!("Failed to init trace: {}", e)))?;

        println!("NDJSON trace: {}", self.log.display());
        if let Some(ref stdout_path) = self.stdout {
            println!("Stdout log: {}", stdout_path.display());
        }
        if let Some(ref stderr_path) = self.stderr {
            println!("Stderr log: {}", stderr_path.display());
        }
        println!(
            "Trace enabled: [itrace={} mtrace={} pmctrace={} ctrace={} banktrace={}]",
            self.trace_config.itrace,
            self.trace_config.mtrace,
            self.trace_config.pmctrace,
            self.trace_config.ctrace,
            self.trace_config.banktrace,
        );

        // Initialize UART
        mmio::init_uart(self.stdout.as_deref())
            .map_err(|e| Whatever::without_source(format!("Failed to init UART: {}", e)))?;

        let console = ConsoleServer::start(
            self.stdout
                .as_ref()
                .and_then(|path| path.parent())
                .ok_or_else(|| Whatever::without_source("stdout path must have a parent directory".to_string()))?,
        )?;
        println!("Console socket: {}", console.socket_path.display());
        println!("UART logs: {}", console.uart_log_dir.display());

        let _stdout_guard = self
            .stdout
            .as_ref()
            .map(|path| FdRedirect::new(std::io::stdout().as_raw_fd(), path, "stdout"))
            .transpose()?;
        let _stderr_guard = self
            .stderr
            .as_ref()
            .map(|path| FdRedirect::new(std::io::stderr().as_raw_fd(), path, "stderr"))
            .transpose()?;

        // Create simulator with +elf= argument for BBSimDRAM
        let elf_arg = format!("+elf={}", self.elf.display());
        let mut simulator = sim::Simulator::new(self.wave.then_some(self.fst.as_path()), self.coverage, &[elf_arg])
            .map_err(|e| Whatever::without_source(format!("Failed to create simulator: {}", e)))?;

        // Run simulation
        simulator.run_batch(|| console.poll_tx());
        console.poll_tx();

        // Finalize
        simulator.finalize();

        drop(_stderr_guard);
        drop(_stdout_guard);
        if self.wave {
            println!("Waveform saved to: {}", self.fst.display());
        }

        // Run disassembler on stderr.log to generate disasm.log
        if let Some(ref stderr_path) = self.stderr {
            let disasm_path = stderr_path.with_file_name("disasm.log");
            let stdin_file = std::fs::File::open(stderr_path)
                .map_err(|e| Whatever::without_source(format!("Failed to open stderr.log: {}", e)))?;
            let stdout_file = std::fs::File::create(&disasm_path)
                .map_err(|e| Whatever::without_source(format!("Failed to create disasm.log: {}", e)))?;

            let reader = std::io::BufReader::new(stdin_file);
            let writer = std::io::BufWriter::new(stdout_file);

            if let Err(e) = bebop_dasm::process_dasm(reader, writer) {
                eprintln!("Warning: Failed to disassemble: {}", e);
            } else {
                println!("Disassembly saved to: {}", disasm_path.display());
            }
        }

        Ok(())
    }
}

struct FdRedirect {
    file: File,
    saved_fd: i32,
    target_fd: i32,
}

impl FdRedirect {
    fn new(target_fd: i32, path: &Path, name: &str) -> Result<Self, Whatever> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Whatever::without_source(format!("Failed to create log directory: {}", e)))?;
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|e| Whatever::without_source(format!("Failed to open {name} log: {e}")))?;

        let file_fd = file.as_raw_fd();

        // SAFETY: dup() saves the current fd and dup2() replaces it with the
        // opened log file. Both return -1 on error, which is checked here.
        let saved_fd = unsafe { libc::dup(target_fd) };
        if saved_fd < 0 {
            return Err(Whatever::without_source(format!("Failed to duplicate {name}")));
        }
        if unsafe { libc::dup2(file_fd, target_fd) } < 0 {
            unsafe {
                libc::close(saved_fd);
            }
            return Err(Whatever::without_source(format!("Failed to redirect {name}")));
        }

        Ok(Self {
            file,
            saved_fd,
            target_fd,
        })
    }
}

impl Drop for FdRedirect {
    fn drop(&mut self) {
        let _ = self.file.flush();
        // SAFETY: saved_fd was created by dup() in FdRedirect::new and remains
        // owned by this guard until it is restored and closed here.
        unsafe {
            libc::dup2(self.saved_fd, self.target_fd);
            libc::close(self.saved_fd);
        }
    }
}

struct ConsoleServer {
    socket_path: PathBuf,
    path_file: PathBuf,
    uart_log_dir: PathBuf,
    uart_logs: Mutex<HashMap<u32, BufWriter<File>>>,
    display: Mutex<BufWriter<File>>,
    stop: Arc<AtomicBool>,
    clients: Arc<Mutex<HashMap<u32, Vec<UnixStream>>>>,
    handles: Vec<std::thread::JoinHandle<()>>,
}

impl ConsoleServer {
    fn start(log_dir: &Path) -> Result<Self, Whatever> {
        std::fs::create_dir_all(log_dir)
            .map_err(|e| Whatever::without_source(format!("Failed to create log directory: {}", e)))?;
        let uart_log_dir = log_dir.join("uart");
        std::fs::create_dir_all(&uart_log_dir)
            .map_err(|e| Whatever::without_source(format!("Failed to create UART log directory: {}", e)))?;

        let path_file = log_dir.join("console.sock.path");
        let socket_path = std::env::temp_dir().join(format!("bebop-console-{}.sock", std::process::id()));
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)
                .map_err(|e| Whatever::without_source(format!("Failed to remove stale console socket: {}", e)))?;
        }

        let listener = UnixListener::bind(&socket_path)
            .map_err(|e| Whatever::without_source(format!("Failed to bind console socket: {}", e)))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| Whatever::without_source(format!("Failed to set console socket nonblocking: {}", e)))?;
        std::fs::write(&path_file, format!("{}\n", socket_path.display()))
            .map_err(|e| Whatever::without_source(format!("Failed to write console socket path file: {}", e)))?;

        // SAFETY: dup() returns a new fd for the current stdout. File takes
        // ownership of the duplicate so later stdout redirection does not affect
        // hart 0 display output.
        let display_fd = unsafe { libc::dup(std::io::stdout().as_raw_fd()) };
        if display_fd < 0 {
            return Err(Whatever::without_source(
                "Failed to duplicate display stdout".to_string(),
            ));
        }
        let display = unsafe { File::from_raw_fd(display_fd) };

        let stop = Arc::new(AtomicBool::new(false));
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let rx_log_path = log_dir.join("console-rx.log");
        let rx_log = Arc::new(Mutex::new(BufWriter::new(
            File::create(&rx_log_path)
                .map_err(|e| Whatever::without_source(format!("Failed to create console RX log: {}", e)))?,
        )));
        let accept_stop = stop.clone();
        let accept_clients = clients.clone();
        let accept_rx_log = rx_log.clone();
        let accept_handle = std::thread::Builder::new()
            .name("verilator-console-accept".to_string())
            .spawn(move || {
                while !accept_stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _addr)) => {
                            if let Err(e) = Self::spawn_client(
                                stream,
                                accept_clients.clone(),
                                accept_stop.clone(),
                                accept_rx_log.clone(),
                            ) {
                                eprintln!("console client error: {e}");
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
            .map_err(|e| Whatever::without_source(format!("Failed to spawn console accept thread: {}", e)))?;

        Ok(Self {
            socket_path,
            path_file,
            uart_log_dir,
            uart_logs: Mutex::new(HashMap::new()),
            display: Mutex::new(BufWriter::new(display)),
            stop,
            clients,
            handles: vec![accept_handle],
        })
    }

    fn spawn_client(
        stream: UnixStream,
        clients: Arc<Mutex<HashMap<u32, Vec<UnixStream>>>>,
        stop: Arc<AtomicBool>,
        rx_log: Arc<Mutex<BufWriter<File>>>,
    ) -> Result<(), String> {
        let mut reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|e| format!("failed to clone console stream for reader: {e}"))?,
        );
        let mut header = String::new();
        let n = reader
            .read_line(&mut header)
            .map_err(|e| format!("failed to read console handshake: {e}"))?;
        if n == 0 {
            return Err("console client closed before handshake".to_string());
        }

        let hart_id = parse_console_handshake(&header)?;
        let write_stream = stream
            .try_clone()
            .map_err(|e| format!("failed to clone console stream for writer: {e}"))?;
        clients.lock().unwrap().entry(hart_id).or_default().push(write_stream);
        Self::log_rx(&rx_log, format_args!("connect hart={hart_id}\n"));

        std::thread::Builder::new()
            .name(format!("verilator-console-rx-hart-{hart_id}"))
            .spawn(move || {
                let mut input = reader;
                let mut buf = [0_u8; 256];

                while !stop.load(Ordering::Relaxed) {
                    match input.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            Self::log_rx(&rx_log, format_args!("rx hart={hart_id} bytes={:?}\n", &buf[..n]));
                            for byte in &buf[..n] {
                                mmio::push_uart_rx(hart_id, *byte);
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

    fn log_rx(log: &Arc<Mutex<BufWriter<File>>>, args: std::fmt::Arguments<'_>) {
        let mut log = log.lock().unwrap();
        log.write_fmt(args).unwrap_or_else(|e| panic!("failed to write console RX log: {e}"));
        log.flush().unwrap_or_else(|e| panic!("failed to flush console RX log: {e}"));
    }

    fn poll_tx(&self) {
        let mut buf = [0_u32; 256];
        loop {
            let n = mmio::drain_uart_tx(&mut buf);
            if n == 0 {
                return;
            }

            for item in &buf[..n] {
                let hart_id = item >> 8;
                let byte = *item as u8;
                self.write_uart_log(hart_id, byte);
                self.write_display(hart_id, byte);
                let mut clients = self.clients.lock().unwrap();
                let Some(streams) = clients.get_mut(&hart_id) else {
                    continue;
                };
                streams.retain_mut(|stream| stream.write_all(&[byte]).and_then(|_| stream.flush()).is_ok());
            }
        }
    }

    fn write_uart_log(&self, hart_id: u32, byte: u8) {
        let mut logs = self.uart_logs.lock().unwrap();
        let writer = logs.entry(hart_id).or_insert_with(|| {
            let path = self.uart_log_dir.join(format!("hart-{hart_id}.log"));
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .unwrap_or_else(|e| panic!("failed to open UART log {}: {}", path.display(), e));
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

        let mut display = self.display.lock().unwrap();
        display
            .write_all(&[byte])
            .unwrap_or_else(|e| panic!("failed to write hart 0 display output: {e}"));
        if byte == b'\n' {
            display
                .flush()
                .unwrap_or_else(|e| panic!("failed to flush hart 0 display output: {e}"));
        }
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
