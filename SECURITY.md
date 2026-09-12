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

## Authentication

### The Conversation
- LightDM runs the PAM stack (`/etc/pam.d/lightdm` on most distros) and sends
  prompts down the pipe. The greeter shows each one and sends back what was
  typed, for as many rounds as the stack wants — a password, then a token code,
  then a new password and its confirmation. It holds no opinion about which
  prompt is which
- A prompt marked `LIGHTDM_PROMPT_TYPE_SECRET` is typed into a field that does
  not echo; a `QUESTION` is typed into one that does. That is the only thing the
  greeter decides
- Starting a second `lightdm_greeter_authenticate` abandons the first.
  `liblightdm` stamps each attempt with a sequence number and drops prompts and
  verdicts belonging to an earlier one, so switching account mid-conversation
  cannot have an old PAM stack answer for the new user
- `lightdm_greeter_start_session_sync` is only reached after LightDM reports
  `is_authenticated`. The greeter never decides that for itself, and LightDM
  checks again regardless

### Password Handling
- Nothing secret is stored. Each answer arrives as an argument to `respond`,
  is moved into a `CString`, handed to LightDM and dropped — there is no field
  holding it between the keystroke and the pipe, and nothing to clear afterwards
- It is never formatted, logged or serialised
- It is never written to disk and never leaves the process except down the pipe
  to LightDM
- A 120s deadline in the UI covers a LightDM that stops answering: it cancels
  the conversation and gives the keyboard back, rather than leaving the login
  screen wedged for the rest of the boot

### What a Failed Login Is Told
The greeter's own verdict is one message, for every rejection:

    Authentication failed.

"No such account", "wrong password", "account expired" and "not a login
account" are indistinguishable in that line, because telling them apart tells
someone at the keyboard which half of a guess was right.

Two other messages exist and mean something different:
- `Login is unavailable.` — the greeter could not reach LightDM at all, or
  LightDM did not answer within 120s. Not a verdict on the credentials
- `That session could not be started.` — authentication succeeded and the
  handoff failed

#### PAM's Own Messages Are Shown
`show-message` text is displayed as well as journalled. This is a deliberate
change from the 0.1.0 behaviour of hiding it, and it is a real trade-off:

- **For.** An expired-password change is a conversation — "You must change your
  password now", then "New password:", then "Retype new password:". Hiding the
  first line leaves someone typing their old password into a prompt they cannot
  interpret, and the change cannot complete. The same holds for a locked
  account, a failed token, or `pam_faillock` counting down. A greeter that
  swallows these is one people cannot use, and it silently diverges from every
  other LightDM greeter, all of which show them
- **Against.** A PAM stack may be configured to say more than an operator wants
  on a login screen — `pam_unix` distinguishes an unknown user from a bad
  password in some configurations
- **What limits it.** Only text PAM itself chose to emit is shown, styled as
  informational unless PAM marked it `LIGHTDM_MESSAGE_TYPE_ERROR`. The greeter
  adds nothing of its own and still refuses to characterise the rejection. An
  operator who wants less on screen configures the PAM stack to say less, which
  is where that decision belongs and the only place it can be made consistently

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
- `pam: <text>` for each PAM message, whether or not it was also shown
- `rejected: <user>` for each failed attempt, from the UI's console
- the real reason for each unavailable-login or failed-session case
- never a password, and never a username echoed back unfiltered

## Power Commands

- `lightdm_suspend`, `lightdm_hibernate`, `lightdm_restart` and
  `lightdm_shutdown` go to `systemd-logind` over the system bus, which applies
  its own polkit rules to the greeter's session
- No `systemctl`, no shell, no `sudo`: nothing is spawned, so there is no
  command to inject into. The action is one of four compile-time constants
  chosen by a four-variant enum, and an unknown name fails deserialisation
- Each is preceded by the matching `lightdm_get_can_*` query, over the same bus
  and as the same polkit subject. `power_actions` runs all four up front, so an
  action logind would refuse is never drawn, and `power` asks again before
  acting on the one that was
- Rate limiting is logind's, not the greeter's

## File Access

Everything the greeter opens, it opens as the unprivileged `lightdm` user:

| Path | Why |
| --- | --- |
| `/etc/tauri-greeter/theme.css` | the operator's stylesheet |
| `/usr/share/tauri-greeter/themes/*.css` | the example stylesheets the picker offers |
| the theme's picture beside either | inlined as a `data:` URI, 16 MB cap |
| `lightdm_user_get_image(user)` | each account's picture, inlined, 4 MB cap |
| — | everything else comes from LightDM over the pipe |

`/etc/passwd`, `/etc/shadow`, the session `.desktop` directories and the X
authority file are never opened: LightDM reads them.

### Theme Path Traversal
`theme_css(name)` accepts a name only if `themes()` — the directory listing —
already contains it. A crafted name cannot walk out of the theme directory,
and even if it could, the process has no privileges worth reaching for.

### Account Pictures
The only path the greeter opens that it did not choose is the one
`lightdm_user_get_image` returns, and LightDM took that from AccountsService or
from `~/.face`. It is read as the unprivileged `lightdm` user, so a 0700 home
simply yields nothing and the drawn fallback avatar is used. It is capped at
4 MB, and its type comes from the first bytes of the file rather than its name
— guessing from an extension would put whatever the file actually is behind an
`image/png` label. Anything that does not begin with a PNG, JPEG, GIF, WebP or
SVG signature is dropped.

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

- The capability in `src-tauri/capabilities/default.json` grants
  `core:event:allow-listen` and `core:event:allow-unlisten` and nothing else.
  Listening is how LightDM's prompts reach the UI; every other core plugin API
  — window, webview, path, filesystem, resources, and `emit` — stays denied, so
  the UI cannot forge an event of its own
- No arbitrary code execution: the UI reaches the system through this crate's
  own commands and nothing else
- No filesystem or network access from JS
- No local storage. The greeter keeps no state between runs. What looks like
  memory — the preselected account, its last session, keyboard and language —
  is LightDM's, read back from the greeter hints and the user list on every
  start, so there is nothing here for one user to leave behind for the next
- Themes are CSS only. They can change every pixel of the login screen, which
  is the point; they cannot execute anything

## Memory Safety

`lightdm.rs` is FFI, so it is the one place in the tree that can be unsafe in
the Rust sense. It uses the safe `glib` wrappers wherever they reach, which
leaves the raw pointer work to the library rather than to this program:
- `GList`s are converted by `glib` into a `Vec<glib::Object>`, which takes a
  reference on each element. There is no pointer walk here to get wrong, and
  the `(transfer none)` list itself is not freed
- `GError`s become a `glib::Error`, which owns and frees them on drop. There is
  no `g_error_free` call left to forget or to make twice
- the main context is pumped through `glib::MainContext`, with no unsafe at all
- what remains unsafe: calls into `liblightdm-gobject-1` itself, with `CString`
  arguments and every returned `*const c_char` copied into an owned `String`
  before the library can invalidate it
- and four signal handlers cast to `GCallback`. C's own API takes nothing
  else; each must match the signature LightDM documents for its signal, and the
  marshaller trusts the signal, not the cast. They are the only `extern "C"`
  functions in the tree

## Out of Scope

Delegated to LightDM or the system, not implemented here:
- 2FA, TOTP, hardware tokens, fingerprint and expired-password changes are
  PAM's. The greeter now carries the conversation they need, but what is asked
  and whether it is accepted is decided entirely above the pipe
- Remote (XDMCP) authentication: `lightdm_greeter_authenticate_remote` is not
  bound, so a remote-login seat offers local accounts only
- Rate limiting and lockout — `pam_faillock`'s, and its counters are the ones
  that matter; the greeter keeps none of its own
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
test-mode`, see [INSTALL.md](INSTALL.md#3-test-it-without-touching-your-seat).

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
- [ ] A wrong password shows `Authentication failed.` and nothing the greeter
      itself made more specific
- [ ] An account with an expired password can complete the change on screen
- [ ] A second factor, where the PAM stack asks for one, is prompted for and
      accepted
- [ ] Switching account mid-prompt starts a clean conversation, and an answer
      typed for the old one cannot reach the new one
- [ ] A username LightDM hides is not listed, and fails if sent anyway
- [ ] Manual login and the guest entry appear only when the hints allow them
- [ ] Changing the keyboard layout changes what the password field types
- [ ] Sleep, hibernate, restart and shut down each work, and an action logind
      refuses is not drawn at all
- [ ] The journal has a line for each failure, and no password anywhere in it

## Reporting Security Issues

Open a GitHub issue at <https://github.com/B-Teague/tauri-greeter>, or, for
anything exploitable, email <br.teague@gmail.com> rather than filing publicly.

## References

- [LightDM](https://github.com/canonical/lightdm) — the daemon and the greeter protocol
- [liblightdm-gobject](https://people.ubuntu.com/~robert-ancell/lightdm/reference/) — the library this links against
- [Linux-PAM](https://github.com/linux-pam/linux-pam) — what LightDM authenticates with
- [Tauri security](https://tauri.app/security/)
