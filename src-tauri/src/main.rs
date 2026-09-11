// Copyright (C) 2026 Brian Teague
// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
    // GPL "Appropriate Legal Notices": the greeter itself is a fullscreen login
    // screen with nowhere sane to put them, so they live here.
    if std::env::args().any(|argument| argument == "--version") {
        println!(
            concat!(
                "cssdm ", env!("CARGO_PKG_VERSION"), "\n",
                "Copyright (C) 2026 Brian Teague\n",
                "License GPLv3+: GNU GPL version 3 or later <https://gnu.org/licenses/gpl.html>\n",
                "This is free software: you are free to change and redistribute it.\n",
                "There is NO WARRANTY, to the extent permitted by law.",
            )
        );
        return;
    }

    // Started with --greeter by the daemon, on the X display the daemon owns.
    // Without it, this is the daemon itself: the process systemd starts.
    if !std::env::args().any(|argument| argument == "--greeter") {
        cssdm_lib::daemon::run()
    }

    cssdm_lib::run()
}
