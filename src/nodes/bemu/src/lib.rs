//! BEMU: the levels, re-exported under one name, and the simulation tops that assemble them.
pub use bebop_bemu_ball as ball;
pub use bebop_bemu_custom as custom;
pub use bebop_bemu_device as device;
pub use bebop_bemu_generic::{
    bank_matrix, banks, blink, clint, configs, ddr, guest, hlink, operators, options, riscv, rocc, stores, tasks,
    timing, tlink, trace,
};
pub use bebop_bemu_hart as hart;
pub use bebop_bemu_ilp as ilp;
pub use bebop_bemu_mlp as mlp;
pub use bebop_bemu_tlp as tlp;
mod compose;
pub mod sim;

pub use compose::cpu_timing;

pub use ball::{Ball, BallModel, ExecContext, BALLS};
pub use bebop_bemu_custom::Accelerator;
pub use bebop_bemu_profile::{
    format_report as format_profile_report, print_report as print_profile_report, BemuProfileReport,
};
pub use bebop_bemu_thread::{workers as thread_workers, ThreadModel};
pub use bebop_bemu_tlp::ThreadGroup;
pub use configs::{
    hart_capacity, hart_count, hart_ids, hart_parameters, shared_bank_fabric, thread_is_ant, thread_signature,
    workload_placement, CoreParams, Endpoint,
};
pub use ddr::DRAM_BASE;
pub use device::DeviceModel;
pub use guest::{GuestAccess, ThreadMemory};
pub use hart::cbus::{HartFactory, HARTS};
pub use hart::{Cpu, HartModel};
pub use hlink::{HLink, Service};
pub use options::ModelOptions;
pub use rocc::{NoRocc, Rocc};
pub use sim::run::Simulation;
pub use snafu::{FromString, Whatever};
pub use timing::{DmaLayout, ServicePhases};
pub use trace::TraceConfig;
