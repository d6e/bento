#!/usr/bin/env bash
set -euo pipefail

# Icon generation script for Bento
# Converts logo.png to multiple sizes for Linux and macOS packaging

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
LOGO="$PROJECT_ROOT/images/logo.png"
ICON_DIR="$PROJECT_ROOT/dist/icons"

# Check if logo.png exists
if [ ! -f "$LOGO" ]; then
    echo "Error: logo.png not found at $LOGO"
    exit 1
fi

# Check if ImageMagick is installed
if ! command -v convert &> /dev/null; then
    echo "Error: ImageMagick (convert) is not installed"
    echo "Install with: apt-get install imagemagick (Linux) or brew install imagemagick (macOS)"
    exit 1
fi

echo "Generating icons from logo.png..."

# Generate PNG icons at standard sizes
for size in 16 32 48 64 128 256 512; do
    echo "Creating ${size}x${size}.png..."
    convert "$LOGO" -resize "${size}x${size}" "$ICON_DIR/${size}x${size}.png"
done

# Generate macOS .icns file if png2icns is available
if command -v png2icns &> /dev/null; then
    echo "Creating bento.icns for macOS..."
    png2icns "$ICON_DIR/bento.icns" \
        "$ICON_DIR/16x16.png" \
        "$ICON_DIR/32x32.png" \
        "$ICON_DIR/48x48.png" \
        "$ICON_DIR/128x128.png" \
        "$ICON_DIR/256x256.png" \
        "$ICON_DIR/512x512.png"
else
    echo "Warning: png2icns not found, skipping .icns generation"
    echo "Install with: apt-get install icnsutils (Linux) or brew install libicns (macOS)"

    # Alternative: use ImageMagick to create a basic .icns (less ideal but works)
    echo "Creating basic .icns with ImageMagick..."
    convert "$ICON_DIR/512x512.png" "$ICON_DIR/bento.icns"
fi

echo "Icon generation complete!"
echo "Generated icons in: $ICON_DIR"
