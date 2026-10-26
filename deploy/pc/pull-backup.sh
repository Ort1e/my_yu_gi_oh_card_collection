#!/bin/bash
# From the computer: bring the NAS's latest nightly copy into the data
# submodule and commit it (the NAS has no git). Weekly, or whenever.
#
#   deploy/pc/pull-backup.sh [--push]
#
# The database (the newest app-YYYY-MM-DD.db) becomes data/app.db; the
# media tracked in git (shipment PDFs, calendar photos) are copied. The
# card pictures are a cache: not copied.
set -euo pipefail
NAS="${NAS:-nas}"
BACKUP_DIR=/Data500/collection-backups
cd "$(git rev-parse --show-toplevel)"

if pgrep -x backend > /dev/null; then
    echo "stop the local server first: data/app.db is about to be replaced"
    exit 1
fi

latest=$(ssh "$NAS" "ls -1 $BACKUP_DIR/app-*.db | sort | tail -1")
[ -n "$latest" ] || { echo "no copy on the NAS"; exit 1; }
scp -q "$NAS:$latest" data/app.db
rm -f data/app.db-wal data/app.db-shm
for folder in shipment_files advent_calendar_photos book_advent_calendar_photos; do
    if ssh "$NAS" "test -d $BACKUP_DIR/media/$folder"; then
        mkdir -p "data/media/$folder"
        scp -rq "$NAS:$BACKUP_DIR/media/$folder/." "data/media/$folder/"
    fi
done

git -C data add app.db media
if git -C data diff --cached --quiet; then
    echo "nothing new since the last copy"
    exit 0
fi
git -C data commit -q -m "Copy of the NAS's data ($(basename "$latest" .db))"
echo "committed in data/: $(git -C data log --oneline -1)"
if [ "${1:-}" = "--push" ]; then
    git -C data push -q
    git add data
    git commit -q -m "data: $(basename "$latest" .db)"
    echo "pushed; the pointer is committed here (push it with the next changes)"
fi
