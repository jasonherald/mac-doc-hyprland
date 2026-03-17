#!/bin/bash
# Install all nwg binaries to ~/.cargo/bin/
set -e

echo "Building and installing..."
cargo install --path crates/mac-dock --force
cargo install --path crates/mac-drawer --force
cargo install --path crates/mac-notifications --force

echo ""
echo "Installed:"
echo "  $(which nwg-dock-hyprland)"
echo "  $(which nwg-drawer)"
echo "  $(which nwg-notifications)"
echo ""
echo "Add to ~/.config/hypr/autostart.conf:"
echo "  exec-once = uwsm-app -- nwg-dock-hyprland -d -i 48 --mb 10 --hide-timeout 400"
echo "  exec-once = uwsm-app -- nwg-notifications --persist"
