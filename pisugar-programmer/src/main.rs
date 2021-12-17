use std::fs;
use std::io;
use std::io::Read;
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;

use clap::{App, Arg};
use rppal::i2c::I2c;
use rppal::i2c::Result as I2cResult;
use sysinfo::{ProcessExt, RefreshKind, SystemExt};

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
const MODE_BOOTAPP: u8 = 0xba;
const SEG_SIZE: usize = 512;

fn show_warning() {
    println!("WARNING:");
    println!("1. PLEASE CONFIRM THAT THE BATTERY IS FULLY CHARGED");
    println!("2. SYSTEMD SERVICE pisugar-server MUST BE STOPPED");
    println!("OTHERWISE UPGRADE MAY NOT SUCCEED!!!");
    println!("CONFIRM? (y or n): ");
    let mut confirm = String::new();
    io::stdin().read_line(&mut confirm).unwrap();
    if !confirm.to_lowercase().trim_start().starts_with("y") {
        exit(0);
    }

    loop {
        let refresh_kind = RefreshKind::default();
        let refresh_kind = refresh_kind.with_processes();
        let sys = sysinfo::System::new_with_specifics(refresh_kind);
        let mut running = false;
        for (pid, p) in sys.processes() {
            if p.name().contains("pisugar-server") {
                println!("WARNING: pisugar-server is running, pid {}", pid);
                println!("Run 'sudo systemctl stop pisugar-server' to stop the service");
                running = true;
                break;
            }
        }
        if !running {
            break;
        }
        sleep(Duration::from_secs(1));
    }
}

fn to_u16(s: &str) -> u16 {
    let mut hexadecimal = false;
    let digits;
    if s.starts_with("0x") {
        digits = s.trim_start_matches("0x");
        hexadecimal = true;
    } else if s.starts_with("0X") {
        digits = s.trim_start_matches("0X");
        hexadecimal = true;
    } else {
        digits = s;
    }
    if hexadecimal {
        return u16::from_str_radix(digits, 16).unwrap();
    }
    return u16::from_str_radix(digits, 10).unwrap();
}

fn main() {
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
                .takes_value(false)
                .help("Automatically reset to bootloader mode"),
        )
        .arg(
            Arg::with_name("file")
                .required(true)
                .help("Firmware file, e.g. pisugar-3-application.bin"),
        )
        .get_matches();

    let bus: u8 = to_u16(matches.value_of("bus").unwrap()) as u8;
    let addr: u16 = to_u16(matches.value_of("addr").unwrap());
    let reset: bool = matches.is_present("reset");
    let file = matches.value_of("file").unwrap();

    let mut i2c = I2c::with_bus(bus).unwrap();
    i2c.set_slave_address(addr).unwrap();

    show_warning();

    let mut f = fs::File::open(file).unwrap();
    println!();
    println!("Firmware size: {}", f.metadata().unwrap().len());

    // Detect pisugar bootloader
    loop {
        // Check pisugar version
        let pisugar_version = i2c.smbus_read_byte(CMD_VER);
        match pisugar_version {
            Ok(version) => {
                if version == PISUGAR_VER {
                    println!("PiSugar version: {}", version);

                    // Check pisugar mode
                    let pisugar_mode = i2c.smbus_read_byte(CMD_MODE);
                    match pisugar_mode {
                        Ok(mode) => {
                            if mode == MODE_BOOTLOADER {
                                println!("PiSugar mode: bootloader({:02x})", mode);
                                println!("PiSugar bootloader mode detected");
                                break;
                            }
                            if mode == MODE_APPLICATION {
                                println!("PiSugar mode: application({:02x})", mode);
                                println!("PiSugar application mode detected");
                                if reset {
                                    println!("Send reset to application...");
                                    let _ = send_reset(&i2c);
                                }
                            }
                            if mode == MODE_BOOTAPP {
                                println!("PiSugar mode: bootapp({:02x})", mode);
                                println!("PiSugar bootapp mode detected");
                                if !file.contains("bootloader") {
                                    println!("PiSugar bootapp need a bootloader file");
                                    return;
                                }
                                break;
                            }
                        }
                        Err(e) => {
                            println!("I2c error: {}", e);
                        }
                    }
                }
            }
            _ => {}
        }

        println!("PiSugar bootloader/bootapp not ready, please reset or wait, retry...");
        sleep(Duration::from_millis(100));
    }

    // Upgrade
    let mut buff = [0; SEG_SIZE];
    let mut offset: u16 = 0;
    while let Ok(n) = f.read(&mut buff) {
        if n == 0 {
            break;
        }
        let buff = &buff[..n];

        println!();
        println!("Seg offset: {}, size: {}", offset, n);

        // Send seg
        while send_seg(&i2c, offset as u16).is_err() {
            println!("Send seg offset {} error, retry...", offset);
            sleep(Duration::from_millis(50));
        }

        // Send seg pos
        let (pos, _) = offset.overflowing_sub(1);
        while send_pos(&i2c, pos).is_err() {
            println!("Send pos {} error, retry...", pos);
            sleep(Duration::from_millis(50));
        }

        // Send data
        for i in 0..buff.len() {
            while send_data(&i2c, buff[i]).is_err() {
                // reset pos to offset - 1
                let (pos, _) = offset.overflowing_sub(1);
                println!("Send data of {} error, reset pos to {}", offset, pos);
                while send_pos(&i2c, pos).is_err() {
                    println!("Send pos {} error, retry...", pos);
                    sleep(Duration::from_millis(50));
                }
            }
            offset += 1;
        }

        // Write flash
        println!("Writing flash...");
        loop {
            let mut ctrl;

            // Read ctrl
            loop {
                match i2c.smbus_read_byte(CMD_CTRL) {
                    Ok(r) => {
                        ctrl = r;
                        break;
                    }
                    _ => {
                        println!("Read upgrade ctrl error, retry...");
                        sleep(Duration::from_millis(50));
                    }
                }
            }

            // Enable write
            ctrl |= 1 << 7;
            ctrl |= 1 << 5;
            while i2c.smbus_write_byte(CMD_CTRL, ctrl).is_err() {
                println!("Enable flash write error, retry...");
                sleep(Duration::from_millis(50));
            }

            // Wait for result
            sleep(Duration::from_millis(50));
            loop {
                match i2c.smbus_read_byte(CMD_CTRL) {
                    Ok(r) => {
                        ctrl = r;
                        break;
                    }
                    _ => {
                        println!("Read upgrade ctrl error, retry...");
                        sleep(Duration::from_millis(50));
                    }
                }

                // Not done
                if ctrl & (1 << 3) != 0 {
                    println!("Stilling writing, retry...");
                    sleep(Duration::from_millis(50));
                    continue;
                }
                break;
            }

            // Write error
            if ctrl & 1 != 0 {
                println!("Write error, retry...");
                sleep(Duration::from_millis(50));
                continue;
            }

            println!("Write ok");
            break;
        }
    }

    println!();
    println!("Upgrade finished!");
    println!("Wait 1s, PiSugar will jump to application soon!");
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
