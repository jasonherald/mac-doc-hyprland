# CLAUDE.md — Project Guide

## What is this?

A macOS-style dock, app drawer, and notification center for Hyprland (Sway support coming), written in Rust. Ported from Go (nwg-dock-hyprland + nwg-drawer) with enhancements: multi-monitor, shared pin state, compositor-abstracted IPC, Launchpad-style drawer, and a full notification daemon replacing mako.

## Build & test

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo test --workspace         # Run all tests
cargo clippy --all-targets     # Lint (should be zero warnings)
cargo fmt --all                # Format
```

## Install

```bash
cargo install --path crates/mac-dock
cargo install --path crates/mac-drawer
cargo install --path crates/mac-notifications
```

## Run locally

```bash
# Dock with auto-hide
nwg-dock-hyprland -d -i 48 --mb 10 --hide-timeout 400

# Drawer
nwg-drawer

# Notification daemon with persistence
nwg-notifications --persist

# Override compositor detection (auto-detects from env vars)
nwg-dock-hyprland --wm hyprland
```

## Architecture

Four crates in a Cargo workspace:

- **dock-common** — shared library (no GTK dependency in types/IPC)
  - `compositor/` — trait-based compositor abstraction (Hyprland backend, Sway planned)
  - `hyprland/` — Hyprland IPC socket, event stream, types (internal to compositor backend)
  - `desktop/` — .desktop parser, icon resolution, categories, preferred-apps
  - `config/` — XDG paths, CSS loading
  - `pinning.rs`, `launch.rs`, `singleton.rs`, `signals.rs`

- **mac-dock** — dock binary (`nwg-dock-hyprland`)
  - `main.rs` — thin coordinator (~130 lines)
  - `config.rs` — clap CLI with Position/Alignment/Layer enums
  - `context.rs` — DockContext bundles shared refs + compositor
  - `dock_windows.rs` — per-monitor window creation
  - `rebuild.rs` — self-referential rebuild function (uses Weak to avoid Rc cycle)
  - `listeners.rs` — pin watcher, signal poller, autohide
  - `events.rs` — compositor event stream → smart rebuild
  - `ui/` — window, dock_box, buttons, menus, hotspot (cursor poller), drag, dock_menu, css

- **mac-drawer** — drawer binary (`nwg-drawer`)
  - `main.rs` — coordinator (~185 lines)
  - `config.rs` — clap CLI with CloseButton enum
  - `state.rs` — DrawerState with AppRegistry sub-struct + compositor
  - `desktop_loader.rs` — scans .desktop files, multi-category assignment
  - `listeners.rs` — keyboard, focus detector, file watcher, signals
  - `ui/` — well_builder, search_handler, app_grid, pinned, file_search, widgets, math, power_bar, search, window
  - `assets/drawer.css` — embedded via include_str!()

- **mac-notifications** — notification daemon (`nwg-notifications`)
  - `main.rs` — coordinator (~160 lines)
  - `config.rs` — clap CLI with PopupPosition enum
  - `notification.rs` — Notification struct, Urgency enum, action parsing
  - `state.rs` — NotificationState: history, groups, DND, dnd_expires
  - `dbus.rs` — gio D-Bus server (org.freedesktop.Notifications), ActionInvoked signal
  - `listeners.rs` — signal poller (panel toggle, DND toggle, DND menu)
  - `persistence.rs` — save/load history as JSON
  - `waybar.rs` — status file + waybar signal (SIGRTMIN+11)
  - `ui/` — popup, panel, panel_content, notification_row, dnd_menu, icons, window, css, constants
  - `assets/notifications.css` — embedded via include_str!()

## Conventions

- **Enums over strings** — Position, Alignment, Layer, CloseButton, PopupPosition, Urgency are all `clap::ValueEnum` or repr enums
- **Named constants** — all UI dimensions in `ui/constants.rs`
- **DockContext** — bundles config/state/data_home/pinned_file/rebuild/compositor for clean function signatures
- **Compositor trait** — all WM IPC goes through `dyn Compositor` (dock-common/src/compositor/traits.rs), never direct hyprland calls from binaries
- **No `#[allow(dead_code)]`** — all code is used
- **No magic numbers** — every numeric literal has a named constant or clear inline comment
- **Error handling** — log errors, never silently discard with `let _ =` (except optional wl-copy)
- **Unsafe** — only in signals.rs / listeners.rs (required for RT signal handling via raw libc), documented with SAFETY comments
- **Tests** — `#[cfg(test)] mod tests` at bottom of file, test behavior not implementation
- **Shared icon resolution** — `ui/icons.rs` module with `resolve_popup_icon` (pixbuf) and `resolve_theme_icon` (theme-only, avoids glycin crashes)

## Compositor abstraction

All compositor IPC goes through the `Compositor` trait in `dock-common/src/compositor/`. Auto-detection checks `HYPRLAND_INSTANCE_SIGNATURE` and `SWAYSOCK` env vars. Override with `--wm hyprland` or `--wm sway`.

Key types: `WmClient`, `WmMonitor`, `WmWorkspace`, `WmEvent` (compositor-neutral).

Backends:
- `compositor/hyprland.rs` — wraps `hyprland/ipc.rs`, converts Hyprland types to Wm types
- `compositor/sway.rs` — planned (Phase B)

## Signal assignments

| Signal | Value | Target | Action |
|--------|-------|--------|--------|
| SIGRTMIN+1 | 35 | Dock/Drawer | Toggle visibility |
| SIGRTMIN+2 | 36 | Dock/Drawer | Show |
| SIGRTMIN+3 | 37 | Dock/Drawer | Hide |
| SIGRTMIN+4 | 38 | Notifications | Toggle panel |
| SIGRTMIN+5 | 39 | Notifications | Toggle DND |
| SIGRTMIN+6 | 40 | Notifications | Show DND menu |
| SIGRTMIN+11 | 45 | Waybar | Refresh notification module |

## Shared pin file

`~/.cache/mac-dock-pinned` — one desktop ID per line, no `.desktop` suffix. Both dock and drawer read/write this file. Changes detected via inotify (dock) and notify crate (drawer).

## Key patterns

### GTK4 button layout
GTK4 has no `SetImage`/`SetImagePosition`. Use a vertical Box:
```rust
let vbox = Box::new(Orientation::Vertical, 4);
vbox.append(&image);
vbox.append(&label);
button.set_child(Some(&vbox));
```
Shared helper: `ui::widgets::app_icon_button()`

### Self-referential rebuild
The dock rebuild function needs to pass itself to buttons (for pin/unpin rebuild). Uses `Weak` reference to avoid Rc cycle. See `rebuild.rs`.

### Cursor-based autohide
Uses compositor IPC cursor position polling (Hyprland `j/cursorpos`). Cached monitor list refreshed every ~10s. See `ui/hotspot.rs`. Sway will use GTK hotspot windows since it lacks cursor position IPC.

### Drag-to-reorder
GTK4 DragSource on each pinned button (including running apps), single DropTarget on the dock box. Cursor poller tracks `drag_outside_dock` state for unpin-by-drag-off. Preview icon cached to avoid glycin reentrancy crashes. Rebuilds deferred via `idle_add_local_once`. Lock state persisted in `~/.cache/mac-dock-locked`. See `ui/drag.rs`, `ui/dock_menu.rs`.

### Click-outside-to-close
Panel and DND menu use a transparent backdrop layer-shell surface behind them. The backdrop must have non-zero opacity (`rgba(0,0,0,0.01)` minimum) for the compositor to deliver input events. Clicking the backdrop hides both the backdrop and the menu/panel.

### D-Bus notification server
Uses gio's `bus_own_name` + `register_object` — runs directly on the glib main loop with no async bridge. D-Bus connection stored in `NotificationState` for emitting `ActionInvoked` signals when action buttons are clicked.

### on_state_change callback
A shared `Rc<dyn Fn()>` threaded through panel, popup, listeners, and D-Bus callbacks. Fires on any state mutation to save history + update waybar. Avoids polling or observer patterns.
