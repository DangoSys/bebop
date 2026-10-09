use bebop_bemu::{format_profile_report, print_profile_report, Core, TraceConfig};
use bebop_vm::{Message, Server};
use clap::Parser;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(name = "bebop-bemu")]
struct Args {
    /// Defaults to the first Buckyball core, so a GNU user process can issue its NPU instructions.
    #[arg(long)]
    core_index: Option<usize>,
    #[arg(long)]
    elf: PathBuf,
    #[arg(long)]
    log_dir: PathBuf,
    #[arg(long)]
    working_directory: Option<PathBuf>,
    /// Mono signed PCM16 little-endian input at 16 kHz.
    #[arg(long)]
    microphone: Option<PathBuf>,
    /// Little-endian u32 events: bit 31 pressed, bits 0..15 key code.
    #[arg(long)]
    keyboard: Option<PathBuf>,
    /// Expose peripherals to the independent VM window over vm.sock.
    #[arg(long)]
    vm: bool,
    #[arg(long)]
    disasm: bool,
    #[arg(long = "tool-profile")]
    tool_profile: bool,
    #[arg(long)]
    itrace: bool,
    #[arg(long)]
    mtrace: bool,
    #[arg(last = true)]
    arguments: Vec<String>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    let core_index = args.core_index.unwrap_or_else(|| {
        (0..bebop_bemu::tile_count())
            .find_map(|tile| bebop_bemu::tile_topology(tile).endpoint_cores.first().map(|(_, core)| *core))
            .unwrap_or(0)
    });
    let mut bemu = Core::new_with_core(
        &args.log_dir,
        TraceConfig::new(args.itrace, args.mtrace),
        args.disasm,
        args.tool_profile,
        core_index,
    )
    .map_err(|e| e.to_string())?;
    let chip = bemu.chip();
    let mut vm = if args.vm {
        chip.capture_uart();
        Some(
            Server::bind(
                &args.log_dir.join("vm.sock"),
                bebop_vga::WIDTH as u32,
                bebop_vga::HEIGHT as u32,
                chip.hart_count() as u32,
            )
            .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    if let Some(server) = &mut vm {
        server
            .publish(Message::Clock(chip.clock()))
            .map_err(|e| e.to_string())?;
    }
    if let Some(path) = args.microphone {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() % 2 != 0 {
            return Err("microphone PCM16 input has an incomplete sample".into());
        }
        chip.push_microphone(
            bytes
                .chunks_exact(2)
                .map(|sample| i16::from_le_bytes(sample.try_into().unwrap())),
        );
    }
    if let Some(path) = args.keyboard {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() % 4 != 0 {
            return Err("keyboard input has an incomplete event".into());
        }
        for event in bytes.chunks_exact(4) {
            let event = u32::from_le_bytes(event.try_into().unwrap());
            if event & 0x7fff_0000 != 0 {
                return Err("keyboard event has reserved bits set".into());
            }
            chip.push_key(event as u16, event >> 31 != 0);
        }
    }
    let mut audio = BufWriter::new(std::fs::File::create(args.log_dir.join("speaker.pcm")).map_err(|e| e.to_string())?);
    bemu.set_arguments(args.arguments);
    bemu.load_elf(&args.elf).map_err(|e| e.to_string())?;
    if let Some(directory) = args.working_directory {
        bemu.set_working_directory(directory.canonicalize().map_err(|e| e.to_string())?);
    }
    bemu.init_hart().map_err(|e| e.to_string())?;
    let started = Instant::now();
    let mut last_progress = started;
    let mut last_clock = started;
    while !bemu.finished() {
        if let Some(server) = &mut vm {
            for message in server.poll().map_err(|e| e.to_string())? {
                match message {
                    Message::Key { code, pressed } => chip.push_key(code, pressed),
                    Message::Microphone(samples) => chip.push_live_microphone(samples)?,
                    Message::Serial { hart, bytes } if (hart as usize) < chip.hart_count() => {
                        for byte in bytes {
                            chip.push_uart(hart as usize, byte);
                        }
                    }
                    Message::SetClock(ns) => {
                        chip.set_clock(ns);
                        server.publish(Message::Clock(ns)).map_err(|e| e.to_string())?;
                    }
                    _ => return Err("invalid request from VM frontend".into()),
                }
            }
        }
        bemu.step(10_000).map_err(|e| e.to_string())?;
        let samples = chip.take_audio();
        for sample in &samples {
            audio.write_all(&sample.to_le_bytes()).map_err(|e| e.to_string())?;
        }
        if !samples.is_empty() {
            if let Some(server) = &mut vm {
                server.publish(Message::Audio(samples)).map_err(|e| e.to_string())?;
            }
        }
        if let Some(frame) = chip.take_frame() {
            let mut image =
                BufWriter::new(std::fs::File::create(args.log_dir.join("vga.ppm")).map_err(|e| e.to_string())?);
            write!(image, "P6\n{} {}\n255\n", bebop_vga::WIDTH, bebop_vga::HEIGHT).map_err(|e| e.to_string())?;
            for pixel in frame.chunks_exact(4) {
                image
                    .write_all(&[pixel[2], pixel[1], pixel[0]])
                    .map_err(|e| e.to_string())?;
            }
            image.flush().map_err(|e| e.to_string())?;
            if let Some(server) = &mut vm {
                server.publish(Message::Frame(frame)).map_err(|e| e.to_string())?;
            }
        }
        if let Some(server) = &mut vm {
            for (hart, bytes) in chip.take_uart() {
                server
                    .publish(Message::Uart { hart, bytes })
                    .map_err(|e| e.to_string())?;
            }
            if last_clock.elapsed() >= Duration::from_millis(250) {
                server
                    .publish(Message::Clock(chip.clock()))
                    .map_err(|e| e.to_string())?;
                last_clock = Instant::now();
            }
        }
        if last_progress.elapsed() >= Duration::from_secs(10) {
            eprintln!(
                "[BEMU] elapsed {:.1}s",
                started.elapsed().as_secs_f64()
            );
            last_progress = Instant::now();
        }
    }
    audio.flush().map_err(|e| e.to_string())?;
    if args.tool_profile {
        let report = bemu
            .profile_report(started.elapsed())
            .ok_or_else(|| "tool-profile enabled but no profile report".to_string())?;
        print_profile_report(&report);
        let profile_path = args.log_dir.join("tool-profile.txt");
        std::fs::write(&profile_path, format_profile_report(&report))
            .map_err(|e| format!("failed to write tool profile {}: {e}", profile_path.display()))?;
    }

    let exit_code = bemu
        .exit_code()
        .ok_or_else(|| "bemu finished without an exit code".to_string())?;
    if let Some(server) = &mut vm {
        server.finish(exit_code).map_err(|e| e.to_string())?;
    }
    if exit_code != 0 {
        return Err(format!("bemu exited with code {exit_code}"));
    }
    Ok(())
}
