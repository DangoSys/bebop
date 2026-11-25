use crate::{configs, hart::cbus, hart::HartModel, sim::run::Args, DeviceModel};

/// The hart simulation (arch: sims/hart/HartTarget): the one hart of the entry
/// alone in a single-hart system.
pub fn instantiate(device: &DeviceModel, args: &Args) -> Result<Vec<Box<dyn HartModel>>, String> {
    let [hart] = configs::hart_ids()[..] else {
        panic!("a hart simulation holds one hart");
    };
    Ok(vec![cbus::hart(&device.link, hart, &args.model_options())?])
}
