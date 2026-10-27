use super::Endpoint;

pub struct CoreParams {
    pub hart_id: usize,
    pub endpoints: Vec<Endpoint>,
    pub threads: Vec<usize>,
    pub virtual_bank_count: usize,
    pub shared_physical_bank_count: usize,
    pub shared_bank_size: usize,
}

use super::design::{BallInstance, GpDomainConfig};

#[derive(Clone)]
pub struct AcceleratorParams {
    pub mem_config: MemConfig,
    pub ball_domain: BallDomainConfig,
    pub gpdomain: Option<GpDomainConfig>,
    pub virtual_bank_count: usize,
}
#[derive(Clone)]
pub struct MemConfig {
    pub bank_num: usize,
    pub bank_width: usize,
    pub bank_entries: usize,
    pub mmio_enable: bool,
    pub mmio_bank_num: usize,
    pub mmio_bank_entries: usize,
    pub mmio_bank_width: usize,
    pub mmio_read_width: usize,
    pub max_private_vbank: usize,
    pub shared_vbank_base: usize,
}
#[derive(Clone)]
pub struct BallDomainConfig {
    pub balls: Vec<BallInstance>,
}
