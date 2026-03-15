# mac-dock-hyprland

A macOS-style dock and application launcher for [Hyprland](https://hyprland.org/), written in Rust.

Replaces [nwg-dock-hyprland](https://github.com/nwg-piotr/nwg-dock-hyprland) and [nwg-drawer](https://github.com/nwg-piotr/nwg-drawer) with a unified, memory-safe implementation that adds inter-app communication, multi-monitor support, and a polished Launchpad-style UI.

## Features

### Dock (`nwg-dock-hyprland-rs`)
- **Multi-monitor** — dock appears on all monitors simultaneously
- **Content-width** — floats centered at screen edge, sized to its icons
- **Auto-hide** — Hyprland IPC cursor tracking with configurable timeout
- **Drag-to-reorder** — click and drag pinned icons to rearrange, order persists
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

### Shared (`dock-common`)
- Custom Hyprland IPC implementation (no external crate dependency)
- XDG desktop entry parser with locale support
- Icon resolution with theme fallbacks
- Pin management with file persistence
- Signal handling (SIGRTMIN+1/2/3 for show/hide/toggle)
- Single-instance enforcement with stale lock detection

## Installation

### From source

```bash
cargo build --release
cp target/release/nwg-dock-hyprland-rs ~/.cargo/bin/
cp target/release/nwg-drawer-rs ~/.cargo/bin/
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

# Full width with exclusive zone
nwg-dock-hyprland-rs -f -x -i 48

# Specific monitor only
nwg-dock-hyprland-rs -d -i 48 -o DP-1
```

### Drawer

```bash
# Basic launch
nwg-drawer-rs

# Resident mode (stays in memory, toggle with signals)
nwg-drawer-rs -r

# Custom icon size and columns
nwg-drawer-rs --icon-size 72 -c 8
```

### Hyprland config

```ini
# ~/.config/hypr/autostart.conf
exec-once = uwsm-app -- nwg-dock-hyprland-rs -d -i 48 --mb 10 --hide-timeout 400
```

### Signal control

```bash
# Toggle dock visibility
pkill -SIGRTMIN+1 nwg-dock-hyprland-rs

# Show/hide drawer
pkill -SIGUSR1 nwg-drawer-rs
```

## Architecture

```
mac-dock-hyprland/
├── crates/
│   ├── dock-common/       # Shared library (Hyprland IPC, desktop entries, pinning)
│   ├── mac-dock/          # Dock binary
│   └── mac-drawer/        # Drawer binary
└── original/              # Go source reference (gitignored)
```

- **5,500+ lines** of Rust across 55+ files
- **37 tests** with zero clippy warnings
- Type-safe enums for all configuration (no stringly-typed APIs)
- Named constants for all UI dimensions
- GTK4 + gtk4-layer-shell for Wayland layer surfaces

## Shared pin file

Both dock and drawer read/write `~/.cache/mac-dock-pinned`. Changes are detected instantly via inotify. Pin an app from either the dock (right-click → Pin) or the drawer (right-click any app). Drag icons in the dock to reorder; drag off to unpin.

## Credits

Ported from the Go implementations by [Piotr Miller](https://github.com/nwg-piotr):
- [nwg-dock-hyprland](https://github.com/nwg-piotr/nwg-dock-hyprland) (MIT)
- [nwg-drawer](https://github.com/nwg-piotr/nwg-drawer) (MIT)

## License

MIT
