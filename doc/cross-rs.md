# Cross compilation with cross-rs

cross-rs is a "zero setup" cross compilation tool (cross-rs depends on docker or podman).

## Usage

```bash
cargo install cross --git https://github.com/cross-rs/cross
cargo install cargo-deb
cargo install cargo-generate-rpm

cross build --target arm-unknown-linux-musleabihf --release

cargo deb --no-build --no-strip --target arm-unknown-linux-musleabihf -p pisugar-server
cargo generate-rpm --target arm-unknown-linux-musleabihf -p pisugar-server
```

NOTE: cross-rs will download a docker image from ghcr.io
