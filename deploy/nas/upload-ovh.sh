#!/bin/bash
# Upload the public copy (EXPORT_DIR, written by `backend export-public`) to
# the OVH hosting: the files new or changed since the last upload, then
# delete there the files gone from the copy (old pictures would otherwise
# fill the 100 MB). What is on the hosting is remembered in
# EXPORT_DIR.uploaded (checksum and path of each file); delete it to upload
# everything again.
#
# OVH_PROTOCOL=ftp (plain: the password travels unencrypted, what the free
# plan accepts today) or sftp (once enabled in the OVH panel). Run by
# nightly.sh, or by hand: sudo /apps/collection/deploy/upload-ovh.sh
set -euo pipefail
# sort and comm must order alike, whatever the NAS's language
export LC_ALL=C
here=$(dirname "$(readlink -f "$0")")
# shellcheck source-path=SCRIPTDIR source=common.sh
. "$here/common.sh"

: "${EXPORT_DIR:?}" "${OVH_HOST:?}" "${OVH_USER:?}" "${OVH_PASSWORD:?}" "${OVH_DIR:?}"
protocol="${OVH_PROTOCOL:-ftp}"
case "$protocol" in
    ftp | sftp) ;;
    *) log "OVH_PROTOCOL must be ftp or sftp, not $protocol"; exit 1 ;;
esac
[ -f "$EXPORT_DIR/index.html" ] || { log "no public copy in $EXPORT_DIR"; exit 1; }

# the login in a file readable by root only, deleted on the way out (never
# on a command line, where `ps` would show it)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
chmod 700 "$work"
printf 'machine %s login %s password %s\n' "$OVH_HOST" "$OVH_USER" "$OVH_PASSWORD" > "$work/netrc"

uploaded="$EXPORT_DIR.uploaded"
touch "$uploaded"
(cd "$EXPORT_DIR" && find . -type f ! -name .public-export | sed 's|^\./||' | sort |
    while IFS= read -r file; do sha256sum "$file"; done) > "$work/now"

# curl for one file or one command; sftp checks the server's key in
# root's known_hosts (add it once: ssh-keyscan OVH_HOST >> /root/.ssh/known_hosts)
remote="$protocol://$OVH_HOST/"
[ "$protocol" = sftp ] && remote="$protocol://$OVH_HOST/~/"
send() {
    curl -sS --fail --retry 3 --netrc-file "$work/netrc" --ftp-create-dirs \
        -T "$EXPORT_DIR/$1" "$remote$OVH_DIR/$1"
}
remove() {
    local command="DELE"
    [ "$protocol" = sftp ] && command="rm"
    curl -sS --fail --netrc-file "$work/netrc" -Q "$command $OVH_DIR/$1" "$remote" -o /dev/null
}

sent=0
while IFS= read -r line; do
    send "${line#*  }"
    sent=$((sent + 1))
done < <(comm -13 <(sort "$uploaded") <(sort "$work/now"))

# the files no longer in the copy (in what was uploaded, not now)
cut -c67- "$uploaded" | sort > "$work/before.paths"
cut -c67- "$work/now" | sort > "$work/now.paths"
removed=0
while IFS= read -r file; do
    if remove "$file"; then
        removed=$((removed + 1))
    else
        log "could not delete $file on the hosting"
    fi
done < <(comm -23 "$work/before.paths" "$work/now.paths")

cp "$work/now" "$uploaded"
log "uploaded to $OVH_HOST/$OVH_DIR: $sent files sent, $removed deleted"
