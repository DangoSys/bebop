use super::HartModel;
use generic::{configs, configs::CoreParams, hlink::HLink, options::ModelOptions};
use linkme::distributed_slice;

/// The model of a Hart component (arch: its HartFactory).
pub struct HartFactory {
    pub component: &'static str,
    pub instantiate: fn(&HLink, &CoreParams, &ModelOptions) -> Result<Box<dyn HartModel>, String>,
}

/// Every Hart model linked into the simulation registers itself here.
#[distributed_slice]
pub static HARTS: [HartFactory];

/// The hart at `id`, built by the model of its component.
pub fn hart(device: &HLink, id: usize, options: &ModelOptions) -> Result<Box<dyn HartModel>, String> {
    let component = configs::hart_component(id);
    let factory = HARTS
        .iter()
        .find(|factory| factory.component == component)
        .unwrap_or_else(|| panic!("no hart model for component {component}"));
    let mut options = options.clone();
    options.log_dir = options.log_dir.join(format!("id-{id}"));
    (factory.instantiate)(device, &configs::hart_parameters(id), &options)
}
