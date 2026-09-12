#!/bin/sh
# Copyright (C) 2026 Brian Teague
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run a whole LightDM -- greeter included -- inside a nested X server, as you.
# No root, nothing on the real seat touched, and a greeter that fails to come
# up costs a closed window rather than a machine you cannot log into.
#
# Everything it writes lives in one temporary directory that goes away on exit,
# so the system's /etc/lightdm and /run/lightdm are never involved.
#
# Needs: lightdm, xorg-server-xephyr, and `make install` already run.
set -eu

command -v Xephyr >/dev/null 2>&1 || {
	echo "Xephyr not found. Install it (Arch: xorg-server-xephyr)." >&2
	exit 1
}
[ -f /usr/share/xgreeters/tauri-greeter.desktop ] || {
	echo "No greeter entry in /usr/share/xgreeters. Run 'make install' first." >&2
	exit 1
}

dir=$(mktemp -d "${TMPDIR:-/tmp}/tauri-greeter-test.XXXXXX")
trap 'rm -rf "$dir"' EXIT INT TERM
mkdir -p "$dir/run" "$dir/log" "$dir/cache"

# No session-wrapper: /etc/lightdm/Xsession expects the privileges test mode
# deliberately does not have. Authentication and the handoff are what this is
# testing; the desktop that follows is the real seat's job.
cat >"$dir/lightdm.conf" <<EOF
[LightDM]
start-default-seat=true

[Seat:*]
type=local
xserver-command=Xephyr -screen ${SCREEN:-1280x800} -resizeable
greeter-session=tauri-greeter
user-session=$(ls /usr/share/xsessions /usr/share/wayland-sessions 2>/dev/null |
	sed -n 's/\.desktop$//p' | head -1)
EOF

echo "config:  $dir/lightdm.conf"
echo "logs:    $dir/log"
echo "Press Ctrl-C here to stop. Closing the Xephyr window will not: bringing a"
echo "dead seat back up is what a display manager does, so LightDM respawns it."
echo

# Not `exec`: that would replace this shell and take the cleanup trap with it,
# leaving the temp directory behind on every run.
lightdm --test-mode --debug \
	--config "$dir/lightdm.conf" \
	--run-dir "$dir/run" \
	--log-dir "$dir/log" \
	--cache-dir "$dir/cache"
