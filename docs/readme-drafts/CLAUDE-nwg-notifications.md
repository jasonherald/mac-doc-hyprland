# CLAUDE.md — nwg-notifications

## What is this?

A D-Bus notification daemon + notification center for Hyprland and Sway, written in Rust. Claims `org.freedesktop.Notifications`, shows popup toasts, and ships a slide-out history panel with Do-Not-Disturb and waybar integration. Replaces mako in the mac-doc-hyprland stack; runs standalone.

Consumes [`nwg-common`](https://github.com/jasonherald/nwg-common) for compositor IPC, `.desktop` parsing, icon resolution, signal plumbing, and layer-shell backdrops.

Pre-split (before v0.3.0) this lived inside the [mac-doc-hyprland](https://github.com/jasonherald/mac-doc-hyprland) monorepo at `crates/nwg-notifications/`.

## Build & test

```bash
cargo build                   # Debug build
cargo build --release         # Release build
cargo test                    # Unit tests
cargo clippy --all-targets    # Lint (should be zero warnings)
cargo fmt --all               # Format
make test                     # Unit tests + clippy
make test-integration         # Headless Sway integration tests (requires sway)
make lint                     # Full check: fmt + clippy + test + deny + audit
```

Per [tests/integration/CLASSIFICATION.md](https://github.com/jasonherald/mac-doc-hyprland/blob/main/tests/integration/CLASSIFICATION.md) in the monorepo, this repo owns daemon-launch + signal-resilience tests (SIGRTMIN+4 panel toggle, SIGRTMIN+5 DND toggle). The D-Bus `notify-send` path isn't exercised in integration today because the test harness uses an isolated D-Bus to avoid interfering with the real desktop session.

## Install (dev workflow)

**Use the no-sudo invocation when iterating locally.** Default `make install` is system-wide:

```bash
make install PREFIX=$HOME/.local BINDIR=$HOME/.cargo/bin
make install-dbus
```

`make install-dbus` is ALWAYS user-scope (no sudo; drops the service file into `~/.local/share/dbus-1/services/`) regardless of `PREFIX`. D-Bus user services are per-user by convention, and installing the service file system-wide would break auto-activation for other users.

See the README for the full install matrix.

## Run locally

```bash
# With history persistence
nwg-notifications --persist

# Force Sway backend (auto-detection is usually enough)
nwg-notifications --wm sway --persist
```

The daemon auto-starts the first time any app calls `org.freedesktop.Notifications` once the D-Bus service file is registered — explicit `exec-once` isn't strictly required, but makes the first toast faster.

## What lives where

```text
src/
├── main.rs            # Coordinator (~160 lines)
├── config.rs          # clap CLI with PopupPosition enum
├── notification.rs    # Notification struct, Urgency enum, action parsing
├── state.rs           # NotificationState: history, groups, DND, dnd_expires
├── dbus.rs            # gio D-Bus server (org.freedesktop.Notifications)
├── listeners.rs       # Signal poller (panel toggle, DND toggle, DND menu)
├── persistence.rs     # Save/load history as JSON
├── waybar.rs          # Status file + waybar signal (SIGRTMIN+11)
└── ui/
    ├── popup.rs              # Auto-dismissing toasts
    ├── panel.rs, panel_content.rs  # History panel
    ├── notification_row.rs
    ├── dnd_menu.rs           # DND duration picker
    ├── icons.rs              # Icon resolution (pixbuf + theme variants)
    ├── window.rs, css.rs, constants.rs

assets/
└── notifications.css  # Embedded default CSS via include_str!()

data/
└── org.freedesktop.Notifications.service  # D-Bus service file (installed by make install-dbus)
```

## Conventions

- **Enums over strings** — PopupPosition, Urgency are `clap::ValueEnum` or repr enums.
- **Named constants** — all UI dimensions in `ui/constants.rs`.
- **D-Bus server uses `gio::bus_own_name`** — no async bridge; runs directly on the glib main loop. D-Bus connection stored in `NotificationState` for emitting `ActionInvoked` signals when action buttons are clicked.
- **Backdrop windows have non-zero opacity** — `rgba(0,0,0,0.01)` minimum. Without that, the compositor doesn't deliver pointer events to the layer-shell surface; clicks pass through to whatever's underneath.
- **`on_state_change` callback** — a shared `Rc<dyn Fn()>` threaded through panel, popup, listeners, and D-Bus callbacks. Fires on any state mutation to save history + update waybar. Avoids polling or observer patterns.
- **No `#[allow(dead_code)]`, no magic numbers, log errors, tests at bottom of file.**
- **Protocol compliance** — every change to the D-Bus surface must be checked against the [Desktop Notifications Specification](https://specifications.freedesktop.org/notification-spec/latest/). The spec is small but strict.

## Key patterns

### D-Bus notification server

`gio::bus_own_name` + `register_object` on the session bus. D-Bus method calls are dispatched from a vtable to handler functions in `dbus.rs`. `ActionInvoked` signals go out via the stored `BusConnection` when users click action buttons.

### Deep-linking

When a notification click is acknowledged and the sending app has a known .desktop entry, the daemon focuses the app via compositor IPC (`Compositor::focus_window`) and then falls through to the desktop entry's Exec for "open the specific item" behavior.

### Auto-dismiss and CloseNotification

Apps like Slack emit `CloseNotification` when the user reads the message in-app. The daemon matches by notification ID and removes the popup + history entry.

### Click-outside-to-close

Panel and DND menu use a transparent backdrop layer-shell surface behind them. Backdrop must have non-zero opacity (`rgba(0,0,0,0.01)` minimum) for the compositor to deliver input events. Clicking the backdrop hides both the backdrop and the menu/panel.

## Signal control

| Signal | Value | Action |
|--------|-------|--------|
| SIGRTMIN+4 | ~38 | Toggle notification panel |
| SIGRTMIN+5 | ~39 | Toggle DND |
| SIGRTMIN+6 | ~40 | Show DND duration menu |
| SIGRTMIN+11 | ~45 | (outbound) Waybar notification-module refresh |

(Values approximate — glibc/musl differ; see `nwg_common::signals::sigrtmin()`.)

## Waybar integration

The daemon writes its current state to `$XDG_RUNTIME_DIR/mac-notifications-status.json` (unread count, DND status, etc.) and signals waybar via `SIGRTMIN+11` on state change — no polling. Waybar's `signal: 11` handler re-reads the file. See the README for the waybar module config snippet.

## See also

- `CHANGELOG.md` — user-visible changes per release, Keep-a-Changelog format.
- `README.md` — public-facing docs + install matrix + waybar config + D-Bus service file setup.
- [`nwg-common`](https://github.com/jasonherald/nwg-common) — shared library (Compositor trait for focus-on-click, icons, signals, layer-shell backdrops).
- Parent monorepo archive: [jasonherald/mac-doc-hyprland](https://github.com/jasonherald/mac-doc-hyprland).
- [Desktop Notifications Specification](https://specifications.freedesktop.org/notification-spec/latest/) — canonical reference for D-Bus protocol compliance.
