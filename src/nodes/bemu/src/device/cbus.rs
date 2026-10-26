use crate::{configs, core::CoreModel, sim::run::Args, CoreParams, DeviceModel, ModelOptions};
use linkme::distributed_slice;

/// The model of a Core component (arch: its CoreFactory).
pub struct CoreFactory {
    pub component: &'static str,
    pub instantiate: fn(&DeviceModel, &CoreParams, &ModelOptions) -> Result<Box<dyn CoreModel>, String>,
}

/// Every Core model linked into the simulation registers itself here.
#[distributed_slice]
pub static CORES: [CoreFactory];

/// Instantiates every core of the device by its component (arch: CBus).
pub fn instantiate(device: &DeviceModel, args: &Args) -> Result<Vec<Box<dyn CoreModel>>, String> {
    let options = args.model_options();
    configs::hart_ids()
        .into_iter()
        .map(|hart| core(device, hart, &options))
        .collect()
}

/// The core at `hart`, built by the model of its component.
pub fn core(device: &DeviceModel, hart: usize, options: &ModelOptions) -> Result<Box<dyn CoreModel>, String> {
    let component = configs::core_component(hart);
    let factory = CORES
        .iter()
        .find(|factory| factory.component == component)
        .unwrap_or_else(|| panic!("no core model for component {component}"));
    let mut options = options.clone();
    options.log_dir = options.log_dir.join(format!("hart-{hart}"));
    (factory.instantiate)(device, &configs::core_parameters(hart), &options)
}
