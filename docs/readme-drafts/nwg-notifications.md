# nwg-notifications

A D-Bus notification daemon and notification center for [Hyprland](https://hyprland.org/) and [Sway](https://swaywm.org/), written in Rust.

Claims `org.freedesktop.Notifications`, shows popup toasts, and ships a slide-out history panel with Do-Not-Disturb controls and optional waybar integration. Built alongside [`nwg-dock`](https://github.com/jasonherald/nwg-dock) and [`nwg-drawer`](https://github.com/jasonherald/nwg-drawer) to replace [mako](https://github.com/emersion/mako) in the mac-doc-hyprland stack, but runs standalone.

## Features

- **D-Bus notification daemon** — replaces mako; claims `org.freedesktop.Notifications`
- **Popup toasts** — top-right corner, auto-dismiss, click-to-focus sending app
- **Deep-linking** — clicking a notification tells the app to open the specific item
- **Auto-dismiss** — popups dismissed when app calls CloseNotification (e.g., Slack read)
- **Action buttons** — shows Reply/Open/etc. buttons, emits `ActionInvoked` D-Bus signal
- **History panel** — slide-out from right, grouped by app with collapse/expand
- **Click-outside-to-close** — backdrop overlay + Escape key
- **Dismiss controls** — per-notification, per-app group, or clear all
- **Do Not Disturb** — toggle via panel button, signal, or waybar right-click menu
- **Timed DND** — 1 hour, 2 hours, until tomorrow with expiry countdown
- **Waybar integration** — bell icon with unread count, left-click toggles panel, right-click opens DND menu
- **Persistence** — notification history saved across restarts with `--persist`
- **Focused monitor** — popups appear on the currently focused monitor

## Install

### Requirements

- **Rust 1.95** or later (pinned in `rust-toolchain.toml`; rustup picks it up automatically)
- **GTK4** and **gtk4-layer-shell** system libraries
- A Wayland compositor with `wlr-layer-shell` support (Hyprland, Sway)

### Install system dependencies

```bash
# Arch Linux
sudo pacman -S gtk4 gtk4-layer-shell

# Ubuntu/Debian
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev

# Fedora
sudo dnf install gtk4-devel gtk4-layer-shell-devel
```

### `make install` — three invocations for the binary + a user-scope `install-dbus`

The binary install has three invocations. The **D-Bus service file always lands in user-scope** (`~/.local/share/dbus-1/services/`) regardless of `PREFIX` — D-Bus user services are per-user by convention, and installing the service file system-wide would break auto-activation for other users on the same machine.

**Default — system-wide binary + user-scope service:**

```bash
sudo make install
make install-dbus
```

Writes:
- `nwg-notifications` → `/usr/local/bin/nwg-notifications`
- D-Bus service file → `~/.local/share/dbus-1/services/org.freedesktop.Notifications.service` (no sudo)

**No-sudo, dev workflow:**

```bash
make install PREFIX=$HOME/.local BINDIR=$HOME/.cargo/bin
make install-dbus
```

**Distro-parity:**

```bash
sudo make install PREFIX=/usr
make install-dbus
```

### From crates.io

```bash
cargo install nwg-notifications
```

`cargo install` only installs the binary. You'll need to write the D-Bus service file yourself (see [D-Bus service](#d-bus-service) below) — it's a ~5-line file pointing at `~/.cargo/bin/nwg-notifications`.

## Usage

```bash
# With history persistence
nwg-notifications --persist

# Force Sway backend (usually auto-detected)
nwg-notifications --wm sway --persist
```

## D-Bus service

`make install-dbus` installs this file into `~/.local/share/dbus-1/services/`. If you're cargo-installing, create it manually:

```ini
# ~/.local/share/dbus-1/services/org.freedesktop.Notifications.service
[D-BUS Service]
Name=org.freedesktop.Notifications
Exec=/home/YOU/.cargo/bin/nwg-notifications --persist
```

Once registered, the daemon auto-starts the first time any app calls `org.freedesktop.Notifications`.

## Hyprland autostart

```ini
# ~/.config/hypr/autostart.conf
exec-once = uwsm-app -- nwg-notifications --persist
```

Autostart isn't strictly required thanks to D-Bus auto-activation, but it makes the daemon ready before the first notification arrives (avoids a few-hundred-millisecond delay on your first toast).

## Signal control

```bash
# Toggle notification panel
pkill -f -38 nwg-notifications     # SIGRTMIN+4

# Toggle DND
pkill -f -39 nwg-notifications     # SIGRTMIN+5

# Open DND duration menu
pkill -f -40 nwg-notifications     # SIGRTMIN+6
```

## Waybar integration

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

The daemon writes its current state to `$XDG_RUNTIME_DIR/mac-notifications-status.json` and signals waybar (`SIGRTMIN+11`, which waybar receives as `signal: 11`) whenever the state changes — no polling.

## Theming

Styling is embedded via `include_str!`; there's no user-writable `notifications.css` today. If you need to customize appearance, fork the crate and edit `assets/notifications.css`, or open an issue to discuss exposing it.

## Contributing

PRs welcome. `main` is protected — open from a feature branch. Run `make lint` (fmt + clippy + test + deny + audit) locally before requesting review.

User-visible PRs add a CHANGELOG bullet under `## [x.y.z] — Unreleased` in `CHANGELOG.md`, following [Keep a Changelog](https://keepachangelog.com).

## Background: why not mako?

Mako is great, but:

1. The mac-doc-hyprland stack wanted a single look/feel across the dock, drawer, and notification center — GTK4 layer-shell surfaces make theming coherent across all three.
2. We wanted history + grouping + click-to-focus as first-class features, not add-ons.
3. Writing a D-Bus notification server in Rust on `gio::bus_own_name` turned out to be less code than expected — no async bridge, no external crate, directly on the glib main loop.

Run `nwg-notifications` instead of mako, or alongside (they'll race for the name — whichever claimed it first wins).

## License

MIT. See `LICENSE`.
