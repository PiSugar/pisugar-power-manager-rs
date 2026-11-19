use crate::image::{Image, Kind, Platform};
use std::fmt;

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
    Transition,
    Timeout(&'static str),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(s) | Self::TransientRam(s) => write!(f, "I2C: {}", s),
            Self::Refused(s) => write!(f, "Refused: {}", s),
            Self::InvalidDeviceHeader(s) => write!(f, "Refused: {}", s),
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

fn snapshot<T: Transport>(bus: &mut T) -> Result<(u8, [u8; 8])> {
    if bus.read(0x00).map_err(Error::Io)? != 3 {
        return Err(Error::InvalidDeviceHeader("Device is not PiSugar 3"));
    }
    let mode = bus.read(0x01).map_err(Error::Io)?;
    if ![MODE_APP, MODE_BOOT, MODE_BOOTAPP].contains(&mode) {
        return Err(Error::InvalidDeviceHeader("Unknown device mode"));
    }
    let mut marker = [0; 8];
    for (n, byte) in marker.iter_mut().enumerate() {
        *byte = bus.read(0x52 + n as u8).map_err(Error::Io)?;
    }
    if bus.read(0x00).map_err(Error::Io)? != 3 {
        return Err(Error::InvalidDeviceHeader("Device identity changed during sampling"));
    }
    if bus.read(0x01).map_err(Error::Io)? != mode {
        return Err(Error::Transition);
    }
    Ok((mode, marker))
}

pub fn identify<T: Transport>(bus: &mut T) -> Result<Identity> {
    let first = snapshot(bus)?;
    let second = snapshot(bus)?;
    if first.0 != second.0 {
        return Err(Error::Transition);
    }
    if first.1 != second.1 {
        return Err(Error::Refused("Device identity is not stable".into()));
    }
    let (mode, marker) = first;
    if marker == [0; 8] {
        // This is a successful read of the legacy FM response, not an I/O
        // failure. Unmarked experimental CH boards must be migrated separately.
        return Ok(Identity {
            platform: Platform::Fm,
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

fn write_pair<T: Transport>(bus: &mut T, high_register: u8, value: u16) -> Result<()> {
    bus.write(high_register, (value >> 8) as u8).map_err(Error::Io)?;
    bus.write(high_register + 1, value as u8).map_err(Error::Io)?;
    let actual = ((bus.read(high_register).map_err(Error::Io)? as u16) << 8)
        | bus.read(high_register + 1).map_err(Error::Io)? as u16;
    if actual != value {
        return Err(Error::Io("Offset readback mismatch".into()));
    }
    Ok(())
}

fn reset<T: Transport>(bus: &mut T) -> Result<()> {
    bus.write(0x0b, 0x29).map_err(Error::Io)?;
    bus.write(0x03, 0x80).map_err(Error::Io)
}

fn wait_for_mode<T: Transport>(bus: &mut T, image: &Image, expected: u8, require_marker: bool) -> Result<Identity> {
    for _ in 0..100 {
        match identify(bus) {
            Ok(identity) => {
                target_matches(identity, image)?;
                if identity.mode == expected {
                    if require_marker && identity.legacy {
                        return Err(Error::Refused(
                            "Updated firmware did not expose its platform identity".into(),
                        ));
                    }
                    return Ok(identity);
                }
            }
            Err(Error::Io(_)) => {} // Expected briefly during reset/CRC validation.
            Err(Error::Transition) => {}
            // Only the read-only, final application handoff waits for registers
            // 0x00/0x01 to initialize. Every pre-write check stays fail-closed.
            Err(Error::InvalidDeviceHeader(_)) if expected == MODE_APP => {}
            Err(error) => return Err(error),
        }
        bus.wait_ms(50);
    }
    Err(Error::Timeout("waiting for the expected firmware mode"))
}

fn send_block<T: Transport>(bus: &mut T, platform: Platform, offset: u16, block: &[u8; BLOCK]) -> Result<()> {
    // Retry only the uncommitted RAM buffer. Never move the write pointer back
    // and continue halfway through a block, or repeat an ambiguous erase command.
    let mut attempt_number = 1;
    loop {
        let attempt = (|| {
            write_pair(bus, 0xd1, offset)?;
            write_pair(bus, 0xd3, offset.wrapping_sub(1))?;
            for byte in block {
                if bus.exchange_ram_byte(*byte)? != *byte {
                    return Err(Error::Io("Buffered byte readback mismatch".into()));
                }
            }
            Ok(())
        })();
        let error = match attempt {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        // CH32 field traces show EIO and EREMOTEIO on data-register transfers
        // across three consecutive attempts. A write error may have advanced
        // the pointer. Reset both segment and position before sending again.
        let transient_data = platform == Platform::Ch32 && matches!(error, Error::TransientRam(_));
        let limit = if transient_data { 8 } else { 3 };
        if attempt_number >= limit {
            return Err(error);
        }
        let delay_ms = if transient_data { (10u64 << (attempt_number - 1)).min(100) } else { 10 };
        log::warn!("Restarting uncommitted RAM block 0x{:04x} after attempt {}: {}; waiting {} ms",
                   offset, attempt_number, error, delay_ms);
        bus.wait_ms(delay_ms);
        attempt_number += 1;
    }
}

fn commit_block<T: Transport>(bus: &mut T) -> Result<()> {
    // A failed write may or may not have reached the MCU. Stop, leaving its
    // bootloader available for an explicit restart; do not blindly erase again.
    bus.write(0xd0, 0xa0).map_err(Error::Io)?;
    bus.wait_ms(50);
    for _ in 0..100 {
        match bus.read(0xd0) {
            Ok(status) if status & !0x09 != 0 => {
                return Err(Error::Refused("Unknown upgrade status bits".into()));
            }
            Ok(status) if status & 8 != 0 => {}
            Ok(status) if status & 1 != 0 => {
                return Err(Error::Refused(
                    "MCU rejected the image/block; no automatic erase retry".into(),
                ))
            }
            Ok(_) => return Ok(()),
            Err(_) => {}
        }
        bus.wait_ms(20);
    }
    Err(Error::Timeout("waiting for Flash acknowledgement"))
}

/// All candidates must already have been validated in memory by image::load.
/// No firmware mutation occurs until platform/type and transition checks pass.
pub fn upgrade<T: Transport>(
    bus: &mut T,
    candidates: &[Image],
    requested: Kind,
    allow_reset: bool,
) -> Result<Identity> {
    let initial = identify(bus)?;
    let mut matches = candidates
        .iter()
        .filter(|image| image.platform == initial.platform && image.kind == requested);
    let image = matches.next().ok_or_else(|| {
        Error::Refused(format!(
            "No {} image for detected {}",
            requested.name(),
            initial.platform.name()
        ))
    })?;
    if matches.next().is_some() {
        return Err(Error::Refused("Ambiguous firmware selection".into()));
    }
    target_matches(initial, image)?;
    let expected = if requested == Kind::Bootloader {
        MODE_BOOTAPP
    } else {
        MODE_BOOT
    };
    if initial.mode != expected {
        if expected == MODE_BOOTAPP {
            return Err(Error::Refused(
                "Install and run the matching CH32 bootapp before updating its bootloader".into(),
            ));
        }
        if !allow_reset {
            return Err(Error::Refused(
                "Application is running; use -r to enter bootloader after validation".into(),
            ));
        }
        reset(bus)?;
        wait_for_mode(bus, image, expected, false)?;
    }
    // Old FM bootloader has no identity field. Only the successful all-zero
    // legacy response is accepted here, bound to the already validated FM image.
    let before_write = identify(bus)?;
    target_matches(before_write, image)?;
    if before_write.mode != expected {
        return Err(Error::Refused("Device left the upgrade mode before writing".into()));
    }
    for (n, data) in image.bytes.chunks(BLOCK).enumerate() {
        let mut block = [0xff; BLOCK];
        block[..data.len()].copy_from_slice(data);
        send_block(bus, image.platform, (n * BLOCK) as u16, &block)?;
        let at_commit = identify(bus)?;
        target_matches(at_commit, image)?;
        if at_commit.mode != expected {
            return Err(Error::Refused("Device changed mode before Flash commit".into()));
        }
        commit_block(bus)?;
        log::info!(
            "Accepted {}/{} bytes",
            ((n + 1) * BLOCK).min(image.bytes.len()),
            image.bytes.len()
        );
    }
    // Re-read identity as well as mode after the actual application starts;
    // successful RAM echo/Flash commands alone do not mean the update booted.
    let final_mode = if requested == Kind::Application {
        MODE_APP
    } else {
        MODE_BOOTAPP
    };
    // A reset can race the acknowledgement. Only an observed valid final mode
    // can turn this handoff into success, regardless of its write result.
    let _handoff_result = bus.write(0xd0, 0x10);
    bus.wait_ms(50);
    if requested == Kind::Bootloader {
        // Bootapp also existed before the update. Seeing it again alone cannot
        // prove reset ran the newly written bootloader. Observe the intermediate
        // boot mode, then explicitly launch bootapp (identity reads extend the
        // bootloader's idle timeout, so polling must not wait for auto-jump).
        wait_for_mode(bus, image, MODE_BOOT, true)?;
        let _launch_result = bus.write(0xd0, 0x10);
        bus.wait_ms(50);
    }
    wait_for_mode(bus, image, final_mode, true)
}
