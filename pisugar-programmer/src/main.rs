use std::fs;
use std::io;
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;

use clap::{App, Arg};
use rppal::i2c::Error as I2cError;
use rppal::i2c::I2c;
use rppal::i2c::Result as I2cResult;

const CMD_VER: u8 = 0x00;
const CMD_MODE: u8 = 0x01;
const CMD_APP_CTR2: u8 = 0x03;
const CMD_CTRL: u8 = 0xd0;
const CMD_SEG_H: u8 = 0xd1;
const CMD_SEG_L: u8 = 0xd2;
const CMD_POS_H: u8 = 0xd3;
const CMD_POS_L: u8 = 0xd4;
const CMD_DATA: u8 = 0xdd;

const PISUGAR_VER: u8 = 3;
const MODE_APPLICATION: u8 = 0x0f;
const MODE_BOOTLOADER: u8 = 0xf0;
const SEG_SIZE: u16 = 512;

fn show_warning() {
    println!("WARNING:");
    println!("1. PLEASE CONFIRM THAT THE BATTERY IS FULLY CHARGED");
    println!("2. SYSTEMD SERVICE pisugar-server MUST BE STOPPED");
    println!("    systemctl stop pisugar-server");
    println!("OTHERWISE UPGRADE MAY NOT SUCCEED!!!");
    print!("CONFIRM? (y or n): ");

    let mut confirm = String::new();
    io::stdin().read_line(&mut confirm).unwrap();
    if !confirm.to_lowercase().eq("y") {
        exit(0);
    }
}

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
        .arg(
            Arg::with_name("reset")
                .short("r")
                .default_value("false")
                .takes_value(true)
                .help("Automatically reset to bootloader mode"),
        )
        .arg(
            Arg::with_name("file")
                .index(1)
                .required(true)
                .help("Firmware file, e.g. pisugar-3-application.bin"),
        )
        .get_matches();

    let bus: u8 = matches.value_of("bus").unwrap().parse().unwrap();
    let addr: u16 = matches.value_of("addr").unwrap().parse().unwrap();
    let reset: bool = matches.value_of("reset").unwrap().parse().unwrap();
    let file = matches.value_of("file").unwrap();

    let i2c = I2c::with_bus(bus).unwrap();
    let content = fs::read(file).unwrap();
    log::info!("Firmware size: {}", content.len());

    show_warning();

    // Detect pisugar bootloader
    loop {
        let mut is_bootloader = false;

        // Check pisugar version
        let pisugar_version = i2c.smbus_read_byte(CMD_VER);
        match pisugar_version {
            Ok(version) => {
                if version == PISUGAR_VER {
                    log::info!("PiSugar version: {}", version);
                } else {
                    log::warn!("PiSugar not ready, retry...");
                    sleep(Duration::from_millis(100));
                    continue;
                }
            }
            _ => {
                log::warn!("PiSugar not ready, retry...");
                sleep(Duration::from_millis(100));
                continue;
            }
        }

        // Check pisugar mode
        let pisugar_mode = i2c.smbus_read_byte(CMD_MODE);
        match pisugar_mode {
            Ok(mode) => {
                log::info!("PiSugar mode: {:x}", mode);
                if mode == MODE_BOOTLOADER {
                    is_bootloader = true;
                    break;
                }
                if mode == MODE_APPLICATION {
                    log::warn!("PiSugar Application mode detected");
                    if reset {
                        log::info!("Send reset to application...");
                        send_reset(&i2c);
                        sleep(Duration::from_millis(100));
                        continue;
                    }
                }
            }
            _ => {}
        }

        if is_bootloader {
            break;
        }

        log::info!("PiSugar bootloader not ready, please reset or wait, retry...");
        sleep(Duration::from_millis(100));
    }

    // Upgrade
    for i in 0..content.len() {
        // Seg
        if i % SEG_SIZE as usize == 0 {
            log::info!("Seg offset: {}", i);

            // Send seg
            while send_seg(&i2c, i as u16).is_err() {
                log::warn!("Send seg offset {} error, retry...", i);
                sleep(Duration::from_millis(100));
            }

            // Send pos
            while send_pos(&i2c, (i - 1) as u16).is_err() {
                log::warn!("Send pos {} error, retry...", i);
                sleep(Duration::from_millis(100));
            }
        }

        // Send data
        loop {
            if send_data(&i2c, content[i]).is_err() {
                log::warn!("Send data of {} error, reset pos", i);
                while send_pos(&i2c, (i - 1) as u16).is_err() {
                    log::warn!("Send pos {} error, retry...", i);
                    sleep(Duration::from_millis(100));
                }
                continue;
            }
            break;
        }

        // Write flash
        if i != 0 && i % (SEG_SIZE as usize) == 0 || i == content.len() - 1 {
            let mut ctrl = 0_u8;

            loop {
                // Read ctrl
                loop {
                    match i2c.smbus_read_byte(CMD_CTRL) {
                        Ok(r) => {
                            ctrl = r;
                            break;
                        }
                        _ => {}
                    }
                    log::warn!("Read upgrade ctrl error, retry...");
                    sleep(Duration::from_millis(100));
                }

                // Enable write
                ctrl |= 1 << 7;
                ctrl |= 1 << 5;
                while i2c.smbus_write_byte(CMD_CTRL, ctrl).is_err() {
                    log::warn!("Enable flash write error, retry...");
                    sleep(Duration::from_millis(100));
                }

                // Wait for result
                sleep(Duration::from_millis(100));
                loop {
                    loop {
                        match i2c.smbus_read_byte(CMD_CTRL) {
                            Ok(r) => {
                                ctrl = r;
                                break;
                            }
                            _ => {}
                        }
                        log::warn!("Read upgrade ctrl error, retry...");
                        sleep(Duration::from_millis(100));
                    }

                    // Not done
                    if ctrl & (1 << 3) != 0 {
                        log::warn!("Stilling writing, retry...");
                        sleep(Duration::from_millis(100));
                        continue;
                    }

                    break;
                }

                // Write error
                if ctrl & 1 != 0 {
                    log::warn!("Write error, retry...");
                    sleep(Duration::from_millis(100));
                    continue;
                }

                break;
            }
        }
    }

    log::info!("Program finished!");
    log::info!("Wait 1s, PiSugar will jump to application soon!");
}

fn send_reset(i2c: &I2c) -> I2cResult<()> {
    i2c.smbus_write_byte(CMD_APP_CTR2, 1 << 7)
}

fn send_seg(i2c: &I2c, offset: u16) -> I2cResult<()> {
    i2c.smbus_write_byte(CMD_SEG_H, (offset >> 8) as u8)?;
    i2c.smbus_write_byte(CMD_SEG_L, (offset & 0xff) as u8)?;
    let seg_h = i2c.smbus_read_byte(CMD_SEG_H)?;
    let seg_l = i2c.smbus_read_byte(CMD_SEG_L)?;
    let seg: u16 = ((seg_h as u16) << 8) | (seg_l as u16);
    if seg != offset {
        return Err(io::Error::from(io::ErrorKind::InvalidData).into());
    }
    Ok(())
}

fn send_pos(i2c: &I2c, offset: u16) -> I2cResult<()> {
    i2c.smbus_write_byte(CMD_POS_H, (offset >> 8) as u8)?;
    i2c.smbus_write_byte(CMD_POS_L, (offset & 0xff) as u8)?;
    let pos_h = i2c.smbus_read_byte(CMD_POS_H)?;
    let pos_l = i2c.smbus_read_byte(CMD_POS_L)?;
    let pos = ((pos_h as u16) << 8) | (pos_l as u16);
    if pos != offset {
        return Err(io::Error::from(io::ErrorKind::InvalidData).into());
    }
    Ok(())
}

fn send_data(i2c: &I2c, data: u8) -> I2cResult<()> {
    i2c.smbus_write_byte(CMD_DATA, data)?;
    let data2 = i2c.smbus_read_byte(CMD_DATA)?;
    if data != data2 {
        return Err(io::Error::from(io::ErrorKind::InvalidData).into());
    }
    Ok(())
}
