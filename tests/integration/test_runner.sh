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
    [ -n "${XDG_RUNTIME_DIR:-}" ] && rm -rf "$XDG_RUNTIME_DIR" 2>/dev/null || true
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

export WLR_BACKENDS=headless
export WLR_RENDERER=pixman
export WLR_LIBINPUT_NO_DEVICES=1
export XDG_RUNTIME_DIR=$(mktemp -d /tmp/nwg-test-XXXXXX)

sway --config "$SCRIPT_DIR/sway_config" &>"$XDG_RUNTIME_DIR/sway.log" &
SWAY_PID=$!

# Wait for Sway to start and create its IPC socket
for i in $(seq 1 30); do
    SOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock 2>/dev/null | head -1)
    if [ -n "$SOCK" ]; then
        export SWAYSOCK="$SOCK"
        break
    fi
    sleep 0.2
done

if [ -z "${SWAYSOCK:-}" ]; then
    echo -e "${RED}Sway failed to start. Log:${NC}"
    cat "$XDG_RUNTIME_DIR/sway.log"
    exit 1
fi

echo "  Sway running (pid $SWAY_PID, socket $SWAYSOCK)"

# ─────────────────────────────────────────────────────────────────────
# Test: Sway IPC basics
# ─────────────────────────────────────────────────────────────────────

echo ""
echo -e "${YELLOW}=== Sway IPC Tests ===${NC}"

# Verify we can communicate with Sway
TREE=$(swaymsg -t get_tree -r 2>/dev/null)
assert_contains "get_tree returns JSON" "$TREE" '"type"'

OUTPUTS=$(swaymsg -t get_outputs -r 2>/dev/null)
assert_contains "get_outputs returns JSON" "$OUTPUTS" '"name"'

VERSION=$(swaymsg -t get_version -r 2>/dev/null)
assert_contains "get_version returns version" "$VERSION" '"human_readable"'

# ─────────────────────────────────────────────────────────────────────
# Test: Our dock binary starts on Sway
# ─────────────────────────────────────────────────────────────────────

echo ""
echo -e "${YELLOW}=== Dock Binary Tests ===${NC}"

"$DOCK_BIN" --wm sway -d -i 48 --mb 10 --hide-timeout 400 &>"$XDG_RUNTIME_DIR/dock.log" &
DOCK_PID=$!
sleep 2

assert_running "dock process alive" "$DOCK_PID"

# Verify dock received the tree (check its log for client refresh)
DOCK_LOG=$(cat "$XDG_RUNTIME_DIR/dock.log" 2>/dev/null || echo "")
# The dock should have started without errors
TOTAL=$((TOTAL + 1))
if ! echo "$DOCK_LOG" | grep -qi "error\|panic\|crash"; then
    echo -e "  ${GREEN}PASS${NC}: dock started without errors"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}: dock log contains errors"
    echo "$DOCK_LOG" | grep -i "error\|panic\|crash" | head -5
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

"$NOTIF_BIN" --wm sway --persist &>"$XDG_RUNTIME_DIR/notif.log" &
NOTIF_PID=$!
sleep 2

assert_running "notification daemon alive" "$NOTIF_PID"

NOTIF_LOG=$(cat "$XDG_RUNTIME_DIR/notif.log" 2>/dev/null || echo "")
TOTAL=$((TOTAL + 1))
if ! echo "$NOTIF_LOG" | grep -qi "panic\|crash"; then
    echo -e "  ${GREEN}PASS${NC}: notification daemon started without crashes"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}FAIL${NC}: notification daemon log contains crashes"
    FAIL=$((FAIL + 1))
fi

# Test sending a notification (if notify-send available)
if command -v notify-send >/dev/null 2>&1; then
    notify-send "Integration Test" "This is a test notification" 2>/dev/null || true
    sleep 1
    assert_contains "daemon received notification" "$(cat "$XDG_RUNTIME_DIR/notif.log")" "Integration Test"
fi

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
