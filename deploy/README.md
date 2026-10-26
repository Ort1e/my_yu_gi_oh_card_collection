# Deploying: the app on the NAS, the public copy on OVH

The app runs on the NAS (a Netgear ReadyNAS), reachable **from the home
network only**. Every night the NAS backs the data up, writes the **public
copy** (the cards, the Pokémon wishlist and the books, with market values)
and uploads it to the **OVH hosting**, then updates the app when `main` has
a new build. Why it is built this way: `docs/specs/deployment.md`.

```
deploy/
  README.md                 this file
  nas/                      on the NAS (in the app's archive, /apps/collection/deploy/)
    install.sh              first install (folders, user, settings, services)
    update.sh               new build of main → backup, replace, restart (or back)
    nightly.sh              the nightly job: backup, public copy + upload, update
    upload-ovh.sh           the public copy → the OVH hosting (changed files only)
    common.sh               the settings and the app's commands, for the scripts
    collection.service      the app, started at boot
    collection-nightly.*    the nightly job and its timer (3:30)
    collection.env.example  the app's settings (passwords)
    nightly.env.example     the nightly job's settings (OVH, GitHub token)
  pc/                       on the computer
    install-on-nas.sh       put a build on the NAS and install it
    move-data-to-nas.sh     move the live data to the NAS (once)
    pull-backup.sh          the NAS's latest copy → the data submodule (git)
```

On the NAS:

| What | Where |
|---|---|
| the app, its stylesheets, the scripts | `/apps/collection/` (root's) |
| the settings (secrets: never in git) | `/apps/collection/collection.env`, `nightly.env` |
| the live data | `/Data1To/collection/` (`app.db`, `media/`, `uploads/`) |
| the public copy, before its upload | `/Data1To/collection/public-export/` |
| the nightly copies (other disks) | `/Data500/collection-backups/` (14 days) |

The app runs as the user `collection`, which can write the data but not
the app itself; the scripts run as root.

## 1. SSH to the NAS (once)

1. In the NAS's admin page: System → Settings → Services → **SSH**: on.
2. Your key on the NAS (the key pair `~/.ssh/nas_maison`):
   `ssh-copy-id -i ~/.ssh/nas_maison.pub root@192.168.1.202` (asks the
   admin password once), or Accounts → Users → admin → Settings → SSH →
   import `nas_maison.pub`.
3. In `~/.ssh/config`:

   ```
   Host nas
       HostName 192.168.1.202
       User root
       IdentityFile ~/.ssh/nas_maison
       IdentitiesOnly yes
   ```

4. `ssh nas uname -m` answers `x86_64`. Then turn the SSH password login
   off in the admin page (the key is enough).

The scripts use the host `nas`; another name: `NAS=… deploy/pc/…`.

## 2. A GitHub token for the updates (once)

The repository is private: the NAS reads its releases with a token.
GitHub → Settings → Developer settings → Fine-grained tokens → new:
repository access **only this repository**, permission **Contents:
read-only**, an expiry (a year; renew it then). It goes in `nightly.env`
(`GITHUB_TOKEN`) and serves `install-on-nas.sh` too.

## 3. First install

The CI builds the app for the NAS on every push to `main` (job
`nas-build`: static, so the NAS's old Debian doesn't matter) and publishes
it as the release `nas-latest`. From the computer, at the repository's
root:

```bash
GITHUB_TOKEN=github_pat_… deploy/pc/install-on-nas.sh
# or with the archive downloaded from the release page:
deploy/pc/install-on-nas.sh ~/Downloads/collection-nas.tar.gz
```

It copies the archive, then runs `install.sh` on the NAS: the user
`collection`, the folders, the services (enabled, not started), and the
two settings files from their examples. Fill them in on the NAS:

```bash
ssh nas
nano /apps/collection/collection.env   # APP_PASSWORD (yours), APP_PASSWORD_FRIEND
nano /apps/collection/nightly.env      # OVH_USER, OVH_PASSWORD, GITHUB_TOKEN
```

`install.sh` can be run again: it never overwrites the settings or the
data.

## 4. Moving the data (once)

On the computer: stop the server (Ctrl-C), commit the data one last time
(docs/recipes.md, "Save your data to git"), then:

```bash
deploy/pc/move-data-to-nas.sh
```

It copies `data/app.db` and `data/media` to the NAS and starts the app.
Open **http://192.168.1.202:3000** (computers and phones on the home
Wi-Fi) and log in. From now on the NAS's data is the live one: don't start
the server on `data/app.db` again (work on a copy, see below).

## Every night (3:30, by itself)

`collection-nightly.timer` runs `nightly.sh`:

1. **backup**: `backend backup --to /Data500/collection-backups`: the
   database copied while the app runs (`app-YYYY-MM-DD.db`, 14 days kept),
   the media mirrored;
2. **public copy**: `backend export-public` into
   `/Data1To/collection/public-export`, then `upload-ovh.sh`: only the files
   changed since the last upload are sent, the removed ones deleted there;
3. **update**: `update.sh`: when the release `nas-latest` has a new build
   of `main`: a backup, then the new version replaces the old (kept in
   `/apps/collection/previous/`) and starts; if it doesn't answer within 30
   seconds, back to the previous version and its database.

Each step runs even when the one before failed. To see what happened:
`ssh nas journalctl -u collection-nightly -n 50`. To run it now:
`ssh nas systemctl start collection-nightly`.

## The OVH hosting

`upload-ovh.sh` sends the copy into `OVH_DIR` (`www`: the folder the
domain shows; for a subdomain like `collection.<domain>`, create it in the
OVH panel, Multisite, pointing to a folder, and put that folder in
`OVH_DIR`).

- **Today: plain FTP** (`OVH_PROTOCOL=ftp`). The free plan refuses FTP over
  TLS, and its SFTP refused the login (checked 2026-10-26): the password
  travels unencrypted every night. What is on the hosting is public
  anyway; the risk is someone changing the site. Use a password of its
  own, used nowhere else.
- **Better: SFTP.** In the OVH panel, the hosting's **FTP-SSH** tab: if the
  user can have SFTP enabled, enable it, set `OVH_PROTOCOL=sftp`, and add the
  server's key once on the NAS:
  `ssh-keyscan ftp.cluster129.hosting.ovh.net >> /root/.ssh/known_hosts`.
- Upload everything again (after cleaning the hosting by hand):
  `rm /Data1To/collection/public-export.uploaded`, then
  `systemctl start collection-nightly`.

## Backups to GitHub (weekly, from the computer)

The NAS has no `git` (and installing packages on a ReadyNAS is risky), so
the computer brings the copies into the data submodule:

```bash
deploy/pc/pull-backup.sh            # the newest copy → data/, committed in the submodule
deploy/pc/pull-backup.sh --push     # and pushed, with the pointer committed here
```

To work on the real data on the computer (development, a script), use a
copy: `DATABASE_URL=sqlite:/tmp/copy.db cargo run -p backend` after
copying `data/app.db` there, never the NAS's live database.

## By hand

| What | Command |
|---|---|
| the app's state, its log | `ssh nas systemctl status collection`, `ssh nas journalctl -u collection -n 100` |
| restart the app | `ssh nas systemctl restart collection` |
| update now | `ssh nas /apps/collection/deploy/update.sh` |
| a backup now | `ssh nas systemctl start collection-nightly` (the whole night's job) |
| back to the previous version | stop the app, move `/apps/collection/previous/*` back, start it (and the database of `/Data500/collection-backups` if a migration ran) |
| change a password | edit `collection.env`, `systemctl restart collection` (that user is logged out) |

## Security, in short

- The box opens **no port** to the NAS: Netgear no longer updates the
  ReadyNAS, so it must stay unreachable from the internet. The app is for
  the home network only, in plain HTTP (`SECURE_COOKIES=0`), every page
  behind the login (`PUBLIC_READS=0`), 5 wrong passwords → 15 minutes of
  refusal.
- The settings files are readable by root (and `collection.env` by the
  app's user): never copy them into git.
- The public copy holds only the collections and the wishlist: a test
  checks that no seller, price paid, note, deck, tournament or budget gets
  in (`crates/backend/src/tests/export.rs`).
