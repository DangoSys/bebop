use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::io::{self, Result};

pub enum Command {
  Step(u32), // Step N times
  Quit,
  Continue,
}

static mut EDITOR: Option<DefaultEditor> = None;

fn get_editor() -> &'static mut DefaultEditor {
  unsafe {
    if EDITOR.is_none() {
      EDITOR = Some(DefaultEditor::new().expect("Failed to create readline editor"));
    }
    EDITOR.as_mut().unwrap()
  }
}

pub fn read_command() -> Result<Command> {
  let editor = get_editor();

  loop {
    match editor.readline("(bebop) ") {
      Ok(line) => {
        let trimmed = line.trim();

        // Add to history if not empty
        if !trimmed.is_empty() {
          let _ = editor.add_history_entry(trimmed);
        }

        // Empty input: step once
        if trimmed.is_empty() {
          return Ok(Command::Step(1));
        }

        // si command: step N times
        if trimmed.starts_with("si") {
          let num_str = trimmed[2..].trim();

          if num_str.is_empty() {
            eprintln!("Error: 'si' requires a number, e.g., 'si 100'");
            continue;
          }

          return match num_str.parse::<u32>() {
            Ok(n) if n > 0 => Ok(Command::Step(n)),
            Ok(_) => {
              eprintln!("Error: step count must be greater than 0");
              continue;
            }
            Err(e) => {
              eprintln!("Error: invalid number '{}': {}", num_str, e);
              continue;
            }
          };
        }

        // q command: quit
        if trimmed == "q" {
          return Ok(Command::Quit);
        }

        // c command: continue
        if trimmed == "c" {
          return Ok(Command::Continue);
        }

        eprintln!(
          "Unknown command: '{}'. Use Enter to step, 'q' to quit, 'c' to continue, or 'si 100' to step N times",
          trimmed
        );
      }
      Err(ReadlineError::Interrupted) => {
        // Ctrl-C: quit
        return Ok(Command::Quit);
      }
      Err(ReadlineError::Eof) => {
        // Ctrl-D: quit
        return Ok(Command::Quit);
      }
      Err(err) => {
        return Err(io::Error::new(io::ErrorKind::Other, err));
      }
    }
  }
}
