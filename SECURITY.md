# Tauri Greeter Security Considerations

The security model of a LightDM greeter is mostly LightDM's. This program is an
unprivileged process that draws a login form and speaks the greeter protocol; it
holds no privileges, opens no PAM handle, and starts no session. What follows is
what it does own, and what it deliberately leaves to LightDM.

## Trust Boundary

```
root                    lightdm daemon — PAM, privilege drop, session exec
  │  pipe pair (LIGHTDM_TO_SERVER_FD / LIGHTDM_FROM_SERVER_FD)
user `lightdm`          tauri-greeter — this program
  │  Tauri IPC
webview                 the Leptos UI
```

Everything privileged is above the pipe. The greeter cannot escalate by being
wrong: the worst a compromised greeter can do is send a username and a password
guess down the pipe, which is what a person at the keyboard can do anyway. It
cannot choose what runs as the user — it names a session key and LightDM
resolves it against the `.desktop` files LightDM itself scanned.

This is the main thing that changed when the project stopped being a display
manager. The greeter used to run as **root** and hold the PAM handle; now it
runs as `lightdm` and holds nothing.

## Authentication

### The Conversation
- LightDM runs the PAM stack (`/etc/pam.d/lightdm` on most distros) and sends
  prompts down the pipe. The greeter answers them
- The password is replayed into the first prompt marked
  `LIGHTDM_PROMPT_TYPE_SECRET`. Any other prompt — a hardware token, a second
  factor, a "new password" — is answered with an empty string, which fails the
  attempt rather than leaving the login screen waiting forever
- `lightdm_greeter_start_session_sync` is only reached after LightDM reports
  `is_authenticated`. The greeter never decides that for itself

### Password Handling
- The password reaches the backend as a command argument, is moved into
  `Conversation.password`, and is `take`n — not copied — by the prompt handler
  that spends it
- `authentication-complete` clears it unconditionally, so a rejected attempt
  leaves nothing behind. So does the timeout path
- It is never formatted, logged or serialised. `Conversation` has no `Debug`
- It is never written to disk and never leaves the process except down the pipe
  to LightDM

### What a Failed Login Is Told
The greeter shows exactly one message for every rejection:

    Incorrect password.

"No such account", "wrong password", "account expired" and "not a login
account" are indistinguishable on screen, because telling them apart tells
someone at the keyboard which half of a guess was right. PAM's own messages —
which can be far more specific — go to stderr, which LightDM puts in the
journal, and never to the screen.

Two other messages exist and mean something different:
- `Login is unavailable.` — the greeter could not reach LightDM at all, or
  LightDM did not answer within 120s. Not a verdict on the credentials
- `That session could not be started.` — authentication succeeded and the
  handoff failed

### Who May Log In
- The user list comes from LightDM, so `/etc/lightdm/users.conf`
  (`minimum-uid`, `hidden-users`, `hidden-shells`) or AccountsService decides
  who is shown — not this program
- Root is excluded by that same configuration, on every default
- The list is only a list. LightDM authenticates whatever username it is sent
  and applies its own policy, so a tampered greeter gains nothing by offering
  an account that is not on it

### Logging
Everything the greeter writes to stderr is captured by LightDM into
`/var/log/lightdm/` and the journal:
- `pam: <text>` for each PAM message
- the real reason for each unavailable-login or failed-session case
- never a password, and never a username echoed back unfiltered

## Power Commands

- `lightdm_suspend`, `lightdm_restart` and `lightdm_shutdown` go to
  `systemd-logind` over the system bus, which applies its own polkit rules to
  the greeter's session
- No `systemctl`, no shell, no `sudo`: nothing is spawned, so there is no
  command to inject into. The action is one of three compile-time constants
  chosen by a three-variant enum, and an unknown name fails deserialisation
- Each is preceded by the matching `lightdm_get_can_*` query, over the same bus
  and as the same polkit subject, so a refusal is reported rather than
  attempted
- Rate limiting is logind's, not the greeter's

## File Access

The greeter reads three things, all as the unprivileged `lightdm` user:

| Path | Why |
| --- | --- |
| `/etc/tauri-greeter/theme.css` | the operator's stylesheet |
| `/usr/share/tauri-greeter/themes/*.css` | the example stylesheets the picker offers |
| — | everything else comes from LightDM over the pipe |

`/etc/passwd`, `/etc/shadow`, the session `.desktop` directories and the X
authority file are no longer touched: LightDM reads them.

### Theme Path Traversal
`theme_css(name)` accepts a name only if `themes()` — the directory listing —
already contains it. A crafted name cannot walk out of the theme directory,
and even if it could, the process has no privileges worth reaching for.

## Content Security Policy

The webview runs under a CSP (`tauri.conf.json`). Its job is the theme: a
stylesheet at `/etc/tauri-greeter/theme.css` can otherwise reach the network
through `url()`, `@import` or a remote font, and a login screen that phones home
before anyone has logged in is a leak. `default-src`/`img-src`/`font-src`/
`connect-src` are same-origin, so a theme cannot fetch anything remote.

`script-src` and `style-src` keep `'unsafe-inline'`, and this is not an
oversight — it was checked against the Tauri in use:
- `tauri-utils/src/html2.rs:72` injects a nonce only into `script[src^='http']`.
  Trunk emits the WASM loader as an **inline** `<script type="module">`, which
  gets no nonce. A strict `script-src` gives a blank login screen at boot.
- The theme `<style>` is created by Leptos at runtime, after Tauri has finished
  processing the HTML, so it gets no nonce either.

Tightening these means giving trunk's loader a hash or nonce at build time.
Until then the CSP is worth having for what it does block.

## Frontend

- No arbitrary code execution: the UI reaches the system through six commands
  and nothing else
- No filesystem or network access from JS
- No local storage. The greeter keeps no state between runs — which is also why
  it does not remember the last user
- Themes are CSS only. They can change every pixel of the login screen, which
  is the point; they cannot execute anything

## Memory Safety

`lightdm.rs` is FFI, so it is the one place in the tree that can be unsafe in
the Rust sense. What it does:
- calls C functions with `CString` arguments, and copies every returned
  `*const c_char` into an owned `String` before the library can invalidate it
- walks `GList`s LightDM owns and frees nothing on them, per the library's
  `(transfer none)` annotations
- frees every `GError` it is handed, in one place (`check`)
- casts three signal handlers to `GCallback`. Each must match the signature
  LightDM documents for its signal; the marshaller trusts the signal, not the
  cast

## Out of Scope

Delegated to LightDM or the system, not implemented here:
- 2FA, TOTP, hardware tokens, fingerprint — PAM's, and only reachable once the
  greeter answers multi-prompt conversations
- Expired-password changes
- Screen locking after a timeout — a greeter is not a lock screen
- Session auditing — the journal has it
- Encrypted home directories — `pam_mount` and friends, above the pipe

## Deployment

```bash
make install
printf '[Seat:*]\ngreeter-session=tauri-greeter\n' |
    sudo install -Dm644 /dev/stdin /etc/lightdm/lightdm.conf.d/50-tauri-greeter.conf
sudo install -Dm644 themes/midnight.css /etc/tauri-greeter/theme.css   # optional
```

Test it in a nested LightDM before selecting it on the real seat — `make
test-mode`, see [LOCAL_TESTING.md](LOCAL_TESTING.md).

### What to Check
```bash
make verify                                  # binary, greeter entry, what LightDM selects
ls -l /etc/tauri-greeter/theme.css           # must be root-owned, world-readable
journalctl -u lightdm -b | grep tauri-greeter
```

- The greeter entry and the binary should be root-owned and not writable by
  anyone else: a writable greeter is a root-equivalent hole, because LightDM
  launches it
- `/etc/tauri-greeter/theme.css` likewise. It cannot execute anything, but a
  theme that hides the error message is a usable phishing surface

### Testing Checklist
- [ ] A correct password starts the selected session
- [ ] A wrong password shows `Incorrect password.` and nothing more specific
- [ ] A username LightDM hides is not listed, and fails if sent anyway
- [ ] Suspend, restart and shut down each work, or report unavailable
- [ ] The journal has a line for each failure, and no password anywhere in it

## Reporting Security Issues

Open a GitHub issue at <https://github.com/B-Teague/tauri-greeter>, or, for
anything exploitable, email <br.teague@gmail.com> rather than filing publicly.

## References

- [LightDM](https://github.com/canonical/lightdm) — the daemon and the greeter protocol
- [liblightdm-gobject](https://people.ubuntu.com/~robert-ancell/lightdm/reference/) — the library this links against
- [Linux-PAM](https://github.com/linux-pam/linux-pam) — what LightDM authenticates with
- [Tauri security](https://tauri.app/security/)
