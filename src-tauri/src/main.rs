// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
    // GPL "Appropriate Legal Notices": the greeter itself is a fullscreen login
    // screen with nowhere sane to put them, so they live here.
    if std::env::args().any(|argument| argument == "--version") {
        println!(
            concat!(
                "tauri-greeter ", env!("CARGO_PKG_VERSION"), "\n",
                "Copyright (C) 2026 Brian Teague\n",
                "License GPLv3+: GNU GPL version 3 or later <https://gnu.org/licenses/gpl.html>\n",
                "This is free software: you are free to change and redistribute it.\n",
                "There is NO WARRANTY, to the extent permitted by law.",
            )
        );
        return;
    }

    // LightDM starts this on the X display it owns, as the `lightdm` user.
    tauri_greeter_lib::run()
}
