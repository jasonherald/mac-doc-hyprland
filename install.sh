#!/bin/bash
# Install all mac-dock-hyprland binaries to ~/.cargo/bin/
set -e

echo "Building and installing..."
cargo install --path crates/mac-dock --force
cargo install --path crates/mac-drawer --force
cargo install --path crates/mac-notifications --force

echo ""
echo "Installed:"
echo "  $(which nwg-dock-hyprland-rs)"
echo "  $(which nwg-drawer-rs)"
echo "  $(which mac-notifications-rs)"
echo ""
echo "Add to ~/.config/hypr/autostart.conf:"
echo "  exec-once = uwsm-app -- nwg-dock-hyprland-rs -d -i 48 --mb 10 --hide-timeout 400"
echo "  exec-once = uwsm-app -- mac-notifications-rs --persist"
