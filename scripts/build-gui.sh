#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
mkdir -p target
swiftc -O \
  -o target/hp-m177-native-gui \
  gui/HP-M177-Scan.swift
echo "built target/hp-m177-native-gui"
