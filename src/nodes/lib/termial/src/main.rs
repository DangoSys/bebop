use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug)]
struct Cli {
    socket: Option<PathBuf>,
    log_dir: Option<PathBuf>,
    hart: u32,
}

fn main() {
    if let Err(e) = parse_args().and_then(run) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn parse_args() -> Result<Cli, String> {
    let mut args = std::env::args().skip(1);
    let mut socket = None;
    let mut log_dir = None;
    let mut hart = 0;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--socket" => {
                let value = args.next().ok_or_else(|| "--socket requires a path".to_string())?;
                socket = Some(PathBuf::from(value));
            }
            "--log-dir" => {
                let value = args.next().ok_or_else(|| "--log-dir requires a path".to_string())?;
                log_dir = Some(PathBuf::from(value));
            }
            "--hart" => {
                let value = args.next().ok_or_else(|| "--hart requires an integer".to_string())?;
                hart = value
                    .parse::<u32>()
                    .map_err(|e| format!("invalid --hart value `{value}`: {e}"))?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument `{arg}`")),
        }
    }

    Ok(Cli { socket, log_dir, hart })
}

fn print_help() {
    println!("Usage: bebop-termial (--socket <path> | --log-dir <dir>) [--hart <id>]");
}

fn run(cli: Cli) -> Result<(), String> {
    let socket = match (cli.socket, cli.log_dir) {
        (Some(socket), None) => socket,
        (None, Some(log_dir)) => resolve_log_dir_socket(&log_dir)?,
        (None, None) => {
            return Err("one of --socket or --log-dir is required".to_string());
        }
        (Some(_), Some(_)) => {
            return Err("--socket and --log-dir are mutually exclusive".to_string());
        }
    };

    let mut stream =
        UnixStream::connect(&socket).map_err(|e| format!("failed to connect {}: {e}", socket.display()))?;
    stream
        .write_all(format!("hart {}\n", cli.hart).as_bytes())
        .map_err(|e| format!("failed to write handshake: {e}"))?;

    let term = RawTerm::enter()?;
    let stop = Arc::new(AtomicBool::new(false));

    let mut rx = stream
        .try_clone()
        .map_err(|e| format!("failed to clone socket for rx: {e}"))?;
    let rx_stop = stop.clone();
    let rx_handle = std::thread::Builder::new()
        .name("bebop-termial-rx".to_string())
        .spawn(move || {
            let mut stdout = std::io::stdout();
            let mut buf = [0_u8; 1024];
            while !rx_stop.load(Ordering::Relaxed) {
                match rx.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if stdout.write_all(&buf[..n]).and_then(|_| stdout.flush()).is_err() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        })
        .map_err(|e| format!("failed to spawn rx thread: {e}"))?;

    let mut stdin = std::io::stdin();
    let mut buf = [0_u8; 256];
    loop {
        match stdin.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Some(pos) = buf[..n].iter().position(|byte| *byte == 0x1d) {
                    if pos > 0 {
                        write_input(&mut stream, &buf[..pos])?;
                    }
                    break;
                }
                write_input(&mut stream, &buf[..n])?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("failed to read stdin: {e}")),
        }
    }

    stop.store(true, Ordering::Relaxed);
    drop(stream);
    let _ = rx_handle.join();
    drop(term);
    Ok(())
}

fn resolve_log_dir_socket(log_dir: &std::path::Path) -> Result<PathBuf, String> {
    let path_file = log_dir.join("console.sock.path");
    let path = std::fs::read_to_string(&path_file)
        .map_err(|e| format!("failed to read console socket path {}: {e}", path_file.display()))?;
    let path = path.trim_end_matches(['\r', '\n']);
    if path.is_empty() {
        return Err(format!("console socket path file {} is empty", path_file.display()));
    }
    Ok(PathBuf::from(path))
}

fn write_input(stream: &mut UnixStream, buf: &[u8]) -> Result<(), String> {
    for byte in buf {
        let out = if *byte == b'\r' { b'\n' } else { *byte };
        stream
            .write_all(&[out])
            .map_err(|e| format!("failed to write input to console: {e}"))?;
    }
    Ok(())
}

struct RawTerm {
    fd: libc::c_int,
    original: libc::termios,
}

impl RawTerm {
    fn enter() -> Result<Self, String> {
        let fd = libc::STDIN_FILENO;
        let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
        if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
            return Err(format!(
                "failed to read terminal attributes: {}",
                std::io::Error::last_os_error()
            ));
        }
        let original = unsafe { original.assume_init() };
        let mut raw = original;
        raw.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
        raw.c_oflag &= !libc::OPOST;
        raw.c_cflag |= libc::CS8;
        raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;

        if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &raw) } != 0 {
            return Err(format!(
                "failed to set raw terminal mode: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(Self { fd, original })
    }
}

impl Drop for RawTerm {
    fn drop(&mut self) {
        let _ = unsafe { libc::tcsetattr(self.fd, libc::TCSAFLUSH, &self.original) };
    }
}
