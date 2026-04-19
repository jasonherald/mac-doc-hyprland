# Changelog

All notable changes to `nwg-common` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Pre-split note:** Prior to v0.3.0, this crate lived inside the
> [`mac-doc-hyprland`](https://github.com/jasonherald/mac-doc-hyprland) monorepo
> as `nwg-dock-common` at version 0.2.0. v0.3.0 is the first release of the
> library under its own repo + crates.io name. The full pre-split history is
> preserved in the monorepo's git log; this file only documents changes from
> v0.3.0 onward.

## [0.3.0] — Unreleased

First standalone release. Extracts the shared library that underpins
[`nwg-dock`](https://github.com/jasonherald/nwg-dock),
[`nwg-drawer`](https://github.com/jasonherald/nwg-drawer), and
[`nwg-notifications`](https://github.com/jasonherald/nwg-notifications) from
the monorepo.

### Added

- `compositor` — compositor-neutral IPC abstraction built on the `Compositor`
  trait, with Hyprland and Sway backends auto-detected from
  `HYPRLAND_INSTANCE_SIGNATURE` / `SWAYSOCK` env vars. `init_or_exit` for
  tools that require a compositor; `init_or_null` for tools (like the
  drawer) that degrade gracefully on unsupported compositors (Niri, river,
  Openbox, etc.).
- `compositor::{WmClient, WmMonitor, WmWorkspace, WmEvent}` — neutral types
  covering the window / output / workspace model used by the three tools.
- `config::paths` — XDG data/config/cache directory resolution
  (`cache_dir`, `config_dir`, `find_data_home`, `ensure_dir`, `copy_file`,
  `load_text_lines`).
- `config::css` — GTK4 CSS provider loading + hot-reload with recursive
  `@import` graph resolution and cycle detection. Watcher handles in-place
  file edits across editors that rename-swap vs. truncate-in-place.
- `config::flags::normalize_legacy_flags` — converts pre-clap single-dash
  long flags (`-daemon` → `--daemon`) for backwards compatibility.
- `desktop::entry` — `.desktop` file parser with locale-aware Name/Comment
  resolution and `StartupWMClass` tracking for class-to-desktop-ID matching.
- `desktop::icons` — icon file lookup + display-name resolution that falls
  back through locale → base name → raw class.
- `desktop::categories` — FreeDesktop main-category assignment with
  multi-category support and secondary-category mapping (Audio/Video →
  AudioVideo, etc.).
- `desktop::preferred_apps` — user-configured `mime-type → desktop-id`
  overrides.
- `desktop::dirs` — `XDG_DATA_DIRS` app directory enumeration.
- `launch` — application launching via direct spawn or compositor `exec`,
  with a shared child-reaper thread so GUI-app processes don't zombify.
  Covers `.desktop` entry launches (field-code stripping, theme prepend,
  terminal handling) and shell-command launches.
- `layer_shell::create_fullscreen_backdrops` — per-monitor transparent
  backdrop surfaces for click-outside-to-close UI patterns.
- `pinning` — case-insensitive pin/unpin + atomic save/load for the
  shared `~/.cache/mac-dock-pinned` file used by the dock and drawer.
- `process::handle_dump_args` — `--dump-args <pid>` helper that reads
  `/proc/<pid>/cmdline` and shell-quotes it for `make upgrade` to
  capture a running instance's arguments before restarting it.
- `signals` — RT signal plumbing (SIGRTMIN+1..+6) for toggle/show/hide
  of the dock/drawer/notifications windows, plus SIGTERM handling and
  `send_signal_to_pid` for cross-instance signaling. `sigrtmin()` queries
  the runtime value so the library is correct on glibc and musl.
- `singleton` — per-user single-instance lock file with stale-PID recovery
  (validates `/proc/<pid>/exe` against our own binary).
- `DockError` + `Result` — unified error type re-exported at the crate
  root for `nwg_common::DockError` / `nwg_common::Result<T>`.

### Changed

- Public API surface is now explicitly sealed: `hyprland::ipc` types,
  `compositor::{hyprland, sway, null}` backends, and various internal
  helpers that don't cross crate boundaries are `pub(crate)` or private.
  Only items that consumers of the library legitimately need are exposed.
- Every public item carries rustdoc. `#![warn(missing_docs)]` is enabled
  at the crate root, so `cargo doc --no-deps -p nwg-common` runs
  warning-free and `cargo rustdoc -p nwg-common -- -D missing-docs`
  succeeds.

### Fixed

- `sigrtmin()` queries `libc::SIGRTMIN()` at runtime instead of
  hardcoding `34`, so the RT-signal offsets resolve correctly on musl
  (glibc reserves two NPTL slots and starts user RT signals at 34;
  musl reserves three and starts at 35).
