# Local Testing Guide for CSSDM

Quick guide to test the production CSSDM binary on your Linux system.

## Release Binary

**Location**: `target/release/cssdm` (build it with `make release` — the UI
bundle must be built by trunk before cargo embeds it)
**Status**: Ready for testing

## Quick Test (Non-Destructive)

### 1. Run the Greeter Alone

The bare binary is the **daemon** — it takes over a VT and starts a desktop.
Don't run that from inside your session. Run the greeter half instead:

```bash
cd /path/to/CSSDM
CSSDM_WINDOWED=1 ./target/release/cssdm --greeter
```

This opens an ordinary 1280x800 window. Everything works except login, which
reports `connect /run/cssdm.sock` — the daemon is what owns that socket.
**Ctrl+C to exit**.

### 2. Test Functionality

- **Clock**: shows the current time and date in your locale
- **Users**: loaded from `/etc/passwd` (uid >= 1000 with a real shell)
- **Sessions**: bottom-left selector lists Wayland and X11 sessions
- **Theme**: `/etc/cssdm/theme.css` is applied if present
- **Keyboard**: password field is focused on start, Enter submits

### 3. Test Without Login

- Try a wrong password (should show a PAM error and clear the field)
- Note that repeated failures count against `pam_faillock`, same as any login
- Leave the power buttons alone unless you mean it — they act immediately

**Note**: Don't actually reboot! Exit with Ctrl+C instead.

## Safe Installation (Testing Only)

To test as the actual display manager:

### Step 1: Note Your Current Setup

```bash
systemctl status display-manager   # the DM you'll restore later
```

### Step 2: Install Binary (Temporary)

```bash
# Copy binary to a test location (NOT system-wide yet)
mkdir -p ~/.local/bin
cp target/release/cssdm ~/.local/bin/

# Make sure it's executable
chmod +x ~/.local/bin/cssdm

# Verify
~/.local/bin/cssdm --version 2>/dev/null || echo "App started successfully"
```

### Step 3: Test the Greeter on a Bare X Server

This is what the daemon does, minus the VT: an X server with the greeter as its
only client, no window manager.

```bash
# From a terminal in your current session
Xorg :9 -nolisten tcp -noreset &      # or Xephyr/Xvfb :9, if you have them
DISPLAY=:9 ~/.local/bin/cssdm --greeter
```

Login still fails here — no daemon, so no socket.

### Step 4: Test the Whole Thing on a Spare VT

This is the real test, and it does not touch your current display manager.
Switch to a free VT (Ctrl+Alt+F3), log in at the console, then:

```bash
sudo XDG_VTNR=3 XDG_SEAT=seat0 ~/.local/bin/cssdm
```

The greeter takes over tty3. Log in and your desktop starts there; your
existing session on tty1 is untouched, so Ctrl+Alt+F1 always gets you back.
Ctrl+C on the console kills the daemon.

### Step 5: Install as the System Display Manager

Only after step 4 works.

```bash
sudo systemctl disable sddm   # whatever you use now
sudo systemctl enable cssdm   # Alias=display-manager.service does the rest
sudo reboot
```

**To recover if something goes wrong** (boot loop): the unit gives up after two
crashes in 30s, so you land on a console rather than a flickering screen.

1. Ctrl+Alt+F2, log in
2. `sudo systemctl disable cssdm && sudo systemctl enable sddm`
3. `sudo reboot`

## Testing Checklist

- [ ] Greeter runs windowed without crashing
- [ ] Greeter covers the screen and takes keystrokes with no window manager
- [ ] Daemon on a spare VT shows the greeter
- [ ] Correct password starts the selected desktop
- [ ] `loginctl session-status` shows the session on the right seat and VT
- [ ] `echo $XDG_RUNTIME_DIR $XDG_CURRENT_DESKTOP` inside the desktop is right
- [ ] `id` inside the desktop lists the user's supplementary groups
- [ ] Logging out returns to the greeter
- [ ] Greeter renders at all — a blank screen means the CSP blocked the WASM
      loader; clear `csp` in `tauri.conf.json` and reopen the issue
- [ ] A wrong password says only "Incorrect password." — no username hints
- [ ] `journalctl -u cssdm` shows the real reason, and no password anywhere
- [ ] Typing `root` is impossible in the dropdown; if you craft the request by
      hand it is refused with "uid 0 is not a login account"
- [ ] `ls -l /run/cssdm.sock` is `srw------- root root` while the greeter is up
- [ ] Clock and date show the right local time
- [ ] Users load correctly
- [ ] Sessions listed in the bottom-left selector
- [ ] `/etc/cssdm/theme.css` is applied when installed
- [ ] Password field is focused on start; Enter submits
- [ ] Invalid credentials show an error and clear the field
- [ ] Power buttons respond (test suspend last)
- [ ] Readable at 1024x768 as well as full resolution

## Debug Information

### View Application Logs

```bash
# Run with debug output
RUST_LOG=debug ./target/release/cssdm --greeter

# Check system logs
journalctl -f  # If running as systemd service
```

### Check User Data

```bash
# Verify users load correctly
cat /etc/passwd | grep -E ':[0-9]{4}:' | head

# Verify sessions detected
ls -la /usr/share/xsessions/
ls -la /usr/share/wayland-sessions/
```

### Test PAM Authentication

```bash
# Test authentication without CSSDM
su $USER -c "echo 'Auth works'"  # Should prompt for password

# This is what CSSDM uses internally
```

## Performance Metrics

On typical hardware:

- **Startup**: ~0.5 seconds
- **User load**: ~50-100 ms
- **Session detect**: ~50-100 ms
- **Theme switch**: ~10 ms (instant to user)
- **Authentication**: ~1 second (via PAM)
- **Memory idle**: ~40-50 MB

## Troubleshooting

| Symptom | Check |
| --- | --- |
| No users listed | `/etc/passwd` readable; accounts need uid ≥ 1000 and a real shell |
| No sessions listed | `ls /usr/share/xsessions /usr/share/wayland-sessions` |
| Login always fails | `journalctl -u cssdm`; test the policy with `su - <user>` |
| Greeter never appears | `Xorg` on `$PATH`? `journalctl -u cssdm` reports how it failed to start |
| Desktop starts then dies | check `XDG_RUNTIME_DIR` exists: `loginctl session-status` |
| Theme ignored | `/etc/cssdm/theme.css` must be world-readable |
| Blank screen | `CSSDM_WINDOWED=1 cssdm --greeter` from a desktop shows whether the webview or the display is at fault |

### Black Screen at Login

**Symptom**: CSSDM starts but shows black screen

**Solution**:
```bash
# Did the X server start, and what did it say?
journalctl -u cssdm -n 50
cat /var/log/Xorg.0.log

# Try the greeter on its own
CSSDM_WINDOWED=1 ./target/release/cssdm --greeter
```

### Users Not Loading

**Symptom**: "Failed to load users" error

**Solution**:
```bash
# Verify /etc/passwd is readable
ls -la /etc/passwd

# Check you have users with UID >= 1000
awk -F: '$3 >= 1000 {print $1, $3}' /etc/passwd
```

### No Sessions Found

**Symptom**: Session dropdown is empty

**Solution**:
```bash
# Install a desktop environment
sudo apt install gnome-session  # GNOME
sudo apt install kde-plasma-desktop  # KDE
sudo apt install xfce4  # XFCE

# Verify sessions installed
ls /usr/share/xsessions/
ls /usr/share/wayland-sessions/
```

### Theme Not Applied

**Symptom**: the greeter shows the built-in look

**Solution**:
```bash
# One file, read at startup, must be world-readable
ls -l /etc/cssdm/theme.css
sudo install -Dm644 themes/midnight.css /etc/cssdm/theme.css
```

## Next Steps After Testing

Once you've verified everything works:

1. **Create distribution packages** (Phase 9):
   - Debian/Ubuntu (.deb)
   - Fedora/RHEL (.rpm)
   - Arch Linux (PKGBUILD)
   - Generic tarball

2. **Set up GitHub releases**:
   - Upload binaries to releases page
   - Create installation scripts per distro

3. **System-wide installation**:
   ```bash
   make systemd-install
   sudo systemctl disable sddm && sudo systemctl enable cssdm
   ```

## Reporting Issues

If you find problems during testing:

1. **Capture debug output**:
   ```bash
   RUST_LOG=debug ./target/release/cssdm 2>&1 | tee cssdm.log
   ```

2. **Include**:
   - OS and distro (lsb_release -a)
   - Desktop environment (echo $DESKTOP_SESSION)
   - Error messages
   - cssdm.log output

3. **Reference**: See [SECURITY.md](SECURITY.md) for known limitations

## Clean Up Test Installation

```bash
# Remove test binary
rm ~/.local/bin/cssdm

# Restore original DM (if changed)
sudo systemctl disable cssdm && sudo systemctl enable sddm

# Remove the installed theme, if you added one
sudo rm -rf /etc/cssdm
```
