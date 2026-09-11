# CSSDM Security Considerations

This document outlines the security model and best practices for CSSDM.

## Authentication

### PAM Integration
- **Method**: `libpam` directly (`pam-client`), against the `login` policy
- **Why**: Routes through the system PAM stack, respecting all configured authentication methods (LDAP, Kerberos, 2FA, faillock)
- **Security**: PAM handles all password validation — CSSDM never reads shadow files directly
- **Checks**: `pam_authenticate` *and* `pam_acct_mgmt`, so locked and expired accounts are rejected
- **Never use `su` for this**: a greeter runs as root, and root may `su` to any
  account without a password, so an `su`-based check accepts every password.

### Password Handling
- ✅ Passwords never logged to console or files
- ✅ Password cleared from memory on auth failure
- ✅ No password storage or caching
- ✅ Never visible in process arguments (`ps` output)
- ✅ `daemon::Request` deliberately derives neither `Debug` nor `Clone`, so a
  `{:?}` added later cannot put a password in the journal
- ✅ A malformed request is logged as `malformed request` and nothing else —
  the serde error would quote the line, and the line is a password

### What a Failed Login Is Told
The greeter shows one message for every rejection: **"Incorrect password."**
Which it actually was — no such account, wrong password, expired account,
not a login account, unknown session — goes to the journal only.

Someone at the keyboard must not be able to tell a bad username from a bad
password. Transport failures are generic too (`"Login is unavailable."`):
socket paths and errno strings describe the machine, not the login.

User *enumeration* is deliberately not defended against here — the greeter
lists every login account on screen by design, as gdm and sddm do.

### Who May Log In
`is_login_account()` in `auth.rs` enforces the same rule the greeter's user
list displays: uid in 1000..65534, and a shell that is not `nologin`/`false`.

It is checked **in the daemon, before PAM is consulted**, for two reasons:
- The greeter chooses what to *offer*; the daemon decides what is *allowed*.
  A greeter that has been tampered with still cannot ask for root.
- A rejected root attempt never reaches `pam_faillock`, so the greeter cannot
  be used to lock root out of the console.

### Logging
Everything goes to stderr, which systemd captures; `journalctl -u cssdm`.
- One line per rejected login, with the reason and the account
- One line per session opened, with account, uid and session name
- One line per session exit, with the status
- Usernames are filtered to `[A-Za-z0-9._-@]` and truncated to 32 characters
  before being logged: they arrive from the greeter, and control characters in
  a log are a forgery tool

## Privilege Escalation

### Display Manager Privilege
- CSSDM runs as `root` (standard for display managers)
- **Purpose**: Required to:
  - Launch sessions for any user
  - Execute power commands (shutdown, reboot)
  - Read system configuration files
- **Risk Mitigation**:
  - Code is minimal and focused
  - Backend written in Rust (memory-safe)
  - No dynamic code execution
  - All system calls carefully controlled

### Session Launch
- The daemon opens a PAM session (`pam_open_session` + `pam_setcred`), which is
  what registers the logind session and creates `XDG_RUNTIME_DIR`
- The desktop is exec'd through `$SHELL -l -c` after `setsid`, `initgroups`,
  `setgid` and `setuid`, in that order, so nothing user-facing keeps root
- Not `Command::uid()`/`gid()`: std clears supplementary groups rather than
  filling them in, which would silently cost the user `video`, `input`, `wheel`
- The PAM handle is held by the daemon for the life of the session and closed
  when the desktop exits; the greeter is gone by then
- The greeter sends a session *name*; the daemon resolves the `Exec=` line from
  its own scan, so the command that runs as the user is never the greeter's choice
- `/run/cssdm.sock` is mode 0600 and root-owned
- The greeter's X server runs `-nolisten tcp` with an `-auth` cookie from
  `/dev/urandom`, kept in root-only `/run/cssdm/Xauthority` and deleted when the
  server stops. Without the cookie any local user — another VT, an ssh session —
  could connect to the login screen's server and read the password being typed
- **Still outstanding**: the greeter itself runs as root. gdm and sddm run
  theirs as a dedicated unprivileged account

## Power Commands

### Systemctl Integration
- ✅ Uses `systemctl` (authenticated via system PAM)
- ✅ Respects polkit policies (if configured)
- ✅ No direct syscalls or kernel module loading
- ✅ Graceful error handling if permissions denied

### Attack Prevention
- Power actions are reachable before login, as on any display manager, and are
  delegated to systemd-logind — restrict them with polkit if a machine needs it
- Each action goes through systemd's auth stack

## File Access

### Paths Accessed
- `/etc/passwd` — read-only, public information
- `/usr/share/xsessions/` — system desktop sessions
- `/usr/share/wayland-sessions/` — system desktop sessions
- `/etc/cssdm/theme.css` — read-only, operator-installed stylesheet

### Permissions
- All reads are from public system directories
- The greeter writes nothing
- No recursive directory traversal
- No arbitrary file access

## Session Isolation

### Per-User Sessions
- Each logged-in user runs their own session
- Sessions are isolated by UID
- One login session per user at a time (standard)

### Display Variables
- `DISPLAY` and `WAYLAND_DISPLAY` set per-session
- No crosstalk between sessions

## The Login Socket

`/run/cssdm.sock`, root-owned, mode 0600.

- The `umask` is narrowed **around the `bind()`**, not fixed up with `chmod`
  afterwards: `bind` applies the umask, so a later `chmod` leaves the socket
  connectable by anyone for the instant in between
- Requests are capped at 4 KiB — `read_line` on an unbounded socket is how a
  process gets OOM-killed
- Read and write timeouts (10s) so a client that connects and says nothing
  cannot wedge the login screen for everyone behind it
- Every code path answers. A greeter left waiting for a reply that never comes
  is a login screen that has stopped accepting logins
- The greeter's wait for a verdict is 120s, deliberately longer than the
  daemon's: PAM delays failures on purpose, and a short timeout would make the
  greeter report "unavailable" for a login that actually succeeded

## Content Security Policy

The webview runs under a CSP (`tauri.conf.json`). Its job is the theme: a
stylesheet at `/etc/cssdm/theme.css` can otherwise reach the network through
`url()`, `@import` or a remote font, and a login screen that phones home before
anyone has logged in is a leak. `default-src`/`img-src`/`font-src`/`connect-src`
are same-origin, so a theme cannot fetch anything remote.

`script-src` and `style-src` keep `'unsafe-inline'`, and this is not an
oversight — it was checked against the Tauri in use:
- `tauri-utils/src/html2.rs:72` injects a nonce only into `script[src^='http']`.
  Trunk emits the WASM loader as an **inline** `<script type="module">`, which
  gets no nonce. A strict `script-src` gives a blank login screen at boot.
- The theme `<style>` is created by Leptos at runtime, after Tauri has finished
  processing the HTML, so it gets no nonce either.

Tightening these means giving trunk's loader a hash or nonce at build time.
Until then the CSP is worth having for what it does block.

## Frontend Security

### Tauri Sandbox
- ✅ No arbitrary code execution
- ✅ No file system access from JS
- ✅ All system calls through Rust backend
- ✅ No network access (no remote themes)
- ✅ CSP (Content Security Policy) configured

### Local Storage
- Unused. The greeter keeps no state between runs

## Configuration Security

### Theme Loading
- ✅ Themes are CSS only (no JavaScript execution)
- ✅ Single root-owned path (`/etc/cssdm/theme.css`); no per-user themes, which
  a login screen could not trust anyway
- ✅ No network fetching of themes
- ⚠️ Themes can override appearance (not attack surface)

### Desktop Files
- Session `.desktop` files parsed (read-only)
- Only `Name=` and `Exec=` fields used
- No arbitrary command injection

## Known Limitations & Future Work

### Not Implemented (Out of Scope)
- 2FA/TOTP (delegated to PAM)
- Screen lock after timeout
- Session logging/auditing (systemd-journald handles)
- Encrypted home directories (system-level)

### Potential Enhancements
- A dedicated `/etc/pam.d/cssdm` policy instead of reusing `login`
- Failed login logging to syslog
- Hardware token support (via PAM)
- Multi-seat support (future)

## Deployment Recommendations

### System Setup
```bash
# Install CSSDM binary
sudo install -m 755 cssdm /usr/local/bin/

# Optional: install a theme
sudo install -Dm644 themes/midnight.css /etc/cssdm/theme.css

# Configure as default display manager
sudo systemctl disable sddm    # or whichever is enabled
sudo systemctl enable cssdm    # Alias=display-manager.service
```

### Display Manager Integration
- Run via systemd service (not from getty); the unit conflicts with
  `getty@tty1.service` and owns tty1
- Leave `KillMode` at its default: the daemon is the session's parent and holds
  its PAM handle, so `systemctl stop cssdm` must take the session down with it
- `StartLimitBurst=2` so a crashing greeter leaves a usable console instead of
  a login screen that flickers forever

### PAM Configuration
- Uses standard "login" PAM config
- Respects system policies (account locking, etc.)
- Works with standard auth backends

### Permissions
```bash
# Typical PAM capabilities for display managers
# These are usually handled by systemd-user-sessions or similar
- Account: Check if account is valid/not expired
- Auth: Validate password via system auth stack
- Session: Set up session environment variables
```

## Audit & Verification

### What to Check
- No plaintext passwords in logs: `grep -r password /var/log/`
- Session launches correctly for different users
- Power commands require successful login
- Theme files don't execute code
- No world-writable configuration

### Testing Checklist
- [ ] Login with valid credentials
- [ ] Reject with invalid credentials, and with a locked account
- [ ] Password cleared after failed login
- [ ] Failures counted by `pam_faillock` (`faillock --user <name>`)
- [ ] No password in the journal (`journalctl -u cssdm`)

## Reporting Security Issues

If you discover a security vulnerability:

1. Do **not** open a public issue
2. Contact the maintainer privately
3. Provide:
   - Description of the issue
   - Steps to reproduce
   - Potential impact
   - Suggested fix (optional)

## References

- [PAM Documentation](http://www.linux-pam.org/)
- [systemd.service Manual](https://man7.org/linux/man-pages/man5/systemd.service.5.html)
- [Tauri Security](https://tauri.app/develop/security/)
- [Linux Display Manager Guidelines](https://wiki.freedesktop.org/wiki/Software/GDM/Security/)
