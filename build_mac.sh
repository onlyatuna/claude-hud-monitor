#!/bin/bash
# One-click macOS Standalone App Builder
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$DIR"

echo "=== Installing dependencies ==="
python3 -m pip install --upgrade pip
python3 -m pip install -r requirements-build.txt

echo "=== Checking macOS ICNS icon ==="
ICON_ARG="assets/app_icon.png"
if [ ! -f "assets/app_icon.icns" ]; then
    python3 -c "from PIL import Image; Image.open('assets/app_icon.png').save('assets/app_icon.icns')" 2>/dev/null || true
fi
if [ -f "assets/app_icon.icns" ]; then
    ICON_ARG="assets/app_icon.icns"
fi

echo "=== Running unit tests ==="
python3 -B -m unittest discover -s tests -v

echo "=== Building ClaudeHUD.app for macOS using ClaudeHUD.spec ==="
python3 -m PyInstaller --noconfirm --clean ClaudeHUD.spec

echo "=== Verifying packaged app offline ==="
python3 -c "import subprocess; subprocess.run(['dist/ClaudeHUD.app/Contents/MacOS/ClaudeHUD', '--smoke-test'], check=True, timeout=45)"

echo "=== Packing into ZIP ==="
cd dist
zip -r ClaudeHUD-macOS.zip ClaudeHUD.app

echo "=== Build complete! Output located in dist/ClaudeHUD.app and dist/ClaudeHUD-macOS.zip ==="
