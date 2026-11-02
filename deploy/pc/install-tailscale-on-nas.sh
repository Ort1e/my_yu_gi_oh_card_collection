#!/bin/bash
# From the computer, at home: install (or update) Tailscale on the NAS, so
# that the app and SSH are reachable from our own devices from anywhere,
# with no port opened on the box (deploy/README.md, "From outside home").
#
#   deploy/pc/install-tailscale-on-nas.sh
#
# Downloads Tailscale's static build (the latest stable one, or
# TAILSCALE_VERSION=1.104.4), checks its SHA-256, copies it to the NAS in
# /apps/tailscale with deploy/nas/tailscaled.service, starts it, then:
#   1. `tailscale up`: the first time, it prints a link to approve the NAS
#      in the Tailscale account (open it, log in, done);
#   2. `tailscale serve`: the app at https://<nas>.<tailnet>.ts.net (a real
#      certificate). The first time, it may print a link to turn on HTTPS
#      for the account.
# Run it again to update Tailscale. NAS: the SSH host (default `nas`).
set -euo pipefail
NAS="${NAS:-nas}"
here=$(dirname "$(readlink -f "$0")")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

version="${TAILSCALE_VERSION:-}"
if [ -z "$version" ]; then
    version=$(curl -fsSL "https://pkgs.tailscale.com/stable/?mode=json" |
        sed -n 's/.*"TarballsVersion": *"\([0-9.]*\)".*/\1/p')
fi
[ -n "$version" ] || { echo "couldn't find Tailscale's latest version" >&2; exit 1; }

archive="tailscale_${version}_amd64.tgz"
curl -fsSL -o "$work/$archive" "https://pkgs.tailscale.com/stable/$archive"
expected=$(curl -fsSL "https://pkgs.tailscale.com/stable/$archive.sha256")
actual=$(sha256sum "$work/$archive" | cut -d' ' -f1)
if [ "$expected" != "$actual" ]; then
    echo "$archive: wrong SHA-256, not installed" >&2
    exit 1
fi
echo "Tailscale $version downloaded and checked"

scp -q "$work/$archive" "$here/../nas/tailscaled.service" "$NAS:/tmp/"
# the script runs on the NAS; only the archive's name is expanded here
# shellcheck disable=SC2029
ssh "$NAS" "bash -s -- '$archive'" <<'REMOTE'
set -euo pipefail
archive=$1
dir=/apps/tailscale

install -d -o root -g root -m 755 "$dir"
install -d -o root -g root -m 700 "$dir/state"
tar -xzf "/tmp/$archive" -C /tmp
unpacked="/tmp/${archive%.tgz}"
install -o root -g root -m 755 "$unpacked/tailscaled" "$unpacked/tailscale" "$dir/"
install -o root -g root -m 644 /tmp/tailscaled.service /etc/systemd/system/
rm -rf "$unpacked" "/tmp/$archive" /tmp/tailscaled.service

systemctl daemon-reload
systemctl enable tailscaled.service
systemctl restart tailscaled.service
ts() { "$dir/tailscale" --socket=/run/tailscale/tailscaled.sock "$@"; }
for _ in $(seq 20); do
    ts version > /dev/null 2>&1 && break
    sleep 1
done
echo "tailscaled $(ts version | head -n 1) running"

echo
echo "== 1. The NAS in the Tailscale account (a link the first time: open it and log in)"
ts up
echo
echo "== 2. The app over HTTPS (a link the first time: turn HTTPS on, then wait)"
ts serve --bg --https=443 http://127.0.0.1:3000
echo
ts serve status
echo
ts status
REMOTE
