use crate::config::{self, P2EOptions, CTB_FPGA_ID, HW_CONFIG, VVAC_TOP_MODULE};
use crate::{BitstreamBuilder, P2ESimulator, SimulatorConfig};
use snafu::{FromString, Whatever};

#[derive(Debug, Clone)]
pub struct P2ECli {
    pub buildbitstream: bool,
    pub runworkload: bool,
}

pub fn run(cli: P2ECli) -> Result<(), Whatever> {
    let options = P2EOptions {
        buildbitstream: cli.buildbitstream,
        runworkload: cli.runworkload,
    };
    config::validate_tasks(&options)?;
    run_with_options(options)
}

fn run_with_options(options: P2EOptions) -> Result<(), Whatever> {
    if options.buildbitstream {
        build_bitstream(&options)?;
    }

    if options.runworkload {
        run_workload(&options)?;
    }

    Ok(())
}

fn build_bitstream(options: &P2EOptions) -> Result<(), Whatever> {
    BitstreamBuilder::new()
        .vvac_top_module(VVAC_TOP_MODULE)
        .output_dir(options.out_dir())
        .hw_config(HW_CONFIG)
        .build()
        .map_err(|e| Whatever::without_source(format!("P2E bitstream build failed: {}", e)))
}

fn run_workload(options: &P2EOptions) -> Result<(), Whatever> {
    let sim_config = SimulatorConfig::new(CTB_FPGA_ID, options.out_dir(), options.rtcfg());
    let mut simulator = P2ESimulator::with_config(sim_config)
        .map_err(|e| Whatever::without_source(format!("failed to create P2E simulator: {}", e)))?;

    simulator
        .reset()
        .map_err(|e| Whatever::without_source(format!("failed to reset P2E simulator: {}", e)))?;

    let exit_code = simulator
        .run_until_exit()
        .map_err(|e| Whatever::without_source(format!("P2E run failed: {}", e)))?;

    println!(
        "P2E exited with code {} after {} cycles",
        exit_code,
        simulator.cycles()
    );

    Ok(())
}
