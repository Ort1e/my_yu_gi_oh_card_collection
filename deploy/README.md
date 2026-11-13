# Deploying: the app on the NAS, the public copy on OVH

The app runs on the NAS (a Netgear ReadyNAS), reachable from the home
network, and from **our own devices anywhere through Tailscale** (no port
opened on the box: [From outside home](#from-outside-home-tailscale)). Every morning the NAS backs the data up, updates the app
when `main` has a new build, then writes the **public copy** (the cards and the
deck versions played in tournaments or chosen, the Pokémon wishlist and the books, with
market values) and uploads it to the
**OVH hosting**. Why it is built this way: [Why this setup](#why-this-setup),
at the end.

```
deploy/
  README.md                 this file
  nas/                      on the NAS (in the app's archive, /apps/collection/deploy/: no `nas/`
                            folder there, the files sit directly in `deploy/`)
    install.sh              first install (folders, user, settings, services)
    update.sh               new build of main → backup, replace, restart (or back)
    nightly.sh              the nightly job: backup, update, public copy + upload
    upload-ovh.sh           the public copy → the OVH hosting (changed files only)
    common.sh               the settings and the app's commands, for the scripts
    collection.service      the app, started at boot
    collection-nightly.*    the nightly job and its timer (7:30)
    tailscaled.service      Tailscale, started at boot (deploy/pc/install-tailscale-on-nas.sh)
    collection.env.example  the app's settings (passwords)
    nightly.env.example     the nightly job's settings (OVH, GitHub token)
  pc/                       on the computer
    install-on-nas.sh       put a build on the NAS and install it
    move-data-to-nas.sh     move the live data to the NAS (once)
    pull-backup.sh          the NAS's latest copy → the data submodule (git)
    set-nas-setting.sh      set one setting on the NAS (a key, a password) and restart
    install-tailscale-on-nas.sh   Tailscale on the NAS (install, update)
```

On the NAS:

| What | Where |
|---|---|
| the app, its stylesheets, the scripts | `/apps/collection/` (root's) |
| the settings (secrets: never in git) | `/apps/collection/collection.env`, `nightly.env` |
| the live data | `/Data1To/collection/` (`app.db`, `media/`, `uploads/`) |
| the public copy, before its upload | `/Data1To/collection/public-export/` |
| the nightly copies (other disks) | `/Data500/collection-backups/` (14 days) |
| Tailscale (its programs, its state: the NAS's identity in the account) | `/apps/tailscale/` (root's) |

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
data. A new build that brings a new setting: `update.sh` adds it, empty,
with its comment, at the end of the settings file (the log says which);
fill it in with `deploy/pc/set-nas-setting.sh` (below) or by hand.

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

## From outside home: Tailscale

[Tailscale](https://tailscale.com) links our own devices (the NAS, the
computers, the phones) wherever they are: each one signs in to the same
Tailscale account, and only they can reach each other. The NAS only makes
outgoing connections: **no port is opened on the Bbox** (which has no VPN
server of its own), the NAS stays invisible from the internet.

**Once, at home** (the NAS on):

1. On the computer, at the repository's root:
   `deploy/pc/install-tailscale-on-nas.sh`. It installs Tailscale on the
   NAS (`/apps/tailscale`, the service `tailscaled`) and starts it.
2. It prints a link (`To authenticate, visit: …`): open it on the computer
   and log in to the Tailscale account. The NAS appears in the account's
   **Machines**.
3. It then publishes the app over HTTPS (`tailscale serve`). The first
   time it may print another link, to turn on **HTTPS** (and MagicDNS) for
   the account: open it, accept, and the script goes on. At the end it
   prints the app's address, `https://lahochenas3.<name>.ts.net`.
4. In the Tailscale admin page (login.tailscale.com), **Machines** → the
   NAS → **⋯** → **Disable key expiry**: otherwise the NAS has to be
   approved again every 6 months.

**On each device**: install the Tailscale app (Android, iPhone, Windows,
Linux: `tailscale up`) and log in to the same account. Then, from
anywhere (the NAS is on from 7:00 to 22:00):

- the app: `https://lahochenas3.<name>.ts.net` (the address step 3
  printed). HTTPS: the pages get the camera, so **the live book scan works
  without Chrome's setting** for the NAS's address. At home,
  `http://192.168.1.202:3000` still works too;
- SSH: `ssh root@lahochenas3`, or this in `~/.ssh/config`, then
  `NAS=nas-ts deploy/pc/…` for any script of `deploy/pc/` from anywhere:

  ```
  Host nas-ts
  	HostName lahochenas3
  	User root
  	IdentityFile ~/.ssh/nas_maison
  	IdentitiesOnly yes
  ```

  Or, so that `nas` itself goes through Tailscale whenever the home
  address doesn't answer (no `NAS=` needed), above `Host nas`:

  ```
  Match host nas !exec "timeout 1 bash -c '</dev/tcp/192.168.1.202/22' 2>/dev/null"
  	HostName lahochenas3
  ```

**The friend**: in the admin page, **Users** → **Invite** (her own
account in our network), or **Machines** → the NAS → **Share** (she sees
only the NAS). She installs the app on her phone and logs in.

**Updates**: Tailscale doesn't update itself here; run
`deploy/pc/install-tailscale-on-nas.sh` again now and then (it keeps the
NAS's identity), at home or from anywhere (`NAS=nas-ts`: the SSH
connection drops when tailscaled restarts, the script connects again). If its download stops on `curl: (60) … unable to get
local issuer certificate` (seen 2026-11-03 in one terminal of the
computer, cause not found: the certificate was Tailscale's real one), give
curl the system's list:
`SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt deploy/pc/install-tailscale-on-nas.sh`.
To remove it: `systemctl disable --now tailscaled`, delete
`/apps/tailscale` and `/etc/systemd/system/tailscaled.service`, and the
machine in the admin page.

Why userspace networking (`--tun=userspace-networking` in
`tailscaled.service`): the NAS's old kernel (4.4) and firewall need nothing
then. The connections coming through Tailscale reach the NAS's services on
127.0.0.1; through `tailscale serve`, the app sees each visitor's address
in `X-Forwarded-For` (the wrong-password limit is per visitor,
`common::web::auth::client`).

## By hand

| What | Command |
|---|---|
| the app's state, its log | `ssh nas systemctl status collection`, `ssh nas journalctl -u collection -n 100` |
| restart the app | `ssh nas systemctl restart collection` |
| update now | `ssh nas /apps/collection/deploy/update.sh` |
| a backup now | `ssh nas systemctl start collection-nightly` (the whole night's job) |
| back to the previous version | stop the app, move `/apps/collection/previous/*` back, start it (and the database of `/Data500/collection-backups` if a migration ran) |
| change a password | edit `collection.env`, `systemctl restart collection` (that user is logged out) |
| Tailscale's state, the app's address | `ssh nas /apps/tailscale/tailscale status`, `ssh nas /apps/tailscale/tailscale serve status` |
| set a setting from the computer (`GOOGLE_BOOKS_API_KEY`…) | `deploy/pc/set-nas-setting.sh NAME`: the value of the local `.env`, else asked (hidden); sent through SSH's input, written in `collection.env`, the app restarted |

## Security

- The box opens **no port** to the NAS: Netgear no longer updates the
  ReadyNAS, so it must stay unreachable from the internet. The app is for
  the home network and, through Tailscale, our own devices only (outgoing
  connections only, the devices of our Tailscale account); in plain HTTP
  at home (`SECURE_COOKIES=0`: the cookie works in both), HTTPS through
  Tailscale, every page
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

- **Who uses it**: the owner and a friend living together, at home
  (computers, phones on the Wi-Fi) and from outside through Tailscale
  (2026-11-02: being away without the NAS was impractical; the Bbox has no
  VPN server, and Tailscale opens no port).
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
