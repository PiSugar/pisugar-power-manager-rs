use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::convert::TryInto;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

pub const HEADER: usize = 0x200;
pub const HEADER_END: usize = HEADER + 32;
pub const MAX_FILE: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    Fm,
    Ch32,
}

impl Platform {
    pub fn name(self) -> &'static str {
        match self {
            Self::Fm => "fm33lc0x",
            Self::Ch32 => "ch32l103",
        }
    }
    pub fn id(self) -> u32 {
        match self {
            Self::Fm => 1,
            Self::Ch32 => 2,
        }
    }
    pub fn board(self) -> u32 {
        match self {
            Self::Fm => 0x03000001,
            Self::Ch32 => 0x03400002,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Application,
    Bootloader,
    Bootapp,
}

impl Kind {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "application" => Ok(Self::Application),
            "bootloader" => Ok(Self::Bootloader),
            "bootapp" => Ok(Self::Bootapp),
            _ => Err(format!("Unknown image type: {}", s)),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Bootloader => "bootloader",
            Self::Bootapp => "bootapp",
        }
    }
    pub fn id(self) -> u32 {
        match self {
            Self::Application => 1,
            Self::Bootloader => 2,
            Self::Bootapp => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Image {
    pub platform: Platform,
    pub kind: Kind,
    pub base: u32,
    pub bytes: Vec<u8>,
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

pub fn image_crc(bytes: &[u8]) -> u32 {
    let mut hash = crc32fast::Hasher::new();
    hash.update(&bytes[..HEADER + 16]);
    hash.update(&[0; 4]);
    hash.update(&bytes[HEADER + 20..]);
    hash.finalize()
}

impl Image {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() < HEADER_END || bytes.len() > 0xc000 {
            return Err("Image length is outside the supported partitions".into());
        }
        if word(&bytes, HEADER) != 0x33435350 || word(&bytes, HEADER + 20) != 2 {
            return Err("Missing supported OTA v2 header; untagged legacy BINs are not accepted".into());
        }
        let platform = match word(&bytes, HEADER + 24) {
            1 => Platform::Fm,
            2 => Platform::Ch32,
            _ => return Err("Unknown MCU platform".into()),
        };
        let kind = match word(&bytes, HEADER + 28) {
            1 => Kind::Application,
            2 => Kind::Bootloader,
            3 => Kind::Bootapp,
            _ => return Err("Unknown image type (factory images are not OTA images)".into()),
        };
        if word(&bytes, HEADER + 4) != platform.board() {
            return Err("Hardware board does not match MCU platform".into());
        }
        let (base, limit) = match (platform, kind) {
            (Platform::Fm, Kind::Application) => (0x4000, 0xb800),
            (Platform::Fm, _) => return Err("FM bootloader maintenance is not supported by this migration".into()),
            (Platform::Ch32, Kind::Bootloader) => (0x08000000, 0x3e00),
            (Platform::Ch32, _) => (0x08004000, 0xc000),
        };
        if word(&bytes, HEADER + 8) != base || word(&bytes, HEADER + 12) as usize != bytes.len() || bytes.len() > limit
        {
            return Err("Image base/length is invalid or overlaps configuration storage".into());
        }
        match platform {
            Platform::Fm => {
                let stack = word(&bytes, 0);
                let entry = word(&bytes, 4);
                // Historical FM linker uses a 24 KiB stack limit. Preserve that
                // compatibility here; do not silently change the deployed layout.
                if stack <= 0x20000000
                    || stack > 0x20006000
                    || stack & 7 != 0
                    || entry & 1 == 0
                    || (entry & !1) < base + HEADER_END as u32
                    || (entry & !1) >= base + bytes.len() as u32
                {
                    return Err("Invalid FM ARM vector table".into());
                }
            }
            Platform::Ch32 => {
                let instruction = word(&bytes, 0);
                let displacement = (((instruction >> 31) << 20)
                    | (((instruction >> 12) & 0xff) << 12)
                    | (((instruction >> 20) & 1) << 11)
                    | (((instruction >> 21) & 0x3ff) << 1)) as i32;
                let displacement = (displacement << 11) >> 11;
                if instruction & 0xfff != 0x06f
                    || displacement < HEADER_END as i32
                    || displacement as usize >= bytes.len()
                {
                    return Err("Invalid CH32 RISC-V reset entry".into());
                }
            }
        }
        if image_crc(&bytes) != word(&bytes, HEADER + 16) {
            return Err("Image CRC32 mismatch".into());
        }
        Ok(Self {
            platform,
            kind,
            base,
            bytes,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u32,
    images: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    platform: String,
    kind: String,
    board: u32,
    base: u32,
    file: String,
    size: usize,
    sha256: String,
}

pub fn load(path: &Path) -> Result<Vec<Image>, String> {
    let file = fs::File::open(path).map_err(|e| format!("Open firmware: {}", e))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE as u64 {
        return Err("Firmware package exceeds 256 KiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE {
        return Err("Firmware file grew beyond the size limit".into());
    }
    load_bytes(bytes)
}

pub fn load_bytes(bytes: Vec<u8>) -> Result<Vec<Image>, String> {
    if !bytes.starts_with(b"PK") {
        return Ok(vec![Image::parse(bytes)?]);
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("Invalid ZIP: {}", e))?;
    if archive.len() < 2 || archive.len() > 3 {
        return Err("OTA ZIP must contain manifest.json and one or two application images".into());
    }
    let mut files = std::collections::HashMap::new();
    let mut expanded = 0usize;
    for n in 0..archive.len() {
        let mut file = archive.by_index(n).map_err(|e| e.to_string())?;
        let name = file.name().to_string();
        if name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name.contains(':')
            || name.contains("..")
            || file.is_dir()
            || file.unix_mode().map(|m| m & 0o170000 == 0o120000).unwrap_or(false)
            || files.contains_key(&name)
        {
            return Err("Unsafe or duplicate archive member".into());
        }
        let limit = if name == "manifest.json" { 16384 } else { 0xc000 };
        if file.size() > limit as u64 {
            return Err("Archive member exceeds its size limit".into());
        }
        let mut data = Vec::new();
        (&mut file)
            .take(limit as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        expanded += data.len();
        if data.len() > limit || expanded > MAX_FILE {
            return Err("Expanded archive exceeds its size limit".into());
        }
        files.insert(name, data);
    }
    let manifest_bytes = files.remove("manifest.json").ok_or("Missing manifest.json")?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|e| format!("Invalid manifest: {}", e))?;
    if manifest.format != "pisugar-ota"
        || manifest.version != 1
        || manifest.images.is_empty()
        || manifest.images.len() > 2
    {
        return Err("Unsupported OTA manifest".into());
    }
    let mut platforms = HashSet::new();
    let mut images = Vec::new();
    for entry in manifest.images {
        let data = files
            .remove(&entry.file)
            .ok_or("Missing or multiply referenced image")?;
        if data.len() != entry.size
            || entry.sha256.len() != 64
            || format!("{:x}", Sha256::digest(&data)) != entry.sha256
        {
            return Err("Package image length/SHA256 mismatch".into());
        }
        let image = Image::parse(data)?;
        if entry.platform != image.platform.name()
            || entry.kind != "application"
            || image.kind != Kind::Application
            || entry.board != image.platform.board()
            || entry.base != image.base
            || !platforms.insert(image.platform)
        {
            return Err("Manifest identity mismatch or ambiguous platform selection".into());
        }
        images.push(image);
    }
    if !files.is_empty() {
        return Err("Unlisted archive members are not allowed".into());
    }
    Ok(images)
}
