#!/bin/bash
# Upload the public copy (EXPORT_DIR, written by `backend export-public`) to
# the OVH hosting: the files new or changed since the last upload, then
# delete there the files gone from the copy (old pictures would otherwise
# fill the 100 MB). What is on the hosting is remembered in
# EXPORT_DIR.uploaded (checksum and path of each file); delete it to upload
# everything again.
#
# OVH_PROTOCOL=sftp (enabled in the OVH panel, FTP-SSH tab: encrypted, the
# NAS's own `sftp`) or ftp (plain: the password travels unencrypted). Run
# by nightly.sh, or by hand: sudo /apps/collection/deploy/upload-ovh.sh
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

# the files new or changed, then the files no longer in the copy (in what
# was uploaded, not now)
comm -13 <(sort "$uploaded") <(sort "$work/now") | cut -c67- > "$work/send"
cut -c67- "$uploaded" | sort > "$work/before.paths"
cut -c67- "$work/now" | sort > "$work/now.paths"
comm -23 "$work/before.paths" "$work/now.paths" > "$work/remove"
sent=$(wc -l < "$work/send")
removed=$(wc -l < "$work/remove")

if [ "$protocol" = sftp ]; then
    # one session for everything: the folders (an existing one is no
    # error: `-`), the files, then the removals. `-o BatchMode=no` before
    # `-b` (which turns it on; ssh keeps the first value), or the password
    # is never tried. The password goes through
    # SSH_ASKPASS (no sshpass on the NAS); the server's key must be in
    # OVH_KNOWN_HOSTS (deploy/README.md).
    known_hosts="${OVH_KNOWN_HOSTS:-$APP_DIR/ovh_known_hosts}"
    [ -s "$known_hosts" ] || { log "no server key in $known_hosts (deploy/README.md)"; exit 1; }
    {
        path=""
        for part in ${OVH_DIR//\// }; do
            path="$path${path:+/}$part"
            echo "-mkdir \"$path\""
        done
        while IFS= read -r file; do
            dir=$(dirname "$file")
            path=""
            if [ "$dir" != . ]; then
                for part in ${dir//\// }; do
                    path="$path/$part"
                    echo "-mkdir \"$OVH_DIR$path\""
                done
            fi
        done < "$work/send" | awk '!seen[$0]++'
        # readable by the web server, whatever the NAS's file modes
        while IFS= read -r file; do
            echo "put \"$EXPORT_DIR/$file\" \"$OVH_DIR/$file\""
            echo "chmod 644 \"$OVH_DIR/$file\""
        done < "$work/send"
        while IFS= read -r file; do
            echo "-rm \"$OVH_DIR/$file\""
        done < "$work/remove"
    } > "$work/batch"
    printf '#!/bin/sh\nprintf "%%s\\n" "$OVH_PASSWORD"\n' > "$work/askpass"
    chmod 700 "$work/askpass"
    SSH_ASKPASS="$work/askpass" SSH_ASKPASS_REQUIRE=force DISPLAY=none \
        setsid -w sftp -o BatchMode=no -o PubkeyAuthentication=no \
        -o StrictHostKeyChecking=yes -o UserKnownHostsFile="$known_hosts" -o ConnectTimeout=30 \
        -b "$work/batch" "$OVH_USER@$OVH_HOST" < /dev/null > /dev/null \
        2> >(grep -v 'remote mkdir' >&2)
else
    # plain FTP, one curl per file
    while IFS= read -r file; do
        curl -sS --fail --retry 3 --netrc-file "$work/netrc" --ftp-create-dirs \
            -T "$EXPORT_DIR/$file" "ftp://$OVH_HOST/$OVH_DIR/$file"
    done < "$work/send"
    while IFS= read -r file; do
        curl -sS --fail --netrc-file "$work/netrc" -Q "DELE $OVH_DIR/$file" \
            "ftp://$OVH_HOST/" -o /dev/null ||
            log "could not delete $file on the hosting"
    done < "$work/remove"
fi

cp "$work/now" "$uploaded"
log "uploaded to $OVH_HOST/$OVH_DIR: $sent files sent, $removed deleted"
