#!/bin/bash
# From the computer: put a build of the app on the NAS and run its
# install.sh (the first time; afterwards the NAS updates itself every morning).
#
#   deploy/pc/install-on-nas.sh [collection-nas.tar.gz]
#
# Without an archive, the latest one (release `nas-latest`, built by the CI
# from main) is downloaded from the repository's public release page. NAS:
# the SSH host to use (default `nas`, see deploy/README.md).
set -euo pipefail
NAS="${NAS:-nas}"
REPO="${GITHUB_REPO:-Ort1e/my_yu_gi_oh_card_collection}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

archive="${1:-}"
if [ -z "$archive" ]; then
    archive="$work/collection-nas.tar.gz"
    curl -fsSL -o "$archive" \
        "https://github.com/$REPO/releases/download/nas-latest/collection-nas.tar.gz"
    echo "downloaded the latest build"
fi

scp -q "$archive" "$NAS:/tmp/collection-nas.tar.gz"
ssh "$NAS" 'set -e
    cd /tmp && rm -rf collection && tar -xzf collection-nas.tar.gz
    ./collection/deploy/install.sh
    rm -rf /tmp/collection /tmp/collection-nas.tar.gz'
