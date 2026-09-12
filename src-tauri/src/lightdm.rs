// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

//! The LightDM side of the greeter.
//!
//! LightDM owns the VT, the X server, PAM, privilege dropping and starting the
//! desktop. What is left for the greeter is a conversation over the pipe pair
//! LightDM hands it on startup, which `liblightdm-gobject` already speaks — so
//! this module is bindings to it plus the forwarding that turns that
//! conversation into events the UI can answer.
//!
//! Nothing here blocks on the conversation. LightDM's prompts arrive as GLib
//! signals on the main context Tauri already turns, each is re-emitted as a
//! Tauri event, and the UI answers with `respond`. That is what lets a PAM
//! stack ask for more than one thing — a token, a second factor, a new password
//! for an expired one.

use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::OnceLock;

use glib::ffi::{gboolean, GError, GList, GTRUE};
use glib::prelude::ObjectType;
use glib::translate::{from_glib_full, FromGlibPtrContainer};
use glib::Object;
use tauri::{AppHandle, Emitter};

/// Opaque `LightDMGreeter`. Only ever handed back to the library.
#[repr(C)]
struct Greeter {
    _private: [u8; 0],
}

/// `LightDMPromptType`: the prompt is for something that must not be echoed.
const PROMPT_SECRET: c_int = 1;
/// `LightDMMessageType`: PAM is reporting a problem, not just informing.
const MESSAGE_ERROR: c_int = 1;

/// The username the UI sends to log in without an account, matching LightDM's
/// own spelling of it in `select-user-hint`.
pub const GUEST: &str = "*guest";

/// Said whenever LightDM cannot be reached or will not act. Never a verdict on
/// a password.
const UNAVAILABLE: &str = "Login is unavailable.";

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
    fn lightdm_greeter_authenticate_as_guest(
        greeter: *mut Greeter,
        error: *mut *mut GError,
    ) -> gboolean;
    fn lightdm_greeter_authenticate_autologin(
        greeter: *mut Greeter,
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
    fn lightdm_greeter_cancel_autologin(greeter: *mut Greeter);
    fn lightdm_greeter_get_is_authenticated(greeter: *mut Greeter) -> gboolean;
    fn lightdm_greeter_get_authentication_user(greeter: *mut Greeter) -> *const c_char;
    fn lightdm_greeter_set_language(
        greeter: *mut Greeter,
        language: *const c_char,
        error: *mut *mut GError,
    ) -> gboolean;
    fn lightdm_greeter_start_session_sync(
        greeter: *mut Greeter,
        session: *const c_char,
        error: *mut *mut GError,
    ) -> gboolean;

    fn lightdm_greeter_get_hide_users_hint(greeter: *mut Greeter) -> gboolean;
    fn lightdm_greeter_get_show_manual_login_hint(greeter: *mut Greeter) -> gboolean;
    fn lightdm_greeter_get_lock_hint(greeter: *mut Greeter) -> gboolean;
    fn lightdm_greeter_get_has_guest_account_hint(greeter: *mut Greeter) -> gboolean;
    fn lightdm_greeter_get_select_user_hint(greeter: *mut Greeter) -> *const c_char;
    fn lightdm_greeter_get_select_guest_hint(greeter: *mut Greeter) -> gboolean;
    fn lightdm_greeter_get_default_session_hint(greeter: *mut Greeter) -> *const c_char;
    fn lightdm_greeter_get_autologin_user_hint(greeter: *mut Greeter) -> *const c_char;
    fn lightdm_greeter_get_autologin_session_hint(greeter: *mut Greeter) -> *const c_char;
    fn lightdm_greeter_get_autologin_guest_hint(greeter: *mut Greeter) -> gboolean;
    fn lightdm_greeter_get_autologin_timeout_hint(greeter: *mut Greeter) -> c_int;

    fn lightdm_get_sessions() -> *mut GList;
    fn lightdm_session_get_key(session: *mut c_void) -> *const c_char;
    fn lightdm_session_get_name(session: *mut c_void) -> *const c_char;
    fn lightdm_session_get_session_type(session: *mut c_void) -> *const c_char;

    fn lightdm_user_list_get_instance() -> *mut c_void;
    fn lightdm_user_list_get_users(list: *mut c_void) -> *mut GList;
    fn lightdm_user_get_name(user: *mut c_void) -> *const c_char;
    fn lightdm_user_get_display_name(user: *mut c_void) -> *const c_char;
    fn lightdm_user_get_image(user: *mut c_void) -> *const c_char;
    fn lightdm_user_get_session(user: *mut c_void) -> *const c_char;
    fn lightdm_user_get_language(user: *mut c_void) -> *const c_char;
    fn lightdm_user_get_layout(user: *mut c_void) -> *const c_char;
    fn lightdm_user_get_logged_in(user: *mut c_void) -> gboolean;

    fn lightdm_get_layouts() -> *mut GList;
    fn lightdm_get_layout() -> *mut c_void;
    fn lightdm_set_layout(layout: *mut c_void);
    fn lightdm_layout_get_name(layout: *mut c_void) -> *const c_char;
    fn lightdm_layout_get_description(layout: *mut c_void) -> *const c_char;

    fn lightdm_get_languages() -> *mut GList;
    fn lightdm_get_language() -> *mut c_void;
    fn lightdm_language_get_code(language: *mut c_void) -> *const c_char;
    fn lightdm_language_get_name(language: *mut c_void) -> *const c_char;
    fn lightdm_language_get_territory(language: *mut c_void) -> *const c_char;

    fn lightdm_get_hostname() -> *const c_char;
    fn lightdm_get_os_pretty_name() -> *const c_char;

    fn lightdm_get_can_suspend() -> gboolean;
    fn lightdm_get_can_hibernate() -> gboolean;
    fn lightdm_get_can_restart() -> gboolean;
    fn lightdm_get_can_shutdown() -> gboolean;
    fn lightdm_suspend(error: *mut *mut GError) -> gboolean;
    fn lightdm_hibernate(error: *mut *mut GError) -> gboolean;
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

/// Set once, at startup, so the signal handlers have somewhere to send what
/// LightDM says. Absent in the unit tests, where nothing is emitted.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Connect to the LightDM daemon. Failure is not fatal: the greeter still draws
/// and still previews themes, it just cannot log anyone in.
pub fn connect(app: AppHandle) {
    let _ = APP.set(app);

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
        signal(
            greeter,
            c"autologin-timer-expired",
            on_autologin_timer_expired as *const (),
        );
    }

    if let Err(detail) =
        check(|error| unsafe { lightdm_greeter_connect_to_daemon_sync(greeter, error) })
    {
        // Expected when run by hand; the journal says which it was.
        eprintln!("tauri-greeter: not connected to LightDM: {detail}");
        return;
    }
    GREETER.store(greeter, Ordering::Relaxed);
}

/// `GCallback` is one erased signature for every signal there is, and the C API
/// takes nothing else -- the marshaller calls back using the signal's own
/// signature, so each handler must match the one LightDM documents for its
/// signal exactly. Nothing but that documentation checks this, which is why the
/// four handlers are the only `extern "C"` functions here.
unsafe fn signal(greeter: *mut Greeter, name: &CStr, callback: *const ()) {
    glib::gobject_ffi::g_signal_connect_data(
        greeter.cast(),
        name.as_ptr(),
        Some(std::mem::transmute::<*const (), unsafe extern "C" fn()>(
            callback,
        )),
        ptr::null_mut(),
        None,
        0,
    );
}

/// Everything LightDM says during a login goes to the UI as one of these. The
/// UI answers a `prompt` with `respond` and nothing else; it never decides for
/// itself that an attempt succeeded.
#[derive(Serialize, Clone)]
struct Prompt {
    text: String,
    /// A password, a PIN, a token code: type it into a field that does not echo.
    secret: bool,
}

#[derive(Serialize, Clone)]
struct Notice {
    text: String,
    error: bool,
}

#[derive(Serialize, Clone)]
struct Complete {
    authenticated: bool,
    /// Who LightDM ruled on, so a reply for an abandoned attempt can be ignored.
    user: String,
}

fn emit(event: &str, payload: impl Serialize + Clone) {
    if let Some(app) = APP.get() {
        if let Err(error) = app.emit(event, payload) {
            eprintln!("tauri-greeter: emit {event}: {error}");
        }
    }
}

/// PAM is asking for something. Which prompt this is, and how many more follow,
/// is the PAM stack's business — the greeter just shows each one and sends back
/// what was typed.
unsafe extern "C" fn on_show_prompt(
    _greeter: *mut Greeter,
    text: *const c_char,
    prompt_type: c_int,
    _data: *mut c_void,
) {
    emit(
        "prompt",
        Prompt {
            text: string(text),
            secret: prompt_type == PROMPT_SECRET,
        },
    );
}

/// PAM's own messages — "your password expires in 3 days", "you must change
/// your password now". These are shown: an expired-password change cannot be
/// completed by someone who is not told what the prompts mean. They are also
/// journalled, since the screen is gone the moment the session starts.
unsafe extern "C" fn on_show_message(
    _greeter: *mut Greeter,
    text: *const c_char,
    message_type: c_int,
    _data: *mut c_void,
) {
    let text = string(text);
    eprintln!("tauri-greeter: pam: {text}");
    emit(
        "message",
        Notice {
            text,
            error: message_type == MESSAGE_ERROR,
        },
    );
}

unsafe extern "C" fn on_authentication_complete(greeter: *mut Greeter, _data: *mut c_void) {
    emit(
        "complete",
        Complete {
            authenticated: lightdm_greeter_get_is_authenticated(greeter) == GTRUE,
            user: string(lightdm_greeter_get_authentication_user(greeter)),
        },
    );
}

/// LightDM's autologin countdown ran out with nobody at the keyboard. The UI
/// answers by calling `authenticate_autologin`; doing it there rather than here
/// keeps the decision in one place with the "any key cancels" handling.
unsafe extern "C" fn on_autologin_timer_expired(greeter: *mut Greeter, _data: *mut c_void) {
    emit(
        "autologin",
        string(lightdm_greeter_get_autologin_user_hint(greeter)),
    );
}

fn greeter() -> Result<*mut Greeter, String> {
    let greeter = GREETER.load(Ordering::Relaxed);
    if greeter.is_null() {
        return Err(UNAVAILABLE.to_string());
    }
    Ok(greeter)
}

/// Report a failure to the journal and hand the UI the one thing it may say.
fn unavailable(what: &str, detail: String) -> String {
    eprintln!("tauri-greeter: {what}: {detail}");
    UNAVAILABLE.to_string()
}

/// Begin authenticating `username`, or the guest account for [`GUEST`].
///
/// Starting a second one replaces the first: `liblightdm` stamps each attempt
/// with a sequence number and drops prompts and verdicts belonging to an
/// earlier one, so switching users mid-conversation needs no cancel first.
pub fn authenticate(username: &str) -> Result<(), String> {
    let greeter = greeter()?;
    if username == GUEST {
        return check(|error| unsafe { lightdm_greeter_authenticate_as_guest(greeter, error) })
            .map_err(|detail| unavailable("authenticate as guest", detail));
    }
    let username = CString::new(username).map_err(|_| UNAVAILABLE.to_string())?;
    check(|error| unsafe { lightdm_greeter_authenticate(greeter, username.as_ptr(), error) })
        .map_err(|detail| unavailable("authenticate", detail))
}

/// Log in the configured autologin account, after LightDM's timer expired.
pub fn authenticate_autologin() -> Result<(), String> {
    let greeter = greeter()?;
    check(|error| unsafe { lightdm_greeter_authenticate_autologin(greeter, error) })
        .map_err(|detail| unavailable("authenticate autologin", detail))
}

/// Answer the prompt that is showing. The answer is spent here and kept
/// nowhere: it is moved into a `CString`, handed to LightDM, and dropped.
pub fn respond(answer: String) -> Result<(), String> {
    let greeter = greeter()?;
    // A NUL cannot cross into C. Nothing typeable produces one, so this is a
    // malformed request rather than a wrong password.
    let answer = CString::new(answer).map_err(|_| UNAVAILABLE.to_string())?;
    check(|error| unsafe { lightdm_greeter_respond(greeter, answer.as_ptr(), error) })
        .map_err(|detail| unavailable("respond", detail))
}

/// Abandon the conversation in flight, so PAM's side is closed rather than left
/// open until it times out.
pub fn cancel() -> Result<(), String> {
    let greeter = greeter()?;
    check(|error| unsafe { lightdm_greeter_cancel_authentication(greeter, error) })
        .map_err(|detail| unavailable("cancel", detail))
}

/// Stop LightDM's autologin timer. Anything the person at the keyboard does
/// means they are here, so the countdown should not fire behind them.
pub fn cancel_autologin() {
    if let Ok(greeter) = greeter() {
        unsafe { lightdm_greeter_cancel_autologin(greeter) };
    }
}

/// Hand the authenticated session over to LightDM, which drops privileges and
/// execs the desktop, then stops the greeter. An empty `session` means
/// LightDM's own default; an empty `language` leaves the session's locale alone.
pub fn start_session(session: &str, language: &str) -> Result<(), String> {
    let greeter = greeter()?;

    if !language.is_empty() {
        match CString::new(language) {
            // Not fatal: a session in the wrong locale beats no session.
            Ok(language) => {
                if let Err(detail) = check(|error| unsafe {
                    lightdm_greeter_set_language(greeter, language.as_ptr(), error)
                }) {
                    eprintln!("tauri-greeter: set language: {detail}");
                }
            }
            Err(_) => eprintln!("tauri-greeter: set language: rejected"),
        }
    }

    let session = CString::new(session).map_err(|_| UNAVAILABLE.to_string())?;
    // NULL, not "", is how LightDM is asked for the seat's own default session.
    // The `CString` has to outlive the call, so the pointer is a separate name.
    let chosen = if session.as_bytes().is_empty() {
        ptr::null()
    } else {
        session.as_ptr()
    };
    check(|error| unsafe { lightdm_greeter_start_session_sync(greeter, chosen, error) }).map_err(
        |detail| {
            eprintln!("tauri-greeter: start session: {detail}");
            "That session could not be started.".to_string()
        },
    )
}

/// What LightDM told the greeter about the seat before it drew anything: who to
/// preselect, whether accounts may be listed at all, whether an autologin is
/// counting down. Ignoring these is what makes a greeter feel like it forgot
/// everything between boots — LightDM remembers, the greeter has to ask.
#[derive(Serialize, Default)]
pub struct Hints {
    /// False when there is no LightDM: the UI still draws, but cannot log in.
    pub connected: bool,
    /// The seat forbids listing accounts; the username must be typed.
    pub hide_users: bool,
    /// Offer a "type a username" entry alongside the listed accounts.
    pub show_manual_login: bool,
    /// The session is locked behind this greeter rather than absent.
    pub lock: bool,
    pub has_guest_account: bool,
    /// The account LightDM wants selected — the last one to log in, usually.
    pub select_user: String,
    pub select_guest: bool,
    pub default_session: String,
    pub autologin_user: String,
    pub autologin_session: String,
    pub autologin_guest: bool,
    /// Seconds before LightDM logs the autologin account in by itself.
    pub autologin_timeout: i32,
    pub hostname: String,
    pub os: String,
}

pub fn hints() -> Hints {
    let mut hints = Hints {
        hostname: unsafe { string(lightdm_get_hostname()) },
        os: unsafe { string(lightdm_get_os_pretty_name()) },
        ..Hints::default()
    };
    let Ok(greeter) = greeter() else {
        return hints;
    };
    unsafe {
        hints.connected = true;
        hints.hide_users = lightdm_greeter_get_hide_users_hint(greeter) == GTRUE;
        hints.show_manual_login = lightdm_greeter_get_show_manual_login_hint(greeter) == GTRUE;
        hints.lock = lightdm_greeter_get_lock_hint(greeter) == GTRUE;
        hints.has_guest_account = lightdm_greeter_get_has_guest_account_hint(greeter) == GTRUE;
        hints.select_user = string(lightdm_greeter_get_select_user_hint(greeter));
        hints.select_guest = lightdm_greeter_get_select_guest_hint(greeter) == GTRUE;
        hints.default_session = string(lightdm_greeter_get_default_session_hint(greeter));
        hints.autologin_user = string(lightdm_greeter_get_autologin_user_hint(greeter));
        hints.autologin_session = string(lightdm_greeter_get_autologin_session_hint(greeter));
        hints.autologin_guest = lightdm_greeter_get_autologin_guest_hint(greeter) == GTRUE;
        hints.autologin_timeout = lightdm_greeter_get_autologin_timeout_hint(greeter);
    }
    hints
}

#[derive(Serialize)]
pub struct User {
    pub name: String,
    /// LightDM's own display name: the GECOS full name, or the account name.
    pub display_name: String,
    /// Path to the account's picture, if it has one. `lib.rs` inlines it.
    pub image: String,
    /// The session this account used last, so it comes back preselected.
    pub session: String,
    pub language: String,
    pub layout: String,
    /// Already has a session running on another VT; logging in switches to it.
    pub logged_in: bool,
}

/// The accounts LightDM is willing to show, which is what `/etc/lightdm/users.conf`
/// (or AccountsService, where it is installed) says: no root, no service accounts,
/// no explicitly hidden ones.
pub fn users() -> Vec<User> {
    let list = unsafe { lightdm_user_list_get_instance() };
    if list.is_null() {
        eprintln!("tauri-greeter: LightDM returned no user list");
        return Vec::new();
    }
    let mut users: Vec<User> = collect(unsafe { lightdm_user_list_get_users(list) })
        .iter()
        .map(|user| {
            let user = user.as_ptr().cast::<c_void>();
            unsafe {
                User {
                    name: string(lightdm_user_get_name(user)),
                    display_name: string(lightdm_user_get_display_name(user)),
                    image: string(lightdm_user_get_image(user)),
                    session: string(lightdm_user_get_session(user)),
                    language: string(lightdm_user_get_language(user)),
                    layout: string(lightdm_user_get_layout(user)),
                    logged_in: lightdm_user_get_logged_in(user) == GTRUE,
                }
            }
        })
        .collect();
    users.retain(|user| !user.name.is_empty());
    users.sort_by(|a, b| a.name.cmp(&b.name));
    users
}

#[derive(Serialize)]
pub struct Session {
    /// LightDM's session key — the `.desktop` file stem. What `start_session` takes.
    pub key: String,
    /// LightDM's own name for it, shown as-is.
    pub name: String,
    /// Not displayed; this is what puts Wayland sessions first, and so decides
    /// the fallback when neither the user nor LightDM names one.
    pub wayland: bool,
}

/// Desktop sessions LightDM can start, Wayland first then X11, alphabetical
/// within each. An empty list is a normal result, not an error.
pub fn sessions() -> Vec<Session> {
    let mut sessions: Vec<Session> = collect(unsafe { lightdm_get_sessions() })
        .iter()
        .map(|session| {
            let session = session.as_ptr().cast::<c_void>();
            unsafe {
                Session {
                    key: string(lightdm_session_get_key(session)),
                    name: string(lightdm_session_get_name(session)),
                    wayland: string(lightdm_session_get_session_type(session)) == "wayland",
                }
            }
        })
        .collect();
    sessions.retain(|session| !session.key.is_empty() && !session.name.is_empty());
    sessions.sort_by(|a, b| (!a.wayland, &a.name).cmp(&(!b.wayland, &b.name)));
    sessions
}

#[derive(Serialize)]
pub struct Layout {
    /// The XKB name, `gb` or `de neo`. What `set_layout` takes.
    pub name: String,
    pub description: String,
}

/// The keyboard layouts this machine has. Without a picker, anyone whose
/// password contains a character the default layout cannot produce is locked
/// out of their own machine — which is why this is not an optional extra.
pub fn layouts() -> Vec<Layout> {
    let mut layouts: Vec<Layout> = collect(unsafe { lightdm_get_layouts() })
        .iter()
        .map(|layout| {
            let layout = layout.as_ptr().cast::<c_void>();
            unsafe {
                Layout {
                    name: string(lightdm_layout_get_name(layout)),
                    description: string(lightdm_layout_get_description(layout)),
                }
            }
        })
        .collect();
    layouts.retain(|layout| !layout.name.is_empty());
    layouts.sort_by(|a, b| a.description.cmp(&b.description));
    layouts
}

pub fn layout() -> String {
    let layout = unsafe { lightdm_get_layout() };
    if layout.is_null() {
        return String::new();
    }
    unsafe { string(lightdm_layout_get_name(layout)) }
}

/// Switch the greeter's own keyboard, so what is typed next matches the picker.
/// LightDM carries the choice into the session it starts.
pub fn set_layout(name: &str) {
    for candidate in collect(unsafe { lightdm_get_layouts() }) {
        let candidate = candidate.as_ptr().cast::<c_void>();
        if unsafe { string(lightdm_layout_get_name(candidate)) } == name {
            unsafe { lightdm_set_layout(candidate) };
            return;
        }
    }
    eprintln!("tauri-greeter: no such layout: {name}");
}

#[derive(Serialize)]
pub struct Language {
    /// The locale, `en_GB.UTF-8`. What `start_session` passes to LightDM.
    pub code: String,
    /// "English", plus the territory where the language has more than one.
    pub name: String,
}

/// The locales this machine has, for the session about to start. LightDM builds
/// this list itself from the system's locale database.
pub fn languages() -> Vec<Language> {
    let mut languages: Vec<Language> = collect(unsafe { lightdm_get_languages() })
        .iter()
        .map(|language| {
            let language = language.as_ptr().cast::<c_void>();
            unsafe { describe_language(language) }
        })
        .collect();
    languages.retain(|language| !language.code.is_empty() && !language.name.is_empty());
    languages.sort_by(|a, b| a.name.cmp(&b.name));
    languages.dedup_by(|a, b| a.code == b.code);
    languages
}

pub fn language() -> String {
    let language = unsafe { lightdm_get_language() };
    if language.is_null() {
        return String::new();
    }
    unsafe { string(lightdm_language_get_code(language)) }
}

unsafe fn describe_language(language: *mut c_void) -> Language {
    let name = string(lightdm_language_get_name(language));
    let territory = string(lightdm_language_get_territory(language));
    Language {
        code: string(lightdm_language_get_code(language)),
        // "English" alone is ambiguous once en_GB and en_US are both installed.
        name: if territory.is_empty() {
            name
        } else {
            format!("{name} ({territory})")
        },
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Suspend,
    Hibernate,
    Reboot,
    Shutdown,
}

impl Action {
    /// What logind says it would do if asked, via the same bus and the same
    /// polkit subject the action itself uses. Asking first turns "the button
    /// did nothing" into a hidden button, and costs one D-Bus round trip.
    pub fn available(self) -> bool {
        GTRUE
            == unsafe {
                match self {
                    Action::Suspend => lightdm_get_can_suspend(),
                    Action::Hibernate => lightdm_get_can_hibernate(),
                    Action::Reboot => lightdm_get_can_restart(),
                    Action::Shutdown => lightdm_get_can_shutdown(),
                }
            }
    }

    pub const ALL: [Action; 4] = [
        Action::Suspend,
        Action::Hibernate,
        Action::Reboot,
        Action::Shutdown,
    ];
}

/// Which power actions this machine will actually perform, so the UI can leave
/// out the ones it would only have to refuse. A laptop without swap has no
/// hibernate; a locked-down seat may have none of them.
pub fn power_actions() -> Vec<Action> {
    Action::ALL
        .into_iter()
        .filter(|action| action.available())
        .collect()
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
            Action::Hibernate => lightdm_hibernate(error),
            Action::Reboot => lightdm_restart(error),
            Action::Shutdown => lightdm_shutdown(error),
        }
    })
    .map_err(|detail| {
        eprintln!("tauri-greeter: {action:?}: {detail}");
        "That is not available.".to_string()
    })
}

/// Call a `gboolean`-returning LightDM function and turn its `GError` out-param
/// into a message. The error is `(transfer full)`, so `glib::Error` takes it and
/// frees it on drop -- there is no `g_error_free` to forget here.
fn check(call: impl FnOnce(*mut *mut GError) -> gboolean) -> Result<(), String> {
    let mut error: *mut GError = ptr::null_mut();
    if call(&mut error) == GTRUE {
        return Ok(());
    }
    if error.is_null() {
        // A LightDM function may fail without setting one; it still has to say so.
        return Err("failed".to_string());
    }
    let error: glib::Error = unsafe { from_glib_full(error) };
    Err(error.to_string())
}

/// The GObjects on a `GList` LightDM owns. `glib` does the walk and takes a
/// reference on each element, so the list cannot be traversed wrongly here and
/// the objects stay alive for as long as the `Vec` does. `(transfer none)`: the
/// list itself stays LightDM's and is not freed.
fn collect(list: *mut GList) -> Vec<Object> {
    unsafe { FromGlibPtrContainer::from_glib_none(list) }
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

    /// The UI sends these four names; a rename on either side would mean a
    /// power button that silently does nothing.
    #[test]
    fn deserializes_action_names_sent_by_the_ui() {
        for (json, expected) in [
            ("\"suspend\"", "Suspend"),
            ("\"hibernate\"", "Hibernate"),
            ("\"reboot\"", "Reboot"),
            ("\"shutdown\"", "Shutdown"),
        ] {
            let action: Action = serde_json::from_str(json).unwrap();
            assert_eq!(format!("{action:?}"), expected);
        }
        assert!(serde_json::from_str::<Action>("\"halt\"").is_err());

        // And they round-trip: `power_actions` serialises them back out for the
        // UI to name in its next `power` call.
        for action in Action::ALL {
            let name = serde_json::to_string(&action).unwrap();
            assert_eq!(
                format!("{:?}", serde_json::from_str::<Action>(&name).unwrap()),
                format!("{action:?}")
            );
        }
    }

    fn session(key: &str, name: &str, wayland: bool) -> Session {
        Session {
            key: key.to_string(),
            name: name.to_string(),
            wayland,
        }
    }

    /// The fallback session is the first entry, used when neither the account's
    /// last session nor LightDM's default-session hint names one that exists.
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

    /// The layout and language lists are read straight out of the system's XKB
    /// and locale databases, which is a lot of GList and `const gchar *` for a
    /// picker that has to be right: a wrong keyboard means an unenterable
    /// password. Every entry must carry the key `set_layout` and
    /// `start_session` will be handed back.
    #[test]
    fn reads_layouts_and_languages_from_lightdm() {
        for layout in layouts() {
            assert!(!layout.name.is_empty());
        }
        for language in languages() {
            assert!(!language.code.is_empty());
            assert!(!language.name.is_empty());
        }
        // Whatever the machine is set to now has to be one of the offered
        // entries, or the picker opens showing something nobody chose.
        let current = layout();
        if !current.is_empty() {
            assert!(layouts().iter().any(|layout| layout.name == current));
        }
    }

    /// `check` has to free the GError and still produce a message; a leak or a
    /// double free here would only show up under a real failure.
    #[test]
    fn check_reports_and_frees_the_error() {
        let ok = check(|_| GTRUE);
        assert!(ok.is_ok());

        let failed = check(|error| unsafe {
            *error = glib::ffi::g_error_new_literal(
                glib::ffi::g_quark_from_static_string(c"test".as_ptr()),
                1,
                c"no such thing".as_ptr(),
            );
            glib::ffi::GFALSE
        });
        assert_eq!(failed.unwrap_err(), "no such thing");

        // A function that fails without setting an error still has to say so.
        assert_eq!(check(|_| glib::ffi::GFALSE).unwrap_err(), "failed");
    }

    /// The one part of the power path that can be exercised without actually
    /// powering the machine off: same library, same system bus, same polkit
    /// subject — only the final verb differs. If this cannot reach logind,
    /// neither can `power`.
    #[test]
    fn asks_logind_what_it_is_allowed_to_do() {
        // A machine may legitimately refuse any of these; what matters is that
        // the query completes rather than hanging or aborting, and that the
        // offered list never contains one that was refused.
        let offered = power_actions();
        for action in Action::ALL {
            let answer = action.available();
            eprintln!("{action:?}: {answer}");
            assert_eq!(
                answer,
                offered.iter().any(|o| format!("{o:?}") == format!("{action:?}"))
            );
        }
    }

    /// Without a LightDM to talk to, every step of the conversation must fail
    /// rather than dereference a null greeter — and say nothing about
    /// credentials while doing it.
    #[test]
    fn the_conversation_is_unavailable_without_a_daemon() {
        assert!(GREETER.load(Ordering::Relaxed).is_null());
        assert_eq!(authenticate("brian").unwrap_err(), UNAVAILABLE);
        assert_eq!(authenticate(GUEST).unwrap_err(), UNAVAILABLE);
        assert_eq!(authenticate_autologin().unwrap_err(), UNAVAILABLE);
        assert_eq!(respond("hunter2".into()).unwrap_err(), UNAVAILABLE);
        assert_eq!(cancel().unwrap_err(), UNAVAILABLE);
        assert_eq!(start_session("plasma", "en_GB.UTF-8").unwrap_err(), UNAVAILABLE);
        // Must not panic, and must not be reported: nothing was counting down.
        cancel_autologin();

        // The hints still describe the machine, so the UI can draw and explain
        // itself rather than showing an empty login form.
        let hints = hints();
        assert!(!hints.connected);
        assert!(!hints.hostname.is_empty());
    }
}
