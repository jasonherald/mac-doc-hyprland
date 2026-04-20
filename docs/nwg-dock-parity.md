# nwg-dock feature-parity audit

Comparison of the Rust port's dock (`crates/nwg-dock/`) against [`nwg-piotr/nwg-dock`](https://github.com/nwg-piotr/nwg-dock) — the Go Sway-only predecessor.

| | |
|---|---|
| **Scope** | `nwg-piotr/nwg-dock@8ecd84c65f` (v0.4.3, 2025-01-10) vs. `jasonherald/mac-doc-hyprland@d907e17b` (2026-04-19) |
| **Audit date** | 2026-04-19 |
| **Tracking epic** | [#126](https://github.com/jasonherald/mac-doc-hyprland/issues/126) |
| **Scheduling** | Deferred until after Phase 4 / split completes — not blocking Phase 2 v0.3.0 publish (see epic #126 rationale) |

## TL;DR

2 real gaps, 2 won't-do items. Sway users at v0.3.0 are **no worse off** than at v0.2.0 — these are Go-feature gaps, not Rust-port regressions. Our Rust port is otherwise a strict superset of both Go predecessors' flag sets and behavior.

## Gaps → tracker issues

| Gap | Severity | Issue | Target repo |
|-----|----------|-------|-------------|
| `WmEvent::WorkspaceChanged` missing from the compositor abstraction | Hard prereq | [#127](https://github.com/jasonherald/mac-doc-hyprland/issues/127) | `nwg-common` |
| Workspace switcher widget + `--nows` flag | High (user-visible) | [#128](https://github.com/jasonherald/mac-doc-hyprland/issues/128) | `nwg-dock` |

## Intentionally won't-do

| Item | Reasoning |
|------|-----------|
| Launcher multi-fallback chain (`nwg-drawer → nwggrid → hide`) | Go-side convenience; one-flag workaround (`-c "nwggrid"`) exists. Not worth hardcoding a fallback ladder. |
| `-v` (lowercase) for `--version` | Clap uses `-V`; lowercase `-v` is already taken by other conventions. `--version` works identically. |

## Non-gaps (parity or Rust-superset)

### CLI flags

Every Go flag is present in the Rust port with equivalent semantics:

| Go flag | Rust equivalent |
|---------|-----------------|
| `-s <file>` | `-s / --css-file` |
| `-o <output>` | `-o / --output` |
| `-d` autohide | `-d / --autohide` |
| `-f` full width/height | `-f / --full` |
| `-g <class-list>` ignore | `-g / --ignore-classes` |
| `-w <n>` num workspaces | `-w / --num-ws` |
| `-p <pos>` position | `-p / --position` |
| `-x` exclusive zone | `-x / --exclusive` |
| `-i <px>` icon size | `-i / --icon-size` |
| `-l <layer>` layer-shell layer | `-l / --layer` |
| `-c <cmd>` launcher command | `-c / --launcher-cmd` |
| `-lp <pos>` launcher button position | `--launcher-pos` (alias `lp`) |
| `-a <align>` alignment | `-a / --alignment` |
| `-mt / -ml / -mr / -mb` margins | `--mt / --ml / --mr / --mb` |
| `-hd <ms>` hotspot delay | `--hotspot-delay` (alias `hd`) |
| `-nolauncher` | `--nolauncher` |
| `-r` resident mode | `-r / --resident` |
| `-debug` | `--debug` |

### Runtime behavior

- **SIGUSR1 toggle** — accepted by both with matching semantics; Rust logs a deprecation pointer to `SIGRTMIN+1` but does not break the Go workflow.
- **Single-instance enforcement** — both enforce; Rust uses `~/.cache/nwg-dock-*.lock` with stale-PID recovery.
- **Right-click "Move to workspace" submenu** — both render it; Rust reads `--num-ws` for the list length.
- **Config file** — neither has one; both are CLI-only.

### Rust-only superset features

Not gaps, but worth recording so "superset" is honest:

- Multi-compositor (Hyprland + Sway) via `nwg_common::compositor::Compositor` trait + runtime auto-detection.
- Per-monitor dock windows (Go creates one window).
- Drag-to-reorder pinned icons; drag-off to unpin.
- Smart rebuild — only re-renders when the client/active-window set actually changes.
- Additional flags: `--ico`, `--ignore-workspaces`, `--hide-timeout`, `--opacity`, `--launch-animation`, `--wm`, `-m / --multi`, `--no-fullscreen-suppress`.
- Shared pin file with [`nwg-drawer`](https://github.com/jasonherald/nwg-drawer) — pins sync instantly in both directions.
- Position `right` (Go only has `bottom` / `top` / `left`).

## Method

Reproducible against the pinned commits in the Scope row above. All steps below fetch exactly the sources audited:

```bash
# Pin both sides
GO_SHA=8ecd84c65f
RUST_SHA=d907e17b

# Go side — fetch and inspect
mkdir -p /tmp/nwg-dock-parity/go
for f in README.md main.go tools.go Makefile; do
    gh api "repos/nwg-piotr/nwg-dock/contents/$f?ref=$GO_SHA" \
        --jq .content | base64 -d > "/tmp/nwg-dock-parity/go/$f"
done

# Rust side — fetch the pinned source tree and enumerate CLI flags
git clone https://github.com/jasonherald/mac-doc-hyprland /tmp/nwg-dock-parity/rust
git -C /tmp/nwg-dock-parity/rust checkout "$RUST_SHA"
cd /tmp/nwg-dock-parity/rust
cargo run -p nwg-dock --bin nwg-dock-hyprland -- --help > /tmp/nwg-dock-parity/rust-help.txt
```

Concrete steps performed:

1. Pulled `README.md`, `main.go`, `tools.go`, `config/` from `nwg-piotr/nwg-dock@8ecd84c65f` via `gh api`.
2. Enumerated Go CLI flags from `main.go`'s `flag.*` declarations.
3. Enumerated Rust CLI flags via `cargo run -p nwg-dock --bin nwg-dock-hyprland -- --help` at the pinned Rust SHA.
4. Cross-checked Go Sway event handlers (`tools.go:swayEventHandler.*`) against our `WmEvent` enum at the pinned Rust SHA.
5. Skimmed Go `config/` contents — only `style.css` + `hotspot.css` (no config-file schema, so nothing to diff).
6. Walked runtime behaviors called out in the Go README (resident mode, SIGUSR1 toggle, autohide, hotspots) and grepped the Rust tree for the equivalent.

## Closing-out convention

Each tracker issue gets stamped here with a "closed YYYY-MM-DD (PR #N)" note when it lands. The epic stays open until all non-won't-do rows are stamped; then epic closes and this doc reads as a historical record.
