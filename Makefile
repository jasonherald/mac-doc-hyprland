# nwg-dock-hyprland / nwg-drawer / nwg-notifications
# Makefile for building, installing, and configuring

PREFIX      ?= $(HOME)/.local
BINDIR      ?= $(HOME)/.cargo/bin
DATADIR     ?= $(PREFIX)/share
DBUS_DIR    ?= $(DATADIR)/dbus-1/services
CARGO       ?= cargo
CARGO_FLAGS ?= --release

BINARIES := nwg-dock-hyprland nwg-drawer nwg-notifications

.PHONY: all build install install-bin install-data install-dbus \
        uninstall upgrade stop start restart \
        deps check-deps check-rust setup-hyprland setup-sway \
        test test-integration test-all \
        help clean

# ─────────────────────────────────────────────────────────────────────
# Default target
# ─────────────────────────────────────────────────────────────────────

all: build

help:
	@echo "Usage:"
	@echo "  make install         Build and install everything (stops running instances)"
	@echo "  make upgrade         Rebuild, stop running instances, reinstall, restart"
	@echo "  make build           Build release binaries only"
	@echo "  make deps            Install system dependencies (requires sudo)"
	@echo "  make uninstall       Remove all installed files"
	@echo "  make stop            Stop all running instances"
	@echo "  make start           Start dock and notification daemon"
	@echo "  make restart         Stop then start"
	@echo "  make setup-hyprland  Print Hyprland autostart configuration"
	@echo "  make setup-sway      Print Sway autostart configuration"
	@echo "  make clean           Remove build artifacts"
	@echo ""
	@echo "Options:"
	@echo "  BINDIR=<path>   Binary install location (default: ~/.cargo/bin)"
	@echo "  PREFIX=<path>   Data/share prefix (default: ~/.local)"

# ─────────────────────────────────────────────────────────────────────
# Dependencies
# ─────────────────────────────────────────────────────────────────────

check-deps:
	@echo "Checking system dependencies..."
	@missing=""; \
	if ! pkg-config --exists gtk4 2>/dev/null; then missing="$$missing gtk4"; fi; \
	if ! pkg-config --exists gtk4-layer-shell-0 2>/dev/null; then missing="$$missing gtk4-layer-shell"; fi; \
	if [ -n "$$missing" ]; then \
		echo ""; \
		echo "Missing dependencies:$$missing"; \
		echo ""; \
		if command -v pacman >/dev/null 2>&1; then \
			echo "Install with:  sudo pacman -S$$missing"; \
		elif command -v apt >/dev/null 2>&1; then \
			echo "Install with:  sudo apt install libgtk-4-dev libgtk4-layer-shell-dev"; \
		elif command -v dnf >/dev/null 2>&1; then \
			echo "Install with:  sudo dnf install gtk4-devel gtk4-layer-shell-devel"; \
		else \
			echo "Please install:$$missing using your package manager"; \
		fi; \
		echo ""; \
		echo "Then run 'make install' again."; \
		exit 1; \
	fi
	@echo "  gtk4 .............. OK"
	@echo "  gtk4-layer-shell .. OK"

check-rust:
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo ""; \
		echo "Rust toolchain not found."; \
		echo ""; \
		read -p "Install Rust via rustup? [Y/n] " answer; \
		case "$$answer" in \
			[nN]*) echo "Aborted. Install Rust manually: https://rustup.rs"; exit 1 ;; \
			*) curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; \
			   echo ""; \
			   echo "Rust installed. Restart your shell or run:"; \
			   echo "  source $$HOME/.cargo/env"; \
			   echo "Then run 'make install' again."; \
			   exit 1 ;; \
		esac; \
	fi
	@echo "  rust .............. OK ($(shell rustc --version 2>/dev/null | cut -d' ' -f2))"

deps:
	@echo "Installing system dependencies..."
	@if command -v pacman >/dev/null 2>&1; then \
		sudo pacman -S --needed gtk4 gtk4-layer-shell; \
	elif command -v apt >/dev/null 2>&1; then \
		sudo apt install -y libgtk-4-dev libgtk4-layer-shell-dev; \
	elif command -v dnf >/dev/null 2>&1; then \
		sudo dnf install -y gtk4-devel gtk4-layer-shell-devel; \
	else \
		echo "Unknown package manager. Please install gtk4 and gtk4-layer-shell manually."; \
		exit 1; \
	fi
	@echo "Dependencies installed."

# ─────────────────────────────────────────────────────────────────────
# Build
# ─────────────────────────────────────────────────────────────────────

build: check-rust check-deps
	@echo ""
	@echo "Building release binaries..."
	$(CARGO) build $(CARGO_FLAGS)
	@echo ""
	@echo "Build complete. Run 'make install' to install."

# ─────────────────────────────────────────────────────────────────────
# Install
# ─────────────────────────────────────────────────────────────────────

install: build install-bin install-data install-dbus
	@echo ""
	@echo "============================================"
	@echo " Installation complete!"
	@echo "============================================"
	@echo ""
	@echo "Installed binaries:"
	@for bin in $(BINARIES); do \
		echo "  $(BINDIR)/$$bin"; \
	done
	@echo ""
	@echo "Installed data:"
	@echo "  $(DATADIR)/nwg-dock-hyprland/"
	@echo "  $(DATADIR)/nwg-drawer/"
	@echo ""
	@echo "Installed D-Bus service:"
	@echo "  $(DBUS_DIR)/org.freedesktop.Notifications.service"
	@echo ""
	@# Detect compositor and show appropriate setup
	@if [ -n "$$HYPRLAND_INSTANCE_SIGNATURE" ] || [ -d "$$HOME/.config/hypr" ]; then \
		echo "Hyprland detected. Run 'make setup-hyprland' for autostart config."; \
	elif [ -n "$$SWAYSOCK" ] || [ -d "$$HOME/.config/sway" ]; then \
		echo "Sway detected. Run 'make setup-sway' for autostart config."; \
	else \
		echo "Run 'make setup-hyprland' or 'make setup-sway' for autostart config."; \
	fi
	@echo ""

install-bin: stop
	@echo "Installing binaries to $(BINDIR)..."
	@mkdir -p $(BINDIR)
	@for bin in $(BINARIES); do \
		install -m 755 target/release/$$bin $(BINDIR)/$$bin; \
		echo "  $$bin"; \
	done

install-data:
	@echo "Installing data files to $(DATADIR)..."
	@mkdir -p $(DATADIR)/nwg-dock-hyprland/images
	@mkdir -p $(DATADIR)/nwg-drawer/img
	@install -m 644 data/nwg-dock-hyprland/style.css $(DATADIR)/nwg-dock-hyprland/
	@install -m 644 data/nwg-dock-hyprland/images/*.svg $(DATADIR)/nwg-dock-hyprland/images/
	@install -m 644 data/nwg-drawer/drawer.css $(DATADIR)/nwg-drawer/
	@install -m 644 data/nwg-drawer/img/*.svg $(DATADIR)/nwg-drawer/img/
	@echo "  nwg-dock-hyprland/style.css + images"
	@echo "  nwg-drawer/drawer.css + img"

install-dbus:
	@echo "Installing D-Bus service..."
	@mkdir -p $(DBUS_DIR)
	@echo "[D-BUS Service]" > $(DBUS_DIR)/org.freedesktop.Notifications.service
	@echo "Name=org.freedesktop.Notifications" >> $(DBUS_DIR)/org.freedesktop.Notifications.service
	@echo "Exec=$(BINDIR)/nwg-notifications --persist" >> $(DBUS_DIR)/org.freedesktop.Notifications.service
	@echo "  org.freedesktop.Notifications.service"
	@# Offer to disable mako if it's active
	@if systemctl --user is-enabled mako 2>/dev/null | grep -q enabled; then \
		echo ""; \
		echo "  Note: mako notification daemon is enabled."; \
		echo "  To use nwg-notifications instead, run:"; \
		echo "    systemctl --user mask mako"; \
	fi

# ─────────────────────────────────────────────────────────────────────
# Compositor setup
# ─────────────────────────────────────────────────────────────────────
# Process management
# ─────────────────────────────────────────────────────────────────────

stop:
	@stopped=""; \
	for bin in $(BINARIES); do \
		if pidof $$bin >/dev/null 2>&1; then \
			killall $$bin 2>/dev/null && stopped="$$stopped $$bin"; \
		fi; \
	done; \
	if [ -n "$$stopped" ]; then \
		echo "Stopped:$$stopped"; \
		sleep 1; \
	fi

start:
	@echo "Starting dock and notification daemon..."
	@if [ -n "$$HYPRLAND_INSTANCE_SIGNATURE" ]; then \
		nohup $(BINDIR)/nwg-dock-hyprland -d -i 48 --mb 10 --hide-timeout 400 >/dev/null 2>&1 & \
		nohup $(BINDIR)/nwg-notifications --persist >/dev/null 2>&1 & \
	elif [ -n "$$SWAYSOCK" ]; then \
		nohup $(BINDIR)/nwg-dock-hyprland --wm sway -d -i 48 --mb 10 --hide-timeout 400 >/dev/null 2>&1 & \
		nohup $(BINDIR)/nwg-notifications --wm sway --persist >/dev/null 2>&1 & \
	else \
		echo "No compositor detected. Start manually or configure autostart."; \
		exit 1; \
	fi
	@echo "  nwg-dock-hyprland started"
	@echo "  nwg-notifications started"

restart: stop start

upgrade: build stop install-bin install-data start
	@echo ""
	@echo "Upgrade complete — running instances restarted."

# ─────────────────────────────────────────────────────────────────────
# Compositor setup
# ─────────────────────────────────────────────────────────────────────

setup-hyprland:
	@echo ""
	@echo "Add the following to ~/.config/hypr/autostart.conf:"
	@echo "─────────────────────────────────────────────────────"
	@echo "exec-once = uwsm-app -- nwg-dock-hyprland -d -i 48 --mb 10 --hide-timeout 400"
	@echo "exec-once = uwsm-app -- nwg-notifications --persist"
	@echo "─────────────────────────────────────────────────────"
	@echo ""
	@echo "Optional: add these keybindings to ~/.config/hypr/binds.conf:"
	@echo "─────────────────────────────────────────────────────"
	@echo "bind = SUPER, D, exec, pidof nwg-dock-hyprland && pkill -35 nwg-dock-hyprland"
	@echo "bind = SUPER, A, exec, nwg-drawer"
	@echo "─────────────────────────────────────────────────────"
	@echo ""
	@echo "Optional: waybar notification bell module:"
	@echo "─────────────────────────────────────────────────────"
	@echo '"custom/notifications": {'
	@echo '    "exec": "cat $$XDG_RUNTIME_DIR/mac-notifications-status.json 2>/dev/null || echo $${q}{\"text\":\"\",\"alt\":\"empty\",\"class\":\"empty\"}$${q}",'
	@echo '    "return-type": "json",'
	@echo '    "format": "{}",'
	@echo '    "on-click": "pkill -38 nwg-notifications",'
	@echo '    "on-click-right": "pkill -40 nwg-notifications",'
	@echo '    "signal": 11,'
	@echo '    "interval": "once"'
	@echo '}'
	@echo "─────────────────────────────────────────────────────"

setup-sway:
	@echo ""
	@echo "Add the following to ~/.config/sway/config:"
	@echo "─────────────────────────────────────────────────────"
	@echo "exec nwg-dock-hyprland --wm sway -d -i 48 --mb 10 --hide-timeout 400"
	@echo "exec nwg-notifications --wm sway --persist"
	@echo "─────────────────────────────────────────────────────"
	@echo ""
	@echo "Optional keybindings:"
	@echo "─────────────────────────────────────────────────────"
	@echo 'bindsym $$mod+d exec pidof nwg-dock-hyprland && pkill -35 nwg-dock-hyprland'
	@echo 'bindsym $$mod+a exec nwg-drawer --wm sway'
	@echo "─────────────────────────────────────────────────────"

# ─────────────────────────────────────────────────────────────────────
# Uninstall
# ─────────────────────────────────────────────────────────────────────

uninstall:
	@echo "Removing installed files..."
	@for bin in $(BINARIES); do \
		rm -f $(BINDIR)/$$bin && echo "  Removed $(BINDIR)/$$bin"; \
	done
	@rm -rf $(DATADIR)/nwg-dock-hyprland && echo "  Removed $(DATADIR)/nwg-dock-hyprland/"
	@rm -rf $(DATADIR)/nwg-drawer && echo "  Removed $(DATADIR)/nwg-drawer/"
	@rm -f $(DBUS_DIR)/org.freedesktop.Notifications.service && echo "  Removed D-Bus service"
	@echo ""
	@echo "Uninstall complete."
	@echo "Config files in ~/.config/ and cache in ~/.cache/ were left in place."

# ─────────────────────────────────────────────────────────────────────
# Testing
# ─────────────────────────────────────────────────────────────────────

test:
	$(CARGO) test --workspace
	$(CARGO) clippy --all-targets

test-integration: build
	@echo "Running headless Sway integration tests..."
	@bash tests/integration/test_runner.sh

test-all: test test-integration

# ─────────────────────────────────────────────────────────────────────
# Clean
# ─────────────────────────────────────────────────────────────────────

clean:
	$(CARGO) clean
