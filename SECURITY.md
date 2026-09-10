# CSSDM Security Considerations

This document outlines the security model and best practices for CSSDM.

## Authentication

### PAM Integration
- **Method**: System `su` command via subprocess
- **Why**: Routes through system PAM stack, respecting all configured authentication methods (LDAP, Kerberos, etc.)
- **Security**: PAM handles all password validation — CSSDM never reads shadow files directly
- **Protection**: Password passed via stdin only, not command-line args

### Password Handling
- ✅ Passwords never logged to console or files
- ✅ Password cleared from memory on auth failure
- ✅ No password storage or caching
- ✅ Never visible in process arguments (`ps` output)

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
- After successful authentication, session is launched via `exec`
- Session inherits user's UID/GID (not root)
- No elevation for unprivileged operations

## Power Commands

### Systemctl Integration
- ✅ Uses `systemctl` (authenticated via system PAM)
- ✅ Respects polkit policies (if configured)
- ✅ No direct syscalls or kernel module loading
- ✅ Graceful error handling if permissions denied

### Attack Prevention
- No unauthenticated power commands
- User must log in before power menu available
- Each action goes through systemd's auth stack

## File Access

### Paths Accessed
- `/etc/passwd` — read-only, public information
- `/usr/share/xsessions/` — system desktop sessions
- `/usr/share/wayland-sessions/` — system desktop sessions
- `~/.local/share/cssdm/themes/` — user theme directory
- `~/.xsession` — written only after successful auth

### Permissions
- All reads are from public system directories
- Write only to user's home directory (after auth)
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

## Frontend Security

### Tauri Sandbox
- ✅ No arbitrary code execution
- ✅ No file system access from JS
- ✅ All system calls through Rust backend
- ✅ No network access (no remote themes)
- ✅ CSP (Content Security Policy) configured

### Local Storage
- User preferences stored in localStorage
- No sensitive data (only theme name, not passwords)
- Isolated per CSSDM instance

## Configuration Security

### Theme Loading
- ✅ Themes are CSS only (no JavaScript execution)
- ✅ Only loads from trusted directories
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
- Password strength validation (at login time)
- Rate limiting on auth failures
- Failed login logging to syslog
- Hardware token support (via PAM)
- Multi-seat support (future)

## Deployment Recommendations

### System Setup
```bash
# Install CSSDM binary
sudo install -m 755 cssdm /usr/local/bin/

# Create system theme directory
sudo mkdir -p /usr/share/cssdm/themes

# Copy default themes
sudo cp -r themes/* /usr/share/cssdm/themes/

# Configure as default display manager
sudo update-alternatives --install /usr/bin/x-session-manager \
  x-session-manager /usr/local/bin/cssdm 100
```

### Display Manager Integration
- Run via systemd service (not from getty)
- Set `KillMode=none` (don't kill session)
- Use `Type=simple` (Tauri manages its own lifecycle)

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
- [ ] Reject with invalid credentials
- [ ] Password cleared after failed login
- [ ] Session selection persists
- [ ] Power menu unavailable before login
- [ ] Each user can only access their own files
- [ ] No sensitive data in `/tmp/`

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
