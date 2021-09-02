use clap::{App, Arg};
use rppal::i2c::I2c;

fn main() {
    env_logger::init();

    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("bus")
                .short("b")
                .default_value("1")
                .takes_value(true)
                .help("I2C bus, e.g. 1 (i.e. /dev/i2c-1)"),
        )
        .arg(
            Arg::with_name("addr")
                .short("a")
                .default_value("0x57")
                .takes_value(true)
                .help("I2C addr, e.g. 0x57"),
        )
        .get_matches();

    let bus: u8 = matches.value_of("bus").unwrap().parse().unwrap();
    let addr: u16 = matches.value_of("addr").unwrap().parse().unwrap();

    let i2c = I2c::with_bus(bus).unwrap();

    // Check 0x00 pisugar version
}
