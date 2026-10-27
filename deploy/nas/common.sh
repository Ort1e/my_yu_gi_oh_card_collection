# shellcheck shell=bash
# Shared by the NAS scripts (sourced, not run): the settings, and running
# the app's commands as its own user.

APP_DIR=/apps/collection
APP_USER=collection

# the settings: the app's, then the nightly job's (secrets)
set -a
# shellcheck disable=SC1091
. "$APP_DIR/collection.env"
if [ -f "$APP_DIR/nightly.env" ]; then
    # shellcheck disable=SC1091
    . "$APP_DIR/nightly.env"
fi
set +a

log() {
    echo "$(date '+%F %T') $*"
}

# A command of the app (`backup`, `export-public`, `fetch-release`…), as the
# app's user, from its folder (it reads `static/` and `VERSION` there).
#
# ReadyNAS has no `runuser`, and its `su` reads any argument after the user
# name as one of its own options (`--to`…): the whole command goes in one
# string, each word quoted by `printf %q` (bash quoting: run by bash, not
# /bin/sh). Without `-l`, su keeps the environment (the settings).
app() {
    local command
    printf -v command 'cd %q && exec' "$APP_DIR"
    printf -v command '%s%s' "$command" "$(printf ' %q' "$APP_DIR/backend" "$@")"
    su -s /bin/bash -c "$command" "$APP_USER"
}
