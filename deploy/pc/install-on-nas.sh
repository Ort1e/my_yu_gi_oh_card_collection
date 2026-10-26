#!/bin/bash
# From the computer: put a build of the app on the NAS and run its
# install.sh (the first time; afterwards the NAS updates itself at night).
#
#   deploy/pc/install-on-nas.sh [collection-nas.tar.gz]
#
# Without an archive, the latest one (release `nas-latest`, built by the CI
# from main) is downloaded with GITHUB_TOKEN (a token that can read the
# repository). NAS: the SSH host to use (default `nas`, see deploy/README.md).
set -euo pipefail
NAS="${NAS:-nas}"
REPO="${GITHUB_REPO:-Ort1e/my_yu_gi_oh_card_collection}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

archive="${1:-}"
if [ -z "$archive" ]; then
    : "${GITHUB_TOKEN:?set GITHUB_TOKEN, or give the archive downloaded from the release page}"
    api="https://api.github.com/repos/$REPO/releases/tags/nas-latest"
    url=$(curl -fsSL -H "Authorization: Bearer $GITHUB_TOKEN" "$api" |
        python3 -c 'import json, sys
for a in json.load(sys.stdin)["assets"]:
    if a["name"] == "collection-nas.tar.gz": print(a["url"])')
    archive="$work/collection-nas.tar.gz"
    curl -fsSL -H "Authorization: Bearer $GITHUB_TOKEN" -H "Accept: application/octet-stream" \
        -o "$archive" "$url"
    echo "downloaded the latest build"
fi

scp -q "$archive" "$NAS:/tmp/collection-nas.tar.gz"
ssh "$NAS" 'set -e
    cd /tmp && rm -rf collection && tar -xzf collection-nas.tar.gz
    ./collection/deploy/install.sh
    rm -rf /tmp/collection /tmp/collection-nas.tar.gz'
