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
app() {
    (cd "$APP_DIR" && runuser -u "$APP_USER" -- "$APP_DIR/backend" "$@")
}
