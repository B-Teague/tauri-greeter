// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

//! The long-lived half of CSSDM.
//!
//! The greeter cannot start the desktop itself: it holds the display, and the
//! desktop needs that display. So the daemon runs an X server and the greeter as
//! children, takes the credentials back over a socket, and only then shuts both
//! down — freeing the screen and leaving the daemon holding the PAM handle that
//! has to outlive the session.

use crate::auth::Login;
use crate::session;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Where the greeter reaches the daemon. Both ends run as root; mode 0600.
const SOCKET: &str = "/run/cssdm.sock";

/// The only thing a failed login is ever told. Which of "no such account",
/// "wrong password", "account expired" or "not a login account" it was stays in
/// the journal — on screen it would tell someone at the keyboard which
/// half of a guess was right.
const DENIED: &str = "Incorrect password.";

/// Runtime files the greeter needs, chiefly its X authority cookie. Root-only,
/// like the socket beside it.
const RUNTIME_DIR: &str = "/run/cssdm";

/// A request is three short strings. Anything longer is not a greeter, and
/// read_line on an unbounded socket is how a process gets OOM-killed.
const MAX_REQUEST: u64 = 4096;

/// A greeter that connects and then says nothing must not wedge the login
/// screen for everyone behind it.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the greeter waits for a verdict. Generous on purpose: PAM delays
/// failures deliberately, and faillock or a remote directory can stretch that.
/// Time out early and the greeter says "unavailable" for a login that in fact
/// succeeded, while the daemon is already starting the desktop behind it.
const REPLY_TIMEOUT: Duration = Duration::from_secs(120);

/// The greeter is a webview window, and a VT is not something a window can be
/// drawn on — it needs a display server. Xorg is the one server present on every
/// distro, runs with no compositor and no window manager, and hosting the greeter
/// on it says nothing about the session started afterwards: an X11 or a Wayland
/// desktop follows either way, once this server has exited and let go of the VT.
const X_SERVER: &str = "Xorg";

/// Displays to consider before giving up. `:0` is free on a machine that boots
/// to the greeter and taken when testing from a live desktop.
const MAX_DISPLAY: u32 = 16;

/// How long Xorg gets to come up and accept a connection. Hardware probing on a
/// cold boot is not instant, and a timeout here is a login screen that never
/// appears.
const DISPLAY_TIMEOUT: Duration = Duration::from_secs(20);

/// Deliberately not `Debug`: `{:?}` on this would put a password in the
/// journal the first time anyone added a trace line. Same for `Clone` — there
/// is no reason for a second copy of a password to exist.
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub username: String,
    pub password: String,
    pub session: String,
}

#[derive(Serialize, Deserialize)]
pub struct Response {
    pub error: Option<String>,
}

/// Greeter, login, desktop, greeter again — for as long as the machine is up.
pub fn run() -> ! {
    loop {
        if let Err(error) = cycle() {
            eprintln!("cssdm: {error}");
            // Don't spin a broken greeter at full speed; systemd's restart
            // limit is the backstop if it never recovers.
            sleep(Duration::from_secs(2));
        }
    }
}

fn cycle() -> Result<(), String> {
    runtime_dir()?;
    let listener = listen()?;
    let display = Display::start()?;
    let mut greeter = spawn_greeter(&display)?;

    let login = accept_login(&listener, &mut greeter);

    // On success the greeter exits by itself. The X server does not: it has to be
    // stopped before the desktop starts, or the VT and the GPU are still ours.
    let _ = greeter.wait();
    drop(display);
    drop(listener);
    let _ = fs::remove_file(SOCKET);

    login?.run()
}

/// The greeter's X server, with the cookie that keeps it to ourselves.
struct Display {
    server: Child,
    number: u32,
    authority: PathBuf,
}

impl Display {
    fn start() -> Result<Self, String> {
        let number = (0..MAX_DISPLAY)
            .find(|n| !Path::new(&format!("/tmp/.X{n}-lock")).exists())
            .ok_or("no free X display")?;
        let authority = authority_file()?;
        let vtnr = std::env::var("XDG_VTNR").unwrap_or_else(|_| "1".to_string());

        let server = Command::new(X_SERVER)
            .arg(format!(":{number}"))
            .arg(format!("vt{vtnr}"))
            .arg("-auth")
            .arg(&authority)
            // -noreset: with one client, the greeter, the server would otherwise
            // reset the moment that client exits. -novtswitch: leave the VT where
            // it is on the way out, because the desktop is about to use it.
            .args(["-nolisten", "tcp", "-noreset", "-novtswitch"])
            .spawn()
            .map_err(|e| format!("start {X_SERVER}: {e} (is an X server installed?)"))?;

        let display = Self {
            server,
            number,
            authority,
        };
        display.wait_until_accepting()?;
        Ok(display)
    }

    /// Connect to the server's own socket rather than watching for the file to
    /// appear: the file exists between bind and listen, and a greeter that starts
    /// in that window dies with "cannot open display".
    fn wait_until_accepting(&self) -> Result<(), String> {
        let socket = format!("/tmp/.X11-unix/X{}", self.number);
        let deadline = Instant::now() + DISPLAY_TIMEOUT;
        while Instant::now() < deadline {
            if UnixStream::connect(&socket).is_ok() {
                return Ok(());
            }
            sleep(Duration::from_millis(100));
        }
        Err(format!("{X_SERVER} did not accept connections on :{}", self.number))
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        // SIGTERM, not Child::kill: on SIGTERM Xorg puts the VT back into text
        // mode, and SIGKILL leaves whoever comes next looking at a black screen.
        unsafe { libc::kill(self.server.id() as libc::pid_t, libc::SIGTERM) };
        let _ = self.server.wait();
        let _ = fs::remove_file(&self.authority);
    }
}

/// An Xauthority file holding one random cookie. Without it the greeter's X
/// server accepts any local connection, and a user on another VT or over ssh
/// could read the password being typed into the login screen.
fn authority_file() -> Result<PathBuf, String> {
    let mut cookie = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut urandom| urandom.read_exact(&mut cookie))
        .map_err(|e| format!("/dev/urandom: {e}"))?;

    let path = PathBuf::from(format!("{RUNTIME_DIR}/Xauthority"));
    let previous_umask = unsafe { libc::umask(0o177) };
    let written = fs::write(&path, authority_record(&cookie));
    unsafe { libc::umask(previous_umask) };
    written.map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// One Xauthority record: family, then address, display number, auth name and
/// auth data, each a big-endian u16 length followed by that many bytes.
/// `FamilyWild` with an empty address and number matches every connection, which
/// is what libXau and the server both want here — the cookie is the secret, and
/// the file is readable only by root.
fn authority_record(cookie: &[u8]) -> Vec<u8> {
    const FAMILY_WILD: u16 = 0xFFFF;
    let mut record = FAMILY_WILD.to_be_bytes().to_vec();
    for field in [b"".as_slice(), b"", b"MIT-MAGIC-COOKIE-1", cookie] {
        record.extend((field.len() as u16).to_be_bytes());
        record.extend_from_slice(field);
    }
    record
}

/// Somewhere for the greeter's runtime files, the X cookie included. A system
/// service is never given an XDG_RUNTIME_DIR — that is pam_systemd's job for
/// logged-in users — and GTK falls back to odd places without one. Only when we
/// have none: an operator running the daemon by hand from a session keeps theirs.
/// Never reaches the desktop: `spawn` clears the environment and pam_systemd
/// supplies the user's own.
fn runtime_dir() -> Result<(), String> {
    if std::env::var_os("XDG_RUNTIME_DIR").is_some() {
        return Ok(());
    }
    fs::create_dir_all(RUNTIME_DIR).map_err(|e| format!("create {RUNTIME_DIR}: {e}"))?;
    fs::set_permissions(RUNTIME_DIR, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("chmod {RUNTIME_DIR}: {e}"))?;
    std::env::set_var("XDG_RUNTIME_DIR", RUNTIME_DIR);
    Ok(())
}

fn listen() -> Result<UnixListener, String> {
    let _ = fs::remove_file(SOCKET); // Stale socket from a crash.

    // bind() applies the umask, and chmod afterwards would leave the socket
    // connectable by anyone for the instant in between. Narrow it first, then
    // assert the end state.
    let previous_umask = unsafe { libc::umask(0o177) };
    let bound = UnixListener::bind(SOCKET);
    unsafe { libc::umask(previous_umask) };

    let listener = bound.map_err(|e| format!("bind {SOCKET}: {e}"))?;
    fs::set_permissions(SOCKET, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("chmod {SOCKET}: {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("{SOCKET}: {e}"))?;
    Ok(listener)
}

fn spawn_greeter(display: &Display) -> Result<Child, String> {
    let binary = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    Command::new(&binary)
        .arg("--greeter")
        .env("DISPLAY", format!(":{}", display.number))
        .env("XAUTHORITY", &display.authority)
        // GTK would pick a Wayland display out of the environment if one were
        // there; the greeter belongs on the server we just started.
        .env("GDK_BACKEND", "x11")
        .env_remove("WAYLAND_DISPLAY")
        .spawn()
        .map_err(|e| format!("start greeter: {e}"))
}

/// Serve login attempts until one succeeds, or the greeter dies under us.
fn accept_login(listener: &UnixListener, greeter: &mut Child) -> Result<Login, String> {
    loop {
        match listener.accept() {
            // A rejected attempt is not an error here: the greeter stays up
            // and prompts again, exactly as it does for a typo.
            Ok((stream, _)) => {
                if let Some(login) = attempt(stream) {
                    return Ok(login);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                // ponytail: a 100ms poll instead of a second thread, purely so a
                // greeter that crashed is noticed and restarted rather than
                // leaving a black screen nobody can log in from.
                if let Ok(Some(status)) = greeter.try_wait() {
                    return Err(format!("greeter exited before login ({status})"));
                }
                sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(format!("accept: {error}")),
        }
    }
}

/// One request/response round. Every path answers — a greeter left waiting on a
/// reply it never gets is a login screen that has stopped accepting logins.
fn attempt(stream: UnixStream) -> Option<Login> {
    let login = handle(&stream);
    if let Err(detail) = &login {
        // The detail is for the journal only; the greeter is told `DENIED`.
        eprintln!("cssdm: login rejected: {detail}");
    }

    let response = Response {
        error: login.as_ref().err().map(|_| DENIED.to_string()),
    };
    match serde_json::to_string(&response) {
        // On success the greeter is waiting for exactly this before it exits.
        Ok(body) => {
            if let Err(error) = writeln!(&stream, "{body}") {
                eprintln!("cssdm: reply: {error}");
                return None; // Never start a session the greeter thinks failed.
            }
        }
        Err(error) => {
            eprintln!("cssdm: encode reply: {error}");
            return None;
        }
    }

    login.ok()
}

/// Read one request and authenticate it. The error is the audit record, so it
/// says what really happened; only `attempt` decides what leaves the machine.
fn handle(stream: &UnixStream) -> Result<Login, String> {
    stream
        .set_read_timeout(Some(REQUEST_TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(REQUEST_TIMEOUT)))
        .map_err(|e| format!("timeouts: {e}"))?;

    let mut line = String::new();
    BufReader::new(stream.take(MAX_REQUEST))
        .read_line(&mut line)
        .map_err(|e| format!("read: {e}"))?;
    // The parse error would quote the input, and the input is a password.
    let request: Request = serde_json::from_str(&line).map_err(|_| "malformed request")?;
    let username = journal_safe(&request.username);

    // The greeter sends a name and the exec line comes from our own scan, so
    // what runs as the user is never something the greeter chose.
    let session = session::sessions()
        .into_iter()
        .find(|candidate| candidate.name == request.session)
        .ok_or_else(|| format!("{username}: unknown session"))?;

    Login::authenticate(&request.username, &request.password, session)
        .map_err(|error| format!("{username}: {error}"))
}

/// Usernames arrive from the greeter, so they are never written to the journal
/// as they came: control characters there are a log-forging tool.
fn journal_safe(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "._-@".contains(*c))
        .take(32)
        .collect()
}

/// Greeter side: hand the credentials to the daemon and wait for the verdict.
pub fn request(request: &Request) -> Result<Response, String> {
    let stream = UnixStream::connect(SOCKET).map_err(|e| format!("connect {SOCKET}: {e}"))?;
    stream
        .set_read_timeout(Some(REPLY_TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(REQUEST_TIMEOUT)))
        .map_err(|e| format!("timeouts: {e}"))?;
    serde_json::to_writer(&stream, request).map_err(|e| e.to_string())?;
    writeln!(&stream).map_err(|e| format!("send: {e}"))?;

    let mut line = String::new();
    BufReader::new(&stream)
        .read_line(&mut line)
        .map_err(|e| format!("read reply: {e}"))?;
    serde_json::from_str(&line).map_err(|e| format!("bad reply: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_control_characters_from_logged_usernames() {
        assert_eq!(journal_safe("brian"), "brian");
        // A forged second journal line, defused.
        assert_eq!(journal_safe("x\ncssdm: opening session for root"), "xcssdmopeningsessionforroot");
        assert_eq!(journal_safe(&"a".repeat(500)).len(), 32);
    }

    /// The two ends are compiled together but talk over bytes; a field renamed
    /// on one side only would be caught here.
    #[test]
    fn request_and_response_survive_the_wire() {
        let line = serde_json::to_string(&Request {
            username: "brian".into(),
            password: "hunter2".into(),
            session: "KDE Plasma".into(),
        })
        .unwrap();
        let back: Request = serde_json::from_str(&line).unwrap();
        assert_eq!(back.session, "KDE Plasma");

        let ok: Response = serde_json::from_str(r#"{"error":null}"#).unwrap();
        assert!(ok.error.is_none());
        let failed: Response =
            serde_json::from_str(&serde_json::to_string(&Response { error: Some("no".into()) }).unwrap())
                .unwrap();
        assert_eq!(failed.error.as_deref(), Some("no"));
    }

    /// libXau reads this file; a wrong length prefix means a greeter that cannot
    /// open its own display, on a machine with no other way in.
    #[test]
    fn authority_record_is_a_wild_mit_magic_cookie() {
        let cookie = [0xAB; 16];
        let record = authority_record(&cookie);

        assert_eq!(&record[..2], &[0xFF, 0xFF], "FamilyWild");
        assert_eq!(&record[2..4], &[0, 0], "empty address");
        assert_eq!(&record[4..6], &[0, 0], "empty display number");
        assert_eq!(&record[6..8], &[0, 18], "name length");
        assert_eq!(&record[8..26], b"MIT-MAGIC-COOKIE-1");
        assert_eq!(&record[26..28], &[0, 16], "cookie length");
        assert_eq!(&record[28..], &cookie);
        assert_eq!(record.len(), 44);
    }
}
