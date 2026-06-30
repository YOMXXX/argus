#!/bin/sh
# Argus installer — downloads and verifies a prebuilt binary from GitHub Releases.
#   curl -fsSL https://raw.githubusercontent.com/YOMXXX/argus/master/install.sh | sh
# Override install dir with ARGUS_INSTALL_DIR (default: ~/.local/bin).
set -eu

REPO="${ARGUS_REPO:-YOMXXX/argus}"
BIN="argus"
VERSION="${ARGUS_VERSION:-latest}"

checksum_file() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    echo "sha256sum or shasum is required to verify the download" >&2
    exit 1
  fi
}

verify_checksum() {
  checksum_archive_path="$1"
  checksum_path="$2"
  expected="$(awk '{print $1; exit}' "$checksum_path")"
  if [ -z "$expected" ]; then
    echo "empty checksum file: $checksum_path" >&2
    exit 1
  fi
  actual="$(checksum_file "$checksum_archive_path")"
  if [ "$actual" != "$expected" ]; then
    echo "checksum mismatch for $(basename "$checksum_archive_path")" >&2
    echo "expected: $expected" >&2
    echo "actual:   $actual" >&2
    exit 1
  fi
}

os="$(uname -s)"
arch="$(uname -m)"

if [ -n "${ARGUS_TARGET:-}" ]; then
  target="$ARGUS_TARGET"
else
  case "$os" in
    Linux)
      target_os="unknown-linux-gnu"
      case "$arch" in
        x86_64|amd64) target_arch="x86_64" ;;
        *) echo "No prebuilt Linux binary for $arch yet; build from source: https://github.com/$REPO" >&2; exit 1 ;;
      esac
      ;;
    Darwin)
      target_os="apple-darwin"
      case "$arch" in
        arm64|aarch64) target_arch="aarch64" ;;
        x86_64) target_arch="x86_64" ;;
        *) echo "Unsupported macOS arch: $arch" >&2; exit 1 ;;
      esac
      ;;
    *)
      echo "Unsupported OS: $os; build from source: https://github.com/$REPO" >&2
      exit 1
      ;;
  esac
  target="${target_arch}-${target_os}"
fi

dest="${ARGUS_INSTALL_DIR:-$HOME/.local/bin}"
archive="argus-${target}.tar.gz"
if [ -n "${ARGUS_RELEASE_BASE_URL:-}" ]; then
  release_base="${ARGUS_RELEASE_BASE_URL%/}"
elif [ "$VERSION" = "latest" ]; then
  release_base="https://github.com/$REPO/releases/latest/download"
else
  release_base="https://github.com/$REPO/releases/download/$VERSION"
fi
url="$release_base/$archive"
checksum_url="$url.sha256"

echo "Installing argus ($target) ..."
mkdir -p "$dest"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

curl -fsSL "$url" -o "$tmp/$archive"
curl -fsSL "$checksum_url" -o "$tmp/$archive.sha256"
verify_checksum "$tmp/$archive" "$tmp/$archive.sha256"
tar -xzf "$tmp/$archive" -C "$tmp"
cp "$tmp/$BIN" "$dest/$BIN"
chmod +x "$dest/$BIN"

echo "argus installed to $dest/$BIN"
case ":$PATH:" in
  *":$dest:"*) ;;
  *) echo "Add $dest to your PATH:  export PATH=\"$dest:\$PATH\"" ;;
esac
"$dest/$BIN" --version || true
