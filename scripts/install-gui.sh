#!/bin/sh
# Build the AppKit GUI and install it next to hp-m177 plus an .app the Dock
# and Spotlight can launch.
set -eu
cd "$(dirname "$0")/.."
# Drop stale AppKit processes so Finder/Spotlight cannot keep an empty window.
killall hp-m177-native-gui 2>/dev/null || true
# Restart the AirScan advertisement under the app name Image Capture shows.
killall hp-m177-bridge 2>/dev/null || true
# dns-sd -R children can outlive the bridge and keep the old sidebar name.
ps -ax -o pid=,command= | awk '/[d]ns-sd -R / && /_uscan/ { print $1 }' | while read pid; do
  kill "$pid" 2>/dev/null || true
done

BIN_DIR="${HOME}/.cargo/bin"
APP_DIR="${HOME}/Applications/HP Color LaserJet Pro MFP M177fw Scanner.app"
OLD_APP="${HOME}/Applications/HP M177 Scanner.app"
if [ -d "${OLD_APP}" ]; then
  rm -rf "${OLD_APP}"
fi
MACOS="${APP_DIR}/Contents/MacOS"
RES="${APP_DIR}/Contents/Resources"

# Always install the CLI from this tree. Copying an older ~/.cargo/bin
# hp-m177 left the GUI talking to a binary without WSD fallback.
cargo install --path . --locked --force
./scripts/build-gui.sh

mkdir -p "${BIN_DIR}" "${MACOS}" "${RES}"
cp target/hp-m177-native-gui "${BIN_DIR}/hp-m177-native-gui"
chmod +x "${BIN_DIR}/hp-m177-native-gui"

cat > "${MACOS}/HP-M177-Scan" <<'WRAP'
#!/bin/bash
export PATH="${HOME}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
HERE="$(cd "$(dirname "$0")" && pwd)"
if [ -x "${HERE}/hp-m177" ]; then
  export HP_M177_BIN="${HERE}/hp-m177"
else
  export HP_M177_BIN="${HOME}/.cargo/bin/hp-m177"
fi
if [ -x "${HERE}/hp-m177-bridge" ]; then
  export HP_M177_BRIDGE="${HERE}/hp-m177-bridge"
else
  export HP_M177_BRIDGE="${HOME}/.cargo/bin/hp-m177-bridge"
fi
exec "${HERE}/hp-m177-native-gui" "$@"
WRAP
chmod +x "${MACOS}/HP-M177-Scan"
cp target/hp-m177-native-gui "${MACOS}/hp-m177-native-gui"
chmod +x "${MACOS}/hp-m177-native-gui"

# Bundle the CLI so Finder/Spotlight launches do not depend on PATH.
for tool in hp-m177 hp-m177-bridge; do
  if command -v "${tool}" >/dev/null 2>&1; then
    cp "$(command -v "${tool}")" "${MACOS}/${tool}"
  elif [ -x "${BIN_DIR}/${tool}" ]; then
    cp "${BIN_DIR}/${tool}" "${MACOS}/${tool}"
  fi
  if [ -x "${MACOS}/${tool}" ]; then
    chmod +x "${MACOS}/${tool}"
  fi
done

if [ -f gui/AppIcon.icns ]; then
  cp gui/AppIcon.icns "${RES}/AppIcon.icns"
fi
if [ -f gui/EmptyPreview.png ]; then
  cp gui/EmptyPreview.png "${RES}/EmptyPreview.png"
fi

cat > "${APP_DIR}/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>HP-M177-Scan</string>
  <key>CFBundleIdentifier</key><string>se.makeitso.hp-m177-scanner</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>HP Color LaserJet Pro MFP M177fw Scanner</string>
  <key>CFBundleDisplayName</key><string>HP Color LaserJet Pro MFP M177fw Scanner</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.2.0</string>
  <key>CFBundleVersion</key><string>16</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Scanned image</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.jpeg</string>
        <string>public.tiff</string>
        <string>public.png</string>
        <string>com.adobe.pdf</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
PLIST

# Custom Finder icon (script wrapper otherwise looks generic).
if [ -f "${RES}/AppIcon.icns" ]; then
  SETICON="$(mktemp -t hp-m177-seticon).swift"
  cat > "${SETICON}" <<'SWIFT'
import AppKit
guard CommandLine.arguments.count >= 3,
      let image = NSImage(contentsOfFile: CommandLine.arguments[2]) else {
    fputs("setIcon: missing image\n", stderr)
    exit(1)
}
let ok = NSWorkspace.shared.setIcon(image, forFile: CommandLine.arguments[1], options: [])
print(ok ? "setIcon ok" : "setIcon failed")
exit(ok ? 0 : 1)
SWIFT
  swift "${SETICON}" "${APP_DIR}" "${RES}/AppIcon.icns" || true
  rm -f "${SETICON}"
fi

# Refresh Launch Services so Spotlight/Finder see the app.
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "${APP_DIR}" >/dev/null 2>&1 || true

# Image Capture lists Automatic Tasks and apps that open JPEG/PDF/TIFF.
for icdir in \
  "${HOME}/Library/Image Capture/Automatic Tasks" \
  "${HOME}/Library/Application Support/Apple/Image Capture/Automatic Tasks"
do
  mkdir -p "${icdir}"
  ln -sfn "${APP_DIR}" "${icdir}/HP Color LaserJet Pro MFP M177fw Scanner.app"
done

echo "installed ${BIN_DIR}/hp-m177-native-gui"
echo "installed ${APP_DIR}"
echo "open with: open \"${APP_DIR}\""
echo "or: hp-m177-gui"
