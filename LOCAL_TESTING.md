# Local Testing Guide for Tauri Greeter

Three ways to exercise the greeter, in increasing order of how much of the real
thing they run. None of the first three touch your login screen.

| | What it covers | Needs |
| --- | --- | --- |
| `make test` | the backend, including live FFI against liblightdm | nothing |
| `TAURI_GREETER_WINDOWED=1` | the UI, user/session lists, theming | a desktop |
| `make test-mode` | **all of it, including login** | `xorg-server-xephyr` |
| selecting it | the real seat | nerve |

## 1. Unit Tests

```bash
make test
```

Seven tests, and four of them talk to the real `liblightdm-gobject-1` rather
than a mock:

- `reads_sessions_from_lightdm` / `reads_users_from_lightdm` — walk the actual
  `GList`s this machine returns, checking that keys and names survive the FFI
  and that root is not among them
- `asks_logind_what_it_is_allowed_to_do` — the one part of the power path that
  can be exercised without powering the machine off. Same library, same system
  bus, same polkit subject; only the final verb differs. If this cannot reach
  logind, neither can the buttons
- `check_reports_and_frees_the_error` — the `GError` path, which otherwise only
  runs when something has already gone wrong

```bash
cargo test -p tauri-greeter --lib -- --nocapture   # see what they actually found
```

## 2. The Greeter Alone, in a Window

```bash
make build
TAURI_GREETER_WINDOWED=1 ./target/debug/tauri-greeter
```

An ordinary 1280x800 window on your current desktop. Without it the greeter
covers the screen and takes the keyboard, which is correct on LightDM's bare X
server and unpleasant here.

There is no LightDM to talk to, so it prints:

    tauri-greeter: not connected to LightDM: Unable to determine socket to daemon

and login answers `Login is unavailable.` Everything else is live.

**Check:**
- [ ] The clock shows the right time and date, in your locale's format
- [ ] The user dropdown lists the accounts `/etc/lightdm/users.conf` allows —
      and not root, and not service accounts
- [ ] The session dropdown lists your installed desktops, Wayland first, each
      labelled `(Wayland)` or `(X11)`
- [ ] The theme picker switches between the entries in
      `/usr/share/tauri-greeter/themes` and back to "Default"
- [ ] The password field has focus on start; Enter submits
- [ ] A login attempt says `Login is unavailable.` and clears the field

**Do not click the power buttons here.** They are not stubbed: they go to
logind over D-Bus and will suspend or shut the machine down.

Compare the dropdowns against what LightDM itself thinks:

```bash
cat /etc/lightdm/users.conf
lightdm --show-config | grep sessions-directory
ls /usr/share/xsessions/ /usr/share/wayland-sessions/
```

## 3. A Whole LightDM, Nested

```bash
sudo pacman -S xorg-server-xephyr     # or your distro's Xephyr package
make test-mode
```

This is the one that tests logging in. `packaging/test-mode.sh` starts a real
LightDM in `--test-mode`: unprivileged, as you, with its X server nested in a
Xephyr window. It writes a config, a run directory, a log directory and a cache
directory into one `mktemp -d` and deletes all of it on exit — the system's
`/etc/lightdm` and `/run/lightdm` are never opened.

`make test-mode` runs `make install` first, because LightDM finds greeters
through `/usr/share/xgreeters/`.

**Check:**
- [ ] The greeter appears in the Xephyr window
- [ ] A **wrong** password shows `Incorrect password.` — and nothing more
      specific, whether the account exists or not
- [ ] The field clears and refocuses, and a second attempt works
- [ ] A **correct** password is accepted and LightDM proceeds to the handoff
- [ ] The journal has a line for the failure and no password anywhere in it

Test mode has no privileges, so what happens *after* authentication is not
representative: LightDM cannot setuid, so the session it tries to start may
fail. Authentication succeeding and LightDM accepting `start_session` is the
part this proves. The desktop actually coming up is step 4's business.

Set the window size if the default is awkward:

```bash
SCREEN=1920x1080 make test-mode
```

### Reading the Logs

The script prints the temporary directory it is using. While it runs:

```bash
tail -f /tmp/tauri-greeter-test.*/log/lightdm.log     # LightDM and the greeter's stderr
tail -f /tmp/tauri-greeter-test.*/log/x-0.log         # Xephyr
```

Every `eprintln!` in the greeter — the PAM messages, the real reason behind each
`Login is unavailable.` — comes out in the first one.

## 4. The Real Seat

Only after 3 passes. See [INSTALL.md](INSTALL.md#4-select-it); the rollback is
one `rm` and a restart.

Keep a way back in before you restart LightDM: a root shell on another VT
(Ctrl+Alt+F3), or sshd running.

**Check, on the real thing:**
- [ ] A correct password starts the selected session, X11 and Wayland both
- [ ] The session honours `XDG_CURRENT_DESKTOP` — portals, autostart and audio
      all work, which is what tells you LightDM built the environment properly
- [ ] Logging out returns to the greeter
- [ ] Suspend resumes to the greeter
- [ ] Restart and shut down do what they say
- [ ] `/etc/tauri-greeter/theme.css` is applied

## Troubleshooting

### Xephyr Window Opens, No Greeter

LightDM started its X server and then failed to launch the greeter.

```bash
grep -i 'greeter\|xgreeters' /tmp/tauri-greeter-test.*/log/lightdm.log
```

Usually `Exec=` in `/usr/share/xgreeters/tauri-greeter.desktop` pointing at a
binary that is not there — `make install` writes `/usr/local/bin`, the Arch
package `/usr/bin`. Re-run `make install`.

### Greeter Appears, Login Says "Login is unavailable."

The greeter is drawing but never connected to the daemon. In test mode this
means LightDM did not hand it the pipe pair:

```bash
grep -i 'connect\|socket' /tmp/tauri-greeter-test.*/log/lightdm.log
```

Outside test mode, the same message means the binary was started by hand rather
than by LightDM, which is expected.

### Authentication Always Fails in Test Mode

Test mode still runs the real PAM stack:

```bash
cat /etc/pam.d/lightdm
faillock --user "$USER"
```

Repeated testing trips `pam_faillock` on real accounts. `faillock --user "$USER"
--reset` clears it.

### No Users or No Sessions

Both lists are LightDM's, not the greeter's — see
[INSTALL.md](INSTALL.md#no-users-or-too-many).

## Performance

A 0.1.0 release build on typical hardware:

- **Startup**: ~0.5 s
- **User and session lists**: ~50–100 ms each (one D-Bus round trip)
- **Theme switch**: ~10 ms
- **Authentication**: ~1 s, set by PAM's own delay
- **Memory, idle**: ~40–50 MB

## Reporting Issues

<https://github.com/B-Teague/tauri-greeter/issues>, with:

```bash
./target/release/tauri-greeter --version
lightdm --version
lightdm --show-config
```

and the relevant part of `/var/log/lightdm/lightdm.log`. Check it for a password
before pasting it — the greeter never logs one, but a PAM module might.

## Clean Up

`make test-mode` cleans up after itself. To undo `make install`:

```bash
make uninstall
```
