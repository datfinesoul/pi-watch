#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
  printf '%s\n' 'The Pi Watch app requires macOS.' >&2
  exit 1
fi
node scripts/check-versions.mjs
# Use one root output directory, independent of the manifest location.
export CARGO_TARGET_DIR="$(pwd)/target"
cargo build --manifest-path apps/macos/Cargo.toml --release --locked
app="dist/Pi Watch.app"
mkdir -p "$app/Contents/MacOS"
cp "$CARGO_TARGET_DIR/release/pi-watch" "$app/Contents/MacOS/pi-watch"
cp apps/macos/resources/Info.plist "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
codesign --force --sign - "$app"
codesign --verify --strict "$app"
printf '\nBuilt %s (local ad-hoc signature, not notarized).\n' "$app"
