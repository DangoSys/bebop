use super::chip::Chip;
use bebop_vm::{Message, Server};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(crate) fn serve(
    chips: &[Chip],
    io_chip: usize,
    directory: &Path,
    finished: &AtomicBool,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let width = bebop_vga::WIDTH as u32;
    let height = bebop_vga::HEIGHT as u32;
    let harts: usize = chips.iter().map(Chip::hart_count).sum();
    let mut server =
        Server::bind(&directory.join("vm.sock"), width, height, harts as u32).map_err(|e| e.to_string())?;
    let io = &chips[io_chip];
    server.publish(Message::Clock(io.clock())).map_err(|e| e.to_string())?;
    let mut audio = BufWriter::new(File::create(directory.join("audio.pcm")).map_err(|e| e.to_string())?);
    let mut last_clock = Instant::now();
    loop {
        for message in server.poll().map_err(|e| e.to_string())? {
            match message {
                Message::Key { code, pressed } => io.push_key(code, pressed),
                Message::Microphone(samples) => io.push_live_microphone(samples)?,
                Message::SetClock(ns) => {
                    for chip in chips {
                        chip.set_clock(ns);
                    }
                }
                Message::Serial { hart, bytes } => {
                    let mut local = hart as usize;
                    let mut target = None;
                    for chip in chips {
                        if local < chip.hart_count() {
                            target = Some(chip);
                            break;
                        }
                        local -= chip.hart_count();
                    }
                    let chip = target.ok_or("VM UART hart is outside the system")?;
                    for byte in bytes {
                        chip.push_uart(local, byte);
                    }
                }
                _ => return Err("invalid VM request".into()),
            }
        }
        let samples = io.take_audio();
        if !samples.is_empty() {
            for sample in &samples {
                audio.write_all(&sample.to_le_bytes()).map_err(|e| e.to_string())?;
            }
            server.publish(Message::Audio(samples)).map_err(|e| e.to_string())?;
        }
        if let Some(frame) = io.take_frame() {
            let mut image = BufWriter::new(File::create(directory.join("vga.ppm")).map_err(|e| e.to_string())?);
            write!(image, "P6\n{width} {height}\n255\n").map_err(|e| e.to_string())?;
            for pixel in frame.chunks_exact(4) {
                image
                    .write_all(&[pixel[2], pixel[1], pixel[0]])
                    .map_err(|e| e.to_string())?;
            }
            image.flush().map_err(|e| e.to_string())?;
            server.publish(Message::Frame(frame)).map_err(|e| e.to_string())?;
        }
        let mut first_hart = 0;
        for chip in chips {
            for (hart, bytes) in chip.take_uart() {
                server
                    .publish(Message::Uart {
                        hart: first_hart + hart,
                        bytes,
                    })
                    .map_err(|e| e.to_string())?;
            }
            first_hart += chip.hart_count() as u32;
        }
        if last_clock.elapsed() >= Duration::from_millis(250) {
            server.publish(Message::Clock(io.clock())).map_err(|e| e.to_string())?;
            last_clock = Instant::now();
        }
        if finished.load(Ordering::Acquire) {
            audio.flush().map_err(|e| e.to_string())?;
            server
                .finish(i32::from(cancelled.load(Ordering::Acquire)))
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
