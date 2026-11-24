# PiSugar programmer 2.4.4

The programmer validates the complete firmware file before resetting the MCU or
writing Flash. It supports FM33LC0x application migration and guarded CH32L103
updates. Existing register meanings and `-b`, `-a`, `-r`, `-d` options are retained.

The [programmer 2.4.4 release](https://github.com/PiSugar/pisugar-power-manager-rs/releases/tag/pisugar-programmer-v2.4.4)
provides Debian packages and standalone Linux executables for Raspberry Pi. The
[PiSugar 3 update script](../scripts/PiSugarUpdate.sh) installs the native
`pisugar-programmer` package on Raspberry Pi OS, replacing an older package at
`/usr/bin/pisugar-programmer`. Other supported Linux installations use
`/usr/local/bin/pisugar-programmer`. The script pins programmer 2.4.4. It
reuses the installed executable when its SHA-256 matches the release for the
host architecture. On Raspberry Pi OS it also checks the installed Debian
package version and architecture. A missing or older installation triggers a
download. A normal update also checks the tool when the board already has the
target firmware.

To install or upgrade just the programmer, run the following from the directory
containing the script. This mode can run with the PiSugar board disconnected.

```sh
sudo bash PiSugarUpdate.sh --install-only
pisugar-programmer --version
```

For a firmware update the script downloads the dual-platform application ZIP from the PiSugar CDN.
It checks the programmer version and the hash of each download, then reads the board's
platform and application version twice on bus 1 at 0x57. For this ZIP, FM targets
v1.4.2 and CH32 targets v1.4.3. If the installed version is equal or newer,
answer `y` at the terminal to reinstall or downgrade. Enter, EOF, or running
without a controlling terminal skips the write. The prompt also works when the
script arrives via `curl | sudo bash`. On unmarked legacy boards it checks
the serial field to distinguish old FM from the zero-padded 12-byte CH32 serial.
After flashing, the script confirms the expected version on the same platform.
Each step prints its status. Long operations report elapsed time every five
seconds, and downloads also report bytes received and percentage.

If the board does not respond, the normal update command keeps waiting.
Press and release the PiSugar Reset button when prompted. A supported CH32
bootloader or bootapp can identify the chip even when the application cannot
start. Recovery then uses the verified platform and skips the installed application
version check. An application with a stable explicit platform identity and an
unreadable version register can also be repaired. A bootloader with no identity
could be legacy FM or early CH32. Recovery from that state alone requires separate
identification of the chip.

The normal script requests a software reset when the application is responsive.
Use `sudo bash PiSugarUpdate.sh --wait-reset` to choose a physical Reset instead.
With the programmer itself, a physical Reset is the default. Start the command,
confirm the power reminder, then press and release Reset when prompted. The
programmer polls every 100 ms until the bootloader appears. Ctrl-C cancels the
wait. Adding `-r` requests one software reset on an identified board and still
allows a physical Reset if needed. Run `bash PiSugarUpdate.sh --check` to verify
the downloads on a Pi without accessing the hardware.

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
The old FM application reports product 3, application mode, eight zero bytes at
`0x52–0x59`, and a 16-byte serial field distinct from the early CH32 layout.
The new FM application exposes an explicit identity. Its original
bootloader still returns zeros; this transition is accepted only for the FM
image already checked in that upgrade session. Keep the same board connected
through the reset and update. After the final application launch,
an FM board can briefly return zero for its product and mode registers. The
updater polls at 50 ms intervals for up to 100 identity checks. It reports success
only after reading the expected platform identity and application mode.

This migration does not rewrite the FM bootloader or bootapp. Users bypassing
this tool with an old programmer can still write incompatible files to an old
FM bootloader. Do not replace existing FM-only download URLs with CH32 binaries.

## CH32 devices

Automatic CH32 updates require the explicit identity in application, bootloader and bootapp
mode. The bootloader identity must advertise guarded OTA before writing. A new
application on an old, unmarked bootloader is refused. Earlier experimental CH32
boards without identity require physical MCU identification for application recovery.

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
enabled in this migration. Manual Reset mode asks for power confirmation unless
you pass `-y` or `--yes`. Using `-r` skips this prompt automatically.

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

Transfers use 512-byte blocks and pad the last block with `0xFF`. On either MCU,
the programmer retries a failed RAM byte until it succeeds or the operator
presses Ctrl-C. Before resending it checks writer identity, mode and segment,
then writes and reads back the position immediately preceding that byte. On a
recovered transfer it checks the buffered block before committing Flash. Retry
messages include the image offset, total retry count and elapsed time.
Flash erase/write is issued once per block after observing an idle writer.
If the command ACK is lost, the programmer polls status. An observed busy-to-idle
transition with a clear error bit confirms completion after rechecking the writer
and segment. An idle response alone is ambiguous. If completion cannot be
confirmed, the tool stops at that offset and prints the recovery command.

### Recover an application from an unmarked bootloader

Keep both devices powered. Verify the MCU marking on the physical board:
legacy FM and early CH32 can both report product `03`, mode `F0`, and an
all-zero platform marker. In this state a checksum identifies the file only.
With the ordinary server stopped, select the verified family explicitly:

```sh
sudo systemctl stop pisugar-server
sudo pisugar-programmer --force-platform fm33lc0x pisugar-3-ota.zip
sudo systemctl start pisugar-server
```

For a physically verified CH32L103 use `--force-platform ch32l103`. The input
may also be the matching tagged application BIN. Recovery starts at offset zero,
checks the image and any readable identity, and requires the new application
to report its expected platform after handoff. A wrong physical choice on an
unmarked writer can install the wrong application. The automatic updater never
selects this override. Firmware release selection is via the supplied ZIP or BIN.

## Tests

```sh
cargo test -p pisugar-programmer
```

The tests exercise the actual file parser and transfer engine with a simulated
SMBus target: platform mismatch before reset, FM migration, CH bootloader identity,
type/address/length/CRC checks, package ambiguity/path rejection, byte retry,
MCU rejection and bounded timeouts. They do not replace physical I2C and
power-interruption acceptance testing.
