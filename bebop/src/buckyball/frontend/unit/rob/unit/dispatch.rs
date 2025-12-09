use crate::buckyball::frontend::unit::rob::bundles::decoder_rob::DecodedInstruction;

/// ROB 调度策略
pub struct DispatchPolicy;

impl DispatchPolicy {
  pub fn can_dispatch(inst: &DecodedInstruction) -> bool {
    inst.domain_id != 255
  }
}
