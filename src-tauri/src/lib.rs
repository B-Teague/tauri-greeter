// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

mod lightdm;

use base64::prelude::{Engine, BASE64_STANDARD};
use lightdm::{Action, Hints, Language, Layout, Session, User};
use serde::Serialize;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use tauri::Manager;

/// Operator stylesheet, injected by the UI on startup when present.
const THEME_CSS: &str = "/etc/tauri-greeter/theme.css";
/// Installed stylesheets the greeter's picker can switch between.
const THEME_DIR: &str = "/usr/share/tauri-greeter/themes";
/// A theme's optional background: a picture beside its stylesheet, same stem.
/// First match wins, so `dusk.css` takes `dusk.png` over `dusk.jpg`.
const BACKGROUND: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
/// The greeter renders in software and holds the decoded frame as well as this
/// base64 copy of the file. A wallpaper past this is a mistake, not a theme.
const BACKGROUND_MAX: u64 = 16 * 1024 * 1024;
/// An account picture is drawn at a few dozen pixels. Anything approaching this
/// is someone's holiday photo in `~/.face`, and there may be one per account.
const AVATAR_MAX: u64 = 4 * 1024 * 1024;

/// What kind of picture this is, from its first bytes rather than its name.
/// A `data:` URI has to carry a truthful type -- the webview will not sniff one
/// -- and the files here are named by LightDM and by whoever wrote the theme,
/// neither of whom promised the extension matches.
fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xff, 0xd8, 0xff, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        [b'<', b's', b'v', b'g', ..] | [b'<', b'?', b'x', b'm', b'l', ..] => Some("image/svg+xml"),
        _ => None,
    }
}

/// A picture as a `data:` URI, or nothing if it is missing, too big, unreadable
/// or not a picture. The webview serves from `tauri://`, where an absolute path
/// is not the filesystem's, so every image has to travel inline.
fn data_uri(path: &Path, limit: u64) -> Option<String> {
    match fs::metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return None,
        Err(error) => {
            eprintln!("tauri-greeter: {}: {error}", path.display());
            return None;
        }
        Ok(metadata) if metadata.len() > limit => {
            eprintln!(
                "tauri-greeter: {}: {} bytes, over the {limit} byte limit",
                path.display(),
                metadata.len()
            );
            return None;
        }
        Ok(_) => {}
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("tauri-greeter: {}: {error}", path.display());
            return None;
        }
    };
    let Some(mime) = image_mime(&bytes) else {
        eprintln!("tauri-greeter: {}: not an image", path.display());
        return None;
    };
    Some(format!(
        "data:{mime};base64,{}",
        BASE64_STANDARD.encode(bytes)
    ))
}

#[tauri::command]
fn hints() -> Hints {
    lightdm::hints()
}

/// A LightDM user with the account picture already inlined, since the webview
/// cannot open `/var/lib/AccountsService/icons/...` for itself.
#[derive(Serialize)]
struct Account {
    #[serde(flatten)]
    user: User,
    /// A `data:` URI, or empty for the drawn fallback avatar.
    picture: String,
}

#[tauri::command]
fn users() -> Vec<Account> {
    lightdm::users()
        .into_iter()
        .map(|user| Account {
            // Unreadable is the normal case for `~/.face` under a 0700 home:
            // the greeter runs as `lightdm`, so it just draws the fallback.
            picture: (!user.image.is_empty())
                .then(|| data_uri(Path::new(&user.image), AVATAR_MAX))
                .flatten()
                .unwrap_or_default(),
            user,
        })
        .collect()
}

#[tauri::command]
fn sessions() -> Vec<Session> {
    lightdm::sessions()
}

#[tauri::command]
fn layouts() -> Vec<Layout> {
    lightdm::layouts()
}

#[tauri::command]
fn layout() -> String {
    lightdm::layout()
}

#[tauri::command]
fn set_layout(name: String) {
    lightdm::set_layout(&name);
}

#[tauri::command]
fn languages() -> Vec<Language> {
    lightdm::languages()
}

#[tauri::command]
fn language() -> String {
    lightdm::language()
}

#[tauri::command]
fn power_actions() -> Vec<Action> {
    lightdm::power_actions()
}

#[tauri::command]
fn power(action: Action) -> Result<(), String> {
    lightdm::power(action)
}

/// Start authenticating. Prompts, messages and the verdict come back as the
/// `prompt`, `message` and `complete` events, not as this call's return value.
#[tauri::command]
fn authenticate(username: String) -> Result<(), String> {
    lightdm::authenticate(&username)
}

#[tauri::command]
fn authenticate_autologin() -> Result<(), String> {
    lightdm::authenticate_autologin()
}

/// Answer the prompt that is showing.
#[tauri::command]
fn respond(answer: String) -> Result<(), String> {
    lightdm::respond(answer)
}

#[tauri::command]
fn cancel() -> Result<(), String> {
    lightdm::cancel()
}

#[tauri::command]
fn cancel_autologin() {
    lightdm::cancel_autologin();
}

/// `session` is a LightDM session key, not a display name: the UI sends back
/// exactly what `sessions` gave it. Only reached after a `complete` event said
/// the attempt authenticated -- LightDM checks that again regardless.
#[tauri::command]
fn start_session(session: String, language: String) -> Result<(), String> {
    lightdm::start_session(&session, &language)
}

#[tauri::command]
fn themes() -> Vec<String> {
    // An empty picker is a working greeter, so a directory that cannot be read
    // is reported and moved past rather than failed on. Not installing the
    // examples at all is a normal way to run.
    let entries = match fs::read_dir(THEME_DIR) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            eprintln!("tauri-greeter: {THEME_DIR}: {error}");
            return Vec::new();
        }
    };

    let mut names = Vec::new();
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(error) => {
                eprintln!("tauri-greeter: {THEME_DIR}: {error}");
                continue;
            }
        };
        if path.extension().is_some_and(|extension| extension == "css") {
            if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names
}

/// The stylesheet a theme's picture makes, or nothing if it has none. Handed
/// back ahead of the theme's own CSS, so a theme that wants the picture placed
/// differently just restates `--backdrop` and wins on order.
fn backdrop(css: &Path) -> String {
    for extension in BACKGROUND {
        if let Some(uri) = data_uri(&css.with_extension(extension), BACKGROUND_MAX) {
            return format!(":root {{ --backdrop: url(\"{uri}\") center / cover no-repeat; }}\n");
        }
    }
    String::new()
}

/// An empty name means the operator's stylesheet, which is what the greeter
/// shows when nobody touches the picker.
#[tauri::command]
fn theme_css(name: String) -> String {
    let path = if name.is_empty() {
        THEME_CSS.to_string()
    } else if themes().contains(&name) {
        // Only a name the listing actually offers is ever opened, so a crafted
        // request cannot walk out of the theme directory and read, say, shadow.
        format!("{THEME_DIR}/{name}.css")
    } else {
        eprintln!("tauri-greeter: no such theme: {name}");
        return String::new();
    };
    let css = match fs::read_to_string(&path) {
        Ok(css) => css,
        // No operator stylesheet is the default state, not a problem. Anything
        // else -- a mode-600 file, a bad symlink -- is worth a line in the
        // journal, because on screen it just looks like the theme was ignored.
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => {
            eprintln!("tauri-greeter: {path}: {error}");
            String::new()
        }
    };
    // A picture on its own is a whole theme, so this runs even with no CSS.
    backdrop(Path::new(&path)) + &css
}

/// A window covering each monitor that is not `primary`. They load the same
/// page as the login screen; the label is what tells the UI to draw only the
/// theme's background there.
fn backdrops(app: &tauri::App, primary: &tauri::Monitor) {
    let Ok(monitors) = app.available_monitors() else {
        return;
    };
    for (index, monitor) in monitors
        .iter()
        .filter(|monitor| monitor.position() != primary.position())
        .enumerate()
    {
        let built = tauri::WebviewWindowBuilder::new(
            app,
            format!("backdrop-{index}"),
            tauri::WebviewUrl::App("index.html".into()),
        )
        // Named apart from the login screen's window so a journal or an
        // `xwininfo` says which is which.
        .title("tauri-greeter backdrop")
        .decorations(false)
        // The keyboard belongs to the login screen; nothing here is typed into.
        .focused(false)
        .skip_taskbar(true)
        .build();
        match built {
            Ok(backdrop) => {
                let _ = backdrop.set_position(*monitor.position());
                let _ = backdrop.set_size(*monitor.size());
                // Where each window went is the only way to tell a misplaced
                // backdrop from a monitor X never reported, after the fact.
                eprintln!(
                    "tauri-greeter: backdrop on {} at {:?}",
                    monitor.name().map_or("?", |name| name.as_str()),
                    monitor.position()
                );
            }
            // One monitor left showing whatever X had on it is a cosmetic fault;
            // the login screen itself is already up.
            Err(error) => eprintln!("tauri-greeter: backdrop window: {error}"),
        }
    }
}

pub fn run() {
    // WebKitGTK renders through a DMABUF buffer it allocates with GBM. On the
    // bare Xorg LightDM starts for its greeter -- no compositor, and whatever
    // driver the machine happens to have -- that allocation fails ("Failed to
    // create GBM buffer ... Invalid argument") and WebKit answers by painting
    // nothing: a white screen, with no error from us to say why. The software
    // path is fast enough for a login form. Set the variable yourself to opt
    // back in on hardware where the fast path works.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .setup(|app| {
            // GLib's main context exists by now and Tauri's GTK loop is about to
            // start turning it, which is where LightDM's replies will arrive.
            // The handle is what those replies are emitted through.
            lightdm::connect(app.handle().clone());

            // TAURI_GREETER_WINDOWED=1 runs the greeter in an ordinary window, so
            // it can be tested from a running desktop instead of covering the
            // screen.
            if let Some(window) = app.get_webview_window("main") {
                if std::env::var_os("TAURI_GREETER_WINDOWED").is_some() {
                    let _ = window.set_fullscreen(false);
                    let _ = window.set_decorations(true);
                    let _ = window.set_size(tauri::LogicalSize::new(1280.0, 800.0));
                    let _ = window.center();
                } else {
                    // No window manager runs on the greeter's display, so there is
                    // nobody to honour a fullscreen request or to hand out focus:
                    // cover the screen by hand and take the keyboard directly.
                    // The login screen covers the primary monitor only -- which is
                    // not necessarily the one at (0, 0) -- and every other monitor
                    // gets a window that draws the theme's background and nothing
                    // else, so a second screen is part of the backdrop rather than
                    // a second login form.
                    let primary = window
                        .primary_monitor()
                        .ok()
                        .flatten()
                        .or_else(|| window.current_monitor().ok().flatten());
                    if let Some(monitor) = &primary {
                        let _ = window.set_position(*monitor.position());
                        let _ = window.set_size(*monitor.size());
                        eprintln!(
                            "tauri-greeter: login screen on {} at {:?}",
                            monitor.name().map_or("?", |name| name.as_str()),
                            monitor.position()
                        );
                        backdrops(app, monitor);
                    }
                    // Last, so a backdrop that grabbed the keyboard on its way up
                    // does not keep it.
                    let _ = window.set_focus();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            hints,
            users,
            sessions,
            layouts,
            layout,
            set_layout,
            languages,
            language,
            themes,
            theme_css,
            power_actions,
            power,
            authenticate,
            authenticate_autologin,
            respond,
            cancel,
            cancel_autologin,
            start_session,
        ])
        .run(tauri::generate_context!())
        // Nothing can be drawn without a webview, and LightDM's fallback greeter
        // is a better answer than a blank screen -- so say why, and let it.
        .expect("tauri-greeter: could not start the webview");
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n and then some";

    /// A background is found by stem, inlined, and skipped when oversized --
    /// the last one matters because the failure is a greeter that draws nothing
    /// while it encodes someone's 4 GB video file.
    #[test]
    fn inlines_a_theme_background_beside_its_stylesheet() {
        let dir = std::env::temp_dir().join("tauri-greeter-backdrop-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let css = dir.join("dusk.css");

        assert_eq!(backdrop(&css), "");

        fs::write(dir.join("dusk.png"), PNG).unwrap();
        let rule = backdrop(&css);
        assert!(rule.contains("--backdrop:"), "{rule}");
        assert!(rule.contains("data:image/png;base64,"), "{rule}");

        // Sparse, so the test does not actually write 16 MB.
        fs::remove_file(dir.join("dusk.png")).unwrap();
        fs::File::create(dir.join("dusk.jpg"))
            .unwrap()
            .set_len(BACKGROUND_MAX + 1)
            .unwrap();
        assert_eq!(backdrop(&css), "");

        fs::remove_dir_all(&dir).unwrap();
    }

    /// The type in the URI comes from the bytes, not the name: LightDM hands
    /// back `/var/lib/AccountsService/icons/<user>`, which has no extension at
    /// all, and a `data:` URI that lies about its type draws nothing.
    #[test]
    fn types_a_picture_by_its_contents_not_its_name() {
        let dir = std::env::temp_dir().join("tauri-greeter-avatar-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let face = dir.join("brian");
        fs::write(&face, b"\xff\xd8\xff\xe0 jpeg-ish").unwrap();
        assert!(data_uri(&face, AVATAR_MAX)
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));

        // Named .png, actually a PNG: still typed from the bytes.
        let named = dir.join("brian.png");
        fs::write(&named, PNG).unwrap();
        assert!(data_uri(&named, AVATAR_MAX)
            .unwrap()
            .starts_with("data:image/png;base64,"));

        // Whatever this is, it is not a picture, and guessing would put a
        // broken image where the avatar goes.
        fs::write(&face, b"root:x:0:0:...").unwrap();
        assert_eq!(data_uri(&face, AVATAR_MAX), None);

        assert_eq!(data_uri(&dir.join("nobody"), AVATAR_MAX), None);

        fs::remove_dir_all(&dir).unwrap();
    }
}
