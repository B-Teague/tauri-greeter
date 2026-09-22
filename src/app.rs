// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

//! The greeter UI.
//!
//! The login is a conversation, not a form submission: the backend forwards
//! whatever LightDM's PAM stack asks for as a `prompt` event, this shows it,
//! and the answer goes back with `respond`. One password is the common case,
//! not the only one — a token, a second factor or an expired password's "new
//! password"/"retype it" pair all arrive the same way and are answered the
//! same way, so nothing here has to know which PAM stack it is talking to.

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wasm_bindgen::prelude::*;

/// The theme the greeter starts on, picked from the installed stylesheets.
const DEFAULT_THEME: &str = "nebula";

/// The account picker's two entries that are not accounts. LightDM spells the
/// guest one this way itself, in `select-user-hint`.
const GUEST: &str = "*guest";
const OTHER: &str = "*other";

/// The only thing a rejected login is ever told, matching what the backend says
/// for every other failure it will not explain. PAM's own messages arrive
/// separately and say more, when the stack chose to.
const DENIED: &str = "Authentication failed.";

/// How long the greeter waits for LightDM to answer before giving the keyboard
/// back. Generous on purpose: PAM delays failures deliberately, and faillock or
/// a remote directory can stretch that. This is only a backstop against a
/// LightDM that has stopped replying, which would otherwise wedge the login
/// screen for the rest of the boot.
const REPLY_TIMEOUT: Duration = Duration::from_secs(120);

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch)]
    async fn listen(event: &str, handler: &JsValue) -> Result<JsValue, JsValue>;

    /// Delivered to every window, which is the point: the backdrop windows on
    /// the other monitors follow the login screen's theme picker this way.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch)]
    async fn emit(event: &str, payload: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "window"], js_name = getCurrentWindow, catch)]
    fn current_window() -> Result<JsValue, JsValue>;
}

/// True in the windows the backend opens on the non-primary monitors. They load
/// this same page, and draw the theme's background and nothing else.
pub fn is_backdrop() -> bool {
    current_window()
        .ok()
        .and_then(|window| js_sys::Reflect::get(&window, &"label".into()).ok())
        .and_then(|label| label.as_string())
        .is_some_and(|label| label.starts_with("backdrop"))
}

/// Calls a Tauri command, flattening JS and command errors into one message.
async fn call<T: DeserializeOwned>(cmd: &str, args: impl Serialize) -> Result<T, String> {
    let args = serde_wasm_bindgen::to_value(&args).map_err(|e| e.to_string())?;
    match invoke(cmd, args).await {
        Ok(value) => serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string()),
        Err(error) => Err(error.as_string().unwrap_or_else(|| format!("{cmd} failed"))),
    }
}

/// Subscribe to one backend event for the life of the greeter. Tauri wraps each
/// payload in an envelope; only the payload is of any interest here.
fn on<T: DeserializeOwned + 'static>(event: &'static str, mut handler: impl FnMut(T) + 'static) {
    spawn_local(async move {
        let closure = Closure::<dyn FnMut(JsValue)>::new(move |envelope: JsValue| {
            let payload = js_sys::Reflect::get(&envelope, &"payload".into())
                .unwrap_or(JsValue::UNDEFINED);
            match serde_wasm_bindgen::from_value::<T>(payload) {
                Ok(value) => handler(value),
                Err(error) => logged(event, error.to_string()),
            }
        });
        if let Err(error) = listen(event, closure.as_ref()).await {
            logged(event, format!("{error:?}"));
        }
        // Nothing ever unsubscribes: the greeter is torn down with the session.
        closure.forget();
    });
}

/// What LightDM told the greeter about this seat. Every default the greeter
/// picks -- which account, which session -- comes from here first.
#[derive(Clone, Default, Deserialize)]
struct Hints {
    connected: bool,
    hide_users: bool,
    show_manual_login: bool,
    lock: bool,
    has_guest_account: bool,
    select_user: String,
    select_guest: bool,
    default_session: String,
    autologin_user: String,
    autologin_session: String,
    autologin_guest: bool,
    autologin_timeout: i32,
    hostname: String,
    os: String,
}

#[derive(Clone, Deserialize)]
struct Account {
    name: String,
    display_name: String,
    /// A `data:` URI from LightDM's account picture, or empty for the fallback.
    picture: String,
    /// The session this account used last.
    session: String,
    language: String,
    layout: String,
    logged_in: bool,
}

#[derive(Clone, Deserialize)]
struct Session {
    /// LightDM's session key. This, not the display name, is what starts it.
    key: String,
    name: String,
}

#[derive(Clone, Deserialize)]
struct Layout {
    name: String,
    description: String,
}

#[derive(Clone, Deserialize)]
struct Language {
    code: String,
    name: String,
}

/// One thing PAM has asked for. `secret` is the only difference between a
/// password and a one-time code as far as this is concerned.
#[derive(Clone, Deserialize)]
struct Prompt {
    text: String,
    secret: bool,
}

/// Something PAM wanted to say — "your password expires in 3 days", "you must
/// change your password now". Shown, because the prompts that follow one of
/// these make no sense without it.
#[derive(Clone, Deserialize)]
struct Notice {
    text: String,
    error: bool,
}

#[derive(Clone, Deserialize)]
struct Complete {
    authenticated: bool,
    user: String,
}

#[derive(Serialize)]
struct Named {
    name: String,
}

#[derive(Serialize)]
struct Username {
    username: String,
}

#[derive(Serialize)]
struct Answer {
    answer: String,
}

#[derive(Serialize)]
struct Start {
    session: String,
    language: String,
}

#[derive(Serialize)]
struct PowerArgs {
    action: String,
}

#[component]
pub fn App() -> impl IntoView {
    let hints = RwSignal::new(Hints::default());
    let users = RwSignal::new(Vec::<Account>::new());
    let sessions = RwSignal::new(Vec::<Session>::new());
    let layouts = RwSignal::new(Vec::<Layout>::new());
    let languages = RwSignal::new(Vec::<Language>::new());
    let actions = RwSignal::new(Vec::<String>::new());
    let theme = RwSignal::new(String::new());
    let operator_theme = RwSignal::new(String::new());
    let theme_names = RwSignal::new(Vec::<String>::new());
    let theme_name = RwSignal::new(String::new());

    let username = RwSignal::new(String::new());
    let manual = RwSignal::new(String::new());
    let session = RwSignal::new(String::new());
    let layout = RwSignal::new(String::new());
    let language = RwSignal::new(String::new());

    let prompt = RwSignal::new(None::<Prompt>);
    let answer = RwSignal::new(String::new());
    let notices = RwSignal::new(Vec::<Notice>::new());
    let error = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let countdown = RwSignal::new(0i32);
    let answer_field = NodeRef::<html::Input>::new();
    let manual_field = NodeRef::<html::Input>::new();

    // Every request to LightDM is stamped with this and given a deadline. A
    // reply of any kind clears `busy`, so a deadline that finds it still set,
    // and its own stamp still current, is a LightDM that stopped answering.
    let attempt = StoredValue::new(0u32);
    // Set while the messages on screen belong to the attempt that was just
    // rejected. They stay up -- on an expired password they are the only
    // explanation there is -- until the next attempt has something of its own
    // to say, rather than piling up one failure's worth per try.
    let stale = StoredValue::new(false);
    let arm = move || {
        let stamp = attempt.get_value().wrapping_add(1);
        attempt.set_value(stamp);
        set_timeout(
            move || {
                if attempt.get_value() == stamp && busy.get_untracked() {
                    busy.set(false);
                    prompt.set(None);
                    error.set("Login is unavailable.".to_string());
                    spawn_local(async move {
                        let _ = call::<()>("cancel", ()).await;
                    });
                }
            },
            REPLY_TIMEOUT,
        );
    };

    // Who the conversation is for: an account, the guest session, or whatever
    // was typed into the manual-login field.
    let who = move || {
        let selected = username.get_untracked();
        if selected == OTHER {
            manual.get_untracked().trim().to_string()
        } else {
            selected
        }
    };

    // (Re)start the conversation. LightDM stamps each attempt with a sequence
    // number and drops prompts belonging to an earlier one, so this can be
    // called over a conversation already in flight -- which is what switching
    // account, or retrying after a rejection, does.
    let start_auth = move || {
        notices.set(Vec::new());
        stale.set_value(false);
        prompt.set(None);
        answer.set(String::new());
        let username = who();
        if username.is_empty() {
            // Nothing to authenticate yet: the manual field is empty.
            busy.set(false);
            return;
        }
        busy.set(true);
        arm();
        spawn_local(async move {
            if let Err(message) = call::<()>("authenticate", Username { username }).await {
                error.set(message);
                busy.set(false);
            }
        });
    };

    let stop_countdown = move || {
        if countdown.get_untracked() > 0 {
            countdown.set(0);
            spawn_local(async move {
                let _ = call::<()>("cancel_autologin", ()).await;
            });
        }
    };

    // Listeners go up before the first `authenticate`, or the prompt it answers
    // with arrives before anything is watching for it.
    on::<Prompt>("prompt", move |incoming| {
        busy.set(false);
        answer.set(String::new());
        prompt.set(Some(incoming));
        if let Some(field) = answer_field.get_untracked() {
            let _ = field.focus();
        }
    });
    on::<Notice>("message", move |notice| {
        if stale.get_value() {
            stale.set_value(false);
            notices.set(vec![notice]);
        } else {
            notices.update(|notices| notices.push(notice));
        }
    });
    on::<Complete>("complete", move |complete| {
        if !complete.authenticated {
            logged("complete", format!("rejected: {}", complete.user));
            error.set(DENIED.to_string());
            // Straight back to a live prompt, so the next attempt needs no
            // clicking. The messages PAM sent stay up: on an expired password
            // they are the only explanation of what went wrong.
            let kept = notices.get_untracked();
            start_auth();
            notices.set(kept);
            stale.set_value(true);
            return;
        }
        // LightDM stops the greeter once the session is up, so on the happy
        // path nothing below this runs again.
        busy.set(true);
        arm();
        let handoff = Start {
            session: session.get_untracked(),
            language: language.get_untracked(),
        };
        spawn_local(async move {
            if let Err(message) = call::<()>("start_session", handoff).await {
                error.set(message);
                busy.set(false);
            }
        });
    });
    on::<String>("autologin", move |user| {
        countdown.set(0);
        logged("autologin", format!("timer expired for {user}"));
        // The autologin account gets the session LightDM configured for it,
        // not whatever the picker happens to be showing.
        let configured = hints.get_untracked().autologin_session;
        if !configured.is_empty() {
            session.set(configured);
        }
        busy.set(true);
        arm();
        spawn_local(async move {
            if let Err(message) = call::<()>("authenticate_autologin", ()).await {
                error.set(message);
                busy.set(false);
            }
        });
    });

    // Everything the account picker's defaults depend on, in one pass: the
    // hints say who to preselect, the accounts say what that person used last.
    spawn_local(async move {
        match call::<Hints>("hints", ()).await {
            Ok(loaded) => hints.set(loaded),
            Err(message) => logged("hints", message),
        }
        match call::<Vec<Session>>("sessions", ()).await {
            Ok(loaded) => sessions.set(loaded),
            Err(message) => logged("sessions", message),
        }
        // A failure leaves the list empty: the prompt still has to draw,
        // because an empty list is also what a locked-down users.conf and a
        // hide-users seat both look like.
        match call::<Vec<Account>>("users", ()).await {
            Ok(loaded) => users.set(loaded),
            Err(message) => logged("users", message),
        }
        match call::<String>("layout", ()).await {
            Ok(current) => layout.set(current),
            Err(message) => logged("layout", message),
        }
        match call::<String>("language", ()).await {
            Ok(current) => language.set(current),
            Err(message) => logged("language", message),
        }

        let hints = hints.get_untracked();
        let known = |name: &str| users.get_untracked().iter().any(|user| user.name == name);
        username.set(match () {
            // A seat that hides its accounts has nothing to preselect.
            _ if hints.hide_users => OTHER.to_string(),
            _ if hints.select_guest && hints.has_guest_account => GUEST.to_string(),
            // LightDM's own memory of who logged in here last.
            _ if known(&hints.select_user) => hints.select_user.clone(),
            _ => users
                .get_untracked()
                .first()
                .map(|user| user.name.clone())
                .unwrap_or_else(|| OTHER.to_string()),
        });
        prefer(username.get_untracked(), users, sessions, &hints, session, layout, language);

        // Autologin is LightDM's countdown; this only mirrors it on screen and
        // offers a way out. LightDM fires `autologin` when it runs out.
        if hints.autologin_timeout > 0 && (!hints.autologin_user.is_empty() || hints.autologin_guest)
        {
            countdown.set(hints.autologin_timeout);
            set_interval(
                move || {
                    if countdown.get_untracked() > 0 {
                        countdown.update(|left| *left -= 1);
                    }
                },
                Duration::from_secs(1),
            );
        }

        start_auth();
    });

    spawn_local(async move {
        match call::<Vec<Layout>>("layouts", ()).await {
            Ok(loaded) => layouts.set(loaded),
            Err(message) => logged("layouts", message),
        }
    });
    spawn_local(async move {
        match call::<Vec<Language>>("languages", ()).await {
            Ok(loaded) => languages.set(loaded),
            Err(message) => logged("languages", message),
        }
    });
    // Only the actions logind says it would perform are offered: a machine with
    // no swap has no hibernate, and a button that can only apologise is worse
    // than no button.
    spawn_local(async move {
        match call::<Vec<String>>("power_actions", ()).await {
            Ok(loaded) => actions.set(loaded),
            Err(message) => logged("power_actions", message),
        }
    });
    spawn_local(async move {
        match call::<Vec<String>>("themes", ()).await {
            Ok(loaded) => {
                // Nothing installed means the picker is empty and the greeter
                // wears the stylesheet built into the binary.
                if loaded.iter().any(|name| name == DEFAULT_THEME) {
                    theme_name.set(DEFAULT_THEME.to_string());
                }
                theme_names.set(loaded);
            }
            Err(message) => logged("themes", message),
        }
    });
    // The operator's file, applied over whatever the picker holds -- it is an
    // override, not an entry, so it is fetched once and never switched away.
    spawn_local(async move {
        let name = String::new();
        match call::<String>("theme_css", Named { name }).await {
            Ok(css) => operator_theme.set(css),
            Err(message) => logged("theme_css", message),
        }
    });

    // Re-reads the stylesheet whenever the picker moves. It starts empty, for
    // the moment before the theme list has arrived; the built-in stylesheet is
    // what draws until then.
    Effect::new(move |_| {
        let name = theme_name.get();
        if name.is_empty() {
            return;
        }
        spawn_local(async move {
            // The other monitors are showing the same background; they fetch
            // the stylesheet themselves rather than have it copied over IPC.
            let _ = emit("theme", JsValue::from_str(&name)).await;
            match call::<String>("theme_css", Named { name }).await {
                // An unreadable stylesheet leaves the built-in one in place.
                Ok(css) => theme.set(css),
                Err(message) => logged("theme_css", message),
            }
        });
    });

    // Anyone touching the keyboard is present, so LightDM's countdown should
    // not fire behind them. Escape abandons a half-finished conversation and
    // starts a fresh one, which is the only way out of a PAM stack mid-way
    // through asking for something nobody can answer.
    window_event_listener(ev::keydown, move |event| {
        stop_countdown();
        if event.key() == "Escape" {
            error.set(String::new());
            start_auth();
        }
    });

    let select_user = move |name: String| {
        stop_countdown();
        error.set(String::new());
        username.set(name.clone());
        prefer(name.clone(), users, sessions, &hints.get_untracked(), session, layout, language);
        if name == OTHER {
            // Nothing to authenticate until a username is typed.
            prompt.set(None);
            notices.set(Vec::new());
            busy.set(false);
            spawn_local(async move {
                let _ = call::<()>("cancel", ()).await;
            });
            if let Some(field) = manual_field.get_untracked() {
                let _ = field.focus();
            }
        } else {
            start_auth();
        }
    };

    let respond = move || {
        if busy.get_untracked() || prompt.get_untracked().is_none() {
            return;
        }
        error.set(String::new());
        let answer_text = answer.get_untracked();
        answer.set(String::new());
        busy.set(true);
        arm();
        spawn_local(async move {
            if let Err(message) = call::<()>("respond", Answer { answer: answer_text }).await {
                error.set(message);
                busy.set(false);
            }
        });
    };

    // The field that matters is focused on arrival and after every reply.
    Effect::new(move |_| {
        if username.get() == OTHER {
            if let Some(field) = manual_field.get() {
                let _ = field.focus();
            }
        } else if let Some(field) = answer_field.get() {
            let _ = field.focus();
        }
    });

    let picture = move || {
        let name = username.get();
        users
            .get()
            .into_iter()
            .find(|user| user.name == name && !user.picture.is_empty())
            .map(|user| user.picture)
    };
    let placeholder = move || match prompt.get() {
        // PAM writes "Password: "; the colon is the label's job, not the text's.
        Some(prompt) => prompt
            .text
            .trim()
            .trim_end_matches(':')
            .trim()
            .to_string(),
        None => "Password".to_string(),
    };
    let secret = move || prompt.get().is_none_or(|prompt| prompt.secret);
    let waiting = move || prompt.get().is_none() || busy.get();

    view! {
        <style>{move || theme.get()}</style>
        <style>{move || operator_theme.get()}</style>
        <main class=move || if hints.get().lock { "screen locked" } else { "screen" }>
            <Clock/>

            <div class="identity">
                {move || match picture() {
                    Some(source) => view! { <img class="avatar" src=source alt=""/> }.into_any(),
                    None => view! { <Avatar/> }.into_any(),
                }}
                // The picker and the typed name are not alternatives: a seat
                // that offers both needs the picker to stay put, or "Other…"
                // is a one-way door out of the account list.
                <Show when=move || {
                    let hints = hints.get();
                    !hints.hide_users && (!users.get().is_empty() || hints.has_guest_account)
                }>
                    <select
                        class="user"
                        aria-label="User"
                        prop:value=move || { users.get(); username.get() }
                        on:change:target=move |ev| select_user(ev.target().value())
                    >
                        <For each=move || users.get() key=|user| user.name.clone() let:user>
                            <option value=user.name.clone()>
                                {if user.logged_in {
                                    format!("{} — logged in", user.display_name)
                                } else {
                                    user.display_name.clone()
                                }}
                            </option>
                        </For>
                        <Show when=move || hints.get().has_guest_account>
                            <option value=GUEST>"Guest session"</option>
                        </Show>
                        <Show when=move || {
                            let hints = hints.get();
                            hints.show_manual_login || users.get().is_empty()
                        }>
                            <option value=OTHER>"Other…"</option>
                        </Show>
                    </select>
                </Show>
                <Show when=move || { username.get() == OTHER }>
                    <input
                        type="text"
                        class="user manual"
                        placeholder="Username"
                        autocomplete="off"
                        autocapitalize="none"
                        spellcheck="false"
                        aria-label="Username"
                        node_ref=manual_field
                        prop:value=move || manual.get()
                        on:input:target=move |ev| manual.set(ev.target().value())
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" {
                                ev.prevent_default();
                                start_auth();
                            }
                        }
                    />
                </Show>
            </div>

            <form
                class="prompt"
                on:submit=move |ev| {
                    ev.prevent_default();
                    if prompt.get_untracked().is_none() { start_auth() } else { respond() }
                }
            >
                <input
                    type=move || if secret() { "password" } else { "text" }
                    class="password"
                    placeholder=placeholder
                    autocomplete="off"
                    autocapitalize="none"
                    spellcheck="false"
                    node_ref=answer_field
                    prop:value=move || answer.get()
                    prop:disabled=waiting
                    on:input:target=move |ev| answer.set(ev.target().value())
                />
                <button
                    type="submit"
                    class="enter"
                    aria-label=move || if hints.get().lock { "Unlock" } else { "Log in" }
                    prop:disabled=waiting
                >
                    "›"
                </button>
            </form>

            // One region for everything the greeter has to say: the rejection,
            // whatever PAM said on the way there, and the autologin countdown.
            <div class="error" role="alert">
                <Show when=move || !error.get().is_empty()>
                    <p class="failure">{move || error.get()}</p>
                </Show>
                {move || {
                    notices
                        .get()
                        .into_iter()
                        .map(|notice| {
                            let class = if notice.error { "message failed" } else { "message" };
                            view! { <p class=class>{notice.text}</p> }
                        })
                        .collect_view()
                }}
                <Show when=move || { countdown.get() > 0 }>
                    <p class="countdown">
                        {move || {
                            let hints = hints.get();
                            let who = if hints.autologin_user.is_empty() {
                                "guest".to_string()
                            } else {
                                hints.autologin_user
                            };
                            format!(
                                "Logging in as {who} in {}s — press any key to stay here.",
                                countdown.get(),
                            )
                        }}
                    </p>
                </Show>
                <Show when=move || !hints.get().connected>
                    <p class="message failed">"Not connected to LightDM — themes only."</p>
                </Show>
            </div>

            <div class="actions">
                <For each=move || actions.get() key=|action| action.clone() let:action>
                    <PowerButton action=action/>
                </For>
            </div>

            <footer class="status">
                <Show when=move || !hints.get().hostname.is_empty()>
                    <span class="host" title=move || hints.get().os>
                        {move || hints.get().hostname}
                    </span>
                </Show>
                <label>
                    "Session: "
                    <select
                        aria-label="Desktop session"
                        prop:value=move || { sessions.get(); session.get() }
                        on:change:target=move |ev| session.set(ev.target().value())
                    >
                        <For each=move || sessions.get() key=|s| s.key.clone() let:candidate>
                            <option value=candidate.key.clone()>{candidate.name.clone()}</option>
                        </For>
                    </select>
                </label>
                <Show when=move || { layouts.get().len() > 1 }>
                    <label>
                        "Keyboard: "
                        <select
                            aria-label="Keyboard layout"
                            // Reading the list here is what re-applies the
                            // value once the options exist: a <select> ignores
                            // a value naming an <option> it does not have yet.
                            prop:value=move || { layouts.get(); layout.get() }
                            on:change:target=move |ev| {
                                let name = ev.target().value();
                                layout.set(name.clone());
                                // Switch the greeter's own keyboard too, or the
                                // password is typed on the layout just left.
                                spawn_local(async move {
                                    let _ = call::<()>("set_layout", Named { name }).await;
                                });
                            }
                        >
                            <For each=move || layouts.get() key=|l| l.name.clone() let:candidate>
                                <option value=candidate.name.clone()>
                                    {candidate.description.clone()}
                                </option>
                            </For>
                        </select>
                    </label>
                </Show>
                <Show when=move || { languages.get().len() > 1 }>
                    <label>
                        "Language: "
                        <select
                            aria-label="Language"
                            prop:value=move || { languages.get(); language.get() }
                            on:change:target=move |ev| language.set(ev.target().value())
                        >
                            <For each=move || languages.get() key=|l| l.code.clone() let:candidate>
                                <option value=candidate.code.clone()>
                                    {candidate.name.clone()}
                                </option>
                            </For>
                        </select>
                    </label>
                </Show>
                <Show when=move || !theme_names.get().is_empty()>
                    <label>
                        "Theme: "
                        <select
                            aria-label="Theme"
                            prop:value=move || theme_name.get()
                            on:change:target=move |ev| theme_name.set(ev.target().value())
                        >
                            <For each=move || theme_names.get() key=|name| name.clone() let:name>
                                <option value=name.clone()>{name.clone()}</option>
                            </For>
                        </select>
                    </label>
                </Show>
            </footer>
        </main>
    }
}

/// Point the session, keyboard and language pickers at what this account used
/// last, falling back to LightDM's seat default and then to the first entry.
/// Getting this wrong is how a greeter makes someone re-pick their desktop
/// every morning.
fn prefer(
    name: String,
    users: RwSignal<Vec<Account>>,
    sessions: RwSignal<Vec<Session>>,
    hints: &Hints,
    session: RwSignal<String>,
    layout: RwSignal<String>,
    language: RwSignal<String>,
) {
    let users = users.get_untracked();
    let account = users.iter().find(|user| user.name == name);

    if let Some(remembered) = account.map(|account| account.layout.clone()) {
        if !remembered.is_empty() {
            layout.set(remembered.clone());
            spawn_local(async move {
                let _ = call::<()>("set_layout", Named { name: remembered }).await;
            });
        }
    }
    if let Some(remembered) = account.map(|account| account.language.clone()) {
        if !remembered.is_empty() {
            language.set(remembered);
        }
    }

    let sessions = sessions.get_untracked();
    let offered = |key: &String| sessions.iter().any(|session| &session.key == key);
    let chosen = account
        .map(|account| account.session.clone())
        .filter(&offered)
        // The account's last session may have been uninstalled since.
        .or_else(|| Some(hints.default_session.clone()).filter(&offered))
        // Neither the account nor LightDM named one that still exists; the list
        // is sorted Wayland first, so its head is the best guess left.
        .or_else(|| sessions.first().map(|session| session.key.clone()));
    if let Some(chosen) = chosen {
        session.set(chosen);
    }
}

/// The window on a monitor that is not the primary one: the theme's stylesheets
/// over an empty page, so the background matches the login screen's without a
/// second login screen on it. The same starting theme the login screen picks,
/// then whatever its picker moves to.
#[component]
pub fn Backdrop() -> impl IntoView {
    let theme = RwSignal::new(String::new());
    let operator_theme = RwSignal::new(String::new());

    let load = move |name: String| {
        spawn_local(async move {
            match call::<String>("theme_css", Named { name }).await {
                Ok(css) => theme.set(css),
                Err(message) => logged("theme_css", message),
            }
        });
    };

    // Not the `theme` event: the login screen emits that before this window has
    // finished subscribing, so the first theme is fetched rather than waited on.
    spawn_local(async move {
        match call::<Vec<String>>("themes", ()).await {
            Ok(names) if names.iter().any(|name| name == DEFAULT_THEME) => {
                load(DEFAULT_THEME.to_string());
            }
            Ok(_) => {}
            Err(message) => logged("themes", message),
        }
    });
    spawn_local(async move {
        let name = String::new();
        match call::<String>("theme_css", Named { name }).await {
            Ok(css) => operator_theme.set(css),
            Err(message) => logged("theme_css", message),
        }
    });
    on::<String>("theme", move |name| load(name));

    view! {
        <style>{move || theme.get()}</style>
        <style>{move || operator_theme.get()}</style>
    }
}

/// A command that failed. Nothing here is fatal -- the greeter draws without
/// any of it -- but the reason should not vanish.
fn logged(command: &str, message: String) {
    leptos::logging::error!("{command}: {message}");
}

#[component]
fn Clock() -> impl IntoView {
    let (time, set_time) = signal(String::new());
    let (date, set_date) = signal(String::new());

    let tick = move || {
        let now = js_sys::Date::new_0();
        set_time.set(localized(&now, &[("hour", "numeric"), ("minute", "2-digit")]));
        set_date.set(localized(
            &now,
            &[
                ("weekday", "long"),
                ("month", "long"),
                ("day", "numeric"),
                ("year", "numeric"),
            ],
        ));
    };
    tick();
    set_interval(tick, Duration::from_secs(1));

    view! {
        <div class="clock">
            <div class="time">{time}</div>
            <div class="date">{date}</div>
        </div>
    }
}

/// Formats a moment with `Intl.DateTimeFormat`, so the 12/24-hour clock and
/// month names follow the machine's locale instead of a hardcoded format.
fn localized(now: &js_sys::Date, options: &[(&str, &str)]) -> String {
    let bag = js_sys::Object::new();
    for (key, value) in options {
        let _ = js_sys::Reflect::set(&bag, &(*key).into(), &(*value).into());
    }
    // An empty locale list means "whatever this system is set to".
    let formatter = js_sys::Intl::DateTimeFormat::new(&js_sys::Array::new(), &bag);
    formatter
        .format()
        .call1(&formatter, now)
        .ok()
        .and_then(|formatted| formatted.as_string())
        .unwrap_or_default()
}

/// The label and glyph for each action the backend offered. Unknown names
/// cannot appear -- the backend serialises them from a four-variant enum -- but
/// one would draw as itself rather than as nothing.
fn describes(action: &str) -> (&'static str, &'static str) {
    match action {
        "suspend" => ("Sleep", "M20 14.5A8.5 8.5 0 0 1 9.5 4 8.5 8.5 0 1 0 20 14.5z"),
        "hibernate" => ("Hibernate", "M12 3v18M4.2 7.5l15.6 9M19.8 7.5l-15.6 9"),
        "reboot" => ("Restart", "M20 12a8 8 0 1 1-2.3-5.7M20.5 3.5v4h-4"),
        _ => ("Shut Down", "M12 3.5v8.5M6.3 6.3a8 8 0 1 0 11.4 0"),
    }
}

#[component]
fn PowerButton(action: String) -> impl IntoView {
    let (label, icon) = describes(&action);
    let name = action.clone();
    let run = move |_| {
        let action = name.clone();
        spawn_local(async move {
            let named = action.clone();
            if let Err(message) = call::<()>("power", PowerArgs { action }).await {
                logged(&named, message);
            }
        });
    };

    view! {
        // The per-action class lets a theme place or restyle one button on its
        // own; the aria-label means a theme may hide the visible text (an icon
        // rail, say) without leaving the button nameless.
        <button class=format!("action {action}") aria-label=label on:click=run>
            <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d=icon/>
            </svg>
            <span>{label}</span>
        </button>
    }
}

#[component]
fn Avatar() -> impl IntoView {
    view! {
        <svg class="avatar" viewBox="0 0 96 96" aria-hidden="true">
            <circle cx="48" cy="48" r="46"/>
            <circle class="head" cx="48" cy="38" r="14"/>
            <path class="body" d="M22 78a26 26 0 0 1 52 0"/>
        </svg>
    }
}
