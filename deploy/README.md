# Deploying: the app on the NAS, the public copy on OVH

The app runs on the NAS (a Netgear ReadyNAS), reachable **from the home
network only**. Every morning the NAS backs the data up, updates the app
when `main` has a new build, then writes the **public copy** (the cards and the
decks played in tournaments, the Pokémon wishlist and the books, with
market values) and uploads it to the
**OVH hosting**. Why it is built this way: [Why this setup](#why-this-setup),
at the end.

```
deploy/
  README.md                 this file
  nas/                      on the NAS (in the app's archive, /apps/collection/deploy/)
    install.sh              first install (folders, user, settings, services)
    update.sh               new build of main → backup, replace, restart (or back)
    nightly.sh              the nightly job: backup, update, public copy + upload
    upload-ovh.sh           the public copy → the OVH hosting (changed files only)
    common.sh               the settings and the app's commands, for the scripts
    collection.service      the app, started at boot
    collection-nightly.*    the nightly job and its timer (7:30)
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

The NAS (Netgear ReadyNAS, x86_64, 2 GB, ReadyNAS OS 6.10 = Debian 8 with
systemd, `curl` and `sqlite3` but no `git`) is at `192.168.1.202`. It is
**on from 7:00 to 22:00** only, switched off otherwise.

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
       # faster: one connection reused for 10 minutes
       ControlMaster auto
       ControlPath ~/.ssh/cm-%r@%h:%p
       ControlPersist 10m
   ```

4. `ssh nas uname -m` answers `x86_64`. Then turn the SSH password login
   off in the admin page (the key is enough).
5. **Slow logins** (several seconds, even for `ssh nas true`): the NAS's
   SSH server looks your computer's name up in DNS. On the NAS, in
   `/etc/ssh/sshd_config`, set `UseDNS no`, then `systemctl restart ssh`
   (ReadyNAS may rewrite that file when SSH is turned off and on in its
   admin page: set it again then). `ssh -v nas true` shows where it
   waits.

The scripts use the host `nas`; another name: `NAS=… deploy/pc/…`.

## 2. Updates without a token

The app's repository is public (only the data repository is private): the
NAS downloads the builds from the release page like anyone, no GitHub
token needed. Leave `GITHUB_TOKEN` empty in `nightly.env`; it would only
serve if the repository became private (a fine-grained token, this
repository only, "Contents: read-only").

## 3. First install

The CI builds the app for the NAS (job `nas-build`: static, so the NAS's
old Debian doesn't matter) on pull requests into `main` and on pushes to
it, not on `dev` (an optimized build takes long: about 15 minutes the
first time, a few minutes once cached); after a push to `main`, with all
the checks green, `nas-release` publishes it as the release `nas-latest`. From the computer, at the repository's
root:

```bash
deploy/pc/install-on-nas.sh
# or with an archive downloaded from the release page:
deploy/pc/install-on-nas.sh ~/Downloads/collection-nas.tar.gz
```

It copies the archive, then runs `install.sh` on the NAS: the user
`collection`, the folders, the services (enabled, not started), and the
two settings files from their examples. Fill them in on the NAS:

```bash
ssh nas
nano /apps/collection/collection.env   # APP_PASSWORD (yours), APP_PASSWORD_FRIEND
nano /apps/collection/nightly.env      # OVH_USER, OVH_PASSWORD
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

## Every morning (7:30, by itself)

`collection-nightly.timer` runs `nightly.sh` at 7:30, half an hour after
the NAS starts (it is off from 22:00 to 7:00: the job was at 3:30 until
2026-10-28 and never ran). A day the NAS starts later, the job runs as
soon as it is up with the network (`Persistent=true`). systemd only
catches up once the timer has run at least once: after a first install,
start the job by hand once (below).

1. **backup**: `backend backup --to /Data500/collection-backups`: the
   database copied while the app runs (`app-YYYY-MM-DD.db`, 14 days kept),
   the media mirrored;
2. **update**: `update.sh`: when the release `nas-latest` has a new build
   of `main`: a backup, then the new version replaces the old (kept in
   `/apps/collection/previous/`) and starts; if it doesn't answer within 30
   seconds, back to the previous version and its database;
3. **public copy**, by the version just installed (a change to the copy is
   online the same morning): `backend export-public` into
   `/Data1To/collection/public-export`, then `upload-ovh.sh`: only the files
   changed since the last upload are sent, the removed ones deleted there.
   If the copy fails, nothing is uploaded: yesterday's stays online.

Each step runs even when the one before failed. To see what happened:
`ssh nas journalctl -u collection-nightly -n 50`. To run it now:
`ssh nas systemctl start collection-nightly`.

## The OVH hosting

`upload-ovh.sh` sends the copy into `OVH_DIR`, a folder the script
creates. The free plan allows **one attached domain** (no subdomain like
`collection.<domain>`: "Attached Domains limit"), so the copy lives in a
subfolder of the site: `OVH_DIR=www/collection` → `https://<domain>/collection/`,
the domain's root (`www/`) staying free for a personal site. The copy's
links are relative, so any folder works; the script only ever deletes the
files it uploaded itself.

- **SFTP** (`OVH_PROTOCOL=sftp`, enabled in the OVH panel's FTP-SSH tab on
  2026-10-26): encrypted, one `sftp` session for all the files (the NAS's
  own `sftp`: its `curl` is too old for OVH's server), the password given
  through `SSH_ASKPASS` (no `sshpass` on the NAS), every file made readable
  by the web server (644). Tested against the hosting on 2026-10-26.
- **The server's key** is checked: `install.sh` writes it into
  `/apps/collection/ovh_known_hosts` and prints its fingerprint, which must
  be `SHA256:xhieLplnoEvvl7+a8sq8wLCh/bvOQvQFIVewi+fK2og` (ED25519). If OVH
  ever changes it, the upload stops: check the new one in the OVH
  documentation before replacing the file.
- **Plain FTP** (`OVH_PROTOCOL=ftp`) still works (`curl`), but the password
  then travels unencrypted. The free plan refuses FTP over TLS.
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

## Security

- The box opens **no port** to the NAS: Netgear no longer updates the
  ReadyNAS, so it must stay unreachable from the internet. The app is for
  the home network only, in plain HTTP (`SECURE_COOKIES=0`), every page
  behind the login (`PUBLIC_READS=0`: a guest on the Wi-Fi sees nothing),
  5 wrong passwords → 15 minutes of refusal.
- One password per person (`APP_PASSWORD`, `APP_PASSWORD_<NAME>`): one can
  be changed alone, and the log names who did what. The owner changes
  everything; the others see everything but change only the books, unless
  `APP_ACCESS_<NAME>=all` (or a list: `books,pokemon`) in `collection.env`.
- Every answer carries a Content-Security-Policy, `X-Frame-Options: DENY`,
  `nosniff` and a `Referrer-Policy`; uploaded photos and PDFs are read
  with limits (huge images, deeply nested PDFs refused). Tests:
  `crates/backend/src/tests/security.rs`, `crates/yugioh/src/tests/security.rs`.
- The CI runs `cargo audit` and `npm audit`; the advisories accepted on
  purpose are explained in `.cargo/audit.toml`.
- The settings files are readable by root (and `collection.env` by the
  app's user): never copy them into git.
- The public copy holds only the collections and the wishlist: a test
  checks that no seller, price paid, note, deck, tournament or budget gets
  in (`crates/backend/src/tests/export.rs`).
- `main` is protected on GitHub: no force push, no deletion, changes
  through a pull request from `dev` with the four checks green (rust,
  audit, deck-builder, nas-build).

## Why this setup

- **Who uses it**: the owner and a friend living together, from the home
  network only (computers, phones on the Wi-Fi). No access from outside,
  so no VPN.
- **What is public**: a read-only copy of the Yu-Gi-Oh! cards, the
  Pokémon cards and wishlist and the books, with market values. No lots,
  sellers, prices paid, budget, decks, tournaments, advent calendars or
  notes.
- **The NAS** is always at home and holds the disks: the app and the live
  data live there. It can't run modern programs from its old Debian, so
  the CI builds one **static** binary (musl, for any x86-64 processor:
  the NAS's is older than the CI's) and the NAS only downloads it.
- **The OVH hosting** (free plan: 100 MB, static files and PHP, no
  program of ours can run there) gets only files: the public copy, about
  20 MB, nothing to attack.
- **The copies**: every morning on the NAS's other volume (other disks), and
  weekly into the private data repository from the computer, since the
  NAS has no `git` (and installing packages on a ReadyNAS is risky).

Still to do once: a scan of the public site (testssl.sh, OWASP ZAP
baseline) and of the app from the home network (TODO.md).
