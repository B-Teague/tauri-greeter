# CSSDM — CSS-based Display Manager

A Linux display manager built with Tauri: a Leptos/WASM greeter you restyle
with one CSS file, and a small Rust backend for authentication, session
discovery and power actions.

**Status**: proof of concept. See [Limitations](#limitations)).

## Features

- **One-file theming** — drop a stylesheet at `/etc/cssdm/theme.css`
- **System PAM authentication** — honours whatever `/etc/pam.d/login` is set to
- **Session discovery** — reads `/usr/share/{wayland-sessions,xsessions}`
- **Power actions** — suspend, restart, shut down via `systemctl`
- **Keyboard-first** — password field is focused on start, Enter submits

## Architecture

```
index.html + styles.css      trunk shell and default theme
src/                         greeter UI (Leptos, CSR/WASM)
  main.rs                    mounts App
  app.rs                     App, Clock, PowerButton, Avatar
src-tauri/src/               backend
  main.rs                    daemon by default, greeter with --greeter
  lib.rs                     the five Tauri commands
  daemon.rs                  greeter supervision, login socket
  auth.rs                    PAM, privilege drop, session exec
  session.rs                 .desktop discovery, session environment
  power.rs                   systemctl wrapper
themes/                      example stylesheets
```

The UI calls five commands: `users`, `sessions`, `theme_css`, `login`, `power`.

One binary, two roles. Systemd starts the **daemon**, which starts an X server
on the VT and runs the **greeter** (`cssdm --greeter`) on it as its only client —
no compositor, no window manager, nothing to install beyond the X server every
distro already ships. The greeter cannot start the desktop itself — it is holding
the display the desktop needs — so it posts the credentials to the daemon over
`/run/cssdm.sock` and exits on success. The daemon then stops the X server,
freeing the VT, and (holding the PAM handle that has to outlive the session)
execs the desktop as the user, X11 or Wayland:

```
systemd → cssdm (daemon, root, tty1)
            ├── Xorg :0 vt1 -auth …             display for the greeter only
            ├── cssdm --greeter                 webview; exits once authenticated
            └── $SHELL -l -c <session Exec>     setsid + initgroups + setgid/setuid
```

## Build

Needs Rust, the `wasm32-unknown-unknown` target, [trunk](https://trunkrs.dev),
and the usual Tauri/webkit2gtk build dependencies (see [INSTALL.md](INSTALL.md)).

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk

make release   # trunk build --release, then cargo build --release -p cssdm
make test      # backend unit tests
```

`trunk` builds the UI into `dist/`, which Tauri embeds in the binary, so the
UI has to be built first — `make` handles the ordering.

For development, `cargo tauri dev` starts `trunk serve` and the app together.

## Install

### Arch / CachyOS package (recommended)

```bash
make package                                  # release build, then makepkg
sudo pacman -U packaging/arch/*.pkg.tar.zst   # pulls in xorg-server and the rest
```

`packaging/arch/PKGBUILD` is a **binary** package: it installs the binary that
`make release` produced, it does not compile one. It lays down
`/usr/bin/cssdm`, `/usr/lib/systemd/system/cssdm.service` (with `ExecStart`
rewritten to `/usr/bin`), the example themes and the docs.

It installs the unit but does **not** enable it — see below.

### Or by hand

```bash
make install          # binary + example themes, under /usr/local
make systemd-install  # enable cssdm.service
```

`cssdm.service` carries `Alias=display-manager.service`, so enabling it is
what points the system at it — disable whatever is there now first:

```bash
sudo systemctl disable sddm    # or gdm, lightdm, …
sudo systemctl enable cssdm
```

## Theming

The greeter loads `/etc/cssdm/theme.css` at startup and applies it over the
built-in stylesheet. There is no theme picker — the machine shows what its
operator installed.

```bash
sudo install -Dm644 themes/midnight.css /etc/cssdm/theme.css
```

See [themes/README.md](themes/README.md) for the custom properties and class
names available.

## Limitations

- **Not a lock screen.** A display manager shows a greeter; locking a running
  session is the session's own job (`kscreenlocker`, `swaylock`, …). Being the
  locker on Wayland means implementing `ext-session-lock-v1`, which a webview
  cannot do.
- **The greeter runs as root**, unlike gdm's and sddm's, which use a dedicated
  unprivileged account and keep only authentication privileged.
- **An X server hosts the greeter**, and it is the only supported host. A
  webview cannot paint on a bare VT, and Xorg is the one display server present
  on every distro. The desktop started afterwards is unaffected.
- **No remembered state.** The last user and their last session are not
  persisted, so the greeter always defaults to the first of each.
- **Single seat.** No multi-seat or remote (XDMCP) support.
- **Single-prompt PAM.** The conversation handler replays one password, so
  2FA, fingerprint and expired-password changes cannot complete.
- **`script-src 'unsafe-inline'`.** Trunk emits the WASM loader inline and
  Tauri nonces only `script[src^='http']`, so a strict `script-src` blanks the
  greeter. The rest of the CSP is same-origin; see [SECURITY.md](SECURITY.md).
- **PAM policy.** Authenticates against the `login` policy rather than shipping
  its own `/etc/pam.d/cssdm`.

## Docs

- [INSTALL.md](INSTALL.md) — install, deploy, roll back
- [LOCAL_TESTING.md](LOCAL_TESTING.md) — test without replacing your DM
- [SECURITY.md](SECURITY.md) — security model
- [themes/README.md](themes/README.md) — theming reference
- [PLAN.md](PLAN.md) — original phase plan (historical)
- [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) — dependency notices

## Author

Brian Teague — <https://github.com/B-Teague>

## License

Copyright (C) 2026 Brian Teague.

CSSDM is free software under the **GNU General Public License v3.0 or later** —
see [LICENSE](LICENSE). It comes with NO WARRANTY, to the extent permitted by
law.

The binary statically links around 180 crates. Every one is available under a
permissive, GPLv3-compatible license — MIT for all but a handful, plus
Unicode-3.0, Zlib, BSL-1.0 and CC0-1.0 — and
[THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) lists each crate with its
elected license and its copyright holders, followed by one copy of each of
those five license texts — the holders live in the table, so no text is
repeated per crate. `make third-party` regenerates it:
[cargo-about](https://github.com/EmbarkStudios/cargo-about) resolves and elects
the licenses, `packaging/third-party-licenses.py` renders them. `about.toml`
holds the accepted-license list, so a dependency arriving under anything else
fails that command instead of shipping unnoticed. Both files are installed to
`/usr/share/licenses/cssdm/`.

The application icon in `src-tauri/icons/` is CSSDM's own, drawn from
`icon.svg` in that directory and covered by the same license. Nothing here
carries the Tauri or Leptos logos.
