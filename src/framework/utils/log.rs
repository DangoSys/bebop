use std::env;

pub fn init_log(verbose: bool) {
    if verbose && env::var_os("RUST_LOG").is_none() {
        env::set_var("RUST_LOG", "info");
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("off")).init();
}
