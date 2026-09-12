# Changelog

## 0.1.0

First release: a CSS-themeable LightDM greeter.

### Added

- **LightDM greeter.** LightDM owns the VT, the X server, PAM, privilege
  dropping and starting the session; this draws the login screen it shows and
  talks to it over the pipe pair LightDM hands its greeter on startup, via
  `liblightdm-gobject-1`.
- **The whole PAM conversation.** `authenticate` starts one and returns; every
  prompt LightDM's stack sends arrives as a `prompt` event, is shown with the
  right kind of field, and is answered with `respond` — for as many rounds as
  the stack wants. 2FA, hardware tokens, fingerprint readers and expired-password
  changes all complete. PAM's `show-message` text is shown as well as journalled,
  since the prompts that follow one make no sense without it. Nothing blocks
  waiting for a verdict, so a slow or silent PAM stack cannot wedge the screen;
  a 120s deadline cancels and hands the keyboard back if LightDM stops replying,
  and Escape abandons a conversation at any point.
- **The seat's hints are honoured.** `select-user-hint` and `select-guest-hint`
  preselect, `hide-users-hint` and `show-manual-login-hint` decide whether
  accounts are listed or the username is typed, `has-guest-account-hint` offers
  the guest session, `lock-hint` puts `.locked` on the screen for themes, and
  `default-session-hint` is the session fallback.
- **Autologin.** LightDM's countdown is mirrored on screen with the account it
  will use; any key cancels it, and when it expires the configured
  autologin session is what starts.
- **Per-account memory, read from LightDM.** The selected account's last
  session, keyboard layout and language come back with it. The greeter still
  writes no state of its own.
- **Keyboard layout and language pickers.** Changing the layout changes the
  greeter's own keyboard immediately, so a password containing a character the
  default layout cannot produce can be typed; LightDM carries both into the
  session it starts.
- **Account pictures and logged-in state.** `lightdm_user_get_image` is inlined
  as a `data:` URI, typed from the file's first bytes rather than its name and
  capped at 4 MB; accounts with a session already running say so.
- **Hibernate**, and only the power actions logind says it would perform are
  drawn at all.
- The hostname in the status bar, with the OS name as its tooltip.
- **Runs unprivileged**, as the `lightdm` user. It holds no PAM handle and can
  start nothing itself; the worst a compromised greeter can do is send a
  password guess down the pipe.
- **One-file theming.** `/etc/tauri-greeter/theme.css` is applied over the
  built-in stylesheet at startup, and a picker switches between the examples in
  `/usr/share/tauri-greeter/themes`.
- **Users and sessions come from LightDM**, so `/etc/lightdm/users.conf`
  (`minimum-uid`, `hidden-users`, `hidden-shells`) or AccountsService decides
  who is listed, and LightDM's `sessions-directory` decides what is offered.
- **Power actions through LightDM** to `systemd-logind` over D-Bus — nothing is
  spawned. Each is preceded by the matching `lightdm_get_can_*` query, so a
  refusal is reported rather than attempted.
- `/usr/share/xgreeters/tauri-greeter.desktop`, so LightDM can find the greeter,
  and an Arch package that installs it.
- `make test-mode` and `packaging/test-mode.sh` — a whole LightDM in a nested X
  server, unprivileged, temp directories only, the real seat untouched.
- Unit tests that exercise the live `liblightdm-gobject-1`: the session and user
  `GList` walks, the `GError` free path, and the logind capability query the
  power buttons depend on.

### Known gaps

No remote (XDMCP) authentication, not resettable (LightDM restarts the greeter
when the seat's hints change), X11 greeter. See
[Limitations](README.md#limitations).
