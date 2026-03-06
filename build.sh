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

# Copy menu bar icons
cp Resources/MenuIconTemplate*.png "$APP_BUNDLE/Contents/Resources/" 2>/dev/null || true

# Generate .icns from AppIcon.png if it exists
if [ -f Resources/AppIcon.png ]; then
    ICONSET="$BUILD_DIR/AppIcon.iconset"
    mkdir -p "$ICONSET"
    sips -z 16 16     Resources/AppIcon.png --out "$ICONSET/icon_16x16.png"      > /dev/null 2>&1
    sips -z 32 32     Resources/AppIcon.png --out "$ICONSET/icon_16x16@2x.png"   > /dev/null 2>&1
    sips -z 32 32     Resources/AppIcon.png --out "$ICONSET/icon_32x32.png"      > /dev/null 2>&1
    sips -z 64 64     Resources/AppIcon.png --out "$ICONSET/icon_32x32@2x.png"   > /dev/null 2>&1
    sips -z 128 128   Resources/AppIcon.png --out "$ICONSET/icon_128x128.png"    > /dev/null 2>&1
    sips -z 256 256   Resources/AppIcon.png --out "$ICONSET/icon_128x128@2x.png" > /dev/null 2>&1
    sips -z 256 256   Resources/AppIcon.png --out "$ICONSET/icon_256x256.png"    > /dev/null 2>&1
    sips -z 512 512   Resources/AppIcon.png --out "$ICONSET/icon_256x256@2x.png" > /dev/null 2>&1
    sips -z 512 512   Resources/AppIcon.png --out "$ICONSET/icon_512x512.png"    > /dev/null 2>&1
    sips -z 1024 1024 Resources/AppIcon.png --out "$ICONSET/icon_512x512@2x.png" > /dev/null 2>&1
    iconutil -c icns "$ICONSET" -o "$APP_BUNDLE/Contents/Resources/AppIcon.icns"
    rm -rf "$ICONSET"
    echo "🎨 App icon generated"
fi

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
