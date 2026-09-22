// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

mod app;

fn main() {
    // One page, two jobs: the login screen on the primary monitor, and on every
    // other monitor a window that only wears the theme's background.
    if app::is_backdrop() {
        leptos::mount::mount_to_body(app::Backdrop);
    } else {
        leptos::mount::mount_to_body(app::App);
    }
}
