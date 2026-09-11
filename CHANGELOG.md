# Changelog

## 0.2.0

**Renamed from `cssdm`, and no longer a display manager.** LightDM is the
display manager; this is the greeter it draws. Upgrading is not transparent —
see [Migrating](#migrating-from-cssdm-01x).

### Changed

- **LightDM greeter, not a display manager.** The VT, the X server, PAM,
  privilege dropping and starting the session are all LightDM's now. The greeter
  talks to it over the pipe pair LightDM hands it on startup, via
  `liblightdm-gobject-1`.
- **Runs unprivileged.** As the `lightdm` user rather than as root. It holds no
  PAM handle and can start nothing itself; the worst a compromised greeter can
  do is send a password guess down the pipe.
- **Users and sessions come from LightDM**, so `/etc/lightdm/users.conf`
  (`minimum-uid`, `hidden-users`, `hidden-shells`) or AccountsService decides who
  is listed, and LightDM's `sessions-directory` decides what is offered.
- **Power actions go through LightDM** to `systemd-logind` over D-Bus, instead of
  spawning `systemctl`. Each is preceded by the matching `lightdm_get_can_*`
  query, so a refusal is reported rather than attempted.
- Every path renamed: `/etc/tauri-greeter/theme.css`,
  `/usr/share/tauri-greeter/themes/`, `TAURI_GREETER_WINDOWED`.

### Removed

- The daemon half, and with it the Xorg supervision, the Xauthority cookie
  generation, the `/run/cssdm.sock` login socket, the PAM conversation, the
  `setsid`/`initgroups`/`setgid`/`setuid` privilege drop, the session environment
  construction, the `.desktop` scanner, and the `--greeter` argument.
- `cssdm.service`. There is no unit any more — `lightdm.service` is the one that
  matters.
- Dependencies: `pam-client` and `libc` are gone, and `serde_json` is a
  dev-dependency. The two added (`glib-sys`, `gobject-sys`) already arrived
  under Tauri's gtk, so nothing new compiles.

### Added

- `/usr/share/xgreeters/tauri-greeter.desktop`, so LightDM can find the greeter.
- `make test-mode` and `packaging/test-mode.sh` — a whole LightDM in a nested X
  server, unprivileged, temp directories only, the real seat untouched.
- Unit tests that exercise the live `liblightdm-gobject-1`: the session and user
  `GList` walks, the `GError` free path, and the logind capability query that
  the power buttons depend on.

### Migrating from cssdm 0.1.x

Removing `cssdm` disables `cssdm.service`, which leaves the machine with **no
display manager enabled**. Enable LightDM in its place before rebooting:

```bash
printf '[Seat:*]\ngreeter-session=tauri-greeter\n' |
    sudo install -Dm644 /dev/stdin /etc/lightdm/lightdm.conf.d/50-tauri-greeter.conf
sudo systemctl enable lightdm
systemctl is-enabled display-manager.service    # check before you reboot
```

Then move any theme across:

```bash
sudo install -Dm644 /etc/cssdm/theme.css /etc/tauri-greeter/theme.css
sudo rm -rf /etc/cssdm
```

Themes themselves need no changes — the custom properties and class names are
unchanged.

### Known gaps

The full login conversation against a running LightDM has not been exercised
end to end; `make test-mode` is the harness for it. See
[LOCAL_TESTING.md](LOCAL_TESTING.md).

## 0.1.0

First release, as `cssdm`: a self-contained Linux display manager with a
Tauri/Leptos greeter, PAM authentication, session discovery and power actions.
See [PLAN.md](PLAN.md) for the phase plan it was built to.
