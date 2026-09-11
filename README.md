# Tauri Greeter — a CSS-themeable LightDM greeter

A LightDM greeter built with Tauri: a Leptos/WASM login screen you restyle with
one CSS file. LightDM does the display management — the VT, the X server, PAM,
privilege dropping, starting the desktop — and this draws the login screen it
shows.

| Download | Installed | Binary | UI (WASM) |
| --- | --- | --- | --- |
| 1.6 MB | 4.7 MB | 4.6 MB | 311 KB |

A single binary holds the greeter and its whole UI — no interpreter, no bundled
runtime, no `node_modules`. Rust compiles the backend into that binary and the
frontend into WebAssembly next to it; Tauri draws it with the webview the system
already ships (`webkit2gtk`), so nothing here carries a browser of its own.
Figures are a 0.2.0 release build for x86_64, from `make package`.

**Status**: proof of concept. See [Limitations](#limitations).

## Features

- **One-file theming** — drop a stylesheet at `/etc/tauri-greeter/theme.css`
- **LightDM authentication** — LightDM's own PAM conversation, over the greeter socket
- **User and session lists** — from LightDM, so `/etc/lightdm/users.conf` and
  AccountsService are honoured
- **Power actions** — suspend, restart, shut down through LightDM, gated on what
  logind says it will allow
- **Keyboard-first** — password field is focused on start, Enter submits

## Architecture

```
index.html + styles.css      trunk shell and default theme
src/                         greeter UI (Leptos, CSR/WASM)
  main.rs                    mounts App
  app.rs                     App, Clock, PowerButton, Avatar
src-tauri/src/               backend
  main.rs                    --version, then the greeter
  lib.rs                     the six Tauri commands
  lightdm.rs                 liblightdm-gobject bindings and the login conversation
themes/                      example stylesheets
packaging/                   LightDM greeter entry, Arch package, test harness
```

The UI calls six commands: `users`, `sessions`, `themes`, `theme_css`, `login`,
`power`.

One process, one job. Systemd starts **LightDM**, which owns the VT, starts the
X server, and runs this greeter on it as the unprivileged `lightdm` user —
handing it a pipe pair on the way in. The greeter has no privileges and never
touches PAM itself: it sends a username down the pipe, answers the prompts
LightDM's PAM conversation sends back, and on success asks LightDM to start the
chosen session. LightDM authenticates, drops privileges and execs the desktop,
then stops the greeter.

```
systemd → lightdm (root)
            ├── Xorg :0 vt1 -auth …        display for the greeter only
            ├── tauri-greeter              webview, as user `lightdm`
            │     └─ LIGHTDM_TO_SERVER_FD / LIGHTDM_FROM_SERVER_FD
            └── the user's session         after LightDM authenticates and drops privileges
```

`liblightdm-gobject-1` speaks that pipe protocol, so `lightdm.rs` is bindings to
it plus the small state machine that turns "username and password" into a
session. Its replies arrive on a GLib watch attached to the default main
context — the same one Tauri's GTK loop already turns — so there is no second
thread and no loop of our own.

## Build

Needs Rust, the `wasm32-unknown-unknown` target, [trunk](https://trunkrs.dev),
and the usual Tauri/webkit2gtk build dependencies (see [INSTALL.md](INSTALL.md)).

LightDM itself is needed at build time too: `lightdm.rs` links against
`liblightdm-gobject-1`, which ships in the `lightdm` package.

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk

make release   # trunk build --release, then cargo build --release -p tauri-greeter
make test      # backend unit tests
```

`trunk` builds the UI into `dist/`, which Tauri embeds in the binary, so the
UI has to be built first — `make` handles the ordering.

For development, `cargo tauri dev` starts `trunk serve` and the app together.

## Install

### Arch / CachyOS package (recommended)

```bash
make package                                  # release build, then makepkg
sudo pacman -U packaging/arch/*.pkg.tar.zst   # pulls in lightdm and the rest
```

`packaging/arch/PKGBUILD` is a **binary** package: it installs the binary that
`make release` produced, it does not compile one. It lays down
`/usr/bin/tauri-greeter`, `/usr/share/xgreeters/tauri-greeter.desktop` (with `Exec`
rewritten to `/usr/bin`), the example themes and the docs.

It registers the greeter but does **not** select it — see below.

### Or by hand

```bash
make install   # binary, example themes, and the /usr/share/xgreeters entry
```

### Selecting it

Installing only makes the greeter available; LightDM keeps drawing whatever it
drew before until it is pointed here:

```bash
printf '[Seat:*]\ngreeter-session=tauri-greeter\n' |
    sudo install -Dm644 /dev/stdin /etc/lightdm/lightdm.conf.d/50-tauri-greeter.conf
sudo systemctl enable lightdm     # if another display manager is enabled,
                                  # disable that one first
```

Try it in a nested LightDM first — `make test-mode`, see
[LOCAL_TESTING.md](LOCAL_TESTING.md).

## Theming

The greeter loads `/etc/tauri-greeter/theme.css` at startup and applies it over the
built-in stylesheet. The picker in the footer switches between the examples in
`/usr/share/tauri-greeter/themes`; an untouched greeter shows what its operator
installed.

```bash
sudo install -Dm644 themes/midnight.css /etc/tauri-greeter/theme.css
```

See [themes/README.md](themes/README.md) for the custom properties and class
names available.

## Limitations

- **Not a lock screen.** A greeter is what a display manager shows; locking a
  running session is the session's own job (`kscreenlocker`, `swaylock`, …).
  Being the locker on Wayland means implementing `ext-session-lock-v1`, which a
  webview cannot do.
- **LightDM only.** The whole backend is `liblightdm-gobject-1`. There is no
  greeter protocol for gdm or sddm that this could also speak.
- **X11 greeter.** LightDM can host a greeter on a Wayland compositor, but a
  Tauri webview here is started under `GDK_BACKEND` as LightDM leaves it and is
  only tested on LightDM's Xorg seat. The desktop started afterwards is
  unaffected — X11 or Wayland, either works.
- **No remembered state.** LightDM's `select-user-hint` and the per-user last
  session are not read, so the greeter always defaults to the first of each.
- **Single seat.** No multi-seat or remote (XDMCP) support, and no guest
  account or autologin.
- **Single-prompt PAM.** One password is replayed into the first secret prompt,
  so 2FA, fingerprint and expired-password changes cannot complete — anything
  else is answered with an empty string, which fails the attempt rather than
  hanging.
- **`script-src \'unsafe-inline\'`.** Trunk emits the WASM loader inline and
  Tauri nonces only `script[src^=\'http\']`, so a strict `script-src` blanks the
  greeter. The rest of the CSP is same-origin; see [SECURITY.md](SECURITY.md).

## Docs

- [CHANGELOG.md](CHANGELOG.md) — what changed, and migrating from cssdm
- [INSTALL.md](INSTALL.md) — install, deploy, roll back
- [LOCAL_TESTING.md](LOCAL_TESTING.md) — test without touching your real seat
- [SECURITY.md](SECURITY.md) — security model
- [themes/README.md](themes/README.md) — theming reference
- [PLAN.md](PLAN.md) — original phase plan, from when this was a display manager (historical)
- [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) — dependency notices

## Author

Brian Teague — <https://github.com/B-Teague>

## License

Copyright (C) 2026 Brian Teague.

Tauri Greeter is free software under the **GNU General Public License v3.0 or later** —
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
`/usr/share/licenses/tauri-greeter/`.

The application icon in `src-tauri/icons/` is Tauri Greeter's own, drawn from
`icon.svg` in that directory and covered by the same license. Nothing here
carries the Tauri or Leptos logos.
