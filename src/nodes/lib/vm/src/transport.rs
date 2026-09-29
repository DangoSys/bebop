use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

const LIMIT: usize = 8 * 1024 * 1024;

pub enum Message {
    Hello {
        width: u32,
        height: u32,
        rate: u32,
        harts: u32,
    },
    Frame(Vec<u8>),
    Audio(Vec<i16>),
    Clock(u64),
    Uart {
        hart: u32,
        bytes: Vec<u8>,
    },
    Key {
        code: u16,
        pressed: bool,
    },
    Microphone(Vec<i16>),
    Serial {
        hart: u32,
        bytes: Vec<u8>,
    },
    SetClock(u64),
    Exit(i32),
}

pub fn connect(path: &Path) -> io::Result<UnixStream> {
    let directory = File::open(path.parent().expect("VM socket parent"))?;
    let socket = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
        .join(path.file_name().expect("VM socket name"));
    UnixStream::connect(socket)
}

pub struct Connection {
    stream: UnixStream,
    input: Vec<u8>,
    output: VecDeque<Vec<u8>>,
    written: usize,
    queued: usize,
    pub closed: bool,
}

impl Connection {
    pub fn new(stream: UnixStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            input: Vec::new(),
            output: VecDeque::new(),
            written: 0,
            queued: 0,
            closed: false,
        })
    }

    pub fn send(&mut self, message: Message) -> io::Result<()> {
        let frame = matches!(&message, Message::Frame(_));
        let mut payload = Vec::new();
        let kind = match message {
            Message::Hello {
                width,
                height,
                rate,
                harts,
            } => {
                for value in [width, height, rate, harts] {
                    payload.extend(value.to_le_bytes());
                }
                1
            }
            Message::Frame(bytes) => {
                payload = bytes;
                2
            }
            Message::Audio(samples) => {
                for sample in samples {
                    payload.extend(sample.to_le_bytes());
                }
                3
            }
            Message::Microphone(samples) => {
                for sample in samples {
                    payload.extend(sample.to_le_bytes());
                }
                7
            }
            Message::Clock(value) => {
                payload.extend(value.to_le_bytes());
                4
            }
            Message::Uart { hart, bytes } => {
                payload.extend(hart.to_le_bytes());
                payload.extend(bytes);
                5
            }
            Message::Key { code, pressed } => {
                payload.extend(code.to_le_bytes());
                payload.push(pressed.into());
                6
            }
            Message::Serial { hart, bytes } => {
                payload.extend(hart.to_le_bytes());
                payload.extend(bytes);
                8
            }
            Message::SetClock(value) => {
                payload.extend(value.to_le_bytes());
                9
            }
            Message::Exit(value) => {
                payload.extend(value.to_le_bytes());
                10
            }
        };
        // The display needs the latest frame, not a backlog of old refreshes.
        if frame {
            if let Some(index) = self
                .output
                .iter()
                .enumerate()
                .position(|(index, packet)| packet[0] == 2 && (index != 0 || self.written == 0))
            {
                self.queued -= self.output.remove(index).unwrap().len();
            }
        }
        if self.queued + payload.len() + 5 > LIMIT {
            return Err(io::Error::other("VM output queue full; frontend cannot keep up"));
        }
        let mut packet = vec![kind];
        packet.extend((payload.len() as u32).to_le_bytes());
        packet.extend(payload);
        self.queued += packet.len();
        self.output.push_back(packet);
        Ok(())
    }

    pub fn poll(&mut self) -> io::Result<Vec<Message>> {
        while let Some(packet) = self.output.front() {
            match self.stream.write(&packet[self.written..]) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
                Ok(n) => {
                    self.written += n;
                    self.queued -= n;
                    if self.written == packet.len() {
                        self.output.pop_front();
                        self.written = 0;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if matches!(e.kind(), io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset) => {
                    self.closed = true;
                    self.output.clear();
                    self.queued = 0;
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        if self.closed {
            return Ok(Vec::new());
        }
        let mut buffer = [0; 65536];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) => {
                    self.closed = true;
                    break;
                }
                Ok(n) => {
                    self.input.extend_from_slice(&buffer[..n]);
                    if self.input.len() > LIMIT {
                        return Err(io::Error::other("VM input queue full"));
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::ConnectionReset => {
                    self.closed = true;
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        let mut messages = Vec::new();
        let mut consumed = 0;
        while self.input.len() - consumed >= 5 {
            let header = &self.input[consumed..];
            let length = u32::from_le_bytes(header[1..5].try_into().unwrap()) as usize;
            if length > LIMIT - 5 {
                return Err(io::Error::other("VM packet too large"));
            }
            if header.len() < length + 5 {
                break;
            }
            let data = &header[5..5 + length];
            let message = match (header[0], length) {
                (1, 16) => Message::Hello {
                    width: u32::from_le_bytes(data[0..4].try_into().unwrap()),
                    height: u32::from_le_bytes(data[4..8].try_into().unwrap()),
                    rate: u32::from_le_bytes(data[8..12].try_into().unwrap()),
                    harts: u32::from_le_bytes(data[12..16].try_into().unwrap()),
                },
                (2, _) => Message::Frame(data.to_vec()),
                (3, n) if n % 2 == 0 => Message::Audio(
                    data.chunks_exact(2)
                        .map(|v| i16::from_le_bytes(v.try_into().unwrap()))
                        .collect(),
                ),
                (4, 8) => Message::Clock(u64::from_le_bytes(data.try_into().unwrap())),
                (5, n) if n >= 4 => Message::Uart {
                    hart: u32::from_le_bytes(data[..4].try_into().unwrap()),
                    bytes: data[4..].to_vec(),
                },
                (6, 3) if data[2] <= 1 => Message::Key {
                    code: u16::from_le_bytes(data[..2].try_into().unwrap()),
                    pressed: data[2] != 0,
                },
                (7, n) if n % 2 == 0 => Message::Microphone(
                    data.chunks_exact(2)
                        .map(|v| i16::from_le_bytes(v.try_into().unwrap()))
                        .collect(),
                ),
                (8, n) if n >= 4 => Message::Serial {
                    hart: u32::from_le_bytes(data[..4].try_into().unwrap()),
                    bytes: data[4..].to_vec(),
                },
                (9, 8) => Message::SetClock(u64::from_le_bytes(data.try_into().unwrap())),
                (10, 4) => Message::Exit(i32::from_le_bytes(data.try_into().unwrap())),
                _ => return Err(io::Error::other("invalid VM packet")),
            };
            messages.push(message);
            consumed += length + 5;
        }
        self.input.drain(..consumed);
        if self.closed && !self.input.is_empty() {
            return Err(io::Error::other("truncated VM packet"));
        }
        Ok(messages)
    }
}

pub struct Server {
    listener: UnixListener,
    path: PathBuf,
    connection: Option<Connection>,
    frame: Vec<u8>,
    width: u32,
    height: u32,
    harts: u32,
    clock: u64,
    keys: BTreeSet<u16>,
    uart: BTreeMap<u32, VecDeque<u8>>,
}

impl Server {
    pub fn bind(path: &Path, width: u32, height: u32, harts: u32) -> io::Result<Self> {
        let directory = File::open(path.parent().expect("VM socket parent"))?;
        let socket = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
            .join(path.file_name().expect("VM socket name"));
        let listener = UnixListener::bind(socket)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            path: path.into(),
            connection: None,
            frame: vec![0; width as usize * height as usize * 4],
            width,
            height,
            harts,
            clock: 0,
            keys: BTreeSet::new(),
            uart: BTreeMap::new(),
        })
    }

    pub fn publish(&mut self, message: Message) -> io::Result<()> {
        if let Message::Frame(ref pixels) = message {
            self.frame.clone_from(pixels);
        }
        if let Message::Clock(value) = message {
            self.clock = value;
        }
        if let Message::Uart { hart, ref bytes } = message {
            let history = self.uart.entry(hart).or_default();
            history.extend(bytes);
            if history.len() > 65536 {
                history.drain(..history.len() - 65536);
            }
        }
        if let Some(connection) = &mut self.connection {
            connection.send(message)?;
        }
        Ok(())
    }

    pub fn poll(&mut self) -> io::Result<Vec<Message>> {
        if self.connection.is_none() {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    let mut connection = Connection::new(stream)?;
                    connection.send(Message::Hello {
                        width: self.width,
                        height: self.height,
                        rate: 16_000,
                        harts: self.harts,
                    })?;
                    connection.send(Message::Frame(self.frame.clone()))?;
                    connection.send(Message::Clock(self.clock))?;
                    for (&hart, bytes) in &self.uart {
                        connection.send(Message::Uart {
                            hart,
                            bytes: bytes.iter().copied().collect(),
                        })?;
                    }
                    self.connection = Some(connection);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(Vec::new()),
                Err(e) => return Err(e),
            }
        }
        let connection = self.connection.as_mut().unwrap();
        let mut messages = connection.poll()?;
        for message in &messages {
            if let Message::Key { code, pressed } = message {
                if *pressed {
                    self.keys.insert(*code);
                } else {
                    self.keys.remove(code);
                }
            }
        }
        if connection.closed {
            self.connection = None;
            messages.extend(
                std::mem::take(&mut self.keys)
                    .into_iter()
                    .map(|code| Message::Key { code, pressed: false }),
            );
        }
        Ok(messages)
    }

    pub fn finish(&mut self, code: i32) -> io::Result<()> {
        self.publish(Message::Exit(code))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while self.connection.as_ref().is_some_and(|c| c.queued != 0) {
            self.poll()?;
            if std::time::Instant::now() >= deadline {
                return Err(io::Error::other("VM did not drain final output"));
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            eprintln!("remove VM socket: {e}");
        }
    }
}
