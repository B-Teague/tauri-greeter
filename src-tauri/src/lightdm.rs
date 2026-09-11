// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

//! The LightDM side of the greeter.
//!
//! LightDM owns everything this program used to do for itself: the VT, the X
//! server, PAM, privilege dropping and starting the desktop. What is left is a
//! conversation over the pipe pair LightDM hands its greeter on startup, which
//! `liblightdm-gobject` already speaks — so this module is bindings to it plus
//! the small state machine that turns "username and password" into a session.

use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

use glib_sys::{gboolean, GError, GList, GFALSE, GTRUE};
use gobject_sys::GCallback;

/// Opaque `LightDMGreeter`. Only ever handed back to the library.
#[repr(C)]
pub struct Greeter {
    _private: [u8; 0],
}

/// `LightDMPromptType`: the prompt is for something that must not be echoed.
const PROMPT_SECRET: c_int = 1;

/// The only thing a failed login is ever told. Which of "no such account",
/// "wrong password", "account expired" or "not a login account" it was stays in
/// the journal — on screen it would tell someone at the keyboard which half of
/// a guess was right.
const DENIED: &str = "Incorrect password.";

/// How long a login attempt may stay unanswered. Generous on purpose: PAM
/// delays failures deliberately, and faillock or a remote directory can stretch
/// that. This is only a backstop against a LightDM that has stopped replying,
/// which would otherwise wedge the login screen for good.
const AUTH_TIMEOUT: Duration = Duration::from_secs(120);

/// How often the conversation is pumped while waiting for a verdict. See
/// `authenticate` for why this polls rather than blocking.
const PUMP_INTERVAL: Duration = Duration::from_millis(10);

#[link(name = "lightdm-gobject-1")]
extern "C" {
    fn lightdm_greeter_new() -> *mut Greeter;
    fn lightdm_greeter_connect_to_daemon_sync(
        greeter: *mut Greeter,
        error: *mut *mut GError,
    ) -> gboolean;
    fn lightdm_greeter_authenticate(
        greeter: *mut Greeter,
        username: *const c_char,
        error: *mut *mut GError,
    ) -> gboolean;
    fn lightdm_greeter_respond(
        greeter: *mut Greeter,
        response: *const c_char,
        error: *mut *mut GError,
    ) -> gboolean;
    fn lightdm_greeter_cancel_authentication(
        greeter: *mut Greeter,
        error: *mut *mut GError,
    ) -> gboolean;
    fn lightdm_greeter_get_is_authenticated(greeter: *mut Greeter) -> gboolean;
    fn lightdm_greeter_start_session_sync(
        greeter: *mut Greeter,
        session: *const c_char,
        error: *mut *mut GError,
    ) -> gboolean;

    fn lightdm_get_sessions() -> *mut GList;
    fn lightdm_session_get_key(session: *mut c_void) -> *const c_char;
    fn lightdm_session_get_name(session: *mut c_void) -> *const c_char;
    fn lightdm_session_get_session_type(session: *mut c_void) -> *const c_char;

    fn lightdm_user_list_get_instance() -> *mut c_void;
    fn lightdm_user_list_get_users(list: *mut c_void) -> *mut GList;
    fn lightdm_user_get_name(user: *mut c_void) -> *const c_char;
    fn lightdm_user_get_display_name(user: *mut c_void) -> *const c_char;

    fn lightdm_get_can_suspend() -> gboolean;
    fn lightdm_get_can_restart() -> gboolean;
    fn lightdm_get_can_shutdown() -> gboolean;
    fn lightdm_suspend(error: *mut *mut GError) -> gboolean;
    fn lightdm_restart(error: *mut *mut GError) -> gboolean;
    fn lightdm_shutdown(error: *mut *mut GError) -> gboolean;
}

/// The connected greeter, or null when there is no LightDM to talk to — which
/// is the normal state when the binary is run from a desktop to preview a theme.
///
/// Everything that touches it runs on the thread that turns GLib's default main
/// context: LightDM's replies arrive on a watch attached to that context, and
/// Tauri dispatches commands from the same GTK loop.
static GREETER: AtomicPtr<Greeter> = AtomicPtr::new(ptr::null_mut());

/// The in-flight login. The password lives here only between the request and
/// the prompt that consumes it, because the prompt arrives as a callback with
/// no way to carry one in.
#[derive(Default)]
struct Conversation {
    /// Deliberately taken, not copied, by the prompt handler.
    password: Option<String>,
    /// `Some` once LightDM has ruled on the attempt.
    authenticated: Option<bool>,
}

static CONVERSATION: Mutex<Conversation> = Mutex::new(Conversation {
    password: None,
    authenticated: None,
});

/// Connect to the LightDM daemon. Failure is not fatal: the greeter still draws
/// and still previews themes, it just cannot log anyone in.
pub fn connect() {
    let greeter = unsafe { lightdm_greeter_new() };
    if greeter.is_null() {
        eprintln!("tauri-greeter: could not create a LightDM greeter");
        return;
    }

    unsafe {
        signal(greeter, c"show-prompt", on_show_prompt as *const ());
        signal(greeter, c"show-message", on_show_message as *const ());
        signal(
            greeter,
            c"authentication-complete",
            on_authentication_complete as *const (),
        );
    }

    if let Err(detail) = check(|error| unsafe {
        lightdm_greeter_connect_to_daemon_sync(greeter, error)
    }) {
        // Expected when run by hand; the journal says which it was.
        eprintln!("tauri-greeter: not connected to LightDM: {detail}");
        return;
    }
    GREETER.store(greeter, Ordering::Relaxed);
}

/// `GCallback` is one erased signature for every signal there is, so the real
/// handler has to be cast to it -- the marshaller uses the signal's own
/// signature to call back, which is why each handler above must match the one
/// LightDM documents for its signal exactly.
unsafe fn signal(greeter: *mut Greeter, name: &CStr, callback: *const ()) {
    gobject_sys::g_signal_connect_data(
        greeter.cast(),
        name.as_ptr(),
        Some(std::mem::transmute::<*const (), GCallback>(callback).unwrap_unchecked()),
        ptr::null_mut(),
        None,
        0,
    );
}

/// PAM is asking for something. The password is the only secret we hold; for
/// anything else (a hardware token, a second factor) an empty answer lets PAM
/// reject the attempt rather than leaving the greeter waiting forever.
unsafe extern "C" fn on_show_prompt(
    greeter: *mut Greeter,
    _text: *const c_char,
    prompt_type: c_int,
    _data: *mut c_void,
) {
    let answer = if prompt_type == PROMPT_SECRET {
        CONVERSATION
            .lock()
            .unwrap()
            .password
            .take()
            .unwrap_or_default()
    } else {
        String::new()
    };
    // A password with a NUL in it cannot be passed to C; an empty answer fails
    // the attempt, which is the right outcome for an unanswerable prompt.
    let answer = CString::new(answer).unwrap_or_default();
    let _ = check(|error| lightdm_greeter_respond(greeter, answer.as_ptr(), error));
}

/// PAM's own messages — "your password expires in 3 days", "account locked".
/// The journal gets them; the screen does not, because the screen is what an
/// attacker can see and these say which half of a guess was right.
unsafe extern "C" fn on_show_message(
    _greeter: *mut Greeter,
    text: *const c_char,
    _message_type: c_int,
    _data: *mut c_void,
) {
    eprintln!("tauri-greeter: pam: {}", string(text));
}

unsafe extern "C" fn on_authentication_complete(greeter: *mut Greeter, _data: *mut c_void) {
    let mut conversation = CONVERSATION.lock().unwrap();
    // A prompt that never came, or an attempt that failed before one: either
    // way the password has no further use.
    conversation.password = None;
    conversation.authenticated = Some(lightdm_greeter_get_is_authenticated(greeter) == GTRUE);
}

/// Authenticate, and on success ask LightDM to start `session`.
///
/// Returns only when LightDM has ruled. On success LightDM stops the greeter
/// itself once the session is up, so there is nothing left to do here.
pub fn login(username: &str, password: String, session: &str) -> Result<(), String> {
    let greeter = GREETER.load(Ordering::Relaxed);
    if greeter.is_null() {
        return Err("Login is unavailable.".to_string());
    }
    let username = CString::new(username).map_err(|_| DENIED.to_string())?;
    let session = CString::new(session).map_err(|_| DENIED.to_string())?;

    {
        let mut conversation = CONVERSATION.lock().unwrap();
        conversation.password = Some(password);
        conversation.authenticated = None;
    }

    check(|error| unsafe { lightdm_greeter_authenticate(greeter, username.as_ptr(), error) })
        .map_err(|detail| {
            eprintln!("tauri-greeter: authenticate: {detail}");
            "Login is unavailable.".to_string()
        })?;

    match wait_for_verdict(greeter) {
        Some(true) => {}
        Some(false) => return Err(DENIED.to_string()),
        None => {
            eprintln!("tauri-greeter: LightDM did not answer within {AUTH_TIMEOUT:?}");
            return Err("Login is unavailable.".to_string());
        }
    }

    check(|error| unsafe {
        lightdm_greeter_start_session_sync(greeter, session.as_ptr(), error)
    })
    .map_err(|detail| {
        eprintln!("tauri-greeter: start session: {detail}");
        "That session could not be started.".to_string()
    })
}

/// Turn the main context until the `authentication-complete` handler has run.
///
/// The reply arrives on a GLib watch that `liblightdm` attaches to the *default*
/// main context. Tauri dispatches commands from that same context's thread, so
/// this is a re-entrant pump of a loop we are already inside — what
/// `gtk_dialog_run` does — and the handlers run on this thread, under us.
///
/// The iteration is non-blocking and paired with a short sleep on purpose:
/// blocking would give up the thread until some source fires, and nothing
/// guarantees one ever does if LightDM has gone away — which is the case
/// `AUTH_TIMEOUT` exists to escape. Each lock below is released before the
/// pump, so a handler firing inside it cannot deadlock against us.
fn wait_for_verdict(greeter: *mut Greeter) -> Option<bool> {
    let deadline = Instant::now() + AUTH_TIMEOUT;
    loop {
        if let Some(authenticated) = CONVERSATION.lock().unwrap().authenticated.take() {
            return Some(authenticated);
        }
        if Instant::now() >= deadline {
            // Leave PAM's side of the conversation closed, or the next attempt
            // is refused as one already in progress.
            let _ = check(|error| unsafe {
                lightdm_greeter_cancel_authentication(greeter, error)
            });
            CONVERSATION.lock().unwrap().password = None;
            return None;
        }
        unsafe { glib_sys::g_main_context_iteration(ptr::null_mut(), GFALSE) };
        sleep(PUMP_INTERVAL);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub name: String,
    /// LightDM's own display name: the GECOS full name, or the account name.
    pub display_name: String,
}

/// The accounts LightDM is willing to show, which is what `/etc/lightdm/users.conf`
/// (or AccountsService, where it is installed) says: no root, no service accounts,
/// no explicitly hidden ones.
pub fn users() -> Vec<User> {
    let mut users: Vec<User> = unsafe {
        collect(lightdm_user_list_get_users(lightdm_user_list_get_instance()), |user| User {
            name: string(lightdm_user_get_name(user)),
            display_name: string(lightdm_user_get_display_name(user)),
        })
    };
    users.retain(|user| !user.name.is_empty());
    users.sort_by(|a, b| a.name.cmp(&b.name));
    users
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// LightDM's session key — the `.desktop` file stem. What `login` takes.
    pub key: String,
    pub name: String,
    pub wayland: bool,
}

/// Desktop sessions LightDM can start, Wayland first then X11, alphabetical
/// within each. An empty list is a normal result, not an error.
pub fn sessions() -> Vec<Session> {
    let mut sessions: Vec<Session> = unsafe {
        collect(lightdm_get_sessions(), |session| Session {
            key: string(lightdm_session_get_key(session)),
            name: string(lightdm_session_get_name(session)),
            wayland: string(lightdm_session_get_session_type(session)) == "wayland",
        })
    };
    sessions.retain(|session| !session.key.is_empty() && !session.name.is_empty());
    sessions.sort_by(|a, b| (!a.wayland, &a.name).cmp(&(!b.wayland, &b.name)));
    sessions
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Suspend,
    Reboot,
    Shutdown,
}

impl Action {
    /// What logind says it would do if asked, via the same bus and the same
    /// polkit subject the action itself uses. Asking first turns "the button
    /// did nothing" into a message, and costs one D-Bus round trip.
    fn available(self) -> bool {
        GTRUE
            == unsafe {
                match self {
                    Action::Suspend => lightdm_get_can_suspend(),
                    Action::Reboot => lightdm_get_can_restart(),
                    Action::Shutdown => lightdm_get_can_shutdown(),
                }
            }
    }
}

/// Hand the request to LightDM, which applies its own polkit rules. Doing it
/// this way rather than calling `systemctl` means the greeter needs no
/// privileges of its own and LightDM gets to stop its seats cleanly first.
pub fn power(action: Action) -> Result<(), String> {
    if !action.available() {
        eprintln!("tauri-greeter: {action:?}: refused by logind or polkit");
        return Err("That is not available.".to_string());
    }
    check(|error| unsafe {
        match action {
            Action::Suspend => lightdm_suspend(error),
            Action::Reboot => lightdm_restart(error),
            Action::Shutdown => lightdm_shutdown(error),
        }
    })
    .map_err(|detail| {
        eprintln!("tauri-greeter: {action:?}: {detail}");
        "That is not available.".to_string()
    })
}

/// Call a `gboolean`-returning LightDM function, turning its `GError` out-param
/// into a message and freeing it.
fn check(call: impl FnOnce(*mut *mut GError) -> gboolean) -> Result<(), String> {
    let mut error: *mut GError = ptr::null_mut();
    if call(&mut error) == GTRUE {
        return Ok(());
    }
    let message = if error.is_null() {
        "failed".to_string()
    } else {
        unsafe { string((*error).message) }
    };
    if !error.is_null() {
        unsafe { glib_sys::g_error_free(error) };
    }
    Err(message)
}

/// Walk a `GList` LightDM owns. The elements are borrowed — the library keeps
/// the list and the objects on it — so nothing here is freed.
unsafe fn collect<T>(list: *mut GList, mut item: impl FnMut(*mut c_void) -> T) -> Vec<T> {
    let mut items = Vec::new();
    let mut node = list;
    while !node.is_null() {
        if !(*node).data.is_null() {
            items.push(item((*node).data));
        }
        node = (*node).next;
    }
    items
}

unsafe fn string(value: *const c_char) -> String {
    if value.is_null() {
        String::new()
    } else {
        CStr::from_ptr(value).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The UI sends these three names; a rename on either side would mean a
    /// power button that silently does nothing.
    #[test]
    fn deserializes_action_names_sent_by_the_ui() {
        for (json, expected) in [
            ("\"suspend\"", "Suspend"),
            ("\"reboot\"", "Reboot"),
            ("\"shutdown\"", "Shutdown"),
        ] {
            let action: Action = serde_json::from_str(json).unwrap();
            assert_eq!(format!("{action:?}"), expected);
        }
        assert!(serde_json::from_str::<Action>("\"hibernate\"").is_err());
    }

    fn session(key: &str, name: &str, wayland: bool) -> Session {
        Session {
            key: key.to_string(),
            name: name.to_string(),
            wayland,
        }
    }

    /// The UI defaults to the first entry, so the order is the default session.
    #[test]
    fn sorts_wayland_first_then_alphabetically() {
        let mut sessions = [
            session("gnome-xorg", "GNOME on Xorg", false),
            session("plasma", "KDE Plasma", true),
            session("awesome", "Awesome", false),
            session("hyprland", "Hyprland", true),
        ];
        sessions.sort_by(|a, b| (!a.wayland, &a.name).cmp(&(!b.wayland, &b.name)));

        let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Hyprland", "KDE Plasma", "Awesome", "GNOME on Xorg"]);
    }

    /// Reads whatever this machine has installed; the point is that the FFI
    /// round-trips without a crash and that keys and names come back non-empty,
    /// since an empty key is a session LightDM cannot start.
    #[test]
    fn reads_sessions_from_lightdm() {
        for session in sessions() {
            assert!(!session.key.is_empty());
            assert!(!session.name.is_empty());
        }
    }

    /// Same: exercises the GList walk and the display-name fallback. Root and
    /// service accounts must not appear — that is LightDM's `users.conf` rule,
    /// and this greeter is trusting it.
    #[test]
    fn reads_users_from_lightdm() {
        for user in users() {
            assert!(!user.name.is_empty());
            assert!(!user.display_name.is_empty());
            assert_ne!(user.name, "root");
        }
    }

    /// `check` has to free the GError and still produce a message; a leak or a
    /// double free here would only show up under a real failure.
    #[test]
    fn check_reports_and_frees_the_error() {
        let ok = check(|_| GTRUE);
        assert!(ok.is_ok());

        let failed = check(|error| unsafe {
            *error = glib_sys::g_error_new_literal(
                glib_sys::g_quark_from_static_string(c"test".as_ptr()),
                1,
                c"no such thing".as_ptr(),
            );
            GFALSE
        });
        assert_eq!(failed.unwrap_err(), "no such thing");

        // A function that fails without setting an error still has to say so.
        assert_eq!(check(|_| GFALSE).unwrap_err(), "failed");
    }

    /// The one part of the power path that can be exercised without actually
    /// powering the machine off: same library, same system bus, same polkit
    /// subject — only the final verb differs. If this cannot reach logind,
    /// neither can `power`.
    #[test]
    fn asks_logind_what_it_is_allowed_to_do() {
        // A machine may legitimately refuse any of these; what matters is that
        // the query completes rather than hanging or aborting.
        for action in [Action::Suspend, Action::Reboot, Action::Shutdown] {
            let answer = action.available();
            eprintln!("{action:?}: {answer}");
        }
    }

    /// Without a LightDM to talk to, a login attempt must fail rather than
    /// dereference a null greeter.
    #[test]
    fn login_without_a_daemon_is_unavailable() {
        assert!(GREETER.load(Ordering::Relaxed).is_null());
        assert_eq!(
            login("brian", "hunter2".into(), "plasma").unwrap_err(),
            "Login is unavailable."
        );
    }
}

