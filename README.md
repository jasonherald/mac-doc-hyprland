# mac-dock-hyprland

A macOS-style dock, application launcher, and notification center for [Hyprland](https://hyprland.org/) and [Sway](https://swaywm.org/), written in Rust.

Replaces [nwg-dock-hyprland](https://github.com/nwg-piotr/nwg-dock-hyprland), [nwg-drawer](https://github.com/nwg-piotr/nwg-drawer), and [mako](https://github.com/emersion/mako) with a unified, memory-safe implementation.

## Features

### Dock (`nwg-dock-hyprland`)
- **Multi-monitor** — dock appears on all monitors simultaneously
- **Content-width** — floats centered at screen edge, sized to its icons
- **Auto-hide** — compositor IPC cursor tracking with configurable timeout
- **Drag-to-reorder** — drag any pinned icon (running or not) to rearrange
- **Drag-to-remove** — drag an icon off the dock to unpin it (like macOS)
- **Dock settings menu** — right-click dock background to lock/unlock arrangement
- **Configurable opacity** — `--opacity 0-100` for translucent or opaque dock
- **Right-click menus** — pin/unpin, close, toggle floating, fullscreen, move to workspace
- **Launch animation** — optional bounce animation on dock icons while an app is starting (`--launch-animation`)
- **Middle-click** — launch new instance of any running app
- **Monitor hotplug** — dock windows reconcile automatically when monitors are added/removed
- **Rotated/scaled monitors** — cursor tracking works correctly with portrait and scaled displays
- **Icon scaling** — icons shrink automatically when many apps are open
- **Instant pin sync** — inotify-based, shared with the drawer
- **Kill-proof** — ignores compositor close requests (e.g. Hyprland `killactive` / Super+Q) so the dock can't be accidentally closed; use `make stop` or `pkill -f nwg-dock-hyprland` to stop it intentionally
- **Go flag compatibility** — accepts original Go nwg-dock-hyprland flag names

### Drawer (`nwg-drawer`)
- **Full-screen overlay** — dark transparent Launchpad-style launcher
- **Keyboard navigation** — arrow keys between icons, Enter to launch, type to search
- **Category filtering** — filter bar with per-category buttons
- **Description line** — status bar shows app description on hover/focus
- **Power bar** — lock/exit/reboot/sleep/poweroff with `--pb-auto` auto-detection
- **Configurable opacity** — `--opacity 0-100` for background transparency
- **Subsequence search** — type to filter apps by name, description, or command
- **File search** — columnar results with system theme icons, sorted alphabetically
- **Math evaluation** — type expressions like `2+2` and get results with clipboard copy
- **Command execution** — prefix with `:` to run arbitrary commands
- **Pin sync** — shared pin file with the dock, changes reflect instantly on both
- **Go flag compatibility** — accepts original Go nwg-drawer flag names (`--pbexit`, `--nocats`, etc.)

### Notification Center (`nwg-notifications`)
- **D-Bus notification daemon** — replaces mako, claims `org.freedesktop.Notifications`
- **Popup toasts** — top-right corner, auto-dismiss, click-to-focus sending app
- **Deep-linking** — clicking a notification tells the app to open the specific item
- **Auto-dismiss** — popups dismissed when app calls CloseNotification (e.g., Slack read)
- **Action buttons** — shows Reply/Open/etc. buttons, emits ActionInvoked D-Bus signal
- **History panel** — slide-out from right, grouped by app with collapse/expand
- **Click-outside-to-close** — backdrop overlay + Escape key
- **Dismiss controls** — per-notification, per-app group, or clear all
- **Do Not Disturb** — toggle via panel button, signal, or waybar right-click menu
- **Timed DND** — 1 hour, 2 hours, until tomorrow with expiry countdown
- **Waybar integration** — bell icon with unread count, left-click toggles panel, right-click opens DND menu
- **Persistence** — notification history saved across restarts with `--persist`
- **Focused monitor** — popups appear on the currently focused monitor

### Shared (`nwg-dock-common`)
- Compositor abstraction layer (Hyprland and Sway supported)
- Custom IPC implementation (no external crate dependency)
- XDG desktop entry parser with locale support
- Icon resolution with theme fallbacks
- Pin management with file persistence
- Signal handling (real-time signals via raw libc for SIGRTMIN+N support)
- Single-instance enforcement with stale lock detection

## Installation

### Quick install

```bash
git clone https://github.com/jasonherald/mac-doc-hyprland.git
cd mac-dock-hyprland
make install
```

This will:
1. Check for system dependencies (gtk4, gtk4-layer-shell) — tells you what to install if missing
2. Check for Rust — offers to install via rustup if not found
3. Build all three binaries in release mode
4. Install binaries to `~/.cargo/bin/`
5. Install data files (icons, default CSS) to `~/.local/share/`
6. Create the D-Bus notification service

After install, run `make setup-hyprland` or `make setup-sway` for autostart configuration.

### Install system dependencies

If `make install` reports missing dependencies:

```bash
# Arch Linux
sudo pacman -S gtk4 gtk4-layer-shell

# Ubuntu/Debian
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev

# Fedora
sudo dnf install gtk4-devel gtk4-layer-shell-devel
```

Or just run `make deps` to auto-detect and install.

### Compositor setup

After installing, configure your compositor to start the dock and notifications:

```bash
# Print Hyprland config snippets
make setup-hyprland

# Print Sway config snippets
make setup-sway
```

These print the autostart entries and optional keybindings for you to add to your config.

### Other make targets

```bash
make build           # Build without installing
make upgrade         # Rebuild + stop running instances + reinstall + restart
make stop            # Stop all running instances
make start           # Start dock and notification daemon
make restart         # Stop then start
make uninstall       # Remove all installed files
make deps            # Install system dependencies (requires sudo)
make clean           # Remove build artifacts
make help            # Show all targets
```

### Manual install (advanced)

If you prefer not to use Make:

```bash
cargo install --path crates/nwg-dock
cargo install --path crates/nwg-drawer
cargo install --path crates/nwg-notifications
```

You'll also need to manually install the data files from `data/` to `~/.local/share/`.

## Usage

### Dock

```bash
# Basic — auto-hide, 48px icons, translucent
nwg-dock-hyprland -d -i 48 --mb 10 --hide-timeout 400 --opacity 75

# With launch animation and drawer
nwg-dock-hyprland -d -i 48 --mb 10 --hide-timeout 400 --opacity 75 --launch-animation -c "nwg-drawer --pb-auto"
```

### Drawer

```bash
# Basic with auto-detected power bar
nwg-drawer --pb-auto

# Fully configured
nwg-drawer --opacity 88 --pb-auto --columns 8

# Resident mode (stays in memory, toggle with signals)
nwg-drawer -r --pb-auto
```

### Notification Center

```bash
# With history persistence
nwg-notifications --persist
```

### Hyprland autostart

```ini
# ~/.config/hypr/autostart.conf
exec-once = uwsm-app -- nwg-dock-hyprland -d -i 48 --mb 10 --hide-timeout 400 --opacity 75 --launch-animation -c "nwg-drawer --opacity 88 --pb-auto"
exec-once = uwsm-app -- nwg-notifications --persist
```

### D-Bus service (auto-start on first notification)

```ini
# ~/.local/share/dbus-1/services/org.freedesktop.Notifications.service
[D-BUS Service]
Name=org.freedesktop.Notifications
Exec=/home/YOU/.cargo/bin/nwg-notifications --persist
```

### Signal control

```bash
# Toggle dock visibility
pkill -f -35 nwg-dock-hyprland     # SIGRTMIN+1

# Toggle notification panel
pkill -f -38 nwg-notifications      # SIGRTMIN+4

# Toggle DND
pkill -f -39 nwg-notifications      # SIGRTMIN+5

# Open DND menu
pkill -f -40 nwg-notifications      # SIGRTMIN+6
```

### Waybar module

Add to `~/.config/waybar/config.jsonc`:
```jsonc
"custom/notifications": {
    "exec": "cat $XDG_RUNTIME_DIR/mac-notifications-status.json 2>/dev/null || echo '{\"text\":\"\",\"alt\":\"empty\",\"class\":\"empty\"}'",
    "return-type": "json",
    "format": "{}",
    "on-click": "pkill -f -38 nwg-notifications",
    "on-click-right": "pkill -f -40 nwg-notifications",
    "signal": 11,
    "interval": "once"
}
```

## Architecture

```
mac-dock-hyprland/
├── crates/
│   ├── nwg-dock-common/       # Shared library
│   ├── nwg-dock/              # Dock binary
│   ├── nwg-drawer/            # Drawer binary
│   └── nwg-notifications/     # Notification daemon
```

- **Four crates** in a Cargo workspace
- **244 tests** (226 unit + 18 integration) with zero clippy warnings
- Type-safe enums for all configuration
- Named constants for all UI dimensions
- GTK4 + gtk4-layer-shell for Wayland layer surfaces
- Zero new dependencies for notification daemon (gio D-Bus is already in GTK4)

## Code Quality

This project maintains high standards through automated analysis:

| Tool | Status | What it checks |
|------|--------|---------------|
| **Cargo Clippy** | Zero warnings | Rust idioms, correctness, performance |
| **SonarQube** | 0 issues, 0% duplication | Cognitive complexity, code smells, duplications, security |
| **CodeRabbit** | All findings addressed | AI-driven code review, security patterns, best practices |
| **Unit Tests** | 226 passing | Sway tree traversal, notification state, pinning, parsing, config, monitor transforms |
| **Integration Tests** | 18 passing | Headless Sway: IPC, window management, signals, multi-monitor |

Run locally:
```bash
make test              # Unit tests + clippy
make test-integration  # Headless Sway integration tests (requires sway, foot)
make sonar             # SonarQube scan (requires sonar-scanner + .env token)
```

## Shared pin file

Both dock and drawer read/write `~/.cache/mac-dock-pinned`. Changes are detected instantly via inotify. Pin an app from either the dock (right-click → Pin) or the drawer (right-click any app). Drag icons in the dock to reorder; drag off to unpin.

## Theming

Both the dock and the drawer load CSS from user-writable config files. Changes are picked up **instantly via inotify** — no restart, no signal, no reload command needed. Just save the file and the new styles apply live.

### CSS file locations

| Binary | Path |
|--------|------|
| `nwg-dock-hyprland` | `~/.config/nwg-dock-hyprland/style.css` |
| `nwg-drawer` | `~/.config/nwg-drawer/drawer.css` |

Override with `-s /path/to/custom.css` if you prefer a different location.

### Priority layers

Three CSS layers are stacked, highest priority last:

1. **Embedded defaults** — compact button sizing, indicator spacing, etc.
2. **Programmatic overrides** — `--opacity` and bounce animation keyframes
3. **Your CSS file** — always wins

This means your CSS file can override anything, including the `--opacity` flag. If you set `background-color` in your file, that's what you get.

### Smooth transitions

GTK4 supports `transition:` properties on most CSS properties. Add them to your own CSS for smooth hover effects, state changes, etc:

```css
button {
    transition: background-color 200ms ease, opacity 200ms ease;
}
```

### base16 themes via tinty

[tinty](https://github.com/tinted-theming/tinty) is a base16 theme manager. Combined with [@BlueInGreen68's base16-nwg-dock](https://git.sr.ht/~blueingreen/base16-nwg-dock) templates, you can switch themes live across your whole system.

**Setup** (one-time):

```bash
# Install tinty
cargo install tinty

# Initialize config
tinty init

# Add the base16-nwg-dock templates to ~/.config/tinted-theming/tinty/config.toml:
```

```toml
[[items]]
name = "base16-nwg-dock-hyprland"
path = "https://git.sr.ht/~blueingreen/base16-nwg-dock"
themes-dir = "themes"
hook = "cp '%f' ~/.config/nwg-dock-hyprland/style.css"
supported-systems = ["base16"]

[[items]]
name = "base16-nwg-drawer"
path = "https://git.sr.ht/~blueingreen/base16-nwg-dock"
themes-dir = "themes"
hook = "cp '%f' ~/.config/nwg-drawer/drawer.css"
supported-systems = ["base16"]
```

**Apply a theme**:

```bash
tinty apply base16-tokyo-night-dark
```

The dock and drawer will update **instantly** — no restart required. Pair with tinty's `apply` hook on other apps (foot, waybar, alacritty, etc.) to retheme your whole session with one command.

## Deviations from Go originals

Intentional differences from the original Go nwg-dock-hyprland and nwg-drawer:

- **Shared pin file** — the Go dock uses `~/.cache/nwg-dock-pinned` and the Go drawer uses `~/.cache/nwg-pin-cache` (separate files). This Rust port shares a single pin file (`~/.cache/mac-dock-pinned`) between dock and drawer so changes in either are instantly reflected in the other.
- **Per-monitor dock windows** — the Go dock creates one window; the Rust dock creates a separate window per monitor for better multi-monitor support.
- **Smart rebuild** — the Go dock force-rebuilds on every active window event; the Rust dock only rebuilds when the client class list or active window actually changes (more efficient).
- **Compositor abstraction** — the Go versions are Hyprland-only; this Rust port supports Hyprland and Sway via a trait-based compositor abstraction with auto-detection.
- **Math evaluation** — the Go drawer uses the `expr` library for arbitrary expression evaluation; the Rust port uses a custom arithmetic parser (safer, covers the common use case).
- **Drag-to-reorder** — new feature not in the Go dock: drag pinned icons to rearrange, drag off to unpin.
- **CLI flag naming** — multi-word flags standardized to kebab-case (e.g., `--nocats` → `--no-cats`, `--pbsize` → `--pb-size`). Multi-char Go short forms (`-hd`, `-iw`, `-is`) not available in clap (single-char only); use the long forms instead.
- **Fuzzy class matching** — compositor classes with hyphens vs spaces (e.g., desktop file `github-desktop` vs compositor class `github desktop`) are matched automatically for correct icon display and process grouping.
- **Launcher auto-detection** — if the configured launcher command (default `nwg-drawer`) is not found on PATH, the launcher button is automatically hidden with a log message.

## Credits

Ported from the Go implementations by [Piotr Miller](https://github.com/nwg-piotr):
- [nwg-dock-hyprland](https://github.com/nwg-piotr/nwg-dock-hyprland) (MIT)
- [nwg-drawer](https://github.com/nwg-piotr/nwg-drawer) (MIT)

## License

MIT
