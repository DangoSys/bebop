use log::LevelFilter;
use serde_json;
use sim::models::model_trait::DevsModel;
use sim::simulator::Simulation;
use std::fs::File;
use std::io::{self, BufWriter, Result, Write};

pub fn model_step(simulation: &mut Simulation, trace_writer: &mut Option<BufWriter<File>>) -> Result<()> {
  // First, drain all pending messages
  let mut messages_to_process = simulation.get_messages();

  while !messages_to_process.is_empty() {
    if log::max_level() >= LevelFilter::Info {
      for msg in messages_to_process.iter() {
        println!(
          "[MSG] t={:.1} {}:{} -> {}:{} | {}",
          msg.time(),
          msg.source_id(),
          msg.source_port(),
          msg.target_id(),
          msg.target_port(),
          msg.content()
        );
      }
    }

    // Write to trace file if enabled
    if let Some(writer) = trace_writer {
      for msg in messages_to_process.iter() {
        let trace_entry = serde_json::json!({
          "time": msg.time(),
          "source": msg.source_id(),
          "source_port": msg.source_port(),
          "target": msg.target_id(),
          "target_port": msg.target_port(),
          "content": msg.content()
        });
        writeln!(writer, "{}", trace_entry)?;
      }
      writer.flush()?;
    }

    let time0 = simulation.get_global_time();
    match simulation.step() {
      Ok(_) => {
        let time1 = simulation.get_global_time();
        if time1 > time0 {
          break;
        }
      },
      Err(e) => {
        eprintln!("Simulation step error: {:?}", e);
        return Err(io::Error::new(
          io::ErrorKind::Other,
          format!("Simulation error: {:?}", e),
        ));
      },
    }

    messages_to_process = simulation.get_messages();
  }

  // Now process internal events until all models are idle or time advances significantly
  loop {
    let until_next_event = simulation
      .models()
      .iter()
      .fold(f64::INFINITY, |min, model| f64::min(min, model.until_next_event()));

    if until_next_event == f64::INFINITY {
      // All models idle, wait for external events
      // thread::sleep(Duration::from_millis(1));
      // thread::sleep(Duration::from_micros(300));
      break;
    }

    // if until_next_event > 1.0 {
    //   break;
    // }

    let time0 = simulation.get_global_time();
    match simulation.step() {
      Ok(_) => {
        let time1 = simulation.get_global_time();
        if time1 > time0 {
          break;
        }
      },
      Err(e) => {
        eprintln!("Simulation step error: {:?}", e);
        return Err(io::Error::new(
          io::ErrorKind::Other,
          format!("Simulation error: {:?}", e),
        ));
      },
    }
  }

  Ok(())
}
