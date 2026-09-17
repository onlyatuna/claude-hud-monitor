#!/bin/bash
# One-click macOS Standalone App Builder
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$DIR"

echo "=== Installing dependencies ==="
python3 -m pip install --upgrade pip
python3 -m pip install -r requirements-build.txt

echo "=== Building ClaudeHUD.app for macOS ==="
python3 -m PyInstaller --windowed --hidden-import pynput.keyboard._darwin --hidden-import pynput.mouse._darwin --name "ClaudeHUD" --icon "assets/app_icon.png" --add-data "assets:assets" main.py

echo "=== Packing into ZIP ==="
cd dist
zip -r ClaudeHUD-macOS.zip ClaudeHUD.app

echo "=== Build complete! Output located in dist/ClaudeHUD.app and dist/ClaudeHUD-macOS.zip ==="
