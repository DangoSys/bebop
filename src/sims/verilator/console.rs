use bebop_fd_redirect::dup_fd;
use bebop_uart::{ConsoleConfig, ConsoleServer as UartConsoleServer};
use bebop_verilator::{drain_uart_tx, push_uart_rx};
use snafu::{FromString, ResultExt, Whatever};
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};

pub(super) struct ConsoleServer {
    uart_log_dir: PathBuf,
    inner: UartConsoleServer,
}

impl ConsoleServer {
    pub(super) fn start(log_dir: &Path) -> Result<Self, Whatever> {
        let uart_log_dir = log_dir.join("uart");
        std::fs::create_dir_all(&uart_log_dir)
            .map_err(|e| Whatever::without_source(format!("failed to create UART log dir: {e}")))?;

        let display_fd =
            dup_fd(std::io::stdout().as_raw_fd(), "display stdout").whatever_context("failed to duplicate stdout")?;
        let display = unsafe { File::from_raw_fd(display_fd) };
        let mut config = ConsoleConfig::new("verilator");
        config.uart_log_dir = Some(uart_log_dir.clone());
        config.rx_log_path = Some(log_dir.join("console-rx.log"));
        config.display = Some(display);
        let inner = UartConsoleServer::start(log_dir, config, push_uart_rx)
            .map_err(|e| Whatever::without_source(format!("failed to start Verilator console: {e}")))?;

        Ok(Self { uart_log_dir, inner })
    }

    pub(super) fn socket_path(&self) -> &Path {
        self.inner.socket_path()
    }

    pub(super) fn uart_log_dir(&self) -> &Path {
        &self.uart_log_dir
    }

    pub(super) fn poll_tx(&self) {
        let mut buf = [0_u32; 256];
        loop {
            let n = drain_uart_tx(&mut buf);
            if n == 0 {
                return;
            }

            for item in &buf[..n] {
                let hart_id = item >> 8;
                let byte = *item as u8;
                self.inner.send_tx(hart_id, byte);
            }
        }
    }
}
