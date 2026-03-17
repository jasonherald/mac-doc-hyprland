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
WAYLAND_DISPLAY=$(basename "$WAYLAND_SOCK")
export WAYLAND_DISPLAY
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

# Stop dock (we'll restart it for functional tests)
kill "$DOCK_PID" 2>/dev/null || true
wait "$DOCK_PID" 2>/dev/null || true
unset DOCK_PID

# ─────────────────────────────────────────────────────────────────────
# Test: Sway window management (functional tests)
# ─────────────────────────────────────────────────────────────────────

echo ""
echo -e "${YELLOW}=== Sway Window Management Tests ===${NC}"

# Helper: run swaymsg against the test Sway instance (query mode — returns output)
smsg() { swaymsg -s "$SWAYSOCK" "$@" 2>/dev/null; }

# Helper: run swaymsg command silently (discards success/error JSON)
smsg_cmd() { swaymsg -s "$SWAYSOCK" "$@" >/dev/null 2>&1 || true; }

# Helper: count window nodes (nodes with a non-null app_id or window_properties)
count_windows() {
    smsg -t get_tree -r | python3 -c "
import json, sys
def count(node):
    c = 1 if (node.get('app_id') or node.get('window_properties')) and node.get('pid',0) > 0 and node.get('type') == 'con' else 0
    for n in node.get('nodes', []) + node.get('floating_nodes', []):
        c += count(n)
    return c
print(count(json.load(sys.stdin)))
" 2>/dev/null || echo "0"
}

# Helper: open a foot terminal in headless Sway
open_test_window() {
    local mark="$1"
    swaymsg -s "$SWAYSOCK" "exec env HOME=$TEST_RUNTIME XDG_RUNTIME_DIR=$TEST_RUNTIME WAYLAND_DISPLAY=wayland-1 foot --app-id=$mark sh -c 'sleep 60'" 2>/dev/null || true
    sleep 1.5
}

# --- Test: Open a window, verify Sway sees it ---
BEFORE=$(count_windows)
open_test_window "test-win-1"
AFTER=$(count_windows)
assert_gt "window appears in Sway tree" "$AFTER" "$BEFORE"

# --- Test: Verify window has correct app_id ---
TREE_JSON=$(smsg -t get_tree -r)
assert_contains "window has correct app_id" "$TREE_JSON" 'test-win-1'

# --- Test: Open a second window ---
open_test_window "test-win-2"
WIN_COUNT=$(count_windows)
assert_gt "two windows in tree" "$WIN_COUNT" "$AFTER"

# --- Test: Focus command works ---
smsg_cmd '[app_id=test-win-1] focus'
sleep 0.5
FOCUSED=$(smsg -t get_tree -r | python3 -c "
import json, sys
def find_focused(node):
    if node.get('focused') and node.get('app_id'):
        return node['app_id']
    for n in node.get('nodes', []) + node.get('floating_nodes', []):
        r = find_focused(n)
        if r: return r
    return None
print(find_focused(json.load(sys.stdin)) or '')
" 2>/dev/null || echo "")
assert_eq "focus command targets correct window" "test-win-1" "$FOCUSED"

# --- Test: Floating toggle ---
smsg_cmd '[app_id=test-win-1] floating toggle'
sleep 0.5
FLOAT_COUNT=$(smsg -t get_tree -r | grep -c 'floating_con' || echo "0")
assert_gt "floating toggle creates floating_con" "$FLOAT_COUNT" "0"

# Unfloat it
smsg_cmd '[app_id=test-win-1] floating toggle'
sleep 0.3

# --- Test: Move to workspace ---
smsg_cmd '[app_id=test-win-2] move to workspace 2'
sleep 0.5
WS2_WINDOWS=$(smsg -t get_tree -r | python3 -c "
import json, sys
def find_ws(node, name):
    if node.get('type') == 'workspace' and node.get('name') == name:
        return node
    for n in node.get('nodes', []) + node.get('floating_nodes', []):
        r = find_ws(n, name)
        if r: return r
    return None
def count_wins(node):
    c = 1 if node.get('app_id') and node.get('pid') else 0
    for n in node.get('nodes', []) + node.get('floating_nodes', []):
        c += count_wins(n)
    return c
tree = json.load(sys.stdin)
ws = find_ws(tree, '2')
print(count_wins(ws) if ws else 0)
" 2>/dev/null || echo "0")
assert_eq "window moved to workspace 2" "1" "$WS2_WINDOWS"

# --- Test: Close window via IPC ---
smsg_cmd '[app_id=test-win-2] kill'
sleep 0.5
AFTER_CLOSE=$(smsg -t get_tree -r | python3 -c "
import json, sys
def has_app(node, name):
    if node.get('app_id') == name: return True
    for n in node.get('nodes', []) + node.get('floating_nodes', []):
        if has_app(n, name): return True
    return False
print('1' if has_app(json.load(sys.stdin), 'test-win-2') else '0')
" 2>/dev/null || echo "0")
assert_eq "close command removes window" "0" "$AFTER_CLOSE"

# --- Test: Multi-monitor (add second headless output) ---
smsg_cmd 'create_output'
sleep 0.5
OUTPUT_COUNT=$(smsg -t get_outputs -r | python3 -c "
import json, sys
outputs = json.load(sys.stdin)
print(len([o for o in outputs if o.get('active')]))
" 2>/dev/null || echo "0")
assert_eq "second headless output active" "2" "$OUTPUT_COUNT"

# Disable second output (cleanup)
smsg_cmd 'output HEADLESS-2 disable'
# Note: Sway may name it HEADLESS-2 or WL-2; we just verify the count
sleep 0.3

# --- Test: Rapid window open/close (stress test) ---
TOTAL=$((TOTAL + 1))
for i in $(seq 1 5); do
    open_test_window "stress-$i"
done
sleep 1
for i in $(seq 1 5); do
    smsg_cmd "[app_id=stress-$i] kill"
done
sleep 1
REMAINING=$(smsg -t get_tree -r | python3 -c "
import json, sys
def count_prefix(node, prefix):
    c = 1 if (node.get('app_id') or '').startswith(prefix) else 0
    for n in node.get('nodes', []) + node.get('floating_nodes', []):
        c += count_prefix(n, prefix)
    return c
print(count_prefix(json.load(sys.stdin), 'stress-'))
" 2>/dev/null || echo "0")
if [ "$REMAINING" -eq 0 ]; then
    echo -e "  ${GREEN}PASS${NC}: rapid open/close stress test (5 windows)"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}: $REMAINING stress windows still in tree"
    FAIL=$((FAIL + 1))
fi

# --- Cleanup remaining test windows ---
smsg_cmd '[app_id=test-win-1] kill'
sleep 0.3

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
