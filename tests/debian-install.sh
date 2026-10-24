#!/bin/bash
# Run ONLY in a disposable Debian container, as root. Uses packaging fixtures,
# real debconf/dpkg/jq, and stubs hardware/systemd; does not test ARM executables.
set -euo pipefail
cd "$(dirname "$0")/.."
test "$(id -u)" = 0
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"
for command in deb-systemd-helper deb-systemd-invoke; do
    printf '#!/bin/sh\nexit 0\n' > "$work/bin/$command"
    chmod +x "$work/bin/$command"
done
export PATH="$work/bin:$PATH"
export DEBIAN_FRONTEND=noninteractive

build_packages() {
    for name in server poweroff; do
        package=pisugar-$name
        dir="$work/$package"
        mkdir -p "$dir/DEBIAN" "$dir/etc/default" "$dir/usr/share/$package"
        # Give each fixture a real, unique payload so dpkg cannot discard an empty package.
        printf 'fixture\n' > "$dir/usr/share/$package/fixture"
        cp "$package/debian/"{config,templates,postinst,postrm} "$dir/DEBIAN/"
        chmod 755 "$dir/DEBIAN/"{config,postinst,postrm}
        cat > "$dir/DEBIAN/control" <<EOF
Package: $package
Version: $1
Architecture: all
Maintainer: Test <test@example.invalid>
Depends: debconf, jq
Description: Disposable packaging regression fixture
EOF
        if [ "$name" = server ]; then
            mkdir -p "$dir/etc/pisugar-server"
            cp pisugar-server/debian/config.json "$dir/etc/pisugar-server/config.json"
            echo /etc/pisugar-server/config.json > "$dir/DEBIAN/conffiles"
        fi
        dpkg-deb --build "$dir" "$work/$package.deb"
    done
}
assert_configured() {
    for package in pisugar-server pisugar-poweroff; do
        test "$(dpkg-query -W -f='${Status}' "$package")" = 'install ok installed'
    done
    test -z "$(dpkg --audit)"
}
# No existing package/debconf state: defaults must complete without stdin/TTY.
dpkg --purge pisugar-server pisugar-poweroff 2>/dev/null || true
build_packages 0.0.1
timeout 60 dpkg -i "$work/"*.deb </dev/null
assert_configured
grep -F -- "--model 'PiSugar 3'" /etc/default/pisugar-server
grep -F -- "--model 'PiSugar 3'" /etc/default/pisugar-poweroff

# Fresh seeded install, including characters previously breaking jq.
dpkg --purge pisugar-server pisugar-poweroff
debconf-set-selections <<'EOF'
pisugar-server pisugar-server/model select PiSugar 2 Pro
pisugar-poweroff pisugar-poweroff/model select PiSugar 2 Pro
pisugar-server pisugar-server/auth-username string test"user
pisugar-server pisugar-server/auth-password password test"\secret
EOF
timeout 60 dpkg -i "$work/"*.deb </dev/null
assert_configured
grep -F -- "--model 'PiSugar 2 Pro'" /etc/default/pisugar-poweroff
jq -e '.auth_user == "test\"user" and .auth_password == "test\"\\secret"' /etc/pisugar-server/config.json
cp /etc/pisugar-server/config.json "$work/expected.json"

# Reinstallation, configuration retry, and upgrade must preserve credentials.
timeout 60 dpkg -i "$work/"*.deb </dev/null
dpkg --unpack "$work/"*.deb
timeout 60 dpkg --configure pisugar-server pisugar-poweroff </dev/null
build_packages 0.0.2
timeout 60 dpkg -i "$work/"*.deb </dev/null
assert_configured
cmp "$work/expected.json" /etc/pisugar-server/config.json

# Real jq failure should be visible, preserve JSON and credentials for a retry.
printf 'broken' > /etc/pisugar-server/config.json
debconf-set-selections <<'EOF'
pisugar-server pisugar-server/auth-username string retry
pisugar-server pisugar-server/auth-password password retry-secret
EOF
if timeout 60 dpkg-reconfigure -f noninteractive pisugar-server </dev/null; then
    echo 'Expected malformed JSON failure' >&2; exit 1
fi
test "$(cat /etc/pisugar-server/config.json)" = broken
cp "$work/expected.json" /etc/pisugar-server/config.json
timeout 60 dpkg-reconfigure -f noninteractive pisugar-server </dev/null
jq -e '.auth_user == "retry" and .auth_password == "retry-secret"' /etc/pisugar-server/config.json
assert_configured

# Exercise the real wrapper + apt + debconf against a simulated CDN of fixtures.
# Only download, sudo and systemd are stubbed; package operations remain real.
dpkg --purge pisugar-server pisugar-poweroff
mkdir -p "$work/programmer/DEBIAN" "$work/programmer/usr/share/pisugar-programmer"
printf 'fixture\n' > "$work/programmer/usr/share/pisugar-programmer/fixture"
cat > "$work/programmer/DEBIAN/control" <<'EOF'
Package: pisugar-programmer
Version: 0.0.2
Architecture: all
Maintainer: Test <test@example.invalid>
Description: Disposable programmer fixture
EOF
dpkg-deb --build "$work/programmer" "$work/pisugar-programmer.deb"
printf '#!/bin/sh\nexec "$@"\n' > "$work/bin/sudo"
cat > "$work/bin/wget" <<'EOF'
#!/bin/sh
case "$3" in
    */pisugar-server_*) package=pisugar-server ;;
    */pisugar-poweroff_*) package=pisugar-poweroff ;;
    */pisugar-programmer_*) package=pisugar-programmer ;;
    *) exit 1 ;;
esac
cp "$FIXTURE_DIR/$package.deb" "$2"
EOF
chmod +x "$work/bin/sudo" "$work/bin/wget"
export FIXTURE_DIR="$work"
timeout 60 bash scripts/pisugar-power-manager.sh --version 0.0.2 --model 'PiSugar 2 (4-LEDs)' </dev/null
assert_configured
test "$(dpkg-query -W -f='${Status}' pisugar-programmer)" = 'install ok installed'
grep -F -- "--model 'PiSugar 2 (4-LEDs)'" /etc/default/pisugar-server
grep -F -- "--model 'PiSugar 2 (4-LEDs)'" /etc/default/pisugar-poweroff

echo 'Debian fixture installation regression tests passed'
