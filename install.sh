#!/bin/sh
# Argus installer — downloads the latest prebuilt binary from GitHub Releases.
#   curl -fsSL https://raw.githubusercontent.com/YOMXXX/argus/master/install.sh | sh
# Override install dir with ARGUS_INSTALL_DIR (default: ~/.local/bin).
set -eu

REPO="YOMXXX/argus"
BIN="argus"

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Linux)
    target_os="unknown-linux-gnu"
    case "$arch" in
      x86_64|amd64) target_arch="x86_64" ;;
      *) echo "No prebuilt Linux binary for $arch yet — build from source: https://github.com/$REPO"; exit 1 ;;
    esac
    ;;
  Darwin)
    target_os="apple-darwin"
    case "$arch" in
      arm64|aarch64) target_arch="aarch64" ;;
      x86_64) target_arch="x86_64" ;;
      *) echo "Unsupported macOS arch: $arch"; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $os — build from source: https://github.com/$REPO"
    exit 1
    ;;
esac

target="${target_arch}-${target_os}"
dest="${ARGUS_INSTALL_DIR:-$HOME/.local/bin}"
url="https://github.com/$REPO/releases/latest/download/argus-${target}.tar.gz"

echo "Installing argus ($target) ..."
mkdir -p "$dest"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$url" | tar -xz -C "$tmp"
mv "$tmp/$BIN" "$dest/$BIN"
chmod +x "$dest/$BIN"

echo "✅ argus installed to $dest/$BIN"
case ":$PATH:" in
  *":$dest:"*) ;;
  *) echo "⚠️  Add $dest to your PATH:  export PATH=\"$dest:\$PATH\"" ;;
esac
"$dest/$BIN" --version || true
