use lazywm::wm::WM;
use log::LevelFilter;

fn main() {
    let wm = WM::new().unwrap();
    systemd_journal_logger::init().unwrap();
    log::set_max_level(LevelFilter::Info);
    wm.init();
    wm.run();
}
