#!/bin/bash
# From the computer: set one of the app's settings on the NAS (in
# /apps/collection/collection.env, where it is, or at the end) and restart
# the app. Run from the repository's root.
#
#   deploy/pc/set-nas-setting.sh GOOGLE_BOOKS_API_KEY
#
# The value is the local .env's when it has that setting, else it is asked
# (not shown). It goes to the NAS through SSH's input, never on a command
# line (no trace in `ps` or a shell history), and the file keeps its owner
# and rights. NAS: the SSH host to use (default `nas`, see deploy/README.md).
set -euo pipefail
NAS="${NAS:-nas}"

name="${1:?usage: $0 SETTING_NAME}"
if [[ ! $name =~ ^[A-Z][A-Z0-9_]*$ ]]; then
    echo "not a setting name: $name" >&2
    exit 2
fi

value=""
if [ -f .env ]; then
    value=$(sed -n "s/^$name=//p" .env | tail -n 1)
fi
if [ -n "$value" ]; then
    echo "$name: the value of the local .env"
else
    read -rsp "$name: " value
    echo
fi
# systemd and bash both read the file: a plain value only (no spaces, quotes…)
if [[ ! $value =~ ^[A-Za-z0-9._~:/+@=-]+$ ]]; then
    echo "an empty value, or characters to quote: edit collection.env on the NAS by hand" >&2
    exit 2
fi

# shellcheck disable=SC2016 # expanded on the NAS
remote='set -euo pipefail
    file=/apps/collection/collection.env
    IFS= read -r VALUE
    export VALUE
    new=$(awk -v name="$1" "
        BEGIN { value = ENVIRON[\"VALUE\"] }
        \$0 ~ \"^#?\" name \"=\" && !done { print name \"=\" value; done = 1; next }
        { print }
        END { if (!done) print name \"=\" value }
    " "$file")
    printf "%s\n" "$new" > "$file"
    systemctl restart collection
    echo "$1 set on the NAS, the app restarted"'

# the name is checked above: expanded here on purpose
# shellcheck disable=SC2029
printf '%s\n' "$value" | ssh "$NAS" "bash -c $(printf %q "$remote") set-nas-setting $name"
