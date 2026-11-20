#!/usr/bin/env bash
# Builds "LFG+ Helper.app" around a macOS build of the helper, signs it ad hoc (no Apple account) and puts it in a
# disk image beside a shortcut to Applications.
#
#   tools/package-macos.sh VERSION BINARY
#   tools/package-macos.sh 2026.11.20.090442 target/universal/lfgplus-helper
#
# VERSION is year.month.day.hourminutesecond. Writes dist/LFG+ Helper.app and dist/lfgplus-helper-macos.dmg, and
# prints the disk image's path. Runs on macOS (codesign, ditto, hdiutil).
set -euo pipefail

VERSION="$1"
BINARY="$2"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/LFG+ Helper.app"
DMG="$DIST/lfgplus-helper-macos.dmg"
STAGE="$DIST/dmg"

rm -rf "$APP" "$DMG" "$STAGE"
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

# What the player sees when the image opens: the app and Applications, to drag it onto. ditto keeps the signature's
# extended attributes, which a plain copy may drop.
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/LFG+ Helper.app"
ln -s /Applications "$STAGE/Applications"

# HFS+ and zlib (UDZO) open on every macOS the app runs on. hdiutil sometimes finds the device busy on a fresh
# machine, so it gets three tries.
for try in 1 2 3; do
    if hdiutil create -volname "LFG+ Helper" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG" >&2; then
        break
    fi
    if [ "$try" = 3 ]; then
        exit 1
    fi
    sleep 5
done
hdiutil verify "$DMG" >&2
rm -rf "$STAGE"
echo "$DMG"
