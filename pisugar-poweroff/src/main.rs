use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use clap::{Arg, Command};
use env_logger::Env;
use pisugar_core::{Model, PiSugarConfig, PiSugarCore, Result};

fn shutdown(config: PiSugarConfig, model: Model, retries: u32) -> Result<()> {
    for _ in 0..retries {
        let core = PiSugarCore::new_without_init(config.clone(), model)?;
        if let Err(e) = core.force_shutdown() {
            log::warn!("{}", e);
        }
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
            Arg::new("log")
                .short('l')
                .long("log")
                .takes_value(true)
                .default_value("info")
                .help("Log level, debug/info/warn/error"),
        )
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
            Arg::new("retries")
                .short('r')
                .long("retries")
                .value_name("RETRIES")
                .default_value("100")
                .help("Retries, e.g. 100"),
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

    let model: Model = matches.value_of("model").unwrap().parse().unwrap();
    let log_level = matches.value_of("log").unwrap();
    let countdown: u64 = matches.value_of("countdown").unwrap().parse().unwrap();
    let retries: u32 = matches.value_of("retries").unwrap().parse().unwrap();
    let config_file: &str = matches.value_of("configfile").unwrap();
    for i in 0..countdown {
        eprint!("{} ", countdown - i);
        sleep(Duration::from_secs(1));
    }
    eprintln!("0...");

    env_logger::Builder::from_env(Env::default().default_filter_or(log_level)).init();

    let mut config = PiSugarConfig::default();
    if let Err(e) = config.load(Path::new(config_file)) {
        log::warn!("Load config file {} error: {}", config_file, e);
    }
    let _ = shutdown(config, model, retries);
}
