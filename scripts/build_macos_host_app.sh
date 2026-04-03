#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="Codex Island.app"
APP_DIR="$ROOT_DIR/macos-host/build/$APP_NAME"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
BIN_DIR="$RESOURCES_DIR/bin"

cd "$ROOT_DIR"

pnpm build
cargo build --manifest-path native-bridge/Cargo.toml --release
swift build --package-path macos-host -c release

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR" "$BIN_DIR"

cp "macos-host/Info.plist" "$CONTENTS_DIR/Info.plist"
cp "macos-host/.build/apple/Products/Release/CodexIslandHostApp" "$MACOS_DIR/CodexIslandHostApp" 2>/dev/null \
  || cp "macos-host/.build/release/CodexIslandHostApp" "$MACOS_DIR/CodexIslandHostApp"
cp "native-bridge/target/release/codex-island-native-bridge" "$BIN_DIR/codex-island-native-bridge"
cp -R "dist" "$RESOURCES_DIR/dist"

chmod +x "$MACOS_DIR/CodexIslandHostApp"
chmod +x "$BIN_DIR/codex-island-native-bridge"

codesign --force --deep --sign - "$APP_DIR" >/dev/null 2>&1 || true

echo "$APP_DIR"
