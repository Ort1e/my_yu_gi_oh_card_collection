#!/bin/bash
# The nightly job (collection-nightly.timer, 7:30 in the morning): each step runs even
# when the one before failed; the job fails if any did (see
# `journalctl -u collection-nightly`).
#
#   1. backup: the data onto the other volume (BACKUP_DIR)
#   2. the public copy: written (EXPORT_DIR), then uploaded to OVH
#   3. the app's update, when main has a new build
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
if step "public copy into $EXPORT_DIR" app export-public --out "$EXPORT_DIR"; then
    step "upload to $OVH_HOST" "$here/upload-ovh.sh"
fi
step "update of the app" "$here/update.sh"

log "done$([ "$failed" = 1 ] && echo ', with failures')"
exit "$failed"
