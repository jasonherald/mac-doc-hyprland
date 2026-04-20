# CLAUDE.md — nwg-common

## What is this?

The shared library behind the mac-doc-hyprland Rust port — compositor-neutral IPC abstraction, `.desktop` parsing, CSS loading, pin-file management, layer-shell helpers, and the various bits of system plumbing shared by [`nwg-dock`](https://github.com/jasonherald/nwg-dock), [`nwg-drawer`](https://github.com/jasonherald/nwg-drawer), and [`nwg-notifications`](https://github.com/jasonherald/nwg-notifications).

Pre-split (before v0.3.0) this lived as `nwg-dock-common` inside the [mac-doc-hyprland](https://github.com/jasonherald/mac-doc-hyprland) monorepo; that repo's git log has the full pre-0.3.0 history.

## Build & test

```bash
cargo build                   # Debug build
cargo build --release         # Release build
cargo test                    # Run unit tests
cargo clippy --all-targets    # Lint (should be zero warnings)
cargo fmt --all               # Format
cargo deny check              # License, advisory, ban, source checks
cargo audit                   # Dependency CVE scan
make test                     # Unit tests + clippy
make lint                     # Full check: fmt + clippy + test + deny + audit
```

`make test-integration` is primarily about exercising the Sway backend against a headless Sway instance — requires `sway` and `foot` installed. Per [tests/integration/CLASSIFICATION.md](https://github.com/jasonherald/mac-doc-hyprland/blob/main/tests/integration/CLASSIFICATION.md) in the monorepo, this crate owns the Sway IPC tests + Sway window-management tests.

## What lives where

```text
crates/nwg-common/ (or src/ post-extraction)
├── compositor/       # Compositor trait + Hyprland/Sway/null backends
├── config/
│   ├── css.rs        # GTK4 CSS provider loading + hot-reload + @import graph
│   ├── flags.rs      # normalize_legacy_flags — pre-clap -foo → --foo
│   └── paths.rs      # XDG cache/config/data resolution
├── desktop/
│   ├── categories.rs # FreeDesktop main-category assignment
│   ├── dirs.rs       # XDG_DATA_DIRS enumeration
│   ├── entry.rs      # .desktop file parser (DesktopEntry)
│   ├── icons.rs      # icon + display-name resolution
│   └── preferred_apps.rs
├── hyprland/         # Private — Hyprland IPC types/events (compositor::hyprland wraps this)
├── launch.rs         # launch / launch_desktop_entry / launch_via_compositor + child-reaper
├── layer_shell.rs    # create_fullscreen_backdrops per monitor
├── pinning.rs        # ~/.cache/mac-dock-pinned read/write (atomic, case-insensitive)
├── process.rs        # handle_dump_args for --dump-args <pid> flow
├── signals.rs        # RT signal handling (SIGRTMIN+N), WindowCommand mapping
├── singleton.rs      # Per-user lock file with stale-PID recovery
├── error.rs          # Private — DockError + Result re-exported at crate root
└── lib.rs
```

## Conventions

- **`#![warn(missing_docs)]` on the crate root** — every `pub` item has a rustdoc comment. `cargo doc --no-deps -p nwg-common` runs warning-free; `cargo rustdoc -p nwg-common -- -D missing-docs` succeeds.
- **Public surface is sealed** — `hyprland::*`, `error::*`, `compositor::{hyprland, sway, null, traits, types}` are all private at the module level. Only items that library consumers legitimately need are exposed. `DockError` and `Result` are re-exported at crate root (`nwg_common::DockError`, `nwg_common::Result<T>`).
- **Compositor trait owns all WM IPC** — no direct hyprland or sway calls leak to consumers. Binaries use `init_or_exit` or `init_or_null` and only touch the trait from then on.
- **Unsafe only in signals.rs** — required for RT signal handling via raw libc (`nix::Signal` doesn't cover `SIGRTMIN+N`). Every unsafe block has a SAFETY comment explaining invariants; return codes from `sigemptyset`/`sigaddset`/`pthread_sigmask` are checked.
- **Tests** — `#[cfg(test)] mod tests` at the bottom of each file, test behavior not implementation.
- **No magic numbers** — every numeric literal has a named constant or clear inline comment.
- **Error handling** — log errors, never silently discard with `let _ =`.

## Stability contract

0.x crate. Breaking changes ship as **minor** bumps (`0.3 → 0.4`) per crates.io convention; additive changes are **patch** or **minor** depending on scope. 1.0.0 is a deliberate decision for later, once the three consumer binaries have soaked the API.

Binaries should pin `nwg-common = "0.3"` (or whatever minor is current) so patch bumps flow automatically while minor bumps are opted into.

## Signal assignments

Shared contract across the three consumer binaries (mutations here need coordinated releases):

| Signal | Value | Target | Action |
|--------|-------|--------|--------|
| SIGRTMIN+1 | ~35 | Dock/Drawer | Toggle visibility |
| SIGRTMIN+2 | ~36 | Dock/Drawer | Show |
| SIGRTMIN+3 | ~37 | Dock/Drawer | Hide |
| SIGRTMIN+4 | ~38 | Notifications | Toggle panel |
| SIGRTMIN+5 | ~39 | Notifications | Toggle DND |
| SIGRTMIN+6 | ~40 | Notifications | Show DND menu |
| SIGRTMIN+11 | ~45 | Waybar | Refresh notification module |

Values are approximate because glibc and musl reserve different numbers of NPTL slots — `signals::sigrtmin()` queries `libc::SIGRTMIN()` at runtime for portability.

## Shared pin file

`~/.cache/mac-dock-pinned` — one desktop ID per line, no `.desktop` suffix. `pinning::{save_pinned, load_pinned, pin_item, unpin_item, is_pinned}` are the public entry points. Writes are atomic (temp file + rename) to prevent corruption on crash.

## Compositor abstraction

All compositor IPC goes through the `Compositor` trait in `compositor/traits.rs`. Auto-detection checks `HYPRLAND_INSTANCE_SIGNATURE` and `SWAYSOCK` env vars.

- `init_or_exit(Option<WmOverride>) -> Box<dyn Compositor>` — dock/notifications (hard-require a compositor).
- `init_or_null(Option<WmOverride>) -> Box<dyn Compositor>` — drawer (graceful fallback on Niri/river/Openbox via `NullCompositor`).

Key types (all public, all documented): `WmClient`, `WmMonitor`, `WmWorkspace`, `WmEvent`, `WmOverride`.

Private backends in `compositor/`:
- `hyprland.rs` — wraps the (private) top-level `hyprland::ipc`, converts Hyprland types to `Wm*` types.
- `sway.rs` — i3-ipc-based Sway backend with magic-byte + payload-size validation.
- `null.rs` — no-op fallback; most methods return `DockError::NoCompositorDetected`.

## See also

- `CHANGELOG.md` — user-visible changes per release, Keep-a-Changelog format.
- `README.md` — public-facing library docs + stability contract + pre-split pointer.
- Parent monorepo archive: [jasonherald/mac-doc-hyprland](https://github.com/jasonherald/mac-doc-hyprland) (pre-0.3.0 history).
