use std::io::{self, Result, Write};

pub enum Command {
  Step(u32), // Step N times
  Quit,
  Continue,
}

pub fn read_command() -> Result<Command> {
  io::stdout().flush()?;

  let mut input = String::new();
  io::stdin().read_line(&mut input)?;
  let trimmed = input.trim();

  if trimmed.is_empty() {
    return Ok(Command::Step(1));
  }

  if trimmed.starts_with("si") {
    let num_str = trimmed[2..].trim();

    if num_str.is_empty() {
      eprintln!("Error: 'si' requires a number, e.g., 'si 100'");
      return read_command();
    }

    return match num_str.parse::<u32>() {
      Ok(n) if n > 0 => Ok(Command::Step(n)),
      Ok(_) => {
        eprintln!("Error: step count must be greater than 0");
        read_command()
      }
      Err(e) => {
        eprintln!("Error: invalid number '{}': {}", num_str, e);
        read_command()
      }
    };
  }

  if trimmed == "q" {
    return Ok(Command::Quit);
  }

  if trimmed == "c" {
    return Ok(Command::Continue);
  }

  eprintln!(
    "Unknown command: '{}'. Use Enter to step, 'q' to quit, 'c' to continue, or 'si 100' to step N times",
    trimmed
  );
  read_command()
}
