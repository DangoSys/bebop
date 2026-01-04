use sim::models::ModelMessage;

pub fn rob_push(inst: usize) -> ModelMessage {
  let msg = ModelMessage {
    port_name: "decoder_rob".to_string(),
    content: inst.to_string(),
  };
  msg
}
