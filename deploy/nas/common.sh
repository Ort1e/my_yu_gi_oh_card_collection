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

# The settings of an example file (`collection.env.example`) that a settings
# file lacks, each with the comment above it, ready to append: a new build's
# new settings reach the NAS (update.sh). A setting is there when the file
# has it, even commented out (`#APP_ACCESS_FRIEND=…`): never set twice.
new_settings() {
    local example=$1 settings=$2
    awk -v settings="$settings" '
        function name_of(line) {
            sub(/^#/, "", line)
            sub(/=.*/, "", line)
            return line
        }
        BEGIN {
            while ((getline line < settings) > 0) {
                if (line ~ /^#?[A-Za-z_][A-Za-z0-9_]*=/) known[name_of(line)] = 1
            }
        }
        /^[A-Za-z_][A-Za-z0-9_]*=/ {
            if (!(name_of($0) in known)) printf "\n%s%s\n", comments, $0
            comments = ""
            next
        }
        /^#/ { comments = comments $0 "\n"; next }
        { comments = "" }
    ' "$example"
}
