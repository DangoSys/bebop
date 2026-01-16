use sim::simulator::{Message, Simulation};

/// Inject message to specified Model
///
/// # Parameters
/// - `simulation`: Simulation instance
/// - `target_model`: Target model name
/// - `latency`: Delay (time unit)
/// - `source_id`: Optional message source ID, defaults to "default"
/// - `source_port`: Optional source port, defaults to "default"
/// - `target_port`: Optional target port, defaults to "default"
///
/// If cycle mode is not enabled (CYCLE_MODE_ENABLED == false), this function returns directly
pub fn inject_message(
  simulation: &mut Simulation,
  target_model: &str,
  source_id: Option<&str>,
  source_port: Option<&str>,
  target_port: Option<&str>,
  content: &str,
) {
  let msg = Message::new(
    source_id.unwrap_or("host").to_string(),
    source_port.unwrap_or("default").to_string(),
    target_model.to_string(),
    target_port.unwrap_or("default").to_string(),
    simulation.get_global_time(),
    content.to_string(),
  );
  simulation.inject_input(msg);
}
