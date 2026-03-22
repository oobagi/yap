#!/bin/bash
#
# convert-sounds.sh — Convert .aiff sound assets to .wav for Windows
#
# Requires ffmpeg: brew install ffmpeg (macOS) or apt install ffmpeg (Linux)
#
# Usage:
#   ./windows/convert-sounds.sh
#
# This converts all .aiff files in Resources/Sounds/ to .wav format
# (16-bit PCM, 44100 Hz) and places them alongside the originals.
# The .wav files are used by the Windows build; the .aiff files
# remain for the macOS build.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
SOUNDS_DIR="$REPO_ROOT/Resources/Sounds"

if ! command -v ffmpeg &> /dev/null; then
    echo "ERROR: ffmpeg is required but not installed."
    echo ""
    echo "Install it with:"
    echo "  macOS:  brew install ffmpeg"
    echo "  Linux:  sudo apt install ffmpeg"
    echo "  Windows: winget install ffmpeg"
    exit 1
fi

echo "Converting .aiff sound assets to .wav..."
echo "Source: $SOUNDS_DIR"
echo ""

converted=0
for aiff_file in "$SOUNDS_DIR"/*.aiff; do
    if [ ! -f "$aiff_file" ]; then
        echo "No .aiff files found in $SOUNDS_DIR"
        exit 0
    fi

    basename="$(basename "$aiff_file" .aiff)"
    wav_file="$SOUNDS_DIR/$basename.wav"

    echo "  $basename.aiff -> $basename.wav"
    ffmpeg -i "$aiff_file" -acodec pcm_s16le -ar 44100 -y "$wav_file" 2>/dev/null
    converted=$((converted + 1))
done

echo ""
echo "Done. Converted $converted file(s)."
echo ""
echo "Converted files:"
ls -lh "$SOUNDS_DIR"/*.wav 2>/dev/null || echo "  (none)"
