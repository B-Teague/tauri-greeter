# CSSDM Implementation Plan

**CSS-based Display Manager**: A Tauri-based login manager for Linux with session selection, power controls, and themeable UI.

---

## Architecture Overview

```
┌─────────────────────────────────────────┐
│   Tauri Window (Login UI)               │
│   ├─ HTML/CSS/JS Frontend               │
│   └─ Session/Power dropdowns            │
└─────────────────┬───────────────────────┘
                  │ Tauri Commands
┌─────────────────▼───────────────────────┐
│   Tauri Backend (Rust)                  │
│   ├─ User auth (PAM)                    │
│   ├─ Session detection (X11/Wayland)    │
│   ├─ Power commands (systemd)           │
│   └─ Theme loading                      │
└─────────────────┬───────────────────────┘
                  │
          ┌───────┴────────┐
          │                │
    ┌─────▼────┐    ┌─────▼─────┐
    │ /etc/X11 │    │ /usr/share │
    │ sessions │    │ themes     │
    └──────────┘    └────────────┘
```

---

## Phase 1: Project Bootstrap ✅
**Deliverable**: Tauri project structure ready for development

- [x] Cargo.toml configured (Tauri with serde, pam crate)
- [x] Trunk.toml set up (frontend build)
- [x] src-tauri/ backend structure
- [x] src/ frontend structure (Leptos or vanilla JS)

---

## Phase 2: Backend - User Authentication ✅

**Goal**: Validate credentials via system auth (Linux auth standard)

### 2.1 Authentication ✅
- [x] Create `src-tauri/src/auth.rs`:
  - Struct: `AuthResult { success: bool, error: Option<String> }`
  - Function: `fn authenticate(username: &str, password: &str) -> AuthResult`
  - Uses `su` command to validate (goes through system PAM)
  - Handles errors gracefully
- [x] Tauri command: `authenticate_user(username: String, password: String) -> AuthResult`
- [x] Error handling for invalid credentials

**Implementation note**: Uses subprocess `su -l user -c /bin/true` with password on stdin. Works with system PAM stack, no extra dependencies. ponytail: simpler than PAM crate, production-ready.

### 2.2 User Enumeration ✅
- [x] Parse `/etc/passwd` to list available users (filter system accounts)
- [x] Tauri command: `get_available_users() -> Result<Vec<UserInfo>, String>`
- [x] Returns: `{uid, username, full_name}`
- [x] Filters: root (uid 0) + regular users (uid >= 1000)
- [x] Tests: unit tests for passwd line parsing ✅

---

## Phase 3: Backend - Session Management ✅

**Goal**: Detect available desktop sessions and set the chosen one

### 3.1 Session Detection ✅
- [x] Scan `/usr/share/xsessions/` for X11 sessions (.desktop files)
- [x] Scan `/usr/share/wayland-sessions/` for Wayland sessions
- [x] Parser: read `Name=` and `Exec=` fields from .desktop
- [x] Struct: `Session { name: String, exec: String, is_wayland: bool }`
- [x] Tauri command: `get_available_sessions() -> Result<Vec<Session>, String>`
- [x] Sorting: Wayland first, then X11, alphabetically within each type
- [x] Tests: desktop parsing + session sorting validated ✅

**Implementation note**: Direct filesystem scan of .desktop files, filters by extension. Graceful handling of missing directories (some systems may not have all session types).

### 3.2 Session Selection ✅
- [x] Function: `fn set_session_env(session_name: &str)` 
  - Writes to `~/.xsession` (standard login manager convention)
  - Format: `exec <session_name>`
- [x] Tauri command: `set_session(session_name: String) -> Result<(), String>`

**Future work** (Phase 8):
- Session launch after login (exec chosen session, no return to login screen)
- Requires integration with login flow (post-auth, before UI closes)

---

## Phase 4: Backend - Power Management ✅

**Goal**: Shutdown, reboot, suspend via systemd

### 4.1 Systemd Integration ✅
- [x] Create `src-tauri/src/power.rs`:
  - Enum: `PowerAction { Shutdown, Reboot, Suspend, Logout }`
  - Function: `fn execute_power(action: PowerAction) -> Result<(), PowerError>`
  - Uses `systemctl` subprocess (universal on systemd systems)
- [x] Tauri commands:
  - `power_shutdown()` → systemctl poweroff
  - `power_reboot()` → systemctl reboot
  - `power_suspend()` → systemctl suspend
  - `power_logout()` → no-op (handled by UI)
- [x] Tests: systemctl args + logout behavior validated ✅

**Implementation note**: Direct systemctl calls via subprocess. Logout is no-op (caller handles UI exit).

### 4.2 Permissions

**Note**: CSSDM runs as root (standard for display managers). Power commands work without prompts. Non-root scenarios (future enhancement):
- Add polkit integration for unprivileged access
- Fallback commands for non-systemd systems

---

## Phase 5: Frontend - Core UI

**Goal**: Responsive login form with dropdowns

### 5.1 HTML Structure
- [ ] `index.html`:
  ```html
  <div class="login-container">
    <div class="login-form">
      <!-- Username/password inputs -->
      <!-- Session dropdown -->
      <!-- Power menu -->
      <!-- Login button -->
    </div>
    <!-- Theme selector (optional) -->
  </div>
  ```

### 5.2 Login Form Fields
- [ ] Username input (autocomplete: off)
- [ ] Password input
- [ ] "Session" dropdown:
  - Populated by `get_sessions()` on init
  - Default to first Wayland, fallback to first X11
  - Show indicator: 🌊 Wayland / 🖥️ X11
- [ ] "Power" dropdown:
  - Suspend, Shutdown, Reboot, Logout
  - Icons/labels
- [ ] Login button + loading state

### 5.3 JavaScript Bridge
- [ ] Tauri invoke calls:
  ```js
  await invoke('authenticate_user', { username, password })
  await invoke('get_sessions', {})
  await invoke('power_shutdown', {})
  ```
- [ ] Error handling: display auth errors below password field
- [ ] Success: hide form, show "Logging in..." message briefly

---

## Phase 6: Frontend - CSS Theming System

**Goal**: Load and apply default + custom themes

### 6.1 Theme Directory Structure
```
~/.local/share/cssdm/themes/
  ├── default/
  │   ├── theme.css
  │   └── theme.json (metadata)
  └── custom-theme/
      ├── theme.css
      └── theme.json
```

### 6.2 Default Theme
- [ ] `styles.css`: 
  - CSS variables: `--bg-primary`, `--text-color`, `--accent`, `--input-bg`, `--button-hover`
  - Responsive grid layout (mobile: single column, desktop: centered form)
  - Dark theme by default (fits display manager context)
  - Focus states + accessibility (WCAG AA)
  - Password field mask, loading spinner

### 6.3 Theme Loading (Backend)
- [ ] Scan `~/.local/share/cssdm/themes/` + `/usr/share/cssdm/themes/`
- [ ] Tauri command: `get_themes() -> Vec<ThemeInfo>`
- [ ] Frontend: dropdown to select theme
- [ ] On select: inject CSS dynamically or reload

### 6.4 CSS Variables (Customizable)
```css
:root {
  --bg-primary: #1a1a1a;
  --bg-secondary: #2d2d2d;
  --text-primary: #ffffff;
  --text-secondary: #aaaaaa;
  --accent: #00d4ff;
  --input-bg: #333333;
  --button-hover: #0099cc;
  --border-radius: 8px;
  --transition: all 0.2s ease;
}
```

---

## Phase 7: Integration & Polish

### 7.1 Startup Flow
- [ ] Tauri window: borderless, fullscreen (covers login screen)
- [ ] On app start:
  1. Load system theme preference (light/dark)
  2. Fetch user list + sessions
  3. Focus username field
  4. Disable interactions until ready

### 7.2 Error Handling
- [ ] Auth failures: show error, clear password, refocus
- [ ] Session launch failures: show error, stay in login
- [ ] Power action failures: notify user, dismiss

### 7.3 Security Considerations
- [ ] Never log passwords
- [ ] Clear sensitive data on logout
- [ ] Use secure PAM integration (no direct shadow file reads)
- [ ] Run display manager as root (or via systemd service)

### 7.4 Testing
- [ ] Manual: test each login session type
- [ ] Manual: test power actions (if safe)
- [ ] UI: responsive at 1024x768 and higher
- [ ] Accessibility: keyboard navigation (Tab, Enter, arrow keys)

---

## Phase 8: Deployment

### 8.1 Build & Install
- [ ] `cargo build --release`
- [ ] Install binary to `/usr/local/bin/cssdm` or `/usr/bin/cssdm`
- [ ] Copy themes to `/usr/share/cssdm/themes/`
- [ ] Create systemd service or add to `/etc/lxdm/lxdm.conf`

### 8.2 System Integration
- [ ] Update `/etc/X11/default-display-manager` or DM selection mechanism
- [ ] Test launch on boot (or within VM)
- [ ] Verify PAM stack works (may need sshd/su config review)

---

## Milestones

| Phase | Task | Est. Time |
|-------|------|-----------|
| 1 | Project bootstrap | ✅ Done |
| 2 | PAM auth backend | 2–3 days |
| 3 | Session detection & launch | 1–2 days |
| 4 | Power management | 1 day |
| 5 | Frontend form & Tauri bridge | 2–3 days |
| 6 | CSS theme system | 1–2 days |
| 7 | Integration & polish | 1–2 days |
| 8 | Deployment & testing | 1 day |

**Total**: ~2 weeks (loose, can parallelize).

---

## Critical Decisions

1. **PAM vs shadow**: Use PAM (system-standard, handles 2FA, account policies).
2. **Session launch**: Direct exec after auth (no session manager overhead).
3. **Wayland-first**: Detect Wayland first, offer X11 as fallback.
4. **Theme format**: CSS + JSON metadata (simple, no framework needed).
5. **Fullscreen**: Borderless, fullscreen window to replace traditional DM.

---

## Known Limitations & Future Work

- **Multi-monitor**: Initial version supports primary display; multi-seat support future.
- **2FA**: PAM-handled, but no custom TOTP UI yet.
- **Theme hot-reload**: Refresh on select; live edit not supported.
- **Offline login**: Requires network (PAM default); cache possibility future.
- **Branding**: Themes only; no in-app branding UI yet.

---

## File Structure (End State)

```
CSSDM/
├── Cargo.toml                    (workspace, tauri config)
├── Trunk.toml                    (frontend build)
├── index.html                    (login UI template)
├── styles.css                    (theme CSS variables + defaults)
├── src/
│   └── lib.rs / main.rs          (Leptos or plain JS frontend)
├── src-tauri/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs               (Tauri app, window setup)
│       ├── auth.rs               (PAM integration)
│       ├── session.rs            (session detection)
│       ├── power.rs              (systemd power control)
│       ├── theme.rs              (theme loading)
│       └── lib.rs                (shared types)
├── themes/
│   └── default/
│       ├── theme.css
│       └── theme.json
└── PLAN.md                       (this file)
```

---

## Next Steps

1. ✅ Read this plan
2. ⬜ Start Phase 2: Set up PAM integration in `src-tauri/src/auth.rs`
3. ⬜ Run `cargo build` to verify Tauri + pam deps compile
4. ⬜ Proceed with session detection & frontend in parallel

**Ready to start Phase 2?**
