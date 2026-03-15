# CLAUDE.md — Project Guide

## What is this?

A macOS-style dock + app drawer for Hyprland, written in Rust. Ported from Go (nwg-dock-hyprland + nwg-drawer) with enhancements: multi-monitor, shared pin state, Hyprland IPC cursor tracking, and a Launchpad-style drawer UI.

## Build & test

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo test --workspace         # Run all 37 tests
cargo clippy --all-targets     # Lint (should be zero warnings)
cargo fmt --all                # Format
```

## Install binaries

```bash
cargo build --release
cp target/release/nwg-dock-hyprland-rs ~/.cargo/bin/
cp target/release/nwg-drawer-rs ~/.cargo/bin/
```

## Run locally

```bash
# Dock with auto-hide
nwg-dock-hyprland-rs -d -i 48 --mb 10 --hide-timeout 400

# Drawer
nwg-drawer-rs
```

## Architecture

Three crates in a Cargo workspace:

- **dock-common** — shared library (no GTK dependency in types/IPC)
  - `hyprland/` — IPC socket, event stream, types
  - `desktop/` — .desktop parser, icon resolution, categories, preferred-apps
  - `config/` — XDG paths, CSS loading
  - `pinning.rs`, `launch.rs`, `singleton.rs`, `signals.rs`

- **mac-dock** — dock binary
  - `main.rs` — thin coordinator (~130 lines)
  - `config.rs` — clap CLI with Position/Alignment/Layer enums
  - `context.rs` — DockContext bundles shared refs
  - `dock_windows.rs` — per-monitor window creation
  - `rebuild.rs` — self-referential rebuild function (uses Weak to avoid Rc cycle)
  - `listeners.rs` — pin watcher, signal poller, autohide
  - `events.rs` — Hyprland event stream → smart rebuild
  - `ui/` — window, dock_box, buttons, menus, hotspot (cursor poller), css

- **mac-drawer** — drawer binary
  - `main.rs` — coordinator (~185 lines)
  - `config.rs` — clap CLI with CloseButton enum
  - `state.rs` — DrawerState with AppRegistry sub-struct
  - `desktop_loader.rs` — scans .desktop files, multi-category assignment
  - `listeners.rs` — keyboard, focus detector, file watcher, signals
  - `ui/` — well_builder, search_handler, app_grid, pinned, file_search, widgets, math, power_bar, search, window
  - `assets/drawer.css` — embedded via include_str!()

## Conventions

- **Enums over strings** — Position, Alignment, Layer, CloseButton are all `clap::ValueEnum`
- **Named constants** — all UI dimensions in `ui/constants.rs`
- **DockContext** — bundles config/state/data_home/pinned_file/rebuild for clean function signatures
- **No `#[allow(dead_code)]`** — all code is used
- **No magic numbers** — every numeric literal has a named constant or clear inline comment
- **Error handling** — log errors, never silently discard with `let _ =` (except optional wl-copy)
- **Unsafe** — only 2 blocks, both in signals.rs (required by nix sigaction API), both documented with SAFETY comments
- **Tests** — `#[cfg(test)] mod tests` at bottom of file, test behavior not implementation

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
Replaced GTK hotspot windows with Hyprland IPC `j/cursorpos` polling. Cached monitor list refreshed every ~10s. See `ui/hotspot.rs`.
