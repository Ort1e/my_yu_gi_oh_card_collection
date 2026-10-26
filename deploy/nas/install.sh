#!/bin/bash
# First install of the app on the NAS, from an unpacked release (the
# folder `collection/` of collection-nas.tar.gz, copied there by
# deploy/pc/install-on-nas.sh). As root, from that folder:
#   sudo ./deploy/install.sh
# Safe to run again: it never overwrites the settings or the data.
#
#   /apps/collection/        the app, its stylesheets, the scripts, the settings
#   /Data1To/collection/     the live data (app.db, media/, uploads/)
#   /Data500/collection-backups/   the nightly copies
set -euo pipefail
from=$(dirname "$(dirname "$(readlink -f "$0")")")
APP_DIR=/apps/collection
DATA_DIR=/Data1To/collection
BACKUP_DIR=/Data500/collection-backups
APP_USER=collection

[ "$(id -u)" = 0 ] || { echo "run it as root (sudo)"; exit 1; }
[ -x "$from/backend" ] || { echo "run it from the unpacked release"; exit 1; }

# the app's own user: no login, no shell
if ! id "$APP_USER" > /dev/null 2>&1; then
    useradd --system --home-dir "$APP_DIR" --no-create-home --shell /usr/sbin/nologin "$APP_USER"
    echo "user $APP_USER created"
fi

# the folders: the app's is root's (the app can't change itself), the data
# and backups are the app's
install -d -o root -g root -m 755 "$APP_DIR"
install -d -o "$APP_USER" -g "$APP_USER" -m 750 "$DATA_DIR" "$DATA_DIR/media" "$DATA_DIR/uploads" "$BACKUP_DIR"
install -d -o "$APP_USER" -g "$APP_USER" -m 755 "$DATA_DIR/public-export"

# the app (the update script replaces it afterwards)
if [ "$from" != "$APP_DIR" ]; then
    for item in backend static deploy VERSION; do
        rm -rf "${APP_DIR:?}/$item"
        cp -a "$from/$item" "$APP_DIR/$item"
    done
fi
chown -R root:root "$APP_DIR/backend" "$APP_DIR/static" "$APP_DIR/deploy" "$APP_DIR/VERSION"
chmod 755 "$APP_DIR/backend" "$APP_DIR"/deploy/*.sh

# the settings, from the examples, only the first time: to fill in
if [ ! -f "$APP_DIR/collection.env" ]; then
    install -o root -g "$APP_USER" -m 640 "$APP_DIR/deploy/collection.env.example" "$APP_DIR/collection.env"
    echo "fill in $APP_DIR/collection.env (the passwords)"
fi
if [ ! -f "$APP_DIR/nightly.env" ]; then
    install -o root -g root -m 600 "$APP_DIR/deploy/nightly.env.example" "$APP_DIR/nightly.env"
    echo "fill in $APP_DIR/nightly.env (OVH, GitHub token)"
fi

# the OVH SFTP server's key, the first time (compare the fingerprint
# printed with deploy/README.md's)
if [ ! -s "$APP_DIR/ovh_known_hosts" ]; then
    ssh-keyscan -t ed25519 ftp.cluster129.hosting.ovh.net > "$APP_DIR/ovh_known_hosts" 2> /dev/null || true
    echo "the OVH server's key: $(ssh-keygen -lf "$APP_DIR/ovh_known_hosts" 2> /dev/null | cut -d' ' -f2)"
fi

# the services
cp "$APP_DIR"/deploy/collection.service "$APP_DIR"/deploy/collection-nightly.service \
    "$APP_DIR"/deploy/collection-nightly.timer /etc/systemd/system/
systemctl daemon-reload
systemctl enable collection.service collection-nightly.timer

echo
echo "installed $(cat "$APP_DIR/VERSION") in $APP_DIR"
if [ ! -f "$DATA_DIR/app.db" ]; then
    echo "no data yet: copy it from the computer (deploy/pc/move-data-to-nas.sh),"
fi
echo "then: systemctl start collection collection-nightly.timer"
