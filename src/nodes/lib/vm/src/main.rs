use bebop_vm::{connect, Connection, Message};
use clap::Parser;
use font8x8::UnicodeFonts;
use sdl2::{
    audio::{AudioCallback, AudioSpecDesired},
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod},
    pixels::{Color, PixelFormatEnum},
    rect::Rect,
};
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Parser)]
#[command(
    about = "Connect to a BEMU peripheral window",
    after_help = "F1: VGA/UART view\nF2: microphone on/off\nF3: set guest RTC from host time"
)]
struct Args {
    #[arg(long)]
    log_dir: PathBuf,
    #[arg(long, default_value_t = 0)]
    hart: u32,
}

struct Capture {
    sender: mpsc::SyncSender<Vec<i16>>,
    overflow: Arc<AtomicBool>,
}

impl AudioCallback for Capture {
    type Channel = i16;
    fn callback(&mut self, samples: &mut [i16]) {
        if self.sender.try_send(samples.to_vec()).is_err() {
            self.overflow.store(true, Ordering::Relaxed);
        }
    }
}

fn main() {
    if let Err(error) = run(Args::parse()) {
        eprintln!("VM: {error}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    let stream = connect(&args.log_dir.join("vm.sock")).map_err(|e| e.to_string())?;
    let mut connection = Connection::new(stream).map_err(|e| e.to_string())?;
    let mut pending = VecDeque::new();
    let (width, height, rate, harts) = loop {
        pending.extend(connection.poll().map_err(|e| e.to_string())?);
        if let Some(message) = pending.pop_front() {
            let Message::Hello {
                width,
                height,
                rate,
                harts,
            } = message
            else {
                return Err("expected VM hello".into());
            };
            break (width, height, rate, harts);
        }
        if connection.closed {
            return Err("simulation closed before VM hello".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if width != 640 || height != 480 || rate != 16_000 || args.hart >= harts {
        return Err("unsupported VM display/audio format or hart index".into());
    }
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Bebop VM", width, height + 24)
        .position_centered()
        .resizable()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas().software().build().map_err(|e| e.to_string())?;
    canvas.set_logical_size(width, height + 24).map_err(|e| e.to_string())?;
    let textures = canvas.texture_creator();
    let mut texture = textures
        .create_texture_streaming(PixelFormatEnum::ARGB8888, width, height)
        .map_err(|e| e.to_string())?;
    texture
        .update(None, &vec![0; width as usize * height as usize * 4], width as usize * 4)
        .map_err(|e| e.to_string())?;
    let audio = sdl.audio()?;
    let desired = AudioSpecDesired {
        freq: Some(rate as i32),
        channels: Some(1),
        samples: Some(512),
    };
    let playback = audio.open_queue::<i16, _>(None, &desired)?;
    playback.resume();
    let (sender, receiver) = mpsc::sync_channel(32);
    let overflow = Arc::new(AtomicBool::new(false));
    let capture = audio.open_capture(None, &desired, |_| Capture {
        sender,
        overflow: overflow.clone(),
    })?;
    let mut microphone = false;
    let mut console = false;
    let mut terminal = vt100::Parser::new(30, 80, 0);
    let mut keys = HashSet::new();
    let mut events = sdl.event_pump()?;
    let mut dirty = true;
    let mut clock = 0;
    let mut stopped = None;
    'window: loop {
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => break 'window,
                Event::KeyDown {
                    keycode: Some(Keycode::F1),
                    repeat: false,
                    ..
                } => {
                    for code in keys.drain() {
                        connection
                            .send(Message::Key { code, pressed: false })
                            .map_err(|e| e.to_string())?;
                    }
                    console = !console;
                    if console {
                        video.text_input().start();
                    } else {
                        video.text_input().stop();
                    }
                    dirty = true;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::F2),
                    repeat: false,
                    ..
                } if stopped.is_none() => {
                    microphone = !microphone;
                    if microphone {
                        capture.resume();
                    } else {
                        capture.pause();
                    }
                    dirty = true;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::F3),
                    repeat: false,
                    ..
                } if stopped.is_none() => {
                    let ns: u64 = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|e| e.to_string())?
                        .as_nanos()
                        .try_into()
                        .map_err(|_| "RTC time out of range")?;
                    connection.send(Message::SetClock(ns)).map_err(|e| e.to_string())?;
                }
                Event::KeyDown {
                    keycode: Some(key),
                    keymod,
                    repeat: false,
                    ..
                } if console && stopped.is_none() => {
                    if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) && (97..=122).contains(&key.into_i32()) {
                        connection
                            .send(Message::Serial {
                                hart: args.hart,
                                bytes: vec![(key.into_i32() - 96) as u8],
                            })
                            .map_err(|e| e.to_string())?;
                        continue;
                    }
                    let bytes = match key {
                        Keycode::Return => b"\n".as_slice(),
                        Keycode::Backspace => b"\x7f".as_slice(),
                        Keycode::Tab => b"\t".as_slice(),
                        Keycode::Escape => b"\x1b".as_slice(),
                        Keycode::Up => b"\x1b[A".as_slice(),
                        Keycode::Down => b"\x1b[B".as_slice(),
                        Keycode::Right => b"\x1b[C".as_slice(),
                        Keycode::Left => b"\x1b[D".as_slice(),
                        _ => continue,
                    };
                    connection
                        .send(Message::Serial {
                            hart: args.hart,
                            bytes: bytes.to_vec(),
                        })
                        .map_err(|e| e.to_string())?;
                }
                Event::TextInput { text, .. } if console && stopped.is_none() => {
                    connection
                        .send(Message::Serial {
                            hart: args.hart,
                            bytes: text.into_bytes(),
                        })
                        .map_err(|e| e.to_string())?;
                }
                Event::KeyDown {
                    scancode: Some(code),
                    repeat: false,
                    ..
                } if !console && stopped.is_none() => {
                    let code = code as u16;
                    keys.insert(code);
                    connection
                        .send(Message::Key { code, pressed: true })
                        .map_err(|e| e.to_string())?;
                }
                Event::KeyUp {
                    scancode: Some(code), ..
                } => {
                    let code = code as u16;
                    if keys.remove(&code) {
                        connection
                            .send(Message::Key { code, pressed: false })
                            .map_err(|e| e.to_string())?;
                    }
                }
                Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => {
                    for code in keys.drain() {
                        connection
                            .send(Message::Key { code, pressed: false })
                            .map_err(|e| e.to_string())?;
                    }
                }
                Event::Window {
                    win_event: WindowEvent::Exposed | WindowEvent::Resized(..),
                    ..
                } => dirty = true,
                _ => {}
            }
        }
        if overflow.load(Ordering::Relaxed) {
            return Err("microphone capture queue full".into());
        }
        for samples in receiver.try_iter() {
            connection
                .send(Message::Microphone(samples))
                .map_err(|e| e.to_string())?;
        }
        if !connection.closed {
            pending.extend(connection.poll().map_err(|e| e.to_string())?);
        }
        for message in pending.drain(..) {
            match message {
                Message::Frame(pixels) => {
                    if pixels.len() != width as usize * height as usize * 4 {
                        return Err("invalid VGA frame size".into());
                    }
                    texture
                        .update(None, &pixels, width as usize * 4)
                        .map_err(|e| e.to_string())?;
                    dirty = true;
                }
                Message::Audio(samples) => {
                    if playback.size() as usize + samples.len() * 2 > rate as usize * 2 {
                        return Err(
                            "speaker queue exceeds one second; simulation audio is faster than playback".into(),
                        );
                    }
                    playback.queue_audio(&samples)?;
                }
                Message::Clock(ns) => {
                    clock = ns / 1_000_000_000;
                    dirty = true;
                }
                Message::Uart { hart, bytes } => {
                    if hart == args.hart {
                        terminal.process(&bytes);
                        dirty = true;
                    }
                }
                Message::Exit(code) => {
                    keys.clear();
                    stopped = Some(code);
                    dirty = true;
                    capture.pause();
                    microphone = false;
                }
                _ => return Err("unexpected message from simulation".into()),
            }
        }
        if connection.closed && stopped.is_none() {
            return Err("simulation disconnected without an exit status".into());
        }
        if dirty {
            let state = stopped.map_or("running".to_string(), |code| format!("stopped ({code})"));
            canvas
                .window_mut()
                .set_title(&format!(
                    "Bebop VM | {} | hart {} | mic {} | RTC {:02}:{:02}:{:02} UTC | {}",
                    if console { "UART" } else { "VGA" },
                    args.hart,
                    if microphone { "on" } else { "off" },
                    clock / 3600 % 24,
                    clock / 60 % 60,
                    clock % 60,
                    state
                ))
                .map_err(|e| e.to_string())?;
            canvas.set_draw_color(Color::BLACK);
            canvas.clear();
            if console {
                canvas.set_draw_color(Color::RGB(220, 230, 220));
                for row in 0..30 {
                    for col in 0..80 {
                        let cell = terminal.screen().cell(row, col).unwrap();
                        for ch in cell.contents().chars() {
                            if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
                                for (y, bits) in glyph.iter().enumerate() {
                                    for x in 0..8 {
                                        if bits & (1 << x) != 0 {
                                            canvas.fill_rect(Rect::new(
                                                col as i32 * 8 + x,
                                                row as i32 * 16 + y as i32 * 2,
                                                1,
                                                2,
                                            ))?;
                                        }
                                    }
                                }
                            } else {
                                canvas.draw_rect(Rect::new(col as i32 * 8 + 1, row as i32 * 16 + 2, 6, 12))?;
                            }
                        }
                    }
                }
                if !terminal.screen().hide_cursor() {
                    let (row, col) = terminal.screen().cursor_position();
                    canvas.fill_rect(Rect::new(col as i32 * 8, row as i32 * 16 + 14, 8, 2))?;
                }
            } else {
                canvas.copy(&texture, None, Some(Rect::new(0, 0, width, height)))?;
            }
            canvas.set_draw_color(Color::RGB(35, 40, 48));
            canvas.fill_rect(Rect::new(0, height as i32, width, 24))?;
            canvas.set_draw_color(Color::RGB(230, 235, 240));
            let status = format!(
                "F1 VGA/UART   F2 Mic: {}   F3 Sync RTC   {:02}:{:02}:{:02} UTC",
                if microphone { "on " } else { "off" },
                clock / 3600 % 24,
                clock / 60 % 60,
                clock % 60
            );
            for (col, ch) in status.chars().enumerate() {
                let glyph = font8x8::BASIC_FONTS.get(ch).unwrap();
                for (y, bits) in glyph.iter().enumerate() {
                    for x in 0..8 {
                        if bits & (1 << x) != 0 {
                            canvas.fill_rect(Rect::new(8 + col as i32 * 8 + x, height as i32 + 8 + y as i32, 1, 1))?;
                        }
                    }
                }
            }
            canvas.present();
            dirty = false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    capture.pause();
    if !connection.closed {
        for code in keys {
            connection
                .send(Message::Key { code, pressed: false })
                .map_err(|e| e.to_string())?;
        }
        connection.poll().map_err(|e| e.to_string())?;
    }
    Ok(())
}
