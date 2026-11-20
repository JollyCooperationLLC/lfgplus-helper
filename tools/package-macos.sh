#!/usr/bin/env bash
# Builds "LFG+ Helper.app" around a macOS build of the helper, signs it ad hoc (no Apple account) and zips it.
#
#   tools/package-macos.sh VERSION BINARY
#   tools/package-macos.sh 2026.11.20.090442 target/universal/lfgplus-helper
#
# VERSION is year.month.day.hourminutesecond. Writes dist/LFG+ Helper.app and dist/lfgplus-helper-macos.zip, and
# prints the zip's path. Runs on macOS (codesign, ditto).
set -euo pipefail

VERSION="$1"
BINARY="$2"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/LFG+ Helper.app"
ZIP="$DIST/lfgplus-helper-macos.zip"

rm -rf "$APP" "$ZIP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BINARY" "$APP/Contents/MacOS/lfgplus-helper"
chmod 755 "$APP/Contents/MacOS/lfgplus-helper"
cp "$ROOT/assets/app.icns" "$APP/Contents/Resources/AppIcon.icns"

# Shown as 2026.11.20; the build number is every digit, so each release counts up.
SHORT="${VERSION%.*}"
BUILD="${VERSION//./}"

# LSUIElement makes it a menu-bar app: no Dock icon and no app menu.
cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>LFG+ Helper</string>
    <key>CFBundleExecutable</key><string>lfgplus-helper</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIdentifier</key><string>plus.lfg.helper</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>LFG+ Helper</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$SHORT</string>
    <key>CFBundleVersion</key><string>$BUILD</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
    <key>LSMinimumSystemVersion</key><string>10.13</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>All rights reserved - Jolly Cooperation LLC</string>
</dict>
</plist>
EOF
plutil -lint "$APP/Contents/Info.plist" >&2

# An ad hoc signature: Apple Silicon runs nothing unsigned, and it seals the bundle's files together. It isn't Apple's
# check, so the first open still asks the player to allow it (README).
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP" >&2

(cd "$DIST" && ditto -c -k --keepParent "LFG+ Helper.app" "$(basename "$ZIP")")
echo "$ZIP"
