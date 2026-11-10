#!/usr/bin/env sh
# Builds Lulo.app from a compiled binary.
# Usage: ./make-bundle.sh path/to/lulo
set -eu
BIN="$1"
HERE="$(cd "$(dirname "$0")" && pwd)"
APP="Lulo.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/lulo"
cp "$HERE/Info.plist" "$APP/Contents/Info.plist"
cp "$HERE/../../assets/icons/lulo.icns" "$APP/Contents/Resources/lulo.icns"
echo "Done: $APP"
