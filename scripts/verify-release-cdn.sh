#!/usr/bin/env bash
# Run from the release-asset directory after qshell authentication.
set -euo pipefail
version=${1:?release version required}
installer=${2:?expected installer required}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 1
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
printf '%s\n' https://cdn.pisugar.com/release/pisugar-power-manager.sh > "$work/urls"
for package in server poweroff programmer; do
    file="pisugar-${package}_${version}-1_arm64.deb"
    test -s "$file"
    printf 'https://cdn.pisugar.com/release/%s\n' "$file" >> "$work/urls"
done
# Overwriting an origin object does not invalidate existing CDN edge caches.
./qshell cdnrefresh -i "$work/urls"
verify() {
    curl --fail --silent --show-error --max-time 60 https://cdn.pisugar.com/release/pisugar-power-manager.sh -o "$work/installer" || return 1
    cmp -s "$installer" "$work/installer" || return 1
    for package in server poweroff programmer; do
        file="pisugar-${package}_${version}-1_arm64.deb"
        curl --fail --silent --show-error --max-time 120 "https://cdn.pisugar.com/release/$file" -o "$work/package" || return 1
        cmp -s "$file" "$work/package" || return 1
    done
}
# CDN refresh is asynchronous. Keep checking canonical URLs without cache-busting.
for attempt in $(seq 1 10); do
    if verify; then
        echo 'Release installer and ARM64 packages verified through CDN'
        exit 0
    fi
    if [ "$attempt" -lt 10 ]; then
        echo "Waiting for CDN refresh propagation ($attempt/10)"
        sleep 30
    fi
done
echo 'CDN still differs from release artifacts after refresh' >&2
exit 1
