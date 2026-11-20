# PiSugar programmer 2.4.2

The programmer validates the complete firmware file before resetting the MCU or
writing Flash. It supports FM33LC0x application migration and guarded CH32L103
updates. Existing register meanings and `-b`, `-a`, `-r`, `-d` options are retained.

The [programmer 2.4.2 release](https://github.com/PiSugar/pisugar-power-manager-rs/releases/tag/pisugar-programmer-v2.4.2)
provides Debian packages and standalone Linux executables for Raspberry Pi. The
[PiSugar 3 update script](../scripts/PiSugarUpdate.sh) installs the native
`pisugar-programmer` package on Raspberry Pi OS, replacing an older package at
`/usr/bin/pisugar-programmer`. Other supported Linux installations use
`/usr/local/bin/pisugar-programmer`. A normal update also installs the tool
when the board already has the target firmware.

To install or upgrade just the programmer, run the following from the directory
containing the script. This mode can run with the PiSugar board disconnected.

```sh
sudo bash PiSugarUpdate.sh --install-only
pisugar-programmer --version
```

For a firmware update the script downloads the dual-platform application ZIP from the PiSugar CDN.
It checks the program version and both download hashes, then reads the board's
platform and application version twice on bus 1 at 0x57. For this ZIP, FM targets
v1.4.2 and CH32 targets v1.4.3. An equal version skips flashing. A newer or
unrecognized version stops the update. On unmarked legacy boards it also checks
the serial field to distinguish old FM from the zero-padded 12-byte CH32 serial.
Ambiguous boards and boards in bootloader mode require a separately verified
recovery. After a write the script confirms the expected version on the same
platform. Run `bash PiSugarUpdate.sh --check` to verify downloads on a Pi
without accessing the hardware.

```sh
# Offline checks work on Linux and Windows and never open I2C.
pisugar-programmer --check pisugar-3-ota.zip

sudo systemctl stop pisugar-server
sudo pisugar-programmer -r pisugar-3-ota.zip
sudo systemctl start pisugar-server
```

The unified ZIP contains `manifest.json` and one or two platform-specific
application images. The programmer checks every listed image, even the platform
it will not install, then selects the image matching the connected device. A
tagged individual BIN is also supported. Filename changes cannot change platform
or image type. Untagged old BINs, factory BINs/HEX files, unknown identities and
damaged files are refused. CRC32 and SHA256 detect accidental corruption; they
are not firmware authentication or digital signatures.

## Existing FM devices

Install programmer 2.4.2 or newer on the Pi **before** updating its FM application.
Public versions through 2.4.1 predate the unified package and platform checks.
Successful, stable reads of PiSugar version 3, a recognized mode and eight zero
bytes at `0x52–0x59` are the legacy FM response. NACKs and timeouts never count as
an FM identity. The new FM application exposes an explicit identity. Its original
bootloader still returns zeros; this transition is accepted only for the FM
image already checked in that upgrade session. After the final application launch,
an FM board can briefly return zero for its product and mode registers. The
updater polls at 50 ms intervals for up to 100 identity checks. It reports success
only after reading the expected platform identity and application mode.

This migration does not rewrite the FM bootloader or bootapp. Users bypassing
this tool with an old programmer can still write incompatible files to an old
FM bootloader. Do not replace existing FM-only download URLs with CH32 binaries.

## CH32 devices

CH32 must expose its explicit identity in application, bootloader and bootapp
mode. The bootloader identity must advertise guarded OTA before writing. A new
application on an old, unmarked bootloader is refused. Earlier experimental CH32
boards without identity must be migrated through a separately verified factory
procedure; absence of identity is never permission to install a CH32 image.

Normal operation updates only `application`. CH32 bootloader maintenance needs
explicit image types and a stable power supply:

```sh
sudo pisugar-programmer -r --image bootapp ch32-bootapp.bin
sudo pisugar-programmer --image bootloader ch32-bootloader.bin
sudo pisugar-programmer -r ch32-application.bin
```

The bootapp and bootloader must both use the supported v2 format. Maintenance
temporarily replaces the application. Bootloader interruption can require a
hardware debugger; this tool does not promise rollback. FM maintenance is not
enabled in this migration. `--yes` skips the interactive power reminder only;
it does not bypass any file, platform or image-type check.

After writing the CH32 bootloader, the tool must observe bootloader mode after
reset before launching and confirming bootapp again. Seeing the pre-existing
bootapp alone is not reported as successful bootloader activation.

## Wire and file contract

Identity registers `0x52–0x59`: bytes `50 53 49 44`, schema `01`, platform
`01` (FM) or `02` (CH32), capabilities `00` (FM) or `01` (guarded CH32), then XOR
of the preceding seven bytes. Two complete, coherent reads must match.

At BIN offset `0x200`, eight little-endian 32-bit words contain magic
`0x33435350`, board (`0x03000001` FM / `0x03400002` CH32), load address, exact
length, CRC32, format `2`, platform (`1` / `2`), and image type (`1` application,
`2` bootloader, `3` bootapp). CRC32 treats its own four bytes as zero. Vectors,
partition boundaries and metadata must all agree. FM application ends before
`0xF800`; CH32 bootloader ends before its configuration pages at `0x08003E00`.

ZIP manifest schema is `{"format":"pisugar-ota","version":1,"images":[...]}`.
Each entry has `platform`, `kind`, `board`, `base`, `file`, `size`, and lowercase
`sha256`. Only a single application per platform is permitted; member names must
be plain filenames and all archive members must be listed. No extraction occurs.

Transfers retain the 512-byte register protocol and pad the last block with
`0xFF`. CH32 holds the first two blocks in RAM until its platform/type checks
pass. If Linux reports `EIO` or `EREMOTEIO` on a CH32 data-register exchange
at `0xDD`, the tool resets the segment and position and resends the entire
uncommitted block. It allows up to eight attempts on these errors, with a delay
starting at 10 ms and capped at 100 ms. A failed Flash commit ends the upgrade.
On a transfer failure, keep power connected and explicitly restart the same
validated update from the beginning.

## Tests

```sh
cargo test -p pisugar-programmer
```

The tests exercise the actual file parser and transfer engine with a simulated
SMBus target: platform mismatch before reset, FM migration, CH bootloader identity,
type/address/length/CRC checks, package ambiguity/path rejection, byte retry,
MCU rejection and bounded timeouts. They do not replace physical I2C and
power-interruption acceptance testing.
