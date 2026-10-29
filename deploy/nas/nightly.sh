#!/bin/bash
# The nightly job (collection-nightly.timer, 7:30 in the morning): each step runs even
# when the one before failed; the job fails if any did (see
# `journalctl -u collection-nightly`).
#
#   1. backup: the data onto the other volume (BACKUP_DIR)
#   2. the app's update, when main has a new build
#   3. the public copy: written (EXPORT_DIR), then uploaded to OVH
#
# The update before the copy: a change to the copy is online the morning
# it is installed. A new build that doesn't start is undone by update.sh;
# one whose copy fails uploads nothing (yesterday's copy stays online).
# update.sh moves this script aside (mv), it doesn't overwrite it: bash
# goes on reading the old one.
set -uo pipefail
here=$(dirname "$(readlink -f "$0")")
# shellcheck source-path=SCRIPTDIR source=common.sh
. "$here/common.sh"

failed=0
# step DESCRIPTION COMMAND…: log, run; a failure is noted, and returned
step() {
    log "$1"
    shift
    if "$@"; then
        return 0
    fi
    log "FAILED: $*"
    failed=1
    return 1
}

step "backup into $BACKUP_DIR" app backup --to "$BACKUP_DIR" --keep "${BACKUP_KEEP_DAYS:-14}"
step "update of the app" "$here/update.sh"
if step "public copy into $EXPORT_DIR" app export-public --out "$EXPORT_DIR"; then
    step "upload to $OVH_HOST" "$here/upload-ovh.sh"
fi

log "done$([ "$failed" = 1 ] && echo ', with failures')"
exit "$failed"
