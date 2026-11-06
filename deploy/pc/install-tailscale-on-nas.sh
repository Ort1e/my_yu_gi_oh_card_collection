#!/bin/bash
# From the computer (at home the first time): install (or update) Tailscale on the NAS, so
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
# Run it again to update Tailscale, at home or through Tailscale itself
# (NAS=nas-ts). NAS: the SSH host (default `nas`).
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

# Through Tailscale (NAS=nas-ts, away from home), restarting tailscaled cuts
# the SSH connection: the install runs in one connection that ends with the
# restart, the rest in a new one. No shared connection (ControlMaster: it
# would hang once cut), and a cut connection noticed within 15 s.
remote() {
    ssh -o ControlPath=none -o ConnectTimeout=10 \
        -o ServerAliveInterval=5 -o ServerAliveCountMax=3 "$NAS" "$@"
}
ssh -O exit "$NAS" 2> /dev/null || true

scp -q -o ControlPath=none "$work/$archive" "$here/../nas/tailscaled.service" "$NAS:/tmp/"
# the script runs on the NAS; only the archive's name is expanded here
# shellcheck disable=SC2029
remote "bash -s -- '$archive'" <<'REMOTE' || [ $? = 255 ]
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
echo "installed; restarting tailscaled (through Tailscale, the connection drops: wait)"
# systemd does it even if this connection is cut on the way
systemctl --no-block restart tailscaled.service
REMOTE

# the NAS again (through Tailscale: once tailscaled is back, up to 2 min)
for _ in $(seq 12); do
    remote true 2> /dev/null && break
    sleep 5
done

remote 'bash -s' <<'REMOTE'
set -euo pipefail
ts() { /apps/tailscale/tailscale --socket=/run/tailscale/tailscaled.sock "$@"; }
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
