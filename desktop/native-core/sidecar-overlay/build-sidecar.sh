#!/bin/bash
# Build the native overlay sidecar for macOS and place it where Electron expects.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARIES_DIR="$SCRIPT_DIR/../binaries"
ENTITLEMENTS="$SCRIPT_DIR/../../electron/entitlements.mac.plist"
mkdir -p "$BINARIES_DIR"

# Detect target triple
ARCH=$(uname -m)
case "$ARCH" in
    arm64)  TRIPLE="aarch64-apple-darwin" ;;
    x86_64) TRIPLE="x86_64-apple-darwin" ;;
    *)      echo "Unknown arch: $ARCH"; exit 1 ;;
esac

echo "Building sidecar overlay for $TRIPLE..."
cd "$SCRIPT_DIR"
swift build -c release --product yap-overlay 2>&1
swift build -c release --product yap-speech 2>&1
swift build -c release --product yap-format 2>&1

# Copy binary to Electron binaries dir with target triple suffix
cp ".build/release/yap-overlay" "$BINARIES_DIR/yap-overlay-$TRIPLE"
cp ".build/release/yap-speech" "$BINARIES_DIR/yap-speech-$TRIPLE"
cp ".build/release/yap-format" "$BINARIES_DIR/yap-format-$TRIPLE"

# Ad-hoc codesign for local dev (Electron's bundler handles signing for distribution)
codesign --force --sign - "$BINARIES_DIR/yap-overlay-$TRIPLE" 2>/dev/null || true
codesign --force --sign - --entitlements "$ENTITLEMENTS" "$BINARIES_DIR/yap-speech-$TRIPLE" 2>/dev/null || true
codesign --force --sign - "$BINARIES_DIR/yap-format-$TRIPLE" 2>/dev/null || true

echo "Sidecar built: $BINARIES_DIR/yap-overlay-$TRIPLE"
echo "Speech helper built: $BINARIES_DIR/yap-speech-$TRIPLE"
echo "Format helper built: $BINARIES_DIR/yap-format-$TRIPLE"
