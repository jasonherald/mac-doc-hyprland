# Integration test classification

Walks `tests/integration/test_runner.sh` block-by-block and assigns each test section to the per-tool repo that'll own it after the Phase 2–4 extractions (#80). Planning input for the extraction PRs so nothing gets silently dropped or duplicated when the script fragments.

## Layout today

`tests/integration/` is one ~430-line shell script plus a Sway config fixture:

| File | Role |
|------|------|
| `test_runner.sh` | Bootstraps headless Sway, runs every test block in sequence, cleans up on exit. |
| `sway_config` | Minimal Sway config used during bootstrap (`bar swaybar_command true`, `swaybg_command true`). |

The script runs 18 assertions across 5 sections. Tests `cargo build --release --workspace`s the full workspace up front so both the dock and the notification daemon exist as built binaries before any assertion runs.

## Per-section classification

### Bootstrap (lines 9–159, 419–432)

Prerequisite checks, Sway headless launch, Wayland/SWAYSOCK socket discovery, teardown trap, result tally. Infrastructure, not tests.

**Target repos:** all four (`nwg-common`, `nwg-dock`, `nwg-drawer`, `nwg-notifications`) — each repo that runs any Sway-driven integration test needs its own copy. Duplication is intentional; each repo should be standalone-testable without pulling in the other repos. A shared "testing-common" crate would add cross-repo coupling we're deliberately avoiding.

**Shared fixture:** `sway_config` travels with the bootstrap to each repo (file is 3 lines; duplication cost is trivial).

### Sway IPC tests (lines 161–176) — 3 assertions

Drives `swaymsg -t get_tree / get_outputs / get_version` against the headless Sway and asserts the responses contain the expected JSON fields.

**Target repo:** `nwg-common`.

These validate that the Sway backend we abstract in `compositor/sway/ipc.rs` has an underlying compositor actually talking back. They're the contract tests for the library's Sway backend. No binary involvement.

### Dock binary tests (lines 178–214) — 2 assertions

Starts the dock with `--wm sway -m -d -i 48 --mb 10 --hide-timeout 400`, checks it stays alive for 2 s, greps the log for errors/panics.

**Target repo:** `nwg-dock`.

Exclusively about the dock binary starting cleanly against a compositor. No cross-binary interaction.

### Sway window-management tests (lines 216–371) — 7 assertions + 1 stress

Opens `foot` terminals via `swaymsg exec`, verifies window appears in tree, focus command targets the right app_id, `floating toggle` creates a floating_con, `move to workspace 2` relocates the window, `kill` removes it, `create_output` adds a second headless output, and a 5-window rapid-open/close stress test leaves the tree clean.

**Target repo:** `nwg-common`.

Despite driving raw `swaymsg` (not our Compositor trait), these are validating the behaviors the library's Sway backend depends on — focus, floating, workspace moves, multi-output, kill semantics. Natural home is alongside the trait they back. A future refactor could rewrite these to use the Rust trait directly; that's out of scope for the split itself (tracked separately if we file it).

### Notification daemon tests (lines 373–417) — 3 assertions + 2 signal checks

Starts `nwg-notifications --wm sway --persist`, verifies it's alive, checks the log for panics, sends `SIGRTMIN+4` (panel toggle) and `SIGRTMIN+5` (DND toggle), verifies the daemon stays alive after each signal.

**Target repo:** `nwg-notifications`.

Exclusively about the daemon binary. The signals are from `nwg-common::signals`, but the assertions target the daemon's resilience, not library behavior.

## Cross-tool tests

**None today.** No current test runs two of our binaries simultaneously. The closest candidate would be "dock writes pin file → drawer picks it up", but:

- We haven't written such a test yet.
- If we do, it would live in `nwg-drawer` (the side that asserts the behavior, per epic §5.8 "whichever binary the test asserts *from*"), and simulate the dock side by writing `~/.cache/mac-dock-pinned` directly rather than spawning the dock.

**No fifth integration repo needed.** Out of scope unless and until a test materializes that genuinely requires two binaries running against a shared compositor — flag that at the time it's written.

## Drawer

`nwg-drawer` has **no integration tests today**. The drawer is on-demand rather than resident, which makes integration testing awkward — it spawns, shows the launcher, and exits on selection or focus loss. Once the repo exists, any future drawer integration tests land there directly.

## Summary table

| Section | Lines | Target repo | Notes |
|---------|-------|-------------|-------|
| Bootstrap + Sway launch | 9–159, 419–432 | all four | Each repo gets its own copy + `sway_config` fixture. |
| Sway IPC tests | 161–176 | `nwg-common` | Contract test for the Sway backend. |
| Dock binary tests | 178–214 | `nwg-dock` | Dock-launch smoke test. |
| Sway window-management | 216–371 | `nwg-common` | Validates the behaviors the Compositor trait wraps. |
| Notification daemon | 373–417 | `nwg-notifications` | Daemon-launch + signal resilience. |
| *(drawer tests)* | — | `nwg-drawer` | No tests today; room for future. |

## What Phase 2–4 PRs need to do

- **#95 (nwg-dock seed)**: copy the bootstrap block + `sway_config` + the "Dock binary tests" section into `tests/integration/test_runner.sh`, scope the `DOCK_BIN` path to the new repo's `target/release/`, drop the other sections.
- **#99 (nwg-drawer seed)**: copy only the bootstrap block + `sway_config` so `make test-integration` exists as a no-op target; add tests as they come up.
- **#103 (nwg-notifications seed)**: copy the bootstrap block + `sway_config` + the "Notification daemon" section, scope `NOTIF_BIN` to the new repo, drop the rest.
- **#91 (nwg-common seed)**: copy the bootstrap block + `sway_config` + the "Sway IPC" section + the "Sway window-management" section. This is the fullest integration-test surface; it'll likely be the first to evolve away from the shell-script-driven approach.

Each extraction PR also updates its repo's `Makefile` `test-integration` target to point at the local `tests/integration/test_runner.sh`.

## Acceptance (for this issue)

- ✅ Every test block in `test_runner.sh` has an explicit target-repo assignment (table above).
- ✅ Cross-tool tests called out (none today; future drawer↔dock pin-file test noted with simulation strategy).
- ✅ A fifth "integration" repo is documented as **not needed** unless a multi-binary test materializes.
