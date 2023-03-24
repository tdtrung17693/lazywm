use lazywm::{config, wm::WM};
use log::LevelFilter;

fn main() {
    let config = config::load_config(Some("/home/arch/CodeRust/lazywm/examples/config.yaml"))
        .expect("config cannot load");
    let wm = WM::new(config).unwrap();
    systemd_journal_logger::init().unwrap();
    log::set_max_level(LevelFilter::Info);
    wm.init();
    wm.run();
}
