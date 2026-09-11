.PHONY: help build release test third-party install uninstall systemd-install systemd-uninstall verify package clean

BINARY := target/release/cssdm

help:
	@echo "build             Build the greeter (debug)"
	@echo "release           Build the greeter (optimized)"
	@echo "test              Run the backend unit tests"
	@echo "third-party       Regenerate THIRD-PARTY-LICENSES.md (needs cargo-about)"
	@echo "install           Install binary and example themes (sudo)"
	@echo "uninstall         Remove the installation (sudo)"
	@echo "systemd-install   Install and enable the systemd unit (sudo)"
	@echo "systemd-uninstall Disable and remove the systemd unit (sudo)"
	@echo "package           Build the release binary and an Arch package"
	@echo "verify            Report what is installed"
	@echo "clean             Remove build artifacts"

# The UI is a Leptos/WASM bundle that trunk writes to dist/; tauri embeds it
# into the binary, so it has to be built first.
build:
	trunk build
	cargo build -p cssdm --features custom-protocol

release:
	trunk build --release
	cargo build --release -p cssdm --features custom-protocol

test:
	cargo test -p cssdm --lib

# The binary statically links ~180 permissively licensed crates, whose notices
# have to ship with it. Rerun after touching Cargo.lock.
# Needs: cargo install cargo-about --locked --features cli
third-party:
	python3 packaging/third-party-licenses.py

install: release
	sudo install -Dm755 $(BINARY) /usr/local/bin/cssdm
	sudo install -Dm644 -t /usr/share/cssdm/themes themes/*.css
	sudo install -Dm644 -t /usr/share/licenses/cssdm LICENSE THIRD-PARTY-LICENSES.md
	@echo "Installed. Next: make systemd-install"
	@echo "Optional theme: sudo install -Dm644 themes/midnight.css /etc/cssdm/theme.css"

uninstall:
	sudo rm -f /usr/local/bin/cssdm
	sudo rm -rf /usr/share/cssdm /usr/share/licenses/cssdm

systemd-install: install
	sudo install -Dm644 cssdm.service /etc/systemd/system/cssdm.service
	sudo systemctl daemon-reload
	sudo systemctl enable cssdm.service
	@echo "Enabled. Start with: sudo systemctl start cssdm"

systemd-uninstall:
	-sudo systemctl disable cssdm.service
	sudo rm -f /etc/systemd/system/cssdm.service
	sudo systemctl daemon-reload

verify:
	@test -x /usr/local/bin/cssdm && echo "ok   binary: /usr/local/bin/cssdm" || echo "MISS binary: /usr/local/bin/cssdm"
	@test -f /etc/systemd/system/cssdm.service && echo "ok   unit: /etc/systemd/system/cssdm.service" || echo "MISS unit: /etc/systemd/system/cssdm.service"
	@systemctl is-enabled cssdm.service 2>/dev/null | sed 's/^/     unit is /' || true
	@test -f /etc/cssdm/theme.css && echo "ok   theme: /etc/cssdm/theme.css" || echo "--   theme: none installed (built-in theme in use)"
	@ls /usr/share/cssdm/themes/*.css 2>/dev/null | sed 's/^/     example /' || true

# makepkg only packages what release built; it never compiles anything itself.
package: release
	cd packaging/arch && makepkg -f
	@ls -1 packaging/arch/*.pkg.tar.* | sed 's/^/built /'
	@echo "Install with: sudo pacman -U packaging/arch/*.pkg.tar.zst"

clean:
	cargo clean
	rm -rf dist
	rm -rf packaging/arch/pkg packaging/arch/src packaging/arch/*.pkg.tar.*
