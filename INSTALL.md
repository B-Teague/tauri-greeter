# Tauri Greeter Installation Guide

Building and installing Tauri Greeter as your LightDM greeter.

Installing it does not switch anything on. LightDM keeps drawing whatever it
drew before until you point it here, which is step 4 — and step 3 lets you try
it first in a nested X server, with the real seat untouched.

## Prerequisites

- Linux, x86_64 or ARM
- **LightDM**, at build time as well as runtime: the greeter links against
  `liblightdm-gobject-1`, which ships in the `lightdm` package
- Rust 1.70+ ([install](https://rustup.rs/)) with the `wasm32-unknown-unknown`
  target and [trunk](https://trunkrs.dev): `rustup target add
  wasm32-unknown-unknown && cargo install trunk`
- Standard build tools: `gcc`, `make`, `pkg-config`
- GTK and webkit2gtk development libraries

There is no PAM dependency any more, and no X server dependency: LightDM
already requires both.

### Install Build Dependencies

**Debian/Ubuntu:**
```bash
sudo apt-get update
sudo apt-get install -y build-essential liblightdm-gobject-1-dev libgtk-3-dev \
  libwebkit2gtk-4.1-dev librsvg2-dev patchelf
```

**Fedora/RHEL:**
```bash
sudo dnf install -y @development-tools lightdm-gobject-devel gtk3-devel \
  webkit2gtk4.1-devel glib2-devel
```

**Arch Linux / CachyOS:**
```bash
sudo pacman -S base-devel lightdm gtk3 webkit2gtk-4.1 glib2
# for step 3:
sudo pacman -S xorg-server-xephyr
```

## Install From the Arch Package

The short path on Arch, CachyOS and derivatives:

```bash
make package                                  # make release, then makepkg
sudo pacman -U packaging/arch/*.pkg.tar.zst
```

`packaging/arch/PKGBUILD` packages an **already-built** binary; it has no build
step and no Rust makedepends. `make release` is what compiles, and `make
package` runs the two in order. pacman resolves `lightdm`, `webkit2gtk-4.1`,
`gtk3` and `glib2` from the repos.

The package registers the greeter with LightDM but does not select it: that
would change your login screen behind your back. Post-install prints the
commands; they are step 4 below.

To uninstall: `sudo pacman -R tauri-greeter`. If LightDM is still pointed at it,
`pre_remove` says so — remove the setting or LightDM falls back to its default
greeter at the next start.

## Installation Steps

### 1. Build

```bash
make release
```

`trunk` compiles the Leptos UI to WebAssembly in `dist/`, then `cargo` builds
the binary with `dist/` embedded in it. `make` handles the ordering; building
the backend alone gives you a greeter with no UI.

### 2. Install

```bash
make install
```

Lays down, with `sudo`:

| Path | What |
| --- | --- |
| `/usr/local/bin/tauri-greeter` | the binary |
| `/usr/share/xgreeters/tauri-greeter.desktop` | how LightDM finds it |
| `/usr/share/tauri-greeter/themes/*.css` | example stylesheets for the picker |
| `/usr/share/licenses/tauri-greeter/` | `LICENSE`, `THIRD-PARTY-LICENSES.md` |

Nothing is enabled and nothing existing is replaced.

### 3. Test It Without Touching Your Seat

```bash
make test-mode
```

Runs a whole LightDM — greeter included — inside a nested X server, as you, with
no root. Everything it writes goes to one temporary directory that is deleted on
exit; the system's `/etc/lightdm` and `/run/lightdm` are never involved. Close
the window to stop it.

Needs `xorg-server-xephyr`. See [LOCAL_TESTING.md](LOCAL_TESTING.md) for what to
check and how to read the logs.

### 4. Select It

```bash
printf '[Seat:*]\ngreeter-session=tauri-greeter\n' |
    sudo install -Dm644 /dev/stdin /etc/lightdm/lightdm.conf.d/50-tauri-greeter.conf
```

A drop-in rather than an edit to `/etc/lightdm/lightdm.conf`, so removing the
file is a complete undo and a package upgrade never fights you for it.

If some other display manager is currently enabled, switch:

```bash
sudo systemctl disable sddm     # or gdm, whichever is enabled
sudo systemctl enable lightdm
```

`lightdm.service` carries `Alias=display-manager.service`; enabling it is what
points the system at LightDM.

### 5. (Optional) Install a Theme

```bash
sudo install -Dm644 themes/midnight.css /etc/tauri-greeter/theme.css
```

One file, read at startup. See [themes/README.md](themes/README.md).

### 6. Verify

```bash
make verify
```

Reports the binary, the greeter entry, and what LightDM's config actually
selects.

### 7. Apply

```bash
sudo systemctl restart lightdm    # ends the session you are sitting in
# or just reboot
```

## Uninstallation

```bash
sudo rm -f /etc/lightdm/lightdm.conf.d/50-tauri-greeter.conf   # first
make uninstall                                                 # then
sudo rm -f /etc/tauri-greeter/theme.css                        # if you added one
```

Order matters: remove the selection before the binary, or LightDM will try to
launch something that is gone. It falls back to its default greeter either way,
but there is no reason to make it.

## Troubleshooting

### LightDM Falls Back to Another Greeter

LightDM tries `greeter-session`, and on failure uses its configured default.

```bash
journalctl -u lightdm -b | tail -50
sudo cat /var/log/lightdm/lightdm.log
lightdm --show-config | grep -A3 'Seat:'
```

Check in order:
- `/usr/share/xgreeters/tauri-greeter.desktop` exists and its `Exec=` points at
  the binary that is actually installed — `make install` writes
  `/usr/local/bin`, the Arch package writes `/usr/bin`
- the binary is executable by the `lightdm` user
- `greeter-session=tauri-greeter` matches the `.desktop` **file stem**, not the
  `Name=` inside it

### Blank or White Greeter

WebKitGTK's DMABUF renderer fails on a bare X server with no compositor. The
greeter forces the software path for exactly this reason:

```bash
grep WEBKIT_DISABLE_DMABUF_RENDERER src-tauri/src/lib.rs
```

If it is still blank, the UI was probably not built into the binary — `cargo
build` alone does not run `trunk`. Use `make release`.

### "Login is unavailable."

The greeter could not reach LightDM, or LightDM did not answer in 120s. This is
not a verdict on the password.

```bash
sudo cat /var/log/lightdm/lightdm.log | grep -i greeter
```

Seen when the binary is run by hand from a desktop — there is no LightDM to talk
to, which is expected. The theme picker still works, which is what that mode is
for.

### Authentication Always Fails

The PAM stack is LightDM's, not this program's:

```bash
cat /etc/pam.d/lightdm
sudo journalctl -u lightdm -b | grep -i pam
faillock --user "$USER"     # locked out by earlier failures?
```

A stack that asks more than one question cannot complete here — see the
single-prompt limitation in [README.md](README.md#limitations).

### No Users, or Too Many

The list is LightDM's:

```bash
cat /etc/lightdm/users.conf              # minimum-uid, hidden-users, hidden-shells
busctl --system tree org.freedesktop.Accounts 2>/dev/null | head
```

If AccountsService is installed, it wins and `users.conf` is ignored.

### No Sessions Listed

LightDM scans `sessions-directory`, which defaults to
`/usr/share/xsessions:/usr/share/wayland-sessions`:

```bash
ls /usr/share/xsessions/ /usr/share/wayland-sessions/
lightdm --show-config | grep sessions-directory
```

A session with `Hidden=true` or `NoDisplay=true` is skipped by LightDM itself.

### Themes Not Loading

```bash
ls -l /etc/tauri-greeter/theme.css        # must be world-readable
ls /usr/share/tauri-greeter/themes/
```

The greeter runs as `lightdm`, not root: a mode-600 theme file is invisible to
it. A CSS syntax error is silently ignored by the webview, so bisect by
truncating the file.

## Monitoring

```bash
journalctl -u lightdm -f                  # live
sudo tail -f /var/log/lightdm/lightdm.log # LightDM's own log
sudo tail -f /var/log/lightdm/x-0.log     # the X server under it
```

Everything the greeter writes to stderr lands in the first two.

## Development

```bash
make build                          # debug build
TAURI_GREETER_WINDOWED=1 ./target/debug/tauri-greeter
```

Runs the greeter in an ordinary window on your current desktop. There is no
LightDM to talk to, so login reports "Login is unavailable." — the UI, the user
and session lists, and the theme picker all work, which makes this the fast loop
for everything but authentication. `make test-mode` is the loop for
authentication.

`cargo tauri dev` starts `trunk serve` and the app together, with hot reload.

## Rollback

```bash
sudo rm -f /etc/lightdm/lightdm.conf.d/50-tauri-greeter.conf
sudo systemctl restart lightdm
```

LightDM goes back to its default greeter. Nothing else needs undoing — there is
no service of ours to disable and no system file of ours to restore.

## Next Steps

- [LOCAL_TESTING.md](LOCAL_TESTING.md) — the nested-LightDM harness in detail
- [SECURITY.md](SECURITY.md) — the trust boundary
- [themes/README.md](themes/README.md) — writing a theme
