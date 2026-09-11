# CSSDM Installation Guide

Instructions for building and installing CSSDM as your display manager.

## Prerequisites

- Linux system (x86_64 or ARM)
- Rust 1.70+ ([install](https://rustup.rs/)) with the `wasm32-unknown-unknown`
  target and [trunk](https://trunkrs.dev): `rustup target add
  wasm32-unknown-unknown && cargo install trunk`
- Standard build tools: `gcc`, `make`, `pkg-config`, `clang` (for PAM bindings)
- Development libraries: `libpam0g-dev`, `libgtk-3-dev` (Debian/Ubuntu)
- **An X server** at runtime (`xorg-server`, `xserver-xorg-core`,
  `xorg-x11-server-Xorg`) — the daemon starts one on the VT to show the greeter
  on. Already present on any distro that can run an X11 session; it does not
  constrain the desktop the user logs into, which may be Wayland.

### Install Build Dependencies

**Debian/Ubuntu:**
```bash
sudo apt-get update
sudo apt-get install -y build-essential clang libpam0g-dev libgtk-3-dev \
  libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

**Fedora/RHEL:**
```bash
sudo dnf install -y @development-tools clang pam-devel gtk3-devel \
  webkit2gtk4.1-devel glib2-devel
```

**Arch Linux:**
```bash
sudo pacman -S base-devel clang pam gtk3 webkit2gtk glib2
```

## Install From the Arch Package

The short path on Arch, CachyOS and derivatives:

```bash
make package                                  # make release, then makepkg
sudo pacman -U packaging/arch/*.pkg.tar.zst
```

`packaging/arch/PKGBUILD` packages an **already-built** binary; it has no
build step and no Rust makedepends. `make release` is what compiles, and
`make package` runs the two in order. pacman resolves `xorg-server`,
`webkit2gtk-4.1`, `gtk3`, `pam` and `systemd-libs` from the repos.

The package installs the service but deliberately leaves it disabled: enabling
it replaces your display manager, and that is not something an install should
do behind your back. Post-install prints the two commands. Jump to step 3 for
them, and to step 6 to test before committing.

To uninstall: `sudo pacman -R cssdm` (its `pre_remove` disables the unit, but
never stops it — stopping a display manager kills the session you are in).

## Installation Steps

The manual route, for non-Arch systems or development.

### 1. Build CSSDM

```bash
cd /path/to/CSSDM
make release  # trunk build --release, then cargo build --release -p cssdm
```

Binary will be at: `target/release/cssdm`

### 2. Install Binary & Themes

```bash
make install  # requires sudo
```

This:
- Installs the binary to `/usr/local/bin/cssdm`
- Copies the example themes to `/usr/share/cssdm/themes/`

The active theme is `/etc/cssdm/theme.css`, installed separately:

```bash
sudo install -Dm644 themes/midnight.css /etc/cssdm/theme.css
```

### 3. Set as Default Display Manager

`cssdm.service` carries `Alias=display-manager.service`, so enabling the unit
is the whole registration. Disable the current display manager first, or the
two will fight over tty1.

```bash
sudo systemctl disable sddm   # or gdm, lightdm, …
sudo systemctl enable cssdm
```

(`update-alternatives` is a Debian mechanism for `x-session-manager`, not how a
display manager is selected on systemd distributions.)

### 4. (Optional) Install systemd Service

For automatic startup on boot:

```bash
make systemd-install  # requires sudo
```

This:
- Copies `cssdm.service` to `/etc/systemd/system/`
- Enables the service
- Starts on next boot

### 5. Verify Installation

```bash
make verify

# Reports the binary, the systemd unit and the installed theme
```

### 6. Test Before Reboot

**Option A: greeter only, in a window**
```bash
CSSDM_WINDOWED=1 cssdm --greeter   # login won't work: no daemon, no socket
```

**Option B: the whole thing on a spare VT (recommended)**
```bash
# From a console on a free VT (Ctrl+Alt+F3):
sudo XDG_VTNR=3 XDG_SEAT=seat0 cssdm
```

Your current session on tty1 stays up; Ctrl+Alt+F1 returns to it.

### 7. Reboot to Deploy

```bash
sudo reboot

# Login screen should show CSSDM
# Select user → session → login
```

## Uninstallation

### Remove Display Manager

```bash
sudo systemctl disable cssdm
sudo systemctl enable sddm    # restore the one you had
make uninstall
```

### Remove systemd Service

```bash
make systemd-uninstall
```

## Troubleshooting

### Login Loop (Boot Loop)

If CSSDM crashes on startup, preventing login:

1. **Get a console:** the unit gives up after two crashes in 30s, so Ctrl+Alt+F2
   reaches a login prompt.

2. **Restore previous DM:**
   ```bash
   sudo systemctl disable cssdm && sudo systemctl enable sddm
   sudo reboot
   ```

3. **Check logs:**
   ```bash
   journalctl -u cssdm -n 50  # Last 50 lines
   ```

### Blank Screen After Login

- Check that desktop environment is installed:
  ```bash
  ls /usr/share/xsessions/
  ```

- Install GNOME or KDE if missing:
  ```bash
  sudo apt install gnome-session      # GNOME
  sudo apt install kde-plasma-desktop # KDE
  ```

### PAM Authentication Fails

- Verify PAM config:
  ```bash
  cat /etc/pam.d/login
  # Should have: auth, account, session lines
  ```

- CSSDM authenticates against the `login` policy; if a distro's `login`
  policy is unusual, change `PAM_SERVICE` in `src-tauri/src/auth.rs` and ship
  an `/etc/pam.d/cssdm`
- Watch the modules decide: `journalctl -u cssdm -f`

### Themes Not Loading

- The greeter reads exactly one file, at startup:
  ```bash
  ls -l /etc/cssdm/theme.css   # must exist and be world-readable
  sudo systemctl restart cssdm # re-read it
  ```

### X11 Sessions Not Showing

- Install X11 desktop environment:
  ```bash
  sudo apt install xfce4          # XFCE
  sudo apt install openbox        # Openbox
  ```

- Check session files:
  ```bash
  ls /usr/share/xsessions/
  ```

### Wayland Sessions Not Showing

- Install Wayland session:
  ```bash
  sudo apt install gnome-session-wayland  # GNOME on Wayland
  ```

- Check session files:
  ```bash
  ls /usr/share/wayland-sessions/
  ```

## Monitoring & Maintenance

### View Logs

```bash
# Real-time logs
journalctl -u cssdm -f

# Last 100 entries
journalctl -u cssdm -n 100

# By priority (err, warning, info, debug)
journalctl -u cssdm -p info
```

### Check Service Status

```bash
systemctl status cssdm

# Restart service
sudo systemctl restart cssdm

# Stop service
sudo systemctl stop cssdm
```

### Test Theme Loading

```bash
# The one file the greeter reads
ls -l /etc/cssdm/theme.css
```

## Development Installation

For development/testing:

```bash
# Build debug version
make build

# Run the greeter directly (no install needed)
CSSDM_WINDOWED=1 target/debug/cssdm --greeter

# Or with environment
RUST_LOG=debug target/debug/cssdm
```

## Performance Tuning

### Reduce Binary Size

```bash
# Strip debug symbols
strip target/release/cssdm

# Size should be ~5-10 MB
ls -lh target/release/cssdm
```

## Security Considerations

See [SECURITY.md](SECURITY.md) for detailed security analysis.

**Key points:**
- CSSDM runs as root (required for DM)
- Authentication via system PAM
- No password storage or logging
- Works with existing PAM policies

## Support & Issues

- 📖 [README.md](README.md) — Project overview
- 🔐 [SECURITY.md](SECURITY.md) — Security guide
- 🎨 [themes/README.md](themes/README.md) — Theme creation
- 📋 [PLAN.md](PLAN.md) — Implementation roadmap

## Next Steps

After installation:

1. **Customize the theme:**
   - Start from `themes/midnight.css` or `themes/nebula.css`
   - Install it as `/etc/cssdm/theme.css`
   - Restart the greeter to apply

2. **Configure PAM:**
   - Add 2FA via `/etc/pam.d/login`
   - Configure account policies
   - Set up LDAP/Kerberos if needed

3. **Monitor deployments:**
   - Watch journalctl for errors
   - Test login on different systems
   - Verify power commands work

## Rollback

If something goes wrong:

```bash
# Revert to previous DM
sudo systemctl disable cssdm && sudo systemctl enable sddm

# Uninstall completely
make uninstall
make systemd-uninstall

# Reboot
sudo reboot
```
