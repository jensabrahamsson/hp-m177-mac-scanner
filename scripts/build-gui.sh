#!/bin/sh
# AppKit helper. Requires Apple swiftc (Xcode CLT) and macOS 12.0+
# (see Info.plist LSMinimumSystemVersion in install-gui.sh).
set -eu
cd "$(dirname "$0")/.."
mkdir -p target
swiftc -O \
  -o target/hp-m177-native-gui \
  gui/HP-M177-Scan.swift
echo "built target/hp-m177-native-gui"
