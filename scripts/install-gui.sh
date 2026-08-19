#!/bin/sh
# Build the AppKit GUI and install it next to hp-m177 plus an .app the Dock
# and Spotlight can launch.
set -eu
cd "$(dirname "$0")/.."

BIN_DIR="${HOME}/.cargo/bin"
APP_DIR="${HOME}/Applications/HP M177 Scanner.app"
MACOS="${APP_DIR}/Contents/MacOS"
RES="${APP_DIR}/Contents/Resources"

if ! command -v hp-m177 >/dev/null 2>&1 && [ ! -x "${BIN_DIR}/hp-m177" ]; then
  echo "hp-m177 is not installed. Run: cargo install --path . --locked" >&2
  exit 1
fi

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
exec "${HERE}/hp-m177-native-gui" "$@"
WRAP
chmod +x "${MACOS}/HP-M177-Scan"
cp target/hp-m177-native-gui "${MACOS}/hp-m177-native-gui"
chmod +x "${MACOS}/hp-m177-native-gui"

# Bundle the CLI so Finder/Spotlight launches do not depend on PATH.
if command -v hp-m177 >/dev/null 2>&1; then
  cp "$(command -v hp-m177)" "${MACOS}/hp-m177"
elif [ -x "${BIN_DIR}/hp-m177" ]; then
  cp "${BIN_DIR}/hp-m177" "${MACOS}/hp-m177"
fi
if [ -x "${MACOS}/hp-m177" ]; then
  chmod +x "${MACOS}/hp-m177"
fi

if [ -f gui/AppIcon.icns ]; then
  cp gui/AppIcon.icns "${RES}/AppIcon.icns"
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
  <key>CFBundleName</key><string>HP M177 Scanner</string>
  <key>CFBundleDisplayName</key><string>HP M177 Scanner</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
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

echo "installed ${BIN_DIR}/hp-m177-native-gui"
echo "installed ${APP_DIR}"
echo "open with: open \"${APP_DIR}\""
echo "or: hp-m177-gui"
