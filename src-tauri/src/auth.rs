// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::session::Session;
use pam_client::conv_mock::Conversation;
use pam_client::{Context, Flag};
use serde::Serialize;
use std::env;
use std::ffi::{CStr, CString};
use std::fs;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

/// PAM policy to authenticate against. `login` ships on every mainstream
/// distro and is what a console greeter is expected to use.
// ponytail: a dedicated /etc/pam.d/cssdm is the upgrade path if a distro's
// `login` policy gets in the way (e.g. pam_securetty).
const PAM_SERVICE: &str = "login";

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub name: String,
    /// GECOS full name, falling back to the account name.
    pub display_name: String,
}

/// The parts of a passwd entry a session launch needs.
#[derive(Debug, Clone)]
pub struct Passwd {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
    pub shell: String,
}

/// An authenticated login, holding the PAM handle that must stay open for as
/// long as the desktop runs.
pub struct Login {
    context: Context<Conversation>,
    user: Passwd,
    session: Session,
}

impl Login {
    /// Verify a password against the system PAM stack.
    ///
    /// This must never be replaced by shelling out to `su`: when the greeter runs
    /// as root, `su` skips authentication entirely and every password is accepted.
    pub fn authenticate(username: &str, password: &str, session: Session) -> Result<Self, String> {
        let mut context = Context::new(
            PAM_SERVICE,
            Some(username),
            Conversation::with_credentials(username, password),
        )
        .map_err(|e| format!("PAM init failed: {e}"))?;

        // pam_systemd reads the seat, VT and session type out of the PAM
        // environment when it registers the logind session. Without them there
        // is no XDG_RUNTIME_DIR, and without that the desktop comes up broken:
        // no dbus, no portals, no audio.
        let seat = env::var("XDG_SEAT").unwrap_or_else(|_| "seat0".to_string());
        let vtnr = env::var("XDG_VTNR").unwrap_or_else(|_| "1".to_string());
        context
            .set_tty(Some(&format!("/dev/tty{vtnr}")))
            .map_err(|e| e.to_string())?;
        for variable in [
            format!("XDG_SEAT={seat}"),
            format!("XDG_VTNR={vtnr}"),
            "XDG_SESSION_CLASS=user".to_string(),
            format!("XDG_SESSION_TYPE={}", session.session_type()),
            format!("XDG_SESSION_DESKTOP={}", session.id),
        ] {
            context.putenv(&variable).map_err(|e| e.to_string())?;
        }

        // Checked before PAM, not after: the greeter only ever offers these
        // accounts, but it is the daemon that decides, so a greeter that has
        // been tampered with still cannot ask for root — and a rejected root
        // attempt never reaches pam_faillock to lock the console out.
        // Enumeration is not a concern to trade against: the greeter lists
        // every one of these accounts on screen already.
        let user = passwd(username).ok_or("no such account")?;
        if !is_login_account(&user) {
            return Err(format!("uid {} is not a login account", user.uid));
        }

        context.authenticate(Flag::NONE).map_err(|e| e.to_string())?;
        // Rejects locked, expired and not-yet-valid accounts.
        context.acct_mgmt(Flag::NONE).map_err(|e| e.to_string())?;

        Ok(Self {
            context,
            user,
            session,
        })
    }

    /// Open the PAM session, run the desktop to completion, then close it.
    /// Blocks until the user logs out.
    pub fn run(mut self) -> Result<(), String> {
        // One journal line per session, with everything an audit needs and
        // nothing it does not. The password is never formatted anywhere: the
        // types that carry it deliberately have no Debug.
        eprintln!(
            "cssdm: opening session {:?} for {} (uid {})",
            self.session.name, self.user.name, self.user.uid
        );
        let pam_session = self
            .context
            .open_session(Flag::NONE)
            .map_err(|e| format!("PAM session: {e}"))?;

        let mut environment = self.session.env(&self.user);
        // Appended last, so pam_systemd's XDG_RUNTIME_DIR and XDG_SESSION_ID win.
        environment.extend(pam_session.envlist().iter_tuples().map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        }));

        let status = spawn(&self.user, &self.session.exec, &environment)?
            .wait()
            .map_err(|e| format!("wait for session: {e}"))?;
        eprintln!("cssdm: session {:?} exited ({status})", self.session.name);

        // Dropping closes the PAM session and deletes the credentials.
        drop(pam_session);
        Ok(())
    }
}

/// Start the desktop as the user, in its own session.
fn spawn(user: &Passwd, exec: &str, environment: &[(String, String)]) -> Result<Child, String> {
    let shell = if user.shell.is_empty() {
        "/bin/sh"
    } else {
        &user.shell
    };
    // A login shell so /etc/profile and the user's own dotfiles are sourced,
    // which is where the locale and per-user PATH come from.
    let mut command = Command::new(shell);
    command
        .arg("-l")
        .arg("-c")
        .arg(exec)
        .current_dir(&user.home)
        .env_clear()
        .envs(environment.iter().map(|(k, v)| (k, v)));

    let name = CString::new(user.name.clone()).map_err(|e| e.to_string())?;
    let (uid, gid) = (user.uid, user.gid);
    // Not Command::uid()/gid(): std wipes the supplementary groups instead of
    // filling them in, which would cost the user `video`, `input` and `wheel`.
    // Order matters — initgroups and setgid have to happen while still root.
    unsafe {
        command.pre_exec(move || {
            // Its own session, so the desktop outlives the greeter's process group.
            if libc::setsid() < 0
                || libc::initgroups(name.as_ptr(), gid) != 0
                || libc::setgid(gid) != 0
                || libc::setuid(uid) != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    command.spawn().map_err(|e| format!("start {exec:?}: {e}"))
}

/// The rule the greeter's user list uses, applied to a resolved account: a
/// regular uid and a real shell. Root and service accounts cannot log in.
fn is_login_account(user: &Passwd) -> bool {
    (1000..65534).contains(&user.uid)
        && !user.shell.ends_with("nologin")
        && !user.shell.ends_with("/false")
}

/// Resolve an account through NSS, so LDAP and SSSD users work too — PAM can
/// authenticate accounts that never appear in /etc/passwd.
// ponytail: getpwnam, not getpwnam_r. One call, at login, from one thread.
fn passwd(name: &str) -> Option<Passwd> {
    let name = CString::new(name).ok()?;
    unsafe {
        let entry = libc::getpwnam(name.as_ptr());
        if entry.is_null() {
            return None;
        }
        let string = |pointer: *const libc::c_char| {
            if pointer.is_null() {
                String::new()
            } else {
                CStr::from_ptr(pointer).to_string_lossy().into_owned()
            }
        };
        Some(Passwd {
            name: string((*entry).pw_name),
            uid: (*entry).pw_uid,
            gid: (*entry).pw_gid,
            home: string((*entry).pw_dir),
            shell: string((*entry).pw_shell),
        })
    }
}

/// Human login accounts, sorted by name.
pub fn users() -> Result<Vec<User>, String> {
    let passwd =
        fs::read_to_string("/etc/passwd").map_err(|e| format!("read /etc/passwd: {e}"))?;
    let mut users: Vec<User> = passwd.lines().filter_map(parse_login_user).collect();
    users.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(users)
}

/// Parses one /etc/passwd line, keeping only accounts a person can log into:
/// a regular uid and a real shell. Root and service accounts are not offered.
fn parse_login_user(line: &str) -> Option<User> {
    let mut fields = line.split(':');
    let name = fields.next()?;
    let uid: u32 = fields.nth(1)?.parse().ok()?;
    let gecos = fields.nth(1)?;
    let shell = fields.nth(1)?;

    if !(1000..65534).contains(&uid) || shell.ends_with("nologin") || shell.ends_with("/false") {
        return None;
    }

    let full_name = gecos.split(',').next().unwrap_or("").trim();
    Some(User {
        name: name.to_string(),
        display_name: if full_name.is_empty() {
            name.to_string()
        } else {
            full_name.to_string()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_human_account_and_prefers_gecos_name() {
        let user =
            parse_login_user("brian:x:1000:1000:Brian Teague,,,:/home/brian:/bin/bash").unwrap();
        assert_eq!(user.name, "brian");
        assert_eq!(user.display_name, "Brian Teague");
    }

    #[test]
    fn falls_back_to_account_name_without_gecos() {
        let user = parse_login_user("dev:x:1001:1001::/home/dev:/bin/zsh").unwrap();
        assert_eq!(user.display_name, "dev");
    }

    /// Guards the hole this replaced: the old `su`-based check returned success
    /// for any password when the greeter ran as root. Uses a username that
    /// cannot exist, so repeated runs never trip pam_faillock on a real account.
    #[test]
    fn pam_rejects_an_unknown_account() {
        let session = Session {
            id: "plasma".into(),
            name: "KDE Plasma".into(),
            exec: "startplasma-wayland".into(),
            desktop_names: "KDE".into(),
            wayland: true,
        };
        assert!(Login::authenticate("cssdm-no-such-user", "irrelevant", session).is_err());
    }

    #[test]
    fn drops_root_service_and_nologin_accounts() {
        for line in [
            "root:x:0:0:root:/root:/bin/bash",
            "http:x:33:33::/srv/http:/usr/bin/nologin",
            "nobody:x:65534:65534:Nobody:/:/usr/bin/nologin",
            "backup:x:1002:1002::/home/backup:/bin/false",
            "truncated:x:1003",
            "",
        ] {
            assert!(parse_login_user(line).is_none(), "should skip: {line}");
        }
    }

    /// root always resolves; this is really checking the pointer copying.
    #[test]
    fn resolves_an_account_through_nss() {
        let root = passwd("root").unwrap();
        assert_eq!(root.uid, 0);
        assert!(!root.home.is_empty());
        assert!(passwd("cssdm-no-such-user").is_none());
    }

    fn account(uid: u32, shell: &str) -> Passwd {
        Passwd {
            name: "x".into(),
            uid,
            gid: uid,
            home: "/home/x".into(),
            shell: shell.into(),
        }
    }

    /// The greeter never offers these, but the greeter is not what decides.
    #[test]
    fn refuses_root_and_service_accounts_whatever_the_greeter_sends() {
        assert!(is_login_account(&account(1000, "/bin/bash")));
        assert!(!is_login_account(&account(0, "/bin/bash")));
        assert!(!is_login_account(&account(33, "/bin/bash")));
        assert!(!is_login_account(&account(65534, "/bin/bash")));
        assert!(!is_login_account(&account(1000, "/usr/bin/nologin")));
        assert!(!is_login_account(&account(1000, "/bin/false")));
    }

    /// The real thing this guards: root must not be able to log in at the
    /// greeter even with the correct root password.
    #[test]
    fn root_cannot_log_in_even_before_pam_is_consulted() {
        let session = Session {
            id: "plasma".into(),
            name: "KDE Plasma".into(),
            exec: "startplasma-wayland".into(),
            desktop_names: "KDE".into(),
            wayland: true,
        };
        // `Login` has no Debug on purpose, so match rather than unwrap_err.
        match Login::authenticate("root", "irrelevant", session) {
            Err(error) => assert!(error.contains("not a login account"), "got: {error}"),
            Ok(_) => panic!("root was allowed to log in"),
        }
    }
}
