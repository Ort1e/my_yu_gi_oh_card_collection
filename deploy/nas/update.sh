#!/bin/bash
# Install the latest build of main (the GitHub release `nas-latest`) when it
# isn't the one running: download (the app's `fetch-release`), backup,
# replace, restart; back to the previous version if the new one doesn't
# answer. Run by nightly.sh, or by hand: sudo /apps/collection/deploy/update.sh
set -euo pipefail
here=$(dirname "$(readlink -f "$0")")
# shellcheck source-path=SCRIPTDIR source=common.sh
. "$here/common.sh"

next="$APP_DIR/next"
install -d -o "$APP_USER" -g "$APP_USER" "$next"
result=$(app fetch-release --to "$next")
log "$result"
case "$result" in
    new:*) ;;
    *) exit 0 ;;
esac

rm -rf "$next/collection"
tar -xzf "$next/collection-nas.tar.gz" -C "$next"
new="$next/collection"
[ -x "$new/backend" ] || { log "the archive has no app"; exit 1; }

log "backup before the update"
app backup --to "$BACKUP_DIR" --keep "${BACKUP_KEEP_DAYS:-14}"
# that copy is consistent (made by SQLite while the app runs): the one to
# go back to if the update fails
before="$BACKUP_DIR/app-$(date +%F).db"
database="${DATABASE_URL#sqlite:}"

log "replacing the app"
systemctl stop collection
rm -rf "$APP_DIR/previous"
mkdir "$APP_DIR/previous"
for item in backend static deploy VERSION; do
    if [ -e "$APP_DIR/$item" ]; then mv "$APP_DIR/$item" "$APP_DIR/previous/"; fi
    cp -a "$new/$item" "$APP_DIR/$item"
done
chown -R root:root "$APP_DIR/backend" "$APP_DIR/static" "$APP_DIR/deploy" "$APP_DIR/VERSION"
# the services as the new version describes them
cp "$APP_DIR"/deploy/collection.service "$APP_DIR"/deploy/collection-nightly.service \
    "$APP_DIR"/deploy/collection-nightly.timer /etc/systemd/system/
systemctl daemon-reload
systemctl start collection

# the new version must answer within 30 seconds (its migrations run first)
for _ in $(seq 30); do
    if curl -fsS -o /dev/null "http://127.0.0.1:${BIND_ADDR##*:}/login"; then
        log "updated to $(cat "$APP_DIR/VERSION")"
        exit 0
    fi
    sleep 1
done

log "the new version doesn't answer: back to the previous one"
systemctl stop collection
for item in backend static deploy VERSION; do
    rm -rf "${APP_DIR:?}/$item"
    if [ -e "$APP_DIR/previous/$item" ]; then mv "$APP_DIR/previous/$item" "$APP_DIR/$item"; fi
done
# its migrations may have run: the database as it was before
rm -f "$database-wal" "$database-shm"
cp "$before" "$database"
chown "$APP_USER:$APP_USER" "$database"
systemctl start collection
exit 1
