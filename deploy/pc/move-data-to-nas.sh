#!/bin/bash
# From the computer, once: move the live data to the NAS (data/app.db and
# data/media → /Data1To/collection), then start the app there. Afterwards
# the NAS's copy is the live one: don't start the server on data/app.db
# again (deploy/pc/pull-backup.sh brings its copies back to git).
#
#   deploy/pc/move-data-to-nas.sh [--force]
#
# Refused if the NAS already has data (--force: replace it).
set -euo pipefail
NAS="${NAS:-nas}"
DATA_DIR=/Data1To/collection
cd "$(git rev-parse --show-toplevel)"

if pgrep -x backend > /dev/null; then
    echo "stop the server first (Ctrl-C): it must close app.db cleanly"
    exit 1
fi
if [ -e data/app.db-wal ]; then
    echo "data/app.db-wal exists: start and stop the server once to fold it in"
    exit 1
fi
if [ "${1:-}" != "--force" ] && ssh "$NAS" "test -e $DATA_DIR/app.db"; then
    echo "the NAS already has $DATA_DIR/app.db: --force to replace it"
    exit 1
fi

ssh "$NAS" 'systemctl stop collection 2>/dev/null || true'
echo "copying the data (about $(du -sh data/app.db data/media | tail -1 | cut -f1) of media)…"
if ssh "$NAS" 'command -v rsync' > /dev/null; then
    rsync -a --info=progress2 data/app.db data/media "$NAS:$DATA_DIR/"
else
    scp -rq data/app.db data/media "$NAS:$DATA_DIR/"
fi
ssh "$NAS" "chown -R collection:collection $DATA_DIR && systemctl start collection collection-nightly.timer"
echo "moved: the app runs on the NAS (http://<NAS address>:3000)"
