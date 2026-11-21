use clap::{Arg, Command};
use pisugar_programmer::image;
#[cfg(target_os = "linux")]
use pisugar_programmer::protocol;
#[cfg(target_os = "linux")]
use std::io;
use std::path::Path;

fn number(s: &str) -> Result<u16, String> {
    let value = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u16::from_str_radix(hex, 16)
    } else {
        s.parse()
    };
    value.map_err(|e| e.to_string())
}

fn run() -> Result<(), String> {
    let args = Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .about("PiSugar 3 platform-checked OTA programmer; waits for a manual Reset by default (Ctrl-C to cancel)")
        .arg(Arg::new("debug").short('d').help("Display debug logs"))
        .arg(
            Arg::new("bus")
                .short('b')
                .takes_value(true)
                .default_value("1")
                .help("I2C bus"),
        )
        .arg(
            Arg::new("addr")
                .short('a')
                .takes_value(true)
                .default_value("0x57")
                .help("I2C address"),
        )
        .arg(
            Arg::new("reset")
                .short('r')
                .help("Request one soft reset after platform and file validation, then wait for the bootloader"),
        )
        .arg(
            Arg::new("image")
                .long("image")
                .takes_value(true)
                .default_value("application")
                .possible_values(["application", "bootapp", "bootloader"])
                .help("Explicit maintenance type; ordinary updates use application"),
        )
        .arg(
            Arg::new("check")
                .long("check")
                .help("Validate all file/package contents without opening I2C"),
        )
        .arg(
            Arg::new("yes")
                .long("yes")
                .short('y')
                .help("Skip the power confirmation when waiting for a manual Reset"),
        )
        .arg(
            Arg::new("file")
                .required(true)
                .help("Tagged OTA v2 BIN or unified OTA ZIP"),
        )
        .get_matches();
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(if args.is_present("debug") { "debug" } else { "info" }),
    )
    .init();
    let requested = image::Kind::parse(args.value_of("image").unwrap())?;
    // Entire package, including the unselected platform, is checked before any
    // reset, write-enable command, or opening of the hardware transport.
    let images = image::load(Path::new(args.value_of("file").unwrap()))?;
    for image in &images {
        log::info!(
            "Validated {} {}: {} bytes, base 0x{:08x}",
            image.platform.name(),
            image.kind.name(),
            image.bytes.len(),
            image.base
        );
    }
    if !images.iter().any(|image| image.kind == requested) {
        return Err("File does not contain the explicitly requested image type".into());
    }
    if args.is_present("check") {
        println!("All images passed platform, type, address, length and checksum validation. No device accessed.");
        return Ok(());
    }
    let bus = number(args.value_of("bus").unwrap())?;
    let address = number(args.value_of("addr").unwrap())?;
    if bus > 255 || !(0x03..=0x77).contains(&address) {
        return Err("Invalid bus or 7-bit I2C address".into());
    }
    hardware_upgrade(
        bus as u8,
        address,
        &images,
        requested,
        args.is_present("reset"),
        args.is_present("yes"),
    )
}

#[cfg(target_os = "linux")]
fn hardware_upgrade(
    bus: u8,
    address: u16,
    images: &[image::Image],
    kind: image::Kind,
    reset: bool,
    yes: bool,
) -> Result<(), String> {
    use sysinfo::{ProcessExt, ProcessRefreshKind, RefreshKind, SystemExt};
    let system =
        sysinfo::System::new_with_specifics(RefreshKind::default().with_processes(ProcessRefreshKind::everything()));
    if system
        .processes()
        .values()
        .any(|process| process.name().contains("pisugar-server"))
    {
        return Err("Stop pisugar-server before upgrading: sudo systemctl stop pisugar-server".into());
    }
    if !reset && !yes {
        println!("Keep a stable power supply connected throughout the update. Continue? [y/N]");
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).map_err(|e| e.to_string())?;
        if answer.trim().to_lowercase() != "y" {
            return Err("Cancelled before accessing the device".into());
        }
    }
    let mut i2c = rppal::i2c::I2c::with_bus(bus).map_err(|e| e.to_string())?;
    i2c.set_slave_address(address).map_err(|e| e.to_string())?;
    i2c.set_timeout(100).map_err(|e| e.to_string())?;
    let mut transport = LinuxTransport(i2c);
    let result = protocol::upgrade(&mut transport, images, kind, reset).map_err(|e| e.to_string())?;
    println!(
        "Upgrade verified: {} mode 0x{:02x}, valid platform identity.",
        result.platform.name(),
        result.mode
    );
    if kind != image::Kind::Application {
        println!("Maintenance stage complete; bootapp is running. Restore the matching application before normal use.");
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn hardware_upgrade(
    _bus: u8,
    _address: u16,
    _images: &[image::Image],
    _kind: image::Kind,
    _reset: bool,
    _yes: bool,
) -> Result<(), String> {
    Err("Hardware programming requires Linux I2C. Use --check for offline validation on this computer.".into())
}

#[cfg(target_os = "linux")]
struct LinuxTransport(rppal::i2c::I2c);
#[cfg(target_os = "linux")]
impl protocol::Transport for LinuxTransport {
    fn read(&mut self, register: u8) -> Result<u8, String> {
        self.0.smbus_read_byte(register).map_err(|e| e.to_string())
    }
    fn write(&mut self, register: u8, value: u8) -> Result<(), String> {
        self.0.smbus_write_byte(register, value).map_err(|e| e.to_string())
    }
    fn exchange_ram_byte(&mut self, value: u8) -> Result<u8, protocol::Error> {
        self.0.smbus_write_byte(0xdd, value).and_then(|_| self.0.smbus_read_byte(0xdd)).map_err(|error| {
            match &error {
                rppal::i2c::Error::Io(io) if matches!(io.raw_os_error(), Some(5) | Some(121)) =>
                    protocol::Error::TransientRam(error.to_string()),
                _ => protocol::Error::Io(error.to_string()),
            }
        })
    }
    fn wait_ms(&mut self, milliseconds: u64) {
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Upgrade stopped: {}", error);
        std::process::exit(1);
    }
}
