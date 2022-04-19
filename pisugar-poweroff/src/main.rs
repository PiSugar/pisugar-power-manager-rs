use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use clap::{Arg, Command};

use pisugar_core::{Model, PiSugarConfig, PiSugarCore, Result};
use std::convert::TryInto;

fn shutdown(config: PiSugarConfig, model: Model) -> Result<()> {
    let core = PiSugarCore::new(config, model)?;
    for _ in 0..3 {
        let _ = core.force_shutdown();
        sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn main() {
    let models = vec![
        Model::PiSugar_3.to_string(),
        Model::PiSugar_2_Pro.to_string(),
        Model::PiSugar_2_2LEDs.to_string(),
        Model::PiSugar_2_4LEDs.to_string(),
    ];
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::new("model")
                .short('m')
                .long("model")
                .value_name("MODEL")
                .help(format!("PiSugar Model, choose from {:?}", models).as_str())
                .takes_value(true)
                .validator(move |x| {
                    if models.contains(&x.to_string()) {
                        Ok(())
                    } else {
                        Err("Invalid model".to_string())
                    }
                })
                .required(true),
        )
        .arg(
            Arg::new("countdown")
                .short('c')
                .long("countdown")
                .value_name("COUNTDOWN")
                .default_value("3")
                .help("Countdown seconds, e.g. 3"),
        )
        .arg(
            Arg::new("configfile")
                .short('f')
                .long("config")
                .value_name("CONFIG")
                .default_value("/etc/pisugar-server/config.json")
                .help("Configuration file"),
        )
        .get_matches();

    let model: Model = matches.value_of("model").unwrap().try_into().unwrap();

    let countdown: u64 = matches.value_of("countdown").unwrap().parse().unwrap();
    let config_file: &str = matches.value_of("configfile").unwrap();
    for i in 0..countdown {
        eprint!("{} ", countdown - i);
        sleep(Duration::from_secs(1));
    }
    eprint!("0...\n");

    let mut config = PiSugarConfig::default();
    let _ = config.load(Path::new(config_file));
    let _ = shutdown(config, model);
}
