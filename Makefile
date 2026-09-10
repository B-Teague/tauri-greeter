.PHONY: help build release install uninstall test clean systemd-install systemd-uninstall

# CSSDM Display Manager Makefile
# Usage: make [target]

help:
	@echo "CSSDM Build & Install Targets"
	@echo ""
	@echo "Build targets:"
	@echo "  make build          - Build CSSDM in debug mode"
	@echo "  make release        - Build CSSDM in release mode (optimized)"
	@echo "  make test           - Run all unit tests"
	@echo ""
	@echo "Install targets:"
	@echo "  make install        - Install CSSDM (requires sudo)"
	@echo "  make uninstall      - Remove CSSDM installation"
	@echo "  make systemd-install - Install systemd service (requires sudo)"
	@echo "  make systemd-uninstall - Remove systemd service"
	@echo ""
	@echo "Utility targets:"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make verify         - Verify installation"

# Build targets
build:
	@echo "Building CSSDM (debug)..."
	cargo build

release:
	@echo "Building CSSDM (release)..."
	cargo build --release

test:
	@echo "Running tests..."
	cd src-tauri && cargo test --lib

# Install targets
install: release
	@echo "Installing CSSDM..."
	@if [ ! -f target/release/cssdm ]; then \
		echo "Error: Release binary not found. Run 'make release' first."; \
		exit 1; \
	fi
	sudo install -m 755 target/release/cssdm /usr/local/bin/
	sudo mkdir -p /usr/share/cssdm/themes
	sudo cp -r themes/light /usr/share/cssdm/themes/
	sudo cp -r themes/high-contrast /usr/share/cssdm/themes/
	@echo "✓ CSSDM installed to /usr/local/bin/cssdm"
	@echo "✓ Themes installed to /usr/share/cssdm/themes/"
	@echo ""
	@echo "Next steps:"
	@echo "  1. Install systemd service: make systemd-install"
	@echo "  2. Set as default DM: sudo update-alternatives --install /usr/bin/x-session-manager x-session-manager /usr/local/bin/cssdm 100"
	@echo "  3. Reboot to test: sudo reboot"

uninstall:
	@echo "Uninstalling CSSDM..."
	@if [ -f /usr/local/bin/cssdm ]; then \
		sudo rm /usr/local/bin/cssdm; \
		echo "✓ Removed /usr/local/bin/cssdm"; \
	fi
	@if [ -d /usr/share/cssdm/themes ]; then \
		sudo rm -rf /usr/share/cssdm/themes; \
		echo "✓ Removed /usr/share/cssdm/themes/"; \
	fi
	@echo "✓ CSSDM uninstalled"

# Systemd service targets
systemd-install: install
	@echo "Installing systemd service..."
	sudo cp cssdm.service /etc/systemd/system/
	sudo systemctl daemon-reload
	sudo systemctl enable cssdm.service
	@echo "✓ systemd service installed and enabled"
	@echo ""
	@echo "To start the service:"
	@echo "  sudo systemctl start cssdm"
	@echo ""
	@echo "To check status:"
	@echo "  systemctl status cssdm"
	@echo ""
	@echo "To view logs:"
	@echo "  journalctl -u cssdm -f"

systemd-uninstall:
	@echo "Removing systemd service..."
	sudo systemctl disable cssdm.service
	@if [ -f /etc/systemd/system/cssdm.service ]; then \
		sudo rm /etc/systemd/system/cssdm.service; \
		echo "✓ Removed /etc/systemd/system/cssdm.service"; \
	fi
	sudo systemctl daemon-reload
	@echo "✓ systemd service removed"

# Utility targets
clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	rm -rf dist/
	@echo "✓ Cleaned"

verify:
	@echo "Verifying CSSDM installation..."
	@echo ""
	@echo "Binary:"
	@if [ -f /usr/local/bin/cssdm ]; then \
		echo "  ✓ /usr/local/bin/cssdm exists"; \
		echo "  ✓ Binary size: $$(du -h /usr/local/bin/cssdm | cut -f1)"; \
	else \
		echo "  ✗ /usr/local/bin/cssdm not found (install not complete)"; \
	fi
	@echo ""
	@echo "Themes:"
	@if [ -d /usr/share/cssdm/themes ]; then \
		echo "  ✓ Theme directory exists"; \
		echo "  ✓ Installed themes:"; \
		ls -1 /usr/share/cssdm/themes/ | sed 's/^/    - /'; \
	else \
		echo "  ✗ Theme directory not found"; \
	fi
	@echo ""
	@echo "systemd service:"
	@if [ -f /etc/systemd/system/cssdm.service ]; then \
		echo "  ✓ systemd service installed"; \
		if systemctl is-enabled cssdm.service >/dev/null 2>&1; then \
			echo "  ✓ Service is enabled"; \
		else \
			echo "  ✗ Service is disabled"; \
		fi; \
	else \
		echo "  ✗ systemd service not installed"; \
	fi
	@echo ""
	@echo "Display manager alternatives:"
	@if update-alternatives --query x-session-manager 2>/dev/null | grep -q "cssdm"; then \
		echo "  ✓ cssdm registered as display manager"; \
	else \
		echo "  ✗ cssdm not registered (run: sudo update-alternatives --install ...)"; \
	fi
