use pisugar_programmer::image::{self, Image, Kind, Platform, HEADER};
use pisugar_programmer::protocol::{self, Error, Transport, MODE_APP, MODE_BOOT, MODE_BOOTAPP};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn stamp(bytes: &mut [u8]) {
    put(bytes, HEADER + 16, image::image_crc(bytes));
}
fn firmware(platform: Platform, kind: Kind, length: usize) -> Image {
    let mut bytes = vec![0x5a; length];
    let base = match (platform, kind) {
        (Platform::Fm, _) => 0x4000,
        (Platform::Ch32, Kind::Bootloader) => 0x08000000,
        _ => 0x08004000,
    };
    if platform == Platform::Fm {
        put(&mut bytes, 0, 0x20004000);
        put(&mut bytes, 4, base + 0x241);
    } else {
        // JAL x0, +0x240.
        put(&mut bytes, 0, 0x2400006f);
    }
    let words = [
        0x33435350,
        platform.board(),
        base,
        length as u32,
        0,
        2,
        platform.id(),
        kind.id(),
    ];
    for (n, value) in words.iter().enumerate() {
        put(&mut bytes, HEADER + n * 4, *value);
    }
    stamp(&mut bytes);
    Image::parse(bytes).unwrap()
}

fn marker(platform: Platform) -> [u8; 8] {
    let mut value = [
        b'P',
        b'S',
        b'I',
        b'D',
        1,
        platform.id() as u8,
        (platform == Platform::Ch32) as u8,
        0,
    ];
    value[7] = value[..7].iter().fold(0, |a, b| a ^ b);
    value
}

struct Device {
    platform: Platform,
    mode: u8,
    legacy: bool,
    marker_override: Option<[u8; 8]>,
    identity_error: bool,
    bus_dead: bool,
    change_on_reset: bool,
    busy: bool,
    reject: bool,
    fail_commit: bool,
    no_boot: bool,
    bad_identity_after_data: bool,
    exit_after_first_block: bool,
    transition_on_reset: bool,
    transition_reads: usize,
    status_override: Option<u8>,
    byte_fault: bool,
    segment: u16,
    position: u16,
    buffer: [u8; 512],
    flash: Vec<u8>,
    commits: usize,
    resets: usize,
    writes: usize,
    reads: usize,
    data_count: usize,
    waited: u64,
}
impl Device {
    fn new(platform: Platform, mode: u8, legacy: bool) -> Self {
        Self {
            platform,
            mode,
            legacy,
            marker_override: None,
            identity_error: false,
            bus_dead: false,
            change_on_reset: false,
            busy: false,
            reject: false,
            fail_commit: false,
            no_boot: false,
            bad_identity_after_data: false,
            exit_after_first_block: false,
            transition_on_reset: false,
            transition_reads: 0,
            status_override: None,
            byte_fault: false,
            segment: 0,
            position: 0xffff,
            buffer: [0xff; 512],
            flash: vec![],
            commits: 0,
            resets: 0,
            writes: 0,
            reads: 0,
            data_count: 0,
            waited: 0,
        }
    }
}
impl Transport for Device {
    fn read(&mut self, register: u8) -> Result<u8, String> {
        self.reads += 1;
        if self.bus_dead || (self.identity_error && (0x52..=0x59).contains(&register)) {
            return Err("NACK".into());
        }
        Ok(match register {
            0 => 3,
            1 => {
                if self.resets > 0 && self.transition_on_reset {
                    self.transition_reads += 1;
                    if self.transition_reads == 2 {
                        self.mode = MODE_BOOT;
                        self.transition_on_reset = false;
                    }
                }
                self.mode
            }
            0x10..=0x1f if self.mode == MODE_APP && (self.platform == Platform::Fm || register < 0x1c) => register,
            0x52..=0x59 => {
                self.marker_override
                    .unwrap_or_else(|| if self.legacy { [0; 8] } else { marker(self.platform) })
                    [(register - 0x52) as usize]
            }
            0xd1 => (self.segment >> 8) as u8,
            0xd2 => self.segment as u8,
            0xd3 => (self.position >> 8) as u8,
            0xd4 => self.position as u8,
            0xdd => self.buffer[self.position.wrapping_sub(self.segment) as usize],
            0xd0 => {
                if let Some(status) = self.status_override {
                    return Ok(status);
                }
                if self.busy {
                    8
                } else if self.reject {
                    1
                } else {
                    0
                }
            }
            _ => 0,
        })
    }
    fn write(&mut self, register: u8, value: u8) -> Result<(), String> {
        self.writes += 1;
        if self.bus_dead {
            return Err("NACK".into());
        }
        match register {
            0x03 if value == 0x80 => {
                self.resets += 1;
                self.mode = MODE_BOOT;
                if self.transition_on_reset {
                    self.mode = MODE_APP;
                }
                self.legacy = self.platform == Platform::Fm;
                if self.change_on_reset {
                    self.marker_override = Some([0; 8]);
                }
            }
            0xd1 => self.segment = (self.segment & 0xff) | ((value as u16) << 8),
            0xd2 => {
                self.segment = (self.segment & 0xff00) | value as u16;
                self.buffer = [0xff; 512];
            }
            0xd3 => self.position = (self.position & 0xff) | ((value as u16) << 8),
            0xd4 => self.position = (self.position & 0xff00) | value as u16,
            0xdd => {
                self.data_count += 1;
                if self.exit_after_first_block && self.data_count >= 1024 {
                    self.mode = MODE_APP;
                }
                if self.bad_identity_after_data && self.data_count >= 512 {
                    self.marker_override = Some([0xaa; 8]);
                }
                if self.byte_fault && self.data_count == 17 {
                    return Err("transient".into());
                }
                self.position = self.position.wrapping_add(1);
                self.buffer[self.position.wrapping_sub(self.segment) as usize] = value;
            }
            0xd0 if value == 0xa0 => {
                self.commits += 1;
                if self.fail_commit {
                    return Err("ambiguous write".into());
                }
                if !self.reject {
                    self.flash.resize(self.segment as usize + 512, 0xff);
                    self.flash[self.segment as usize..self.segment as usize + 512].copy_from_slice(&self.buffer);
                }
            }
            0xd0 if value == 0x10 => {
                if !self.no_boot {
                    let length = u32::from_le_bytes(self.flash[HEADER + 12..HEADER + 16].try_into().unwrap()) as usize;
                    let updated = Image::parse(self.flash[..length].to_vec()).map_err(|e| e.to_string())?;
                    self.mode = match (updated.kind, self.mode) {
                        (Kind::Application, _) => MODE_APP,
                        (Kind::Bootloader, MODE_BOOTAPP) => MODE_BOOT,
                        _ => MODE_BOOTAPP,
                    };
                    self.legacy = false;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn wait_ms(&mut self, milliseconds: u64) {
        self.waited += milliseconds;
    }
}
use std::convert::TryInto;

/// Inject valid I2C byte responses while the application register cache starts.
/// The first handoff is the last write allowed, including at timeout.
struct StartingApplication {
    device: Device,
    register: u8,
    value: u8,
    remaining: usize,
    launched: bool,
    inject_before_launch: bool,
}

impl StartingApplication {
    fn new(register: u8, value: u8, remaining: usize) -> Self {
        Self {
            device: Device::new(Platform::Fm, MODE_APP, true),
            register, value, remaining, launched: false, inject_before_launch: false,
        }
    }

    fn update(&mut self) -> Result<protocol::Identity, Error> {
        protocol::upgrade(self, &[firmware(Platform::Fm, Kind::Application, 1101)], Kind::Application, true)
    }
}

impl Transport for StartingApplication {
    fn read(&mut self, register: u8) -> Result<u8, String> {
        if (self.launched || self.inject_before_launch) && self.remaining > 0 && register == self.register {
            self.remaining -= 1;
            return Ok(self.value);
        }
        self.device.read(register)
    }

    fn write(&mut self, register: u8, value: u8) -> Result<(), String> {
        assert!(!self.launched, "unexpected write after application handoff");
        self.device.write(register, value)?;
        if (register, value) == (0xd0, 0x10) {
            self.launched = true;
        }
        Ok(())
    }

    fn wait_ms(&mut self, milliseconds: u64) {
        self.device.wait_ms(milliseconds);
    }
}

#[test]
fn application_handoff_waits_for_initialized_version_and_mode() {
    for register in [0x00, 0x01] {
        let mut bus = StartingApplication::new(register, 0, 3);
        let identity = bus.update().unwrap();
        assert_eq!((identity.platform, identity.mode, identity.legacy), (Platform::Fm, MODE_APP, false));
        assert_eq!(bus.remaining, 0);
        assert_eq!(bus.device.commits, 3);
    }
}

#[test]
fn application_handoff_times_out_with_persistently_invalid_device_header() {
    let mut bus = StartingApplication::new(0x00, 0xff, 101);
    assert!(matches!(bus.update(), Err(Error::Timeout(_))));
    assert_eq!(bus.remaining, 1);
    assert_eq!(bus.device.commits, 3);
}

#[test]
fn application_handoff_refuses_an_unknown_marker_even_if_only_the_first_sample_is_bad() {
    let mut bus = StartingApplication::new(0x57, 0x7f, 1);
    assert!(matches!(bus.update(), Err(Error::Refused(_))));
    assert!(bus.launched);
    assert_eq!(bus.device.commits, 3);
}

#[test]
fn invalid_device_header_is_not_retried_before_reset_or_flash_writes() {
    for register in [0x00, 0x01] {
        let mut bus = StartingApplication::new(register, 2, 1);
        bus.inject_before_launch = true;
        assert!(bus.update().is_err());
        assert_eq!((bus.device.writes, bus.device.commits, bus.device.waited), (0, 0, 0));
    }
}

#[test]
fn identity_distinguishes_real_legacy_from_failed_reads() {
    for platform in [Platform::Fm, Platform::Ch32] {
        for mode in [MODE_APP, MODE_BOOT, MODE_BOOTAPP] {
            let mut device = Device::new(platform, mode, false);
            assert_eq!(protocol::identify(&mut device).unwrap().platform, platform);
        }
    }
    let mut legacy = Device::new(Platform::Fm, MODE_APP, true);
    assert!(protocol::identify(&mut legacy).unwrap().legacy);
    legacy.identity_error = true;
    assert!(matches!(protocol::identify(&mut legacy), Err(Error::Io(_))));
    assert_eq!(legacy.writes, 0);
}

#[test]
fn malformed_and_unknown_identities_refuse_without_writes() {
    for index in 0..8 {
        let mut device = Device::new(Platform::Ch32, MODE_APP, false);
        let mut damaged = marker(Platform::Ch32);
        damaged[index] ^= 0x40;
        device.marker_override = Some(damaged);
        assert!(protocol::upgrade(
            &mut device,
            &[firmware(Platform::Ch32, Kind::Application, 1100)],
            Kind::Application,
            true
        )
        .is_err());
        assert_eq!(device.writes, 0);
    }
}

#[test]
fn cross_platform_images_are_rejected_before_reset_or_erase() {
    for (device_platform, image_platform) in [(Platform::Fm, Platform::Ch32), (Platform::Ch32, Platform::Fm)] {
        for mode in [MODE_APP, MODE_BOOT] {
            let mut device = Device::new(device_platform, mode, false);
            assert!(protocol::upgrade(
                &mut device,
                &[firmware(image_platform, Kind::Application, 1100)],
                Kind::Application,
                true
            )
            .is_err());
            assert_eq!((device.writes, device.commits, device.resets), (0, 0, 0));
        }
    }
}

#[test]
fn legacy_fm_first_migration_and_subsequent_update_work() {
    let image = firmware(Platform::Fm, Kind::Application, 1101);
    for legacy in [true, false] {
        let mut device = Device::new(Platform::Fm, MODE_APP, legacy);
        let result = protocol::upgrade(&mut device, &[image.clone()], Kind::Application, true).unwrap();
        assert_eq!(
            result,
            protocol::Identity {
                platform: Platform::Fm,
                mode: MODE_APP,
                legacy: false
            }
        );
        assert_eq!(&device.flash[..image.bytes.len()], &image.bytes);
        assert!(device.flash[image.bytes.len()..].iter().all(|b| *b == 0xff));
        assert_eq!(device.resets, 1);
    }
}

#[test]
fn normal_ch_update_and_transient_buffer_retry_are_exact() {
    for transient in [false, true] {
        let image = firmware(Platform::Ch32, Kind::Application, 1301);
        let mut device = Device::new(Platform::Ch32, MODE_APP, false);
        device.byte_fault = transient;
        protocol::upgrade(&mut device, &[image.clone()], Kind::Application, true).unwrap();
        assert_eq!(&device.flash[..image.bytes.len()], &image.bytes);
        assert_eq!(device.commits, 3);
    }
}

#[derive(Clone, Copy)]
enum RamFault {
    ReadNack,
    LostWriteAck,
    WrongEcho,
    OtherIo,
}

struct RamLink {
    device: Device,
    faults: Vec<RamFault>,
    injected: usize,
    starts: Vec<(u16, u16)>,
    waits_before_commit: Vec<u64>,
}

impl RamLink {
    fn new(platform: Platform, faults: Vec<RamFault>) -> Self {
        Self {
            device: if platform == Platform::Fm {
                Device::new(platform, MODE_APP, true)
            } else {
                Device::new(platform, MODE_BOOT, false)
            },
            faults, injected: 0, starts: Vec::new(), waits_before_commit: Vec::new(),
        }
    }
}

impl Transport for RamLink {
    fn read(&mut self, register: u8) -> Result<u8, String> {
        self.device.read(register)
    }

    fn write(&mut self, register: u8, value: u8) -> Result<(), String> {
        self.device.write(register, value)?;
        if register == 0xd4 {
            self.starts.push((self.device.segment, self.device.position));
        }
        Ok(())
    }

    fn exchange_ram_byte(&mut self, value: u8) -> Result<u8, Error> {
        // Every failed write in this model reached the target and advanced its
        // position. Retrying DD alone would write the wrong next byte.
        self.device.write(0xdd, value).map_err(Error::Io)?;
        let fail_at = [17, 241, 407][self.injected % 3];
        if self.device.segment == 0 && self.device.position == fail_at {
            if let Some(fault) = self.faults.get(self.injected).copied() {
                self.injected += 1;
                return match fault {
                    RamFault::ReadNack => Err(Error::TransientRam("read DD: EREMOTEIO".into())),
                    RamFault::LostWriteAck => Err(Error::TransientRam("write DD: EIO".into())),
                    RamFault::WrongEcho => Ok(value ^ 1),
                    RamFault::OtherIo => Err(Error::Io("batch input unavailable".into())),
                };
            }
        }
        self.device.read(0xdd).map_err(Error::Io)
    }

    fn wait_ms(&mut self, milliseconds: u64) {
        if self.device.commits == 0 {
            self.waits_before_commit.push(milliseconds);
        }
        self.device.wait_ms(milliseconds);
    }
}

#[test]
fn mixed_ch_ram_faults_restage_the_entire_segment_then_commit_exactly_once() {
    let mut link = RamLink::new(Platform::Ch32, vec![RamFault::ReadNack, RamFault::LostWriteAck, RamFault::ReadNack]);
    let image = firmware(Platform::Ch32, Kind::Application, 1301);
    protocol::upgrade(&mut link, std::slice::from_ref(&image), Kind::Application, true).unwrap();
    assert_eq!(link.injected, 3);
    assert_eq!(link.starts, [(0, 0xffff), (0, 0xffff), (0, 0xffff), (0, 0xffff), (512, 511), (1024, 1023)]);
    assert_eq!(link.waits_before_commit, [10, 20, 40]);
    assert_eq!(link.device.commits, 3);
    assert_eq!(&link.device.flash[..image.bytes.len()], &image.bytes);
    assert!(link.device.flash[image.bytes.len()..].iter().all(|byte| *byte == 0xff));
}

#[test]
fn persistent_ch_ram_io_stops_with_capped_backoff_and_no_commit() {
    let mut link = RamLink::new(Platform::Ch32, vec![RamFault::ReadNack; 9]);
    let image = firmware(Platform::Ch32, Kind::Application, 1301);
    assert!(protocol::upgrade(&mut link, &[image], Kind::Application, true).is_err());
    assert_eq!(link.injected, 8);
    assert_eq!(link.starts, [(0, 0xffff); 8]);
    assert_eq!(link.waits_before_commit, [10, 20, 40, 80, 100, 100, 100]);
    assert_eq!(link.device.commits, 0);
    assert!(link.device.flash.is_empty());
}

#[test]
fn wrong_echo_other_io_and_fm_keep_the_original_three_attempt_limit() {
    for (platform, fault) in [(Platform::Ch32, RamFault::WrongEcho), (Platform::Ch32, RamFault::OtherIo),
                              (Platform::Fm, RamFault::LostWriteAck)] {
        let mut link = RamLink::new(platform, vec![fault; 9]);
        let image = firmware(platform, Kind::Application, 1301);
        assert!(protocol::upgrade(&mut link, &[image], Kind::Application, true).is_err());
        assert_eq!(link.injected, 3);
        assert_eq!(link.starts, [(0, 0xffff); 3]);
        assert_eq!(link.device.commits, 0);
        assert!(link.device.flash.is_empty());
    }
}

#[test]
fn unmarked_ch_prototype_is_never_selected_for_ch_image() {
    let mut device = Device::new(Platform::Ch32, MODE_APP, true);
    assert!(protocol::upgrade(
        &mut device,
        &[firmware(Platform::Ch32, Kind::Application, 1100)],
        Kind::Application,
        true
    )
    .is_err());
    assert_eq!(device.writes, 0);
}

#[test]
fn new_ch_app_on_old_unmarked_bootloader_cannot_write() {
    let mut device = Device::new(Platform::Ch32, MODE_APP, false);
    device.change_on_reset = true;
    assert!(protocol::upgrade(
        &mut device,
        &[firmware(Platform::Ch32, Kind::Application, 1100)],
        Kind::Application,
        true
    )
    .is_err());
    assert_eq!(device.resets, 1);
    assert_eq!(device.commits, 0);
}

#[test]
fn maintenance_requires_explicit_type_and_matching_mode() {
    let boot = firmware(Platform::Ch32, Kind::Bootloader, 1100);
    let mut device = Device::new(Platform::Ch32, MODE_APP, false);
    assert!(protocol::upgrade(&mut device, &[boot.clone()], Kind::Application, true).is_err());
    assert!(protocol::upgrade(&mut device, &[boot.clone()], Kind::Bootloader, true).is_err());
    assert_eq!(device.writes, 0);
    let mut device = Device::new(Platform::Ch32, MODE_BOOTAPP, false);
    assert_eq!(
        protocol::upgrade(&mut device, &[boot], Kind::Bootloader, true)
            .unwrap()
            .mode,
        MODE_BOOTAPP
    );
}

#[test]
fn ignored_bootloader_reset_does_not_report_existing_bootapp_as_success() {
    let mut device = Device::new(Platform::Ch32, MODE_BOOTAPP, false);
    device.no_boot = true;
    let result = protocol::upgrade(
        &mut device,
        &[firmware(Platform::Ch32, Kind::Bootloader, 1100)],
        Kind::Bootloader,
        true,
    );
    assert!(matches!(result, Err(Error::Timeout(_))));
    assert_eq!(device.mode, MODE_BOOTAPP);
}

#[test]
fn changed_identity_before_first_commit_preserves_flash() {
    let mut device = Device::new(Platform::Ch32, MODE_BOOT, false);
    device.bad_identity_after_data = true;
    assert!(protocol::upgrade(
        &mut device,
        &[firmware(Platform::Ch32, Kind::Application, 1100)],
        Kind::Application,
        true
    )
    .is_err());
    assert_eq!(device.commits, 0);
    assert!(device.flash.is_empty());
}

#[test]
fn leaving_boot_mode_between_blocks_stops_before_next_commit() {
    let mut device = Device::new(Platform::Ch32, MODE_BOOT, false);
    device.exit_after_first_block = true;
    assert!(protocol::upgrade(
        &mut device,
        &[firmware(Platform::Ch32, Kind::Application, 1100)],
        Kind::Application,
        true
    )
    .is_err());
    assert_eq!(device.commits, 1);
}

#[test]
fn mode_transition_during_reset_sampling_can_be_retried() {
    let mut device = Device::new(Platform::Ch32, MODE_APP, false);
    device.transition_on_reset = true;
    let result = protocol::upgrade(
        &mut device,
        &[firmware(Platform::Ch32, Kind::Application, 1100)],
        Kind::Application,
        true,
    )
    .unwrap();
    assert_eq!(result.mode, MODE_APP);
}

#[test]
fn unknown_flash_status_does_not_advance_to_next_segment() {
    let mut device = Device::new(Platform::Ch32, MODE_BOOT, false);
    device.status_override = Some(0x20);
    assert!(protocol::upgrade(
        &mut device,
        &[firmware(Platform::Ch32, Kind::Application, 1100)],
        Kind::Application,
        true
    )
    .is_err());
    assert_eq!(device.commits, 1);
}

#[test]
fn mcu_rejection_and_ambiguous_commit_are_not_retried() {
    for ambiguous in [false, true] {
        let mut device = Device::new(Platform::Ch32, MODE_BOOT, false);
        device.reject = !ambiguous;
        device.fail_commit = ambiguous;
        assert!(protocol::upgrade(
            &mut device,
            &[firmware(Platform::Ch32, Kind::Application, 1100)],
            Kind::Application,
            true
        )
        .is_err());
        assert_eq!(device.commits, 1);
        assert!(device.flash.is_empty());
    }
}

#[test]
fn busy_device_and_failure_to_start_have_bounded_timeouts() {
    for busy in [true, false] {
        let mut device = Device::new(Platform::Ch32, MODE_BOOT, false);
        device.busy = busy;
        device.no_boot = !busy;
        assert!(matches!(
            protocol::upgrade(
                &mut device,
                &[firmware(Platform::Ch32, Kind::Application, 1100)],
                Kind::Application,
                true
            ),
            Err(Error::Timeout(_))
        ));
        assert!(device.waited < 6000);
        assert!(device.reads < 7000);
    }
}

/// Attach the target late, then reset it by hand after the former five-second
/// timeout. Reject any data or command writes before its bootloader appears.
struct DelayedTarget {
    device: Device,
    polls: usize,
    reset_commands: usize,
    unlocks: usize,
    ready_at: usize,
    writer_platform: Platform,
}

impl Transport for DelayedTarget {
    fn read(&mut self, register: u8) -> Result<u8, String> {
        match self.polls {
            0..=59 => Err("NACK".into()),
            60..=63 => Ok(0xff),
            64..=67 if register == 1 => Ok(0),
            _ => self.device.read(register),
        }
    }

    fn write(&mut self, register: u8, value: u8) -> Result<(), String> {
        if self.polls < self.ready_at {
            assert!(self.polls >= 68 && self.device.mode == MODE_APP);
            match (register, value) {
                (0x0b, 0x29) => self.unlocks += 1,
                (0x03, 0x80) => {
                    self.reset_commands += 1;
                    return Err("reset ACK lost".into());
                }
                pair => panic!("unexpected write before manual Reset: {:?}", pair),
            }
        }
        self.device.write(register, value)
    }

    fn wait_ms(&mut self, milliseconds: u64) {
        if self.polls < self.ready_at {
            assert_eq!(milliseconds, 100);
            self.polls += 1;
            if self.polls == self.ready_at {
                self.device.mode = MODE_BOOT;
                self.device.platform = self.writer_platform;
                self.device.legacy = self.writer_platform == Platform::Fm;
            }
        }
        self.device.wait_ms(milliseconds);
    }
}

#[test]
fn late_target_waits_for_manual_reset_and_does_not_repeat_an_unacknowledged_soft_reset() {
    for platform in [Platform::Fm, Platform::Ch32] {
        for soft_reset in [false, true] {
            let mut bus = DelayedTarget {
                device: Device::new(platform, MODE_APP, platform == Platform::Fm),
                polls: 0, reset_commands: 0, unlocks: 0, ready_at: 150, writer_platform: platform,
            };
            let image = firmware(platform, Kind::Application, 1100);
            protocol::upgrade(&mut bus, std::slice::from_ref(&image), Kind::Application, soft_reset).unwrap();
            assert_eq!(bus.polls, 150);
            assert_eq!((bus.unlocks, bus.reset_commands), (soft_reset as usize, soft_reset as usize));
            assert_eq!((bus.device.commits, bus.device.data_count), (3, 1536));
            assert_eq!(&bus.device.flash[..image.bytes.len()], &image.bytes);
        }
    }
}

#[test]
fn delayed_identified_ch_bootloader_accepts_a_direct_start_without_application_evidence() {
    for soft_reset in [false, true] {
        let mut bus = DelayedTarget {
            device: Device::new(Platform::Ch32, MODE_BOOT, false),
            polls: 0, reset_commands: 0, unlocks: 0, ready_at: 68, writer_platform: Platform::Ch32,
        };
        let image = firmware(Platform::Ch32, Kind::Application, 1100);
        protocol::upgrade(&mut bus, &[image], Kind::Application, soft_reset).unwrap();
        assert_eq!((bus.polls, bus.device.resets, bus.device.commits), (68, 0, 3));
    }
}

#[test]
fn manual_wait_never_reselects_another_platform_from_the_same_package() {
    let images = [
        firmware(Platform::Fm, Kind::Application, 1100),
        firmware(Platform::Ch32, Kind::Application, 1100),
    ];
    let mut bus = DelayedTarget {
        device: Device::new(Platform::Fm, MODE_APP, true),
        polls: 0, reset_commands: 0, unlocks: 0, ready_at: 150, writer_platform: Platform::Ch32,
    };
    assert!(matches!(protocol::upgrade(&mut bus, &images, Kind::Application, false), Err(Error::Refused(_))));
    assert_eq!((bus.polls, bus.device.writes, bus.device.commits), (150, 0, 0));
}

#[test]
fn unmarked_boot_targets_and_zero_padded_ch_applications_never_select_fm_from_a_unified_package() {
    let images = [
        firmware(Platform::Fm, Kind::Application, 1100),
        firmware(Platform::Ch32, Kind::Application, 1100),
    ];
    for mode in [MODE_BOOT, MODE_BOOTAPP, MODE_APP] {
        for soft_reset in [false, true] {
            let mut device = Device::new(Platform::Ch32, mode, true);
            assert!(matches!(
                protocol::upgrade(&mut device, &images, Kind::Application, soft_reset),
                Err(Error::Refused(_))
            ));
            assert_eq!((device.writes, device.commits), (0, 0));
        }
    }
}

#[test]
fn bad_headers_vectors_lengths_and_crc_are_rejected() {
    let original = firmware(Platform::Ch32, Kind::Application, 1100).bytes;
    for field in [0, 4, 8, 12, 20, 24, 28] {
        let mut damaged = original.clone();
        put(&mut damaged, HEADER + field, 0xdeadbeef);
        stamp(&mut damaged);
        assert!(Image::parse(damaged).is_err(), "field {}", field);
    }
    let mut crc_damage = original.clone();
    crc_damage[900] ^= 1;
    assert!(Image::parse(crc_damage).is_err());
    assert!(Image::parse(original[..1099].to_vec()).is_err());
    let mut bad_vector = original.clone();
    put(&mut bad_vector, 0, 0x20004000);
    stamp(&mut bad_vector);
    assert!(Image::parse(bad_vector).is_err());
    let mut trailing = original;
    trailing.push(0xff);
    assert!(Image::parse(trailing).is_err());
}

#[test]
fn fm_partition_cannot_overwrite_persistent_pages() {
    let mut oversized = firmware(Platform::Fm, Kind::Application, 0xb800).bytes;
    oversized.resize(0xb801, 0xff);
    put(&mut oversized, HEADER + 12, 0xb801);
    stamp(&mut oversized);
    assert!(Image::parse(oversized).is_err());
}

fn archive(images: &[Image], alter_manifest: bool, bad_member: bool) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut entries = Vec::new();
    for image in images {
        let name = if bad_member {
            "../image.bin".to_string()
        } else {
            format!("{}.bin", image.platform.name())
        };
        zip.start_file(&name, options).unwrap();
        zip.write_all(&image.bytes).unwrap();
        entries.push(serde_json::json!({"platform": image.platform.name(), "kind":"application", "board":image.platform.board(),"base":image.base,"file":name,"size":image.bytes.len(),"sha256":format!("{:x}",Sha256::digest(&image.bytes))}));
    }
    if alter_manifest {
        entries[0]["platform"] = serde_json::json!("wrong");
    }
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(
        serde_json::to_string(&serde_json::json!({"format":"pisugar-ota","version":1,"images":entries}))
            .unwrap()
            .as_bytes(),
    )
    .unwrap();
    zip.finish().unwrap().into_inner()
}

#[test]
fn unified_package_selects_each_platform_and_validates_other_image_too() {
    let images = [
        firmware(Platform::Fm, Kind::Application, 1100),
        firmware(Platform::Ch32, Kind::Application, 1200),
    ];
    let parsed = image::load_bytes(archive(&images, false, false)).unwrap();
    for platform in [Platform::Fm, Platform::Ch32] {
        let mut device = Device::new(platform, MODE_APP, platform == Platform::Fm);
        protocol::upgrade(&mut device, &parsed, Kind::Application, true).unwrap();
        let chosen = parsed.iter().find(|i| i.platform == platform).unwrap();
        assert_eq!(&device.flash[..chosen.bytes.len()], &chosen.bytes);
    }
    let mut damaged = images.clone();
    damaged[1].bytes[900] ^= 1;
    assert!(image::load_bytes(archive(&damaged, false, false)).is_err());
    assert!(image::load_bytes(archive(&images, true, false)).is_err());
    assert!(image::load_bytes(archive(&images, false, true)).is_err());
    assert!(image::load_bytes(archive(&[images[0].clone(), images[0].clone()], false, false)).is_err());
}
