use crate::{configs, core::CoreModel, device::cbus, sim::run::Args, DeviceModel};

/// The core simulation (arch: sims/core/CoreTarget): the one core of the entry
/// alone in a single-core system.
pub fn instantiate(device: &DeviceModel, args: &Args) -> Result<Vec<Box<dyn CoreModel>>, String> {
    let [hart] = configs::hart_ids()[..] else {
        panic!("a core simulation holds one core");
    };
    Ok(vec![cbus::core(device, hart, &args.model_options())?])
}
