# Tauri Greeter — a CSS-themeable LightDM greeter

A LightDM greeter built with Tauri: a Leptos/WASM login screen you restyle with
one CSS file. LightDM does the display management — the VT, the X server, PAM,
privilege dropping, starting the desktop — and this draws the login screen it
shows.

A single binary less than 5 MB holds the greeter and its UI — no interpreter, no bundled
runtime, no `node_modules`. Rust compiles the backend into that binary and the
frontend into WebAssembly next to it; Tauri draws it with the webview the system
already ships (`webkit2gtk`), so nothing here carries a browser of its own.
Figures are a 0.1.0 release build for x86_64, from `make package`.

**Status**: feature-complete against the LightDM greeter protocol, bar the
exceptions in [Limitations](#limitations).

## Features

- **One-file theming** — drop a stylesheet at `/etc/tauri-greeter/theme.css`
- **Full PAM conversation** — every prompt LightDM's PAM stack sends is shown and
  answered in turn, so 2FA, a hardware token, a fingerprint reader and an
  expired password's "new password"/"retype it" pair all complete. PAM's own
  messages are shown alongside them
- **User and session lists** — from LightDM, so `/etc/lightdm/users.conf` and
  AccountsService are honoured, with the account's picture and its
  already-logged-in state
- **Remembers what LightDM remembers** — `select-user-hint` preselects the last
  account, and that account's last session, keyboard layout and language come
  back with it; `default-session-hint` is the fallback
- **Guest sessions and manual login** — offered when the seat's hints allow them;
  a `hide-users` seat asks for the username instead of listing accounts
- **Autologin** — LightDM's countdown, mirrored on screen, cancelled by any key
- **Keyboard layout and language pickers** — switching the layout switches the
  greeter's own keyboard, so a password with a non-US character can be typed
- **Power actions** — sleep, hibernate, restart and shut down through LightDM,
  and only the ones logind says it will actually perform are drawn
- **Keyboard-first** — the prompt is focused on start, Enter submits, Escape
  abandons a stuck conversation and starts a fresh one

## Architecture

```
index.html                   trunk shell, links the default theme
src/                         greeter UI (Leptos, CSR/WASM)
  main.rs                    mounts App
  app.rs                     App, Clock, PowerButton, Avatar
src-tauri/src/               backend
  main.rs                    --version, then the greeter
  lib.rs                     the Tauri commands, and image inlining
  lightdm.rs                 liblightdm-gobject bindings, hints, and the
                             PAM conversation forwarded as events
themes/                      nebula.css (shown at startup), dusk.css, midnight.css
packaging/                   LightDM greeter entry, Arch package, test harness
```

The UI reads the seat with `hints`, `users`, `sessions`, `layouts`, `layout`,
`languages`, `language`, `power_actions`, `themes` and `theme_css`, and acts
with `set_layout`, `power`, `authenticate`, `respond`, `cancel`,
`authenticate_autologin`, `cancel_autologin` and `start_session`.

A login is a conversation, not a form submission. `authenticate` only starts
one; what PAM asks for arrives back as `prompt`, `message` and `complete`
events and is answered with `respond`, as many times as the stack wants. That
is what makes a second factor or an expired-password change possible, and it is
why nothing in the greeter blocks waiting for a verdict.

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
it plus the forwarding that turns its signals into Tauri events. Its replies
arrive on a GLib watch attached to the default main context — the same one
Tauri's GTK loop already turns — so there is no second thread, no loop of our
own, and nothing that blocks the UI while PAM thinks.

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
[INSTALL.md](INSTALL.md#3-test-it-without-touching-your-seat).

## Theming

The greeter starts on `nebula`, and the picker in the footer switches between
the other stylesheets in `/usr/share/tauri-greeter/themes`. Over whichever one
is showing it applies `/etc/tauri-greeter/theme.css`, the operator's own file --
so that is what an untouched greeter wears. A picture beside either one
(`theme.jpg`, `nebula.png`) becomes the background.

```bash
sudo install -Dm644 themes/midnight.css /etc/tauri-greeter/theme.css
```

See [themes/README.md](themes/README.md) for the custom properties and class
names available.

## Limitations

- **LightDM only.** The whole backend is `liblightdm-gobject-1`. There is no
  greeter protocol for gdm or sddm that this could also speak.
- **X11 greeter.** LightDM can host a greeter on a Wayland compositor, but a
  Tauri webview here is started under `GDK_BACKEND` as LightDM leaves it and is
  only tested on LightDM's Xorg seat. The desktop started afterwards is
  unaffected — X11 or Wayland, either works.
- **No remote login.** `lightdm_greeter_authenticate_remote` and
  `lightdm_get_remote_sessions` are not bound, so an XDMCP or remote-login seat
  offers local accounts only. Multi-seat itself is LightDM's business and works:
  this is one greeter per seat, as LightDM starts it.
- **Not resettable.** `lightdm_greeter_set_resettable` is left off, so when the
  seat's hints change LightDM restarts the greeter instead of sending it a
  `reset` — correct, but a redraw where an update would do.
- **The theme picker is not remembered.** It is a preview; the next greeter is
  back to `nebula` under `/etc/tauri-greeter/theme.css`. The greeter writes no
  state of its own, and LightDM has nowhere to keep a greeter's preferences.
- **`script-src \'unsafe-inline\'`.** Trunk emits the WASM loader inline and
  Tauri nonces only `script[src^=\'http\']`, so a strict `script-src` blanks the
  greeter. The rest of the CSP is same-origin; see [SECURITY.md](SECURITY.md).

## Docs

- [CHANGELOG.md](CHANGELOG.md) — release notes
- [INSTALL.md](INSTALL.md) — build, install, test, deploy, roll back
- [SECURITY.md](SECURITY.md) — security model
- [themes/README.md](themes/README.md) — theming reference
- [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) — dependency notices

## Credits

Built on other people's work:

- **[Tauri](https://tauri.app)** ([repo](https://github.com/tauri-apps/tauri), MIT/Apache-2.0)
  — the app framework. It hosts the UI in the system's own webview and provides
  the command/event bridge between the Rust backend and the frontend, which is
  what keeps this a single small binary with no bundled runtime.
- **[Leptos](https://leptos.dev)** ([repo](https://github.com/leptos-rs/leptos),
  MIT) — the reactive Rust framework the login screen is written in, compiled to
  WebAssembly.
- **[Trunk](https://trunkrs.dev)** ([repo](https://github.com/trunk-rs/trunk),
  MIT/Apache-2.0) — builds the WASM frontend and its asset bundle.
- **[LightDM](https://github.com/canonical/lightdm)** (GPLv3/LGPLv3) — the
  display manager this is a greeter for; `liblightdm-gobject-1` does the
  privileged work and the PAM conversation.
- **[webkit2gtk](https://webkitgtk.org)** (LGPLv2.1/BSD) — the system webview
  Tauri renders into.

Tauri Greeter is not affiliated with or endorsed by the Tauri, Leptos or LightDM
projects. See [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) for the full
dependency list.

## Author

Brian Teague — <https://github.com/B-Teague>

## License

Copyright (C) 2026 Brian Teague.

Tauri Greeter is free software under the **GNU General Public License v3.0 or later** —
see [LICENSE](LICENSE). It comes with NO WARRANTY, to the extent permitted by
law.
