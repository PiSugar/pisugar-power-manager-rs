#!/usr/bin/env bash
set -euo pipefail
[[ "$RELEASE_TAG" =~ ^pisugar-programmer-v([0-9]+\.[0-9]+\.[0-9]+)$ ]]
version=${BASH_REMATCH[1]}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
git fetch --no-tags --depth=1 origin "refs/tags/$RELEASE_TAG"
installer="$work/PiSugarUpdate.sh"
manager_installer="$PWD/scripts/pisugar-power-manager.sh"
git show FETCH_HEAD:scripts/PiSugarUpdate.sh > "$installer"
grep -Fq " tool_version=$version" "$installer"
fw_url=$(sed -n 's/^    ota_url=//p' "$installer")
fw_sha=$(sed -n 's/^    local ota_sha=//p' "$installer")
[[ "$fw_url" =~ ^https://cdn\.pisugar\.com/(release/PiSugar3Firmware/combine/(v[0-9]+\.[0-9]+\.[0-9]+))/pisugar-3-ota\.zip$ ]]
fw_dir=${BASH_REMATCH[1]}
fw_tag=${BASH_REMATCH[2]}
tool_dir=release/PiSugar3Firmware/programmer/v$version
cd "$work"
mkdir tools fw
gh release download "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY" --dir tools
(cd tools && sha256sum -c SHA256SUMS && sha256sum -c SHA256SUMS-deb)
cmp "$installer" tools/PiSugarUpdate.sh
# The existing OTA ZIP is public on the CDN. The tagged installer pins its hash.
curl -fsSL --retry 3 "$fw_url" -o fw/pisugar-3-ota.zip
(cd fw && echo "$fw_sha  pisugar-3-ota.zip" | sha256sum -c - && unzip -jo pisugar-3-ota.zip 'pisugar-3-*-application.bin')
test -s fw/pisugar-3-fm33lc0x-application.bin
test -s fw/pisugar-3-ch32l103-application.bin
curl -fsSL --retry 3 https://github.com/qiniu/qshell/releases/download/v2.16.1/qshell-v2.16.1-linux-amd64.tar.gz -o qshell.tar.gz
tar -xzf qshell.tar.gz
chmod +x qshell
./qshell account -w "$QINIU_AK" "$QINIU_SK" deploy-account
publish() {
    local file=$1 key=$2 mime=${3:-application/octet-stream}
    ./qshell rput --overwrite --mimetype "$mime" "$QINIU_BUCKET" "$key" "$file"
    printf '%s\t%s\n' "$file" "https://cdn.pisugar.com/$key" >> verify.tsv
}
verify() {
    local file url
    cut -f2 verify.tsv > urls
    ./qshell cdnrefresh -i urls
    for attempt in {1..12}; do
        local failed=0
        while IFS=$'\t' read -r file url; do
            if ! curl -fsSL --retry 1 --max-time 45 "$url" -o fetched || ! cmp -s "$file" fetched; then
                echo "Awaiting $url"
                failed=1
                break
            fi
        done < verify.tsv
        if (( ! failed )); then cat verify.tsv; return 0; fi
        sleep 15
    done
    return 1
}
for file in tools/pisugar-programmer* tools/SHA256SUMS*; do publish "$file" "$tool_dir/${file##*/}"; done
for file in fw/*; do publish "$file" "$fw_dir/${file##*/}"; done
verify
: > verify.tsv
publish "$installer" "$fw_dir/PiSugarUpdate.sh" text/plain
publish "$installer" release/PiSugarUpdate.sh text/plain
publish "$manager_installer" release/pisugar-power-manager.sh text/plain
verify
