# nwg-common

Shared library backing the [`nwg-dock`](https://github.com/jasonherald/nwg-dock),
[`nwg-drawer`](https://github.com/jasonherald/nwg-drawer), and
[`nwg-notifications`](https://github.com/jasonherald/nwg-notifications) ports —
the macOS-style dock, app drawer, and notification daemon for
[Hyprland](https://hyprland.org/) and [Sway](https://swaywm.org/), written in Rust.

## What's in the box

- **Compositor-neutral IPC abstraction.** A `Compositor` trait with Hyprland
  and Sway backends, auto-detected from `HYPRLAND_INSTANCE_SIGNATURE` /
  `SWAYSOCK` env vars. A null backend for graceful degradation on unsupported
  compositors (Niri, river, Openbox, etc.). Consumers call `init_or_exit` or
  `init_or_null` and only talk to the trait from then on.
- **`.desktop` file handling.** Parser with locale-aware `Name` / `Comment`
  resolution, FreeDesktop category assignment with secondary-category
  mapping, icon file lookup, and user-configured preferred-app overrides.
- **CSS loading + hot-reload.** GTK4 CssProvider loading with recursive
  `@import` graph resolution, cycle detection, and an in-place file watcher
  that survives both truncate-in-place and rename-swap save strategies.
- **XDG paths.** Data / config / cache directory resolution following the
  XDG Base Directory spec.
- **Application launching.** Direct spawn and compositor-`exec` pathways,
  plus a shared child-reaper thread so GUI-app processes don't zombify.
  Handles `.desktop` field codes, theme prepends, and terminal launches.
- **Shared pin file.** Atomic save / load with case-insensitive pin / unpin
  for the `~/.cache/mac-dock-pinned` file used by the dock and drawer.
- **RT-signal plumbing.** `SIGRTMIN+1..+6` helpers with runtime `SIGRTMIN`
  query (correct on glibc and musl), plus SIGTERM and single-instance
  signaling.
- **Single-instance lock.** Per-user lock file with stale-PID recovery that
  validates `/proc/<pid>/exe` against the caller's own binary to avoid
  acting on recycled PIDs.
- **Fullscreen layer-shell backdrops.** Per-monitor transparent surfaces
  for click-outside-to-close UI patterns, across the drawer / notifications
  panel / DND menu.

## Stability contract

`nwg-common` is in **0.x**. Per crates.io semver convention for `0.x`
crates:

- Breaking changes ship as **minor** bumps (`0.3 → 0.4`).
- Additive changes ship as **patch** or **minor** depending on scope.
- **1.0.0 is a deliberate decision** we'll make once the public API has
  had real-world soak time across the three binaries that depend on it.
  Don't expect it imminently — the point of the 0.x window is to let the
  surface settle.

Binaries consuming `nwg-common` should pin to `nwg-common = "0.3"` (or
whatever the current minor is) so patch bumps flow automatically while
minor bumps are opted into deliberately.

The public surface is sealed behind `#![warn(missing_docs)]` — every item
you can import is documented. See `cargo doc --open -p nwg-common`.

## Pre-split history

Prior to v0.3.0, this crate lived inside the
[`mac-doc-hyprland`](https://github.com/jasonherald/mac-doc-hyprland)
monorepo as `nwg-dock-common` at version 0.2.0. The full pre-split git
history is preserved in the monorepo; this crate's `CHANGELOG.md`
documents changes from v0.3.0 onward.

## License

MIT. See the `LICENSE` file in the repo root.
