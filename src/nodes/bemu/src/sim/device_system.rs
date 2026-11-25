use crate::{configs, hart::cbus, hart::HartModel, sim::run::Args, DeviceModel};

/// The device simulation (arch: sims/device): every hart of the device, each built by the
/// model of its component.
pub fn instantiate(device: &DeviceModel, args: &Args) -> Result<Vec<Box<dyn HartModel>>, String> {
    let options = args.model_options();
    configs::hart_ids()
        .into_iter()
        .map(|id| cbus::hart(&device.link, id, &options))
        .collect()
}
