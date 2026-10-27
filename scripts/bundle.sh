#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --locked
app="dist/Pi Watch.app"
mkdir -p "$app/Contents/MacOS"
cp target/release/pi-watch "$app/Contents/MacOS/pi-watch"
cp resources/Info.plist "$app/Contents/Info.plist"
codesign --force --sign - "$app"
printf '\nBuilt %s (local ad-hoc signature, not notarized).\n' "$app"
