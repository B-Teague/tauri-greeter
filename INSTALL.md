# CSSDM Installation Guide

Instructions for building and installing CSSDM as your display manager.

## Prerequisites

- Linux system (x86_64 or ARM)
- Rust 1.70+ ([install](https://rustup.rs/))
- Standard build tools: `gcc`, `make`, `pkg-config`
- Development libraries: `libssl-dev`, `libgtk-3-dev` (Debian/Ubuntu)

### Install Build Dependencies

**Debian/Ubuntu:**
```bash
sudo apt-get update
sudo apt-get install -y build-essential libssl-dev libgtk-3-dev \
  libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

**Fedora/RHEL:**
```bash
sudo dnf install -y @development-tools openssl-devel gtk3-devel \
  webkit2gtk4.1-devel glib2-devel
```

**Arch Linux:**
```bash
sudo pacman -S base-devel openssl gtk3 webkit2gtk glib2
```

## Installation Steps

### 1. Build CSSDM

```bash
cd /path/to/CSSDM
make release  # or: cargo build --release
```

Binary will be at: `target/release/cssdm`

### 2. Install Binary & Themes

```bash
make install  # requires sudo
```

This:
- Installs binary to `/usr/local/bin/cssdm`
- Creates `/usr/share/cssdm/themes/`
- Copies included themes (light, high-contrast)

### 3. Set as Default Display Manager

```bash
# Register CSSDM as display manager
sudo update-alternatives --install /usr/bin/x-session-manager \
  x-session-manager /usr/local/bin/cssdm 100

# Verify it's selected
sudo update-alternatives --config x-session-manager
```

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

# Output should show:
# Binary: ✓ /usr/local/bin/cssdm exists
# Themes: ✓ Theme directory exists
# Service: [depends on systemd install]
```

### 6. Test Before Reboot

**Option A: Fullscreen test**
```bash
# Starts CSSDM in fullscreen (Ctrl+C to exit)
cssdm
```

**Option B: Nested display (safer)**
```bash
# Requires Xvfb or Wayland compositor
DISPLAY=:1 cssdm &
```

### 7. Reboot to Deploy

```bash
sudo reboot

# Login screen should show CSSDM
# Select user → session → login
```

## Uninstallation

### Remove Display Manager

```bash
# Unset CSSDM as default
sudo update-alternatives --remove x-session-manager /usr/local/bin/cssdm

# Uninstall files
make uninstall
```

### Remove systemd Service

```bash
make systemd-uninstall
```

## Troubleshooting

### Login Loop (Boot Loop)

If CSSDM crashes on startup, preventing login:

1. **Boot to recovery mode:**
   ```bash
   # At GRUB menu: e to edit, add "single" or "recovery"
   ```

2. **Restore previous DM:**
   ```bash
   sudo update-alternatives --config x-session-manager
   # Select previous DM
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

- Test authentication manually:
  ```bash
  su testuser -c "echo $USER"  # Should print: testuser
  ```

### Themes Not Loading

- Check theme directory:
  ```bash
  ls -la /usr/share/cssdm/themes/
  # Should show: light/, high-contrast/
  ```

- Verify theme.json format:
  ```bash
  cat /usr/share/cssdm/themes/light/theme.json | jq .
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
# Check if themes are accessible
ls -la /usr/share/cssdm/themes/

# Verify theme metadata
cat /usr/share/cssdm/themes/*/theme.json | jq .
```

## Development Installation

For development/testing:

```bash
# Build debug version
make build

# Run directly (no install needed)
target/debug/cssdm

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

### Memory Usage

CSSDM typically uses:
- **Idle**: ~50 MB
- **Loading data**: ~60-70 MB
- **After login**: ~40 MB (freed)

### Startup Time

Typical startup timeline:
- Binary load: ~100 ms
- User enumeration: ~50 ms
- Session detection: ~50 ms
- Theme loading: ~30 ms
- **Total**: < 300 ms

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

1. **Customize theme:**
   - Copy theme to `~/.local/share/cssdm/themes/my-theme/`
   - Modify `theme.css` to your liking
   - Select from theme menu at login

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
sudo update-alternatives --config x-session-manager

# Uninstall completely
make uninstall
make systemd-uninstall

# Reboot
sudo reboot
```
