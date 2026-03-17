#!/bin/bash
# Legacy install script — prefer 'make install' instead.
set -e

echo "Note: prefer 'make install' for a complete installation."
echo ""
echo "Building and installing..."
cargo install --path crates/nwg-dock --force
cargo install --path crates/nwg-drawer --force
cargo install --path crates/nwg-notifications --force

echo ""
echo "Installed:"
echo "  $(which nwg-dock-hyprland)"
echo "  $(which nwg-drawer)"
echo "  $(which nwg-notifications)"
echo ""
echo "Run 'make setup-hyprland' or 'make setup-sway' for autostart config."
