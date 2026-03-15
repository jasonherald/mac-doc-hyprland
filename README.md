# mac-dock-hyprland

A macOS-style dock, application launcher, and notification center for [Hyprland](https://hyprland.org/), written in Rust.

Replaces [nwg-dock-hyprland](https://github.com/nwg-piotr/nwg-dock-hyprland), [nwg-drawer](https://github.com/nwg-piotr/nwg-drawer), and [mako](https://github.com/emersion/mako) with a unified, memory-safe implementation.

## Features

### Dock (`nwg-dock-hyprland-rs`)
- **Multi-monitor** — dock appears on all monitors simultaneously
- **Content-width** — floats centered at screen edge, sized to its icons
- **Auto-hide** — Hyprland IPC cursor tracking with configurable timeout
- **Drag-to-reorder** — drag any pinned icon (running or not) to rearrange
- **Drag-to-remove** — drag an icon off the dock to unpin it (like macOS)
- **Dock settings menu** — right-click dock background to lock/unlock arrangement
- **Transparency** — semi-transparent background for a modern look
- **Right-click menus** — pin/unpin, close, toggle floating, fullscreen, move to workspace
- **Middle-click** — launch new instance of any running app
- **Icon scaling** — icons shrink automatically when many apps are open
- **Instant pin sync** — inotify-based, shared with the drawer

### Drawer (`nwg-drawer-rs`)
- **Full-screen overlay** — dark transparent Launchpad-style launcher
- **Unified well** — favorites section with divider, then all apps in a grid
- **Subsequence search** — type to filter apps by name, description, or command
- **File search** — columnar results with system theme icons, sorted alphabetically
- **Math evaluation** — type expressions like `2+2` and get results with clipboard copy
- **Command execution** — prefix with `:` to run arbitrary commands
- **Pin sync** — shared pin file with the dock, changes reflect instantly on both

### Notification Center (`mac-notifications-rs`)
- **D-Bus notification daemon** — replaces mako, claims `org.freedesktop.Notifications`
- **Popup toasts** — top-right corner, auto-dismiss, click-to-focus sending app
- **Action buttons** — shows Reply/Open/etc. buttons, emits ActionInvoked D-Bus signal
- **History panel** — slide-out from right, grouped by app, unread dot indicators
- **Click-outside-to-close** — backdrop overlay + Escape key
- **Dismiss controls** — per-notification, per-app group, or clear all
- **Do Not Disturb** — toggle via panel button, signal, or waybar right-click menu
- **Timed DND** — 1 hour, 2 hours, until tomorrow with expiry countdown
- **Waybar integration** — bell icon with unread count, left-click toggles panel, right-click opens DND menu
- **Persistence** — notification history saved across restarts with `--persist`
- **Focused monitor** — popups appear on the currently focused monitor

### Shared (`dock-common`)
- Custom Hyprland IPC implementation (no external crate dependency)
- XDG desktop entry parser with locale support
- Icon resolution with theme fallbacks
- Pin management with file persistence
- Signal handling (real-time signals via raw libc for SIGRTMIN+N support)
- Single-instance enforcement with stale lock detection

## Installation

```bash
cargo install --path crates/mac-dock
cargo install --path crates/mac-drawer
cargo install --path crates/mac-notifications
```

### Dependencies

- GTK4
- gtk4-layer-shell
- Hyprland (running)

On Arch Linux:
```bash
pacman -S gtk4 gtk4-layer-shell
```

## Usage

### Dock

```bash
# Basic — auto-hide, 48px icons, 10px bottom margin, 400ms hide timeout
nwg-dock-hyprland-rs -d -i 48 --mb 10 --hide-timeout 400
```

### Drawer

```bash
# Resident mode (stays in memory, toggle with signals)
nwg-drawer-rs -r
```

### Notification Center

```bash
# With history persistence
mac-notifications-rs --persist
```

### Hyprland autostart

```ini
# ~/.config/hypr/autostart.conf
exec-once = uwsm-app -- nwg-dock-hyprland-rs -d -i 48 --mb 10 --hide-timeout 400
exec-once = uwsm-app -- mac-notifications-rs --persist
```

### D-Bus service (auto-start on first notification)

```ini
# ~/.local/share/dbus-1/services/org.freedesktop.Notifications.service
[D-BUS Service]
Name=org.freedesktop.Notifications
Exec=/home/YOU/.cargo/bin/mac-notifications-rs --persist
```

### Signal control

```bash
# Toggle dock visibility
pkill -f -35 nwg-dock-hyprland-rs     # SIGRTMIN+1

# Toggle notification panel
pkill -f -38 mac-notifications-rs      # SIGRTMIN+4

# Toggle DND
pkill -f -39 mac-notifications-rs      # SIGRTMIN+5

# Open DND menu
pkill -f -40 mac-notifications-rs      # SIGRTMIN+6
```

### Waybar module

Add to `~/.config/waybar/config.jsonc`:
```jsonc
"custom/notifications": {
    "exec": "cat $XDG_RUNTIME_DIR/mac-notifications-status.json 2>/dev/null || echo '{\"text\":\"\",\"alt\":\"empty\",\"class\":\"empty\"}'",
    "return-type": "json",
    "format": "{}",
    "on-click": "pkill -f -38 mac-notifications-rs",
    "on-click-right": "pkill -f -40 mac-notifications-rs",
    "signal": 11,
    "interval": "once"
}
```

## Architecture

```
mac-dock-hyprland/
├── crates/
│   ├── dock-common/           # Shared library
│   ├── mac-dock/              # Dock binary
│   ├── mac-drawer/            # Drawer binary
│   └── mac-notifications/     # Notification daemon
```

- **Four crates** in a Cargo workspace
- **52 tests** with zero clippy warnings
- Type-safe enums for all configuration
- Named constants for all UI dimensions
- GTK4 + gtk4-layer-shell for Wayland layer surfaces
- Zero new dependencies for notification daemon (gio D-Bus is already in GTK4)

## Shared pin file

Both dock and drawer read/write `~/.cache/mac-dock-pinned`. Changes are detected instantly via inotify. Pin an app from either the dock (right-click → Pin) or the drawer (right-click any app). Drag icons in the dock to reorder; drag off to unpin.

## Credits

Ported from the Go implementations by [Piotr Miller](https://github.com/nwg-piotr):
- [nwg-dock-hyprland](https://github.com/nwg-piotr/nwg-dock-hyprland) (MIT)
- [nwg-drawer](https://github.com/nwg-piotr/nwg-drawer) (MIT)

## License

MIT
