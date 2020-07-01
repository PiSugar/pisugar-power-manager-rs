use std::thread::sleep;
use std::time::Duration;

use clap::{App, Arg};

use pisugar_core::{PiSugarConfig, PiSugarCore};

fn main() {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("countdown")
                .short("c")
                .long("countdown")
                .value_name("COUNTDOWN")
                .default_value("3")
                .help("Countdown seconds, e.g. 3"),
        )
        .get_matches();

    let countdown: u64 = matches.value_of("countdown").unwrap().parse().unwrap();
    for i in 0..countdown {
        eprint!("{} ", countdown - i);
        sleep(Duration::from_secs(1));
    }
    eprint!("0...\n");

    let config = PiSugarConfig::default();
    if let Ok(core) = PiSugarCore::new(config, 4) {
        for _ in 0..3 {
            let _ = core.force_shutdown();
            sleep(Duration::from_millis(10));
        }
    } else {
        eprintln!("Failed to connect PiSugar");
    }
}
