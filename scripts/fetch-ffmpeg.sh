#!/usr/bin/env bash
# Downloads a static FFmpeg build and places it as the Tauri sidecar binary
# for the current platform's target triple.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$ROOT/src-tauri/binaries"
mkdir -p "$BIN_DIR"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64) TRIPLE="x86_64-unknown-linux-gnu";;
      aarch64) TRIPLE="aarch64-unknown-linux-gnu";;
      *) echo "Unsupported arch: $ARCH"; exit 1;;
    esac
    URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-${ARCH}-static.tar.xz"
    TARGET="$BIN_DIR/ffmpeg-$TRIPLE"
    if [ -f "$TARGET" ]; then echo "FFmpeg already present: $TARGET"; exit 0; fi
    TMP="$(mktemp -d)"
    echo "Downloading $URL ..."
    curl -L "$URL" -o "$TMP/ffmpeg.tar.xz"
    tar -xJf "$TMP/ffmpeg.tar.xz" -C "$TMP"
    FF="$(find "$TMP" -type f -name ffmpeg | head -n1)"
    cp "$FF" "$TARGET"; chmod +x "$TARGET"
    ;;
  Darwin)
    case "$ARCH" in
      arm64) TRIPLE="aarch64-apple-darwin";;
      x86_64) TRIPLE="x86_64-apple-darwin";;
      *) echo "Unsupported arch: $ARCH"; exit 1;;
    esac
    URL="https://evermeet.cx/ffmpeg/getrelease/zip"
    TARGET="$BIN_DIR/ffmpeg-$TRIPLE"
    if [ -f "$TARGET" ]; then echo "FFmpeg already present: $TARGET"; exit 0; fi
    TMP="$(mktemp -d)"
    echo "Downloading $URL ..."
    curl -L "$URL" -o "$TMP/ffmpeg.zip"
    unzip -o "$TMP/ffmpeg.zip" -d "$TMP" >/dev/null
    cp "$TMP/ffmpeg" "$TARGET"; chmod +x "$TARGET"
    ;;
  *)
    echo "Unsupported OS: $OS"; exit 1;;
esac

echo "FFmpeg ready: $TARGET"
"$TARGET" -version | head -n1
