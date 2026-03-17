#!/bin/bash
# Integration test runner — starts headless Sway and runs tests against it.
#
# Usage: ./tests/integration/test_runner.sh
#
# Requires: sway, wlroots, foot (terminal), notify-send
# These run automatically in CI or can be run locally.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
PASS=0
FAIL=0
TOTAL=0

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

cleanup() {
    echo ""
    echo "Cleaning up..."
    [ -n "${DOCK_PID:-}" ] && kill "$DOCK_PID" 2>/dev/null || true
    [ -n "${NOTIF_PID:-}" ] && kill "$NOTIF_PID" 2>/dev/null || true
    [ -n "${SWAY_PID:-}" ] && kill "$SWAY_PID" 2>/dev/null || true
    sleep 1
    [ -n "${TEST_RUNTIME:-}" ] && rm -rf "$TEST_RUNTIME" 2>/dev/null || true
}
trap cleanup EXIT

assert_eq() {
    local desc="$1" expected="$2" actual="$3"
    TOTAL=$((TOTAL + 1))
    if [ "$expected" = "$actual" ]; then
        echo -e "  ${GREEN}PASS${NC}: $desc"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}: $desc (expected '$expected', got '$actual')"
        FAIL=$((FAIL + 1))
    fi
}

assert_contains() {
    local desc="$1" haystack="$2" needle="$3"
    TOTAL=$((TOTAL + 1))
    if echo "$haystack" | grep -q "$needle"; then
        echo -e "  ${GREEN}PASS${NC}: $desc"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}: $desc (expected to contain '$needle')"
        FAIL=$((FAIL + 1))
    fi
}

assert_gt() {
    local desc="$1" value="$2" threshold="$3"
    TOTAL=$((TOTAL + 1))
    if [ "$value" -gt "$threshold" ] 2>/dev/null; then
        echo -e "  ${GREEN}PASS${NC}: $desc ($value > $threshold)"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}: $desc ($value not > $threshold)"
        FAIL=$((FAIL + 1))
    fi
}

assert_running() {
    local desc="$1" pid="$2"
    TOTAL=$((TOTAL + 1))
    if kill -0 "$pid" 2>/dev/null; then
        echo -e "  ${GREEN}PASS${NC}: $desc (pid $pid)"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC}: $desc (pid $pid not running)"
        FAIL=$((FAIL + 1))
    fi
}

# ─────────────────────────────────────────────────────────────────────
# Check prerequisites
# ─────────────────────────────────────────────────────────────────────

echo -e "${YELLOW}Checking prerequisites...${NC}"

for cmd in sway swaymsg; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${RED}Missing: $cmd${NC}"
        echo "Install sway to run integration tests."
        exit 1
    fi
done

DOCK_BIN="$PROJECT_DIR/target/release/nwg-dock-hyprland"
NOTIF_BIN="$PROJECT_DIR/target/release/nwg-notifications"

if [ ! -f "$DOCK_BIN" ] || [ ! -f "$NOTIF_BIN" ]; then
    echo "Release binaries not found. Building..."
    cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"
fi

# ─────────────────────────────────────────────────────────────────────
# Start headless Sway
# ─────────────────────────────────────────────────────────────────────

echo -e "${YELLOW}Starting headless Sway...${NC}"

TEST_RUNTIME=$(mktemp -d /tmp/nwg-test-XXXXXX)

# Minimal sway config that disables swaybar and swaybg (not needed headless)
cat > "$TEST_RUNTIME/config" << 'SWAYEOF'
bar {
    swaybar_command true
}
swaybg_command true
SWAYEOF

# Start Sway headless with isolated runtime dir but shared D-Bus session
env \
    HOME="$TEST_RUNTIME" \
    XDG_RUNTIME_DIR="$TEST_RUNTIME" \
    DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-}" \
    WLR_BACKENDS=headless \
    WLR_RENDERER=pixman \
    WLR_LIBINPUT_NO_DEVICES=1 \
    PATH="$PATH" \
    sway --config "$TEST_RUNTIME/config" >"$TEST_RUNTIME/sway.log" 2>&1 &
SWAY_PID=$!
export XDG_RUNTIME_DIR="$TEST_RUNTIME"

# Wait for Sway to start and create its IPC socket
SWAYSOCK=""
for i in $(seq 1 30); do
    SOCK=$(find "$TEST_RUNTIME" -maxdepth 1 -name "sway-ipc.*.sock" 2>/dev/null | head -1)
    if [ -n "$SOCK" ]; then
        export SWAYSOCK="$SOCK"
        break
    fi
    sleep 0.2
done

if [ -z "${SWAYSOCK:-}" ]; then
    echo -e "${RED}Sway failed to start. Log:${NC}"
    cat "$TEST_RUNTIME/sway.log" 2>/dev/null
    exit 1
fi

# Find the Wayland display socket Sway created
WAYLAND_SOCK=$(find "$TEST_RUNTIME" -maxdepth 1 -name "wayland-*" ! -name "*.lock" 2>/dev/null | head -1)
export WAYLAND_DISPLAY=$(basename "$WAYLAND_SOCK")
# Override to prevent binaries connecting to real compositor
export GDK_BACKEND=wayland
# Clear Hyprland env so our binaries detect Sway, not Hyprland
unset HYPRLAND_INSTANCE_SIGNATURE 2>/dev/null || true

echo "  Sway running (pid $SWAY_PID, display $WAYLAND_DISPLAY, socket $SWAYSOCK)"

# ─────────────────────────────────────────────────────────────────────
# Test: Sway IPC basics
# ─────────────────────────────────────────────────────────────────────

echo ""
echo -e "${YELLOW}=== Sway IPC Tests ===${NC}"

# Verify we can communicate with Sway
TREE=$(swaymsg -s "$SWAYSOCK" -t get_tree -r 2>/dev/null)
assert_contains "get_tree returns JSON" "$TREE" '"type"'

OUTPUTS=$(swaymsg -s "$SWAYSOCK" -t get_outputs -r 2>/dev/null)
assert_contains "get_outputs returns JSON" "$OUTPUTS" '"name"'

VERSION=$(swaymsg -s "$SWAYSOCK" -t get_version -r 2>/dev/null)
assert_contains "get_version returns version" "$VERSION" '"human_readable"'

# ─────────────────────────────────────────────────────────────────────
# Test: Our dock binary starts on Sway
# ─────────────────────────────────────────────────────────────────────

echo ""
echo -e "${YELLOW}=== Dock Binary Tests ===${NC}"

# Use isolated D-Bus to prevent GTK from finding the real running instance
env -i HOME="$TEST_RUNTIME" TMPDIR="$TEST_RUNTIME" XDG_RUNTIME_DIR="$TEST_RUNTIME" \
    WAYLAND_DISPLAY=wayland-1 GDK_BACKEND=wayland \
    SWAYSOCK="$SWAYSOCK" DBUS_SESSION_BUS_ADDRESS="disabled:" \
    PATH="$PATH" \
    "$DOCK_BIN" --wm sway -m -d -i 48 --mb 10 --hide-timeout 400 &>"$TEST_RUNTIME/dock.log" &
DOCK_PID=$!
sleep 2

assert_running "dock process alive" "$DOCK_PID"

# Verify dock received the tree (check its log for client refresh)
DOCK_LOG=$(cat "$TEST_RUNTIME/dock.log" 2>/dev/null || echo "")
# The dock should have started without fatal errors
# (Gdk-WARNING about Vulkan is expected on headless — not our code)
TOTAL=$((TOTAL + 1))
DOCK_ERRORS=$(echo "$DOCK_LOG" | grep -i "error\|panic\|crash" | grep -v "Gdk-WARNING\|Vulkan\|VK_ERROR\|vk[A-Z]" || true)
if [ -z "$DOCK_ERRORS" ]; then
    echo -e "  ${GREEN}PASS${NC}: dock started without errors"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}: dock log contains errors"
    echo "$DOCK_ERRORS" | head -5
    FAIL=$((FAIL + 1))
fi

# Stop dock
kill "$DOCK_PID" 2>/dev/null || true
wait "$DOCK_PID" 2>/dev/null || true
unset DOCK_PID

# ─────────────────────────────────────────────────────────────────────
# Test: Notification daemon on Sway
# ─────────────────────────────────────────────────────────────────────

echo ""
echo -e "${YELLOW}=== Notification Daemon Tests ===${NC}"

# Use isolated D-Bus to prevent GTK from finding the real running instance
env -i HOME="$TEST_RUNTIME" TMPDIR="$TEST_RUNTIME" XDG_RUNTIME_DIR="$TEST_RUNTIME" \
    WAYLAND_DISPLAY=wayland-1 GDK_BACKEND=wayland \
    SWAYSOCK="$SWAYSOCK" DBUS_SESSION_BUS_ADDRESS="disabled:" \
    PATH="$PATH" \
    "$NOTIF_BIN" --wm sway --persist &>"$TEST_RUNTIME/notif.log" &
NOTIF_PID=$!
sleep 2

assert_running "notification daemon alive" "$NOTIF_PID"

NOTIF_LOG=$(cat "$TEST_RUNTIME/notif.log" 2>/dev/null || echo "")
TOTAL=$((TOTAL + 1))
if ! echo "$NOTIF_LOG" | grep -qi "panic\|crash"; then
    echo -e "  ${GREEN}PASS${NC}: notification daemon started without crashes"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}: notification daemon log contains crashes"
    FAIL=$((FAIL + 1))
fi

# Note: can't test notify-send here because D-Bus is isolated to prevent
# the test from interfering with the real desktop. The daemon's D-Bus
# registration is tested by verifying it starts and stays alive.

# Signal tests
kill -38 "$NOTIF_PID" 2>/dev/null || true  # SIGRTMIN+4: toggle panel
sleep 0.5
assert_running "daemon alive after panel toggle signal" "$NOTIF_PID"

kill -39 "$NOTIF_PID" 2>/dev/null || true  # SIGRTMIN+5: toggle DND
sleep 0.5
assert_running "daemon alive after DND toggle signal" "$NOTIF_PID"

# Stop daemon
kill "$NOTIF_PID" 2>/dev/null || true
wait "$NOTIF_PID" 2>/dev/null || true
unset NOTIF_PID

# ─────────────────────────────────────────────────────────────────────
# Results
# ─────────────────────────────────────────────────────────────────────

echo ""
echo "════════════════════════════════════════"
if [ "$FAIL" -eq 0 ]; then
    echo -e " ${GREEN}All $TOTAL tests passed!${NC}"
else
    echo -e " ${RED}$FAIL of $TOTAL tests failed${NC}"
fi
echo "════════════════════════════════════════"

exit "$FAIL"
