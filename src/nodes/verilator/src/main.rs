use snafu::{FromString, Whatever};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
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
    stdout: Option<PathBuf>,
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
        let fst = cli.fst_dir.join("waveform.fst");

        Ok(Self {
            elf: cli.elf,
            log,
            fst,
            stdout,
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
        if let Some(parent) = self.fst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Whatever::without_source(format!("Failed to create fst directory: {}", e)))?;
        }

        // Redirect stderr to stdout.log if specified
        let _stderr_guard = if let Some(ref stdout_path) = self.stdout {
            // Create parent directory if needed
            if let Some(parent) = stdout_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| Whatever::without_source(format!("Failed to create log directory: {}", e)))?;
            }

            // Open the stdout log file
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(stdout_path)
                .map_err(|e| Whatever::without_source(format!("Failed to open stdout log: {}", e)))?;

            // Redirect stderr to the file
            let stderr_fd = std::io::stderr().as_raw_fd();
            let file_fd = file.as_raw_fd();

            // SAFETY: libc FFI for file descriptor manipulation. dup() duplicates stderr fd
            // for later restoration; dup2() redirects stderr to the log file. Both return -1
            // on error which we check. old_stderr is closed in the restoration block below.
            unsafe {
                let old_stderr = libc::dup(stderr_fd);
                if old_stderr < 0 {
                    return Err(Whatever::without_source("Failed to duplicate stderr".to_string()));
                }
                if libc::dup2(file_fd, stderr_fd) < 0 {
                    libc::close(old_stderr);
                    return Err(Whatever::without_source("Failed to redirect stderr".to_string()));
                }

                // Return a guard that will restore stderr on drop
                Some((file, old_stderr))
            }
        } else {
            None
        };

        // Initialize trace logging
        trace::init_trace(&self.log, self.trace_config.clone())
            .map_err(|e| Whatever::without_source(format!("Failed to init trace: {}", e)))?;

        println!("NDJSON trace: {}", self.log.display());
        if let Some(ref stdout_path) = self.stdout {
            println!("Stdout log: {}", stdout_path.display());
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

        // Create simulator with +elf= argument for BBSimDRAM
        let elf_arg = format!("+elf={}", self.elf.display());
        let mut simulator = sim::Simulator::new(&self.fst, self.coverage, &[elf_arg])
            .map_err(|e| Whatever::without_source(format!("Failed to create simulator: {}", e)))?;

        // Run simulation
        simulator.run_batch(|| console.poll_tx());

        // Finalize
        simulator.finalize();
        println!("Waveform saved to: {}", self.fst.display());

        // Restore stderr if it was redirected
        if let Some((_, old_stderr)) = _stderr_guard {
            // SAFETY: libc FFI to restore stderr from the duplicated fd saved earlier.
            // old_stderr is valid (created by dup() above) and closed after restoration.
            unsafe {
                libc::dup2(old_stderr, std::io::stderr().as_raw_fd());
                libc::close(old_stderr);
            }
        }

        // Run disassembler on stdout.log to generate disasm.log
        if let Some(ref stdout_path) = self.stdout {
            let disasm_path = stdout_path.with_file_name("disasm.log");
            let stdin_file = std::fs::File::open(stdout_path)
                .map_err(|e| Whatever::without_source(format!("Failed to open stdout.log: {}", e)))?;
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

struct ConsoleServer {
    socket_path: PathBuf,
    path_file: PathBuf,
    stop: Arc<AtomicBool>,
    clients: Arc<Mutex<HashMap<u32, Vec<UnixStream>>>>,
    handles: Vec<std::thread::JoinHandle<()>>,
}

impl ConsoleServer {
    fn start(log_dir: &Path) -> Result<Self, Whatever> {
        std::fs::create_dir_all(log_dir)
            .map_err(|e| Whatever::without_source(format!("Failed to create log directory: {}", e)))?;

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

        let stop = Arc::new(AtomicBool::new(false));
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let accept_stop = stop.clone();
        let accept_clients = clients.clone();
        let accept_handle = std::thread::Builder::new()
            .name("verilator-console-accept".to_string())
            .spawn(move || {
                while !accept_stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _addr)) => {
                            if let Err(e) = Self::spawn_client(stream, accept_clients.clone(), accept_stop.clone()) {
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
            stop,
            clients,
            handles: vec![accept_handle],
        })
    }

    fn spawn_client(
        stream: UnixStream,
        clients: Arc<Mutex<HashMap<u32, Vec<UnixStream>>>>,
        stop: Arc<AtomicBool>,
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

        std::thread::Builder::new()
            .name(format!("verilator-console-rx-hart-{hart_id}"))
            .spawn(move || {
                let mut input = reader;
                let mut buf = [0_u8; 256];

                while !stop.load(Ordering::Relaxed) {
                    match input.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
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

    fn poll_tx(&self) {
        let mut buf = [0_u32; 256];
        let n = mmio::drain_uart_tx(&mut buf);
        if n == 0 {
            return;
        }

        let mut clients = self.clients.lock().unwrap();
        for item in &buf[..n] {
            let hart_id = item >> 8;
            let byte = *item as u8;
            let Some(streams) = clients.get_mut(&hart_id) else {
                continue;
            };
            streams.retain_mut(|stream| stream.write_all(&[byte]).and_then(|_| stream.flush()).is_ok());
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
