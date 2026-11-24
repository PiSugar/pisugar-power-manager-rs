use crate::image::{Image, Kind, Platform};
use std::fmt;
use std::time::{Duration, Instant};

pub const MODE_APP: u8 = 0x0f;
pub const MODE_BOOT: u8 = 0xf0;
pub const MODE_BOOTAPP: u8 = 0xba;
const BLOCK: usize = 512;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Io(String),
    TransientRam(String),
    Refused(String),
    InvalidDeviceHeader(&'static str),
    NotReady,
    Transition,
    Timeout(&'static str),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(s) | Self::TransientRam(s) => write!(f, "I2C: {}", s),
            Self::Refused(s) => write!(f, "Refused: {}", s),
            Self::InvalidDeviceHeader(s) => write!(f, "Refused: {}", s),
            Self::NotReady => write!(f, "PiSugar device registers are not ready"),
            Self::Transition => write!(f, "Device changed mode during identity sampling"),
            Self::Timeout(s) => write!(f, "Timed out: {}", s),
        }
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

/// Each read/write is one complete SMBus byte-data transaction, including STOP.
pub trait Transport {
    fn read(&mut self, register: u8) -> std::result::Result<u8, String>;
    fn write(&mut self, register: u8, value: u8) -> std::result::Result<(), String>;
    fn wait_ms(&mut self, milliseconds: u64);

    fn exchange_ram_byte(&mut self, value: u8) -> Result<u8> {
        self.write(0xdd, value).map_err(Error::Io)?;
        self.read(0xdd).map_err(Error::Io)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub platform: Platform,
    pub mode: u8,
    pub legacy: bool,
}

impl fmt::Display for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match self.mode {
            MODE_APP => "application",
            MODE_BOOT => "bootloader",
            MODE_BOOTAPP => "bootapp",
            _ => "unknown mode",
        };
        write!(f, "{} {} (0x{:02X})", self.platform.name(), mode, self.mode)
    }
}

#[derive(PartialEq, Eq)]
struct Snapshot {
    mode: u8,
    marker: [u8; 8],
    serial: Option<[u8; 16]>,
}

fn snapshot<T: Transport>(bus: &mut T) -> Result<Snapshot> {
    match bus.read(0x00).map_err(Error::Io)? {
        3 => {}
        0 | 0xff => return Err(Error::NotReady),
        _ => return Err(Error::InvalidDeviceHeader("Device is not PiSugar 3")),
    }
    let mode = bus.read(0x01).map_err(Error::Io)?;
    if mode == 0 || mode == 0xff {
        return Err(Error::NotReady);
    }
    if ![MODE_APP, MODE_BOOT, MODE_BOOTAPP].contains(&mode) {
        return Err(Error::InvalidDeviceHeader("Unknown device mode"));
    }
    let mut marker = [0; 8];
    for (n, byte) in marker.iter_mut().enumerate() {
        *byte = bus.read(0x52 + n as u8).map_err(Error::Io)?;
    }
    let serial = if mode == MODE_APP && marker == [0; 8] {
        let mut serial = [0; 16];
        for (n, byte) in serial.iter_mut().enumerate() {
            *byte = bus.read(0x10 + n as u8).map_err(Error::Io)?;
        }
        Some(serial)
    } else {
        None
    };
    if bus.read(0x00).map_err(Error::Io)? != 3 {
        return Err(Error::Transition);
    }
    if bus.read(0x01).map_err(Error::Io)? != mode {
        return Err(Error::Transition);
    }
    Ok(Snapshot { mode, marker, serial })
}

pub fn identify<T: Transport>(bus: &mut T) -> Result<Identity> {
    identify_for_session(bus, None)
}

fn identify_for_session<T: Transport>(bus: &mut T, unmarked_writer: Option<Platform>) -> Result<Identity> {
    let first = snapshot(bus)?;
    let second = snapshot(bus)?;
    if first.mode != second.mode {
        return Err(Error::Transition);
    }
    if first != second {
        return Err(Error::Refused("Device identity is not stable".into()));
    }
    let Snapshot { mode, marker, serial } = first;
    if marker == [0; 8] {
        if let Some(serial) = serial {
            // Early CH applications always zero-pad the final four UID bytes.
            if serial[12..] == [0; 4] || serial[..12] == [0; 12] || serial[..12] == [0xff; 12] {
                return Err(Error::Refused("Unmarked application with ambiguous serial number".into()));
            }
        }
        let platform = match mode {
            MODE_APP => Platform::Fm,
            // Production CH32 bootloaders expose PSID. The original FM bootloader
            // returns eight zero bytes on both identity reads.
            MODE_BOOT => unmarked_writer.unwrap_or(Platform::Fm),
            _ => return Err(Error::Refused("Unmarked bootapp has no verified platform".into())),
        };
        return Ok(Identity {
            platform,
            mode,
            legacy: true,
        });
    }
    if &marker[..4] != b"PSID" || marker[4] != 1 || marker[..7].iter().fold(0, |x, b| x ^ b) != marker[7] {
        return Err(Error::Refused("Unknown or malformed platform identity".into()));
    }
    let platform = match (marker[5], marker[6]) {
        (1, 0) => Platform::Fm,
        (2, 1) => Platform::Ch32,
        _ => return Err(Error::Refused("Unsupported platform/capability identity".into())),
    };
    Ok(Identity {
        platform,
        mode,
        legacy: false,
    })
}

fn target_matches(identity: Identity, image: &Image) -> Result<()> {
    if identity.platform != image.platform {
        return Err(Error::Refused(format!(
            "device is {}, image is {}",
            identity.platform.name(),
            image.platform.name()
        )));
    }
    if identity.platform == Platform::Fm && image.kind != Kind::Application {
        return Err(Error::Refused(
            "FM bootloader maintenance is intentionally excluded".into(),
        ));
    }
    Ok(())
}

fn read_pair<T: Transport>(bus: &mut T, high_register: u8) -> Result<u16> {
    let high = bus.read(high_register).map_err(Error::Io)?;
    let low = bus.read(high_register + 1).map_err(Error::Io)?;
    Ok(u16::from_be_bytes([high, low]))
}

fn write_pair<T: Transport>(bus: &mut T, high_register: u8, value: u16) -> Result<()> {
    bus.write(high_register, (value >> 8) as u8).map_err(Error::Io)?;
    bus.write(high_register + 1, value as u8).map_err(Error::Io)?;
    if read_pair(bus, high_register)? != value {
        return Err(Error::Io("Offset readback mismatch".into()));
    }
    Ok(())
}

fn reset<T: Transport>(bus: &mut T) -> Result<()> {
    bus.write(0x0b, 0x29).map_err(Error::Io)?;
    bus.write(0x03, 0x80).map_err(Error::Io)
}

fn wait_for_mode<T: Transport>(bus: &mut T, image: &Image, expected: u8, unmarked_writer: Option<Platform>) -> Result<Identity> {
    for _ in 0..100 {
        match identify_for_session(bus, unmarked_writer) {
            Ok(identity) => {
                target_matches(identity, image)?;
                if identity.mode == expected {
                    if identity.legacy {
                        return Err(Error::Refused(
                            "Updated firmware did not expose its platform identity".into(),
                        ));
                    }
                    return Ok(identity);
                }
            }
            Err(Error::Io(_)) => {} // Expected briefly during reset/CRC validation.
            Err(Error::Transition) => {}
            // The application register cache is briefly uninitialized on handoff.
            Err(Error::NotReady | Error::InvalidDeviceHeader(_)) if expected == MODE_APP => {}
            Err(error) => return Err(error),
        }
        bus.wait_ms(50);
    }
    Err(Error::Timeout("waiting for the expected firmware mode"))
}

fn check_writer<T: Transport>(bus: &mut T, image: &Image, mode: u8, unmarked_writer: Option<Platform>) -> Result<()> {
    let identity = identify_for_session(bus, unmarked_writer)?;
    target_matches(identity, image)?;
    if identity.mode != mode {
        return Err(Error::Refused("Device left the expected writer mode before Flash commit".into()));
    }
    Ok(())
}

/// Only RAM preparation and transfer use this loop. Flash commands are never replayed here.
fn retry_ram<T: Transport, F: FnMut(&mut T, bool) -> Result<()>>(
    bus: &mut T, offset: u16, failures: &mut u64, started: Instant, mut transfer: F,
) -> Result<bool> {
    let mut retry = false;
    let mut delay_ms = 10;
    let mut next_notice = Instant::now();
    loop {
        match transfer(bus, retry) {
            Ok(()) => return Ok(retry),
            Err(Error::Io(error) | Error::TransientRam(error)) => {
                retry = true;
                *failures = failures.saturating_add(1);
                if Instant::now() >= next_notice {
                    log::warn!("RAM at image offset 0x{:04x}: {} ({} cumulative retries, {}s elapsed). Ctrl-C to cancel.",
                               offset, error, failures, started.elapsed().as_secs());
                    next_notice = Instant::now() + Duration::from_secs(5);
                }
                bus.wait_ms(delay_ms);
                delay_ms = (delay_ms * 2).min(100);
            }
            Err(error) => return Err(error),
        }
    }
}

fn check_segment<T: Transport>(bus: &mut T, offset: u16) -> Result<()> {
    if read_pair(bus, 0xd1)? != offset {
        return Err(Error::Refused("RAM segment changed; restart the update from the beginning".into()));
    }
    Ok(())
}

/// Wait for a known idle writer before sending a single erase/write command.
fn wait_flash_idle<T: Transport>(bus: &mut T) -> Result<()> {
    for _ in 0..100 {
        match bus.read(0xd0) {
            Ok(0 | 1) => return Ok(()), // bit 0 can describe a previous attempt
            Ok(8 | 9) | Err(_) => bus.wait_ms(20),
            Ok(_) => return Err(Error::Refused("Unknown upgrade status bits before Flash commit".into())),
        }
    }
    Err(Error::Timeout("waiting for idle Flash before commit"))
}

fn commit_block<T: Transport>(bus: &mut T, image: &Image, mode: u8, writer: Option<Platform>, offset: u16) -> Result<()> {
    wait_flash_idle(bus)?;
    check_writer(bus, image, mode, writer)?;
    check_segment(bus, offset)?;
    let write_error = bus.write(0xd0, 0xa0).err();
    let mut observed_busy = false;
    if let Some(error) = &write_error {
        log::warn!("Flash command at 0x{:04x}: {}. Checking status without sending the command again.", offset, error);
    } else {
        bus.wait_ms(50);
    }
    for _ in 0..100 {
        match bus.read(0xd0) {
            Ok(8 | 9) => observed_busy = true,
            Ok(1) => return Err(Error::Refused("MCU reported a Flash write error".into())),
            Ok(0) if write_error.is_none() || observed_busy => {
                match check_writer(bus, image, mode, writer).and_then(|_| check_segment(bus, offset)) {
                    Ok(()) => return Ok(()),
                    Err(Error::Io(_)) => {} // Only repeat reads while confirming the commit.
                    Err(error) => return Err(error),
                }
            }
            Ok(0) | Err(_) => {} // Idle alone cannot confirm a write whose ACK was lost.
            Ok(_) => return Err(Error::Refused("Unknown upgrade status bits after Flash commit".into())),
        }
        bus.wait_ms(20);
    }
    Err(Error::Timeout("Flash commit outcome unknown"))
}

fn select_image(candidates: &[Image], platform: Platform, requested: Kind) -> Result<&Image> {
    let mut matches = candidates
        .iter()
        .filter(|image| image.platform == platform && image.kind == requested);
    let image = matches.next().ok_or_else(|| {
        Error::Refused(format!(
            "No {} image for detected {}",
            requested.name(),
            platform.name()
        ))
    })?;
    if matches.next().is_some() {
        return Err(Error::Refused("Ambiguous firmware selection".into()));
    }
    Ok(image)
}

fn wait_for_writer<'a, T: Transport>(
    bus: &mut T,
    candidates: &'a [Image],
    requested: Kind,
    expected: u8,
    allow_reset: bool,
) -> Result<&'a Image> {
    let mut selected = None;
    let mut reset_attempted = false;
    let started = Instant::now();
    let mut next_notice = started;
    let mut probes = 0u64;
    let mut io_errors = 0u64;
    let mut not_ready = 0u64;
    let mut transitions = 0u64;
    let mut last_mode = None;
    loop {
        probes += 1;
        let observation = identify(bus);
        let first_seen = match observation {
            Ok(identity) => {
                let image = match selected {
                    Some(image) => image,
                    None => select_image(candidates, identity.platform, requested)?,
                };
                target_matches(identity, image)?;
                selected = Some(image);
                if identity.mode == expected {
                    log::info!("Detected {} after {} probes (I2C errors={}, not ready={}, mode changes during reads={})",
                               identity, probes, io_errors, not_ready, transitions);
                    return Ok(image);
                }
                if expected == MODE_BOOTAPP {
                    return Err(Error::Refused(
                        "Install and run the matching CH32 bootapp before updating its bootloader".into(),
                    ));
                }
                if allow_reset && !reset_attempted {
                    // A missing ACK may mean the reset already took effect.
                    // From here to writer mode only read the device.
                    reset_attempted = true;
                    if let Err(error) = reset(bus) {
                        log::warn!("Soft reset was not acknowledged: {}", error);
                    }
                }
                last_mode.replace(identity.mode) != Some(identity.mode)
            }
            Err(Error::Io(_)) => {
                io_errors += 1;
                io_errors == 1
            }
            Err(Error::NotReady) => {
                not_ready += 1;
                not_ready == 1
            }
            Err(Error::Transition) => {
                transitions += 1;
                transitions == 1
            }
            Err(error) => return Err(error),
        };
        let now = Instant::now();
        if first_seen || now >= next_notice {
            let latest = match &observation {
                Ok(identity) => identity as &dyn fmt::Display,
                Err(error) => error,
            };
            log::info!("Waiting for PiSugar bootloader ({}s, {} probes). Latest: {}. I2C errors={}, not ready={}, mode changes during reads={}. Press Reset or Ctrl-C to cancel. 请按 Reset，或按 Ctrl-C 取消。",
                       now.duration_since(started).as_secs(), probes, latest, io_errors, not_ready, transitions);
            next_notice = now + Duration::from_secs(5);
        }
        bus.wait_ms(100);
    }
}

/// All candidates must already have been validated in memory by image::load.
pub fn upgrade<T: Transport>(
    bus: &mut T,
    candidates: &[Image],
    requested: Kind,
    allow_reset: bool,
) -> Result<Identity> {
    let expected = if requested == Kind::Bootloader {
        MODE_BOOTAPP
    } else {
        MODE_BOOT
    };
    let image = wait_for_writer(bus, candidates, requested, expected, allow_reset)?;
    program_image(bus, image, expected, None)
}

/// Application recovery on a physically identified board already in bootloader mode.
pub fn recover_application<T: Transport>(bus: &mut T, candidates: &[Image], platform: Platform) -> Result<Identity> {
    let image = select_image(candidates, platform, Kind::Application)?;
    let identity = identify_for_session(bus, Some(platform))?;
    target_matches(identity, image)?;
    if identity.mode != MODE_BOOT {
        return Err(Error::Refused("Forced recovery requires bootloader mode 0xF0".into()));
    }
    let unmarked_writer = if identity.legacy { Some(platform) } else { None };
    program_image(bus, image, MODE_BOOT, unmarked_writer)
}

fn program_image<T: Transport>(bus: &mut T, image: &Image, expected: u8, unmarked_writer: Option<Platform>) -> Result<Identity> {
    let started = Instant::now();
    let mut failures = 0;
    for (n, data) in image.bytes.chunks(BLOCK).enumerate() {
        let mut block = [0xff; BLOCK];
        block[..data.len()].copy_from_slice(data);
        let offset = (n * BLOCK) as u16;
        retry_ram(bus, offset, &mut failures, started, |bus, _| {
            check_writer(bus, image, expected, unmarked_writer)?;
            write_pair(bus, 0xd1, offset)?;
            write_pair(bus, 0xd3, offset.wrapping_sub(1))
        })?;
        let mut recovered = false;
        for (index, &byte) in block.iter().enumerate() {
            let address = offset + index as u16;
            recovered |= retry_ram(bus, address, &mut failures, started, |bus, retry| {
                if retry {
                    check_writer(bus, image, expected, unmarked_writer)?;
                    check_segment(bus, offset)?;
                    // DD pre-increments the position. A lost ACK may have advanced it.
                    // Rewriting the segment here would clear the CH32 RAM buffer.
                    write_pair(bus, 0xd3, address.wrapping_sub(1))?;
                }
                if bus.exchange_ram_byte(byte)? != byte {
                    return Err(Error::Io("Buffered byte readback mismatch".into()));
                }
                Ok(())
            })?;
        }
        retry_ram(bus, offset, &mut failures, started, |bus, retry| {
            check_writer(bus, image, expected, unmarked_writer)?;
            if recovered || retry {
                // An interrupted link can hide a reset that cleared segment zero.
                check_segment(bus, offset)?;
                for (index, &byte) in block.iter().enumerate() {
                    write_pair(bus, 0xd3, offset + index as u16)?;
                    if bus.read(0xdd).map_err(Error::Io)? != byte {
                        return Err(Error::Refused("RAM contents changed before Flash commit; restart the update".into()));
                    }
                }
                check_writer(bus, image, expected, unmarked_writer)?;
            }
            Ok(())
        })?;
        if let Err(error) = commit_block(bus, image, expected, unmarked_writer, offset) {
            log::error!("Flash commit at image offset 0x{:04x} was not confirmed: {}. Stopping without resending the erase/write command.", offset, error);
            if image.kind == Kind::Application {
                log::error!("Keep power connected and run the same updater again. It will recheck the bootloader and restart the application from byte zero.");
            }
            return Err(error);
        }
        log::info!(
            "Accepted {}/{} bytes",
            ((n + 1) * BLOCK).min(image.bytes.len()),
            image.bytes.len()
        );
    }
    // Re-read identity as well as mode after the actual application starts;
    // successful RAM echo/Flash commands alone do not mean the update booted.
    let final_mode = if image.kind == Kind::Application {
        MODE_APP
    } else {
        MODE_BOOTAPP
    };
    // A reset can race the acknowledgement. Only an observed valid final mode
    // can turn this handoff into success, regardless of its write result.
    let _handoff_result = bus.write(0xd0, 0x10);
    bus.wait_ms(50);
    if image.kind == Kind::Bootloader {
        // Bootapp also existed before the update. Seeing it again alone cannot
        // prove reset ran the newly written bootloader. Observe the intermediate
        // boot mode, then explicitly launch bootapp (identity reads extend the
        // bootloader's idle timeout, so polling must not wait for auto-jump).
        wait_for_mode(bus, image, MODE_BOOT, unmarked_writer)?;
        let _launch_result = bus.write(0xd0, 0x10);
        bus.wait_ms(50);
    }
    wait_for_mode(bus, image, final_mode, unmarked_writer)
}
