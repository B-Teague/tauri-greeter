.PHONY: help build release test third-party install uninstall test-mode verify package clean

BINARY := target/release/tauri-greeter

help:
	@echo "build             Build the greeter (debug)"
	@echo "release           Build the greeter (optimized)"
	@echo "test              Run the backend unit tests"
	@echo "third-party       Regenerate THIRD-PARTY-LICENSES.md (needs cargo-about)"
	@echo "install           Install binary, themes and the LightDM greeter entry (sudo)"
	@echo "uninstall         Remove the installation (sudo)"
	@echo "test-mode         Run the greeter under a nested LightDM (no root, no risk)"
	@echo "package           Build the release binary and an Arch package"
	@echo "verify            Report what is installed"
	@echo "clean             Remove build artifacts"

# The UI is a Leptos/WASM bundle that trunk writes to dist/; tauri embeds it
# into the binary, so it has to be built first.
build:
	trunk build
	cargo build -p tauri-greeter --features custom-protocol

release:
	trunk build --release
	cargo build --release -p tauri-greeter --features custom-protocol

test:
	cargo test -p tauri-greeter --lib

# The binary statically links ~180 permissively licensed crates, whose notices
# have to ship with it. Rerun after touching Cargo.lock.
# Needs: cargo install cargo-about --locked --features cli
third-party:
	python3 packaging/third-party-licenses.py

install: release
	sudo install -Dm755 $(BINARY) /usr/local/bin/tauri-greeter
	sudo install -Dm644 -t /usr/share/tauri-greeter/themes themes/*.css
	sudo install -Dm644 -t /usr/share/licenses/tauri-greeter LICENSE THIRD-PARTY-LICENSES.md
	sudo install -Dm644 packaging/tauri-greeter.desktop /usr/share/xgreeters/tauri-greeter.desktop
	@echo "Installed, but LightDM is still using its current greeter. To switch:"
	@echo "  printf '[Seat:*]\\ngreeter-session=tauri-greeter\\n' |"
	@echo "      sudo install -Dm644 /dev/stdin /etc/lightdm/lightdm.conf.d/50-tauri-greeter.conf"
	@echo "Optional theme: sudo install -Dm644 themes/midnight.css /etc/tauri-greeter/theme.css"

uninstall:
	sudo rm -f /usr/local/bin/tauri-greeter
	sudo rm -rf /usr/share/tauri-greeter /usr/share/licenses/tauri-greeter
	sudo rm -f /usr/share/xgreeters/tauri-greeter.desktop

# Runs a whole LightDM inside a nested X server as your own user. Nothing on
# the real seat is touched, so a greeter that fails here costs nothing.
# Needs: lightdm, xorg-server-xephyr. See LOCAL_TESTING.md.
test-mode: install
	packaging/test-mode.sh

verify:
	@test -x /usr/local/bin/tauri-greeter && echo "ok   binary: /usr/local/bin/tauri-greeter" || echo "MISS binary: /usr/local/bin/tauri-greeter"
	@test -f /usr/share/xgreeters/tauri-greeter.desktop && echo "ok   greeter entry: /usr/share/xgreeters/tauri-greeter.desktop" || echo "MISS greeter entry: /usr/share/xgreeters/tauri-greeter.desktop"
	@selected=$$(grep -rhs "^greeter-session=" /etc/lightdm/lightdm.conf /etc/lightdm/lightdm.conf.d/ 2>/dev/null); \
	  if [ -n "$$selected" ]; then echo "$$selected" | sed 's/^/     lightdm /'; \
	  else echo "     lightdm greeter-session: unset (LightDM's default greeter is in use)"; fi
	@test -f /etc/tauri-greeter/theme.css && echo "ok   theme: /etc/tauri-greeter/theme.css" || echo "--   theme: none installed (built-in theme in use)"
	@ls /usr/share/tauri-greeter/themes/*.css 2>/dev/null | sed 's/^/     example /' || true

# makepkg only packages what release built; it never compiles anything itself.
package: release
	cd packaging/arch && makepkg -f
	@ls -1 packaging/arch/*.pkg.tar.* | sed 's/^/built /'
	@echo "Install with: sudo pacman -U packaging/arch/*.pkg.tar.zst"

clean:
	cargo clean
	rm -rf dist
	rm -rf packaging/arch/pkg packaging/arch/src packaging/arch/*.pkg.tar.*
