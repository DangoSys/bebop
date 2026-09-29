use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::Path;
use std::thread::JoinHandle;

pub struct FdRedirect {
    file: Option<File>,
    saved_fd: i32,
    target_fd: i32,
    handle: Option<JoinHandle<io::Result<()>>>,
}

impl FdRedirect {
    pub fn new(target_fd: i32, path: &Path, name: &str) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
        let saved_fd = dup_fd(target_fd, name)?;
        if unsafe { libc::dup2(file.as_raw_fd(), target_fd) } < 0 {
            unsafe {
                libc::close(saved_fd);
            }
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            file: Some(file),
            saved_fd,
            target_fd,
            handle: None,
        })
    }

    pub fn new_tee(target_fd: i32, path: &Path, name: &str) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut log = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
        let saved_fd = dup_fd(target_fd, name)?;
        let display_fd = dup_fd(saved_fd, &format!("{name} display"))?;

        let mut pipe_fds = [0; 2];
        if unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } < 0 {
            unsafe {
                libc::close(display_fd);
                libc::close(saved_fd);
            }
            return Err(io::Error::last_os_error());
        }

        if unsafe { libc::dup2(pipe_fds[1], target_fd) } < 0 {
            unsafe {
                libc::close(pipe_fds[0]);
                libc::close(pipe_fds[1]);
                libc::close(display_fd);
                libc::close(saved_fd);
            }
            return Err(io::Error::last_os_error());
        }
        unsafe {
            libc::close(pipe_fds[1]);
        }

        let mut input = unsafe { File::from_raw_fd(pipe_fds[0]) };
        let mut display = unsafe { File::from_raw_fd(display_fd) };
        let tee_name = name.to_string();
        let handle = std::thread::Builder::new()
            .name(format!("{tee_name}-tee"))
            .spawn(move || {
                let mut buf = [0_u8; 8192];
                loop {
                    let n = input.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    log.write_all(&buf[..n])?;
                    display.write_all(&buf[..n])?;
                    log.flush()?;
                    display.flush()?;
                }
                Ok(())
            })
            .map_err(io::Error::other)?;

        Ok(Self {
            file: None,
            saved_fd,
            target_fd,
            handle: Some(handle),
        })
    }
}

impl Drop for FdRedirect {
    fn drop(&mut self) {
        if self.target_fd == std::io::stdout().as_raw_fd() {
            let _ = std::io::stdout().flush();
        } else if self.target_fd == std::io::stderr().as_raw_fd() {
            let _ = std::io::stderr().flush();
        }
        if let Some(file) = self.file.as_mut() {
            let _ = file.flush();
        }
        unsafe {
            libc::dup2(self.saved_fd, self.target_fd);
            libc::close(self.saved_fd);
        }
        if let Some(handle) = self.handle.take() {
            if let Ok(Err(e)) = handle.join() {
                eprintln!("{e}");
            }
        }
    }
}

pub fn dup_fd(fd: i32, name: &str) -> io::Result<i32> {
    let duplicated = unsafe { libc::dup(fd) };
    if duplicated < 0 {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("failed to duplicate {name} fd: {}", io::Error::last_os_error()),
        ))
    } else {
        Ok(duplicated)
    }
}
