// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Calls a Tauri command, flattening JS and command errors into one message.
async fn call<T: DeserializeOwned>(cmd: &str, args: impl Serialize) -> Result<T, String> {
    let args = serde_wasm_bindgen::to_value(&args).map_err(|e| e.to_string())?;
    match invoke(cmd, args).await {
        Ok(value) => serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string()),
        Err(error) => Err(error.as_string().unwrap_or_else(|| format!("{cmd} failed"))),
    }
}

#[derive(Clone, Deserialize)]
struct User {
    name: String,
    display_name: String,
}

#[derive(Clone, Deserialize)]
struct Session {
    name: String,
    wayland: bool,
}

#[derive(Clone, Serialize)]
struct ThemeArgs {
    name: String,
}

#[derive(Clone, Serialize)]
struct Credentials {
    username: String,
    password: String,
    session: String,
}

#[component]
pub fn App() -> impl IntoView {
    let users = RwSignal::new(Vec::<User>::new());
    let sessions = RwSignal::new(Vec::<Session>::new());
    let theme = RwSignal::new(String::new());
    let theme_names = RwSignal::new(Vec::<String>::new());
    let theme_name = RwSignal::new(String::new());
    let username = RwSignal::new(String::new());
    let session = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let password_field = NodeRef::<html::Input>::new();

    // Everything the greeter needs, loaded once and in parallel.
    spawn_local(async move {
        if let Ok(loaded) = call::<Vec<User>>("users", ()).await {
            username.set(loaded.first().map_or(String::new(), |u| u.name.clone()));
            users.set(loaded);
        }
    });
    spawn_local(async move {
        if let Ok(loaded) = call::<Vec<Session>>("sessions", ()).await {
            // Wayland sorts first, so the first entry is the best default.
            session.set(loaded.first().map_or(String::new(), |s| s.name.clone()));
            sessions.set(loaded);
        }
    });
    spawn_local(async move {
        theme_names.set(call::<Vec<String>>("themes", ()).await.unwrap_or_default());
    });

    // Re-reads the stylesheet whenever the picker moves. The first run has an
    // empty name, which is the operator's /etc/cssdm/theme.css -- so an
    // untouched greeter looks exactly as it did before the picker existed.
    Effect::new(move |_| {
        let name = theme_name.get();
        spawn_local(async move {
            theme.set(call::<String>("theme_css", ThemeArgs { name }).await.unwrap_or_default());
        });
    });

    let error = RwSignal::new(String::new());
    let submit = Action::new_local(move |credentials: &Credentials| {
        let credentials = credentials.clone();
        async move {
            error.set(String::new());
            if let Err(message) = call::<()>("login", credentials).await {
                error.set(message);
                password.set(String::new());
                if let Some(field) = password_field.get_untracked() {
                    let _ = field.focus();
                }
            }
        }
    });
    let busy = submit.pending();

    // The password field is the only thing a user normally has to touch.
    Effect::new(move |_| {
        if let Some(field) = password_field.get() {
            let _ = field.focus();
        }
    });

    view! {
        <style>{move || theme.get()}</style>
        <main class="screen">
            <Clock/>

            <div class="identity">
                <Avatar/>
                <select
                    class="user"
                    aria-label="User"
                    prop:value=move || username.get()
                    on:change:target=move |ev| username.set(ev.target().value())
                >
                    <For each=move || users.get() key=|user| user.name.clone() let:user>
                        <option value=user.name.clone()>{user.display_name.clone()}</option>
                    </For>
                </select>
            </div>

            <form
                class="prompt"
                on:submit=move |ev| {
                    ev.prevent_default();
                    submit
                        .dispatch(Credentials {
                            username: username.get_untracked(),
                            password: password.get_untracked(),
                            session: session.get_untracked(),
                        });
                }
            >
                <input
                    type="password"
                    class="password"
                    placeholder="Password"
                    autocomplete="off"
                    required
                    node_ref=password_field
                    prop:value=move || password.get()
                    prop:disabled=move || busy.get()
                    on:input:target=move |ev| password.set(ev.target().value())
                />
                <button type="submit" class="enter" aria-label="Log in" prop:disabled=move || busy.get()>
                    "›"
                </button>
            </form>

            <p class="error" role="alert">{move || error.get()}</p>

            <div class="actions">
                <PowerButton
                    action="suspend"
                    label="Sleep"
                    icon="M20 14.5A8.5 8.5 0 0 1 9.5 4 8.5 8.5 0 1 0 20 14.5z"
                />
                <PowerButton
                    action="reboot"
                    label="Restart"
                    icon="M20 12a8 8 0 1 1-2.3-5.7M20.5 3.5v4h-4"
                />
                <PowerButton
                    action="shutdown"
                    label="Shut Down"
                    icon="M12 3.5v8.5M6.3 6.3a8 8 0 1 0 11.4 0"
                />
            </div>

            <footer class="status">
                <label>
                    "Theme: "
                    <select
                        aria-label="Theme"
                        prop:value=move || theme_name.get()
                        on:change:target=move |ev| theme_name.set(ev.target().value())
                    >
                        <option value="">"Default"</option>
                        <For each=move || theme_names.get() key=|name| name.clone() let:name>
                            <option value=name.clone()>{name.clone()}</option>
                        </For>
                    </select>
                </label>
                <label>
                    "Desktop Session: "
                    <select
                        prop:value=move || session.get()
                        on:change:target=move |ev| session.set(ev.target().value())
                    >
                        <For each=move || sessions.get() key=|s| s.name.clone() let:candidate>
                            <option value=candidate.name.clone()>
                                {format!(
                                    "{} ({})",
                                    candidate.name,
                                    if candidate.wayland { "Wayland" } else { "X11" },
                                )}
                            </option>
                        </For>
                    </select>
                </label>
            </footer>
        </main>
    }
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

#[derive(Serialize)]
struct PowerArgs {
    action: &'static str,
}

#[component]
fn PowerButton(action: &'static str, label: &'static str, icon: &'static str) -> impl IntoView {
    let run = move |_| {
        spawn_local(async move {
            if let Err(message) = call::<()>("power", PowerArgs { action }).await {
                leptos::logging::error!("{action} failed: {message}");
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
