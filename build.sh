#!/bin/bash
set -e

APP_NAME="VoiceType"
BUILD_DIR="build"
APP_BUNDLE="$BUILD_DIR/$APP_NAME.app"

echo "🔨 Building $APP_NAME..."

# Clean previous build
rm -rf "$BUILD_DIR"
mkdir -p "$APP_BUNDLE/Contents/MacOS"
mkdir -p "$APP_BUNDLE/Contents/Resources"

# Compile all Swift sources
swiftc \
    -o "$APP_BUNDLE/Contents/MacOS/$APP_NAME" \
    Sources/*.swift \
    -framework Cocoa \
    -framework AVFoundation \
    -framework Speech \
    -O \
    -suppress-warnings

# Copy Info.plist
cp Resources/Info.plist "$APP_BUNDLE/Contents/"

# Ad-hoc code sign
codesign --force --sign - "$APP_BUNDLE"

echo ""
echo "✅ Built: $APP_BUNDLE"
echo ""
echo "To install:"
echo "  cp -r $APP_BUNDLE /Applications/"
echo ""
echo "To run:"
echo "  open /Applications/$APP_NAME.app"
echo ""
echo "First run: grant Microphone, Speech Recognition, and Accessibility"
echo "in System Settings → Privacy & Security."
