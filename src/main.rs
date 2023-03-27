use clap::Parser;
use lazywm::{config, wm::WM};
use log::{info, LevelFilter};

mod cli;

fn main() {
    let args = cli::Args::parse();
    let config =
        config::load_config(args.config.as_ref().map(|c| c.as_str())).expect("config cannot load");
    let wm = WM::new(config).unwrap();
    systemd_journal_logger::init().unwrap();
    log::set_max_level(LevelFilter::Info);
    wm.init();
    wm.run();
}
