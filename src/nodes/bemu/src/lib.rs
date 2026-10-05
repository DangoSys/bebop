mod chip;
mod sim;
mod input;

mod accel;
mod user_vm;
mod process;
#[path = "../root/mod.rs"]
pub mod root;
mod stdio;

#[path = "emu/bank/mod.rs"]
mod bank;

#[path = "emu/config.rs"]
mod config;

#[path = "emu/inst/mod.rs"]
mod inst;

mod trace;

pub use bebop_bemu_profile::{format_report as format_profile_report, print_report as print_profile_report};
pub use config::{workload_placement, core_signature, core_hart_id, hart_capacity, private_bank_geometry, tile_count, tile_topology, TileTopology};
pub use root::tile::Tile;
pub use sim::Core;
pub use trace::TraceConfig;
