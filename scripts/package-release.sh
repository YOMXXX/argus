#!/bin/sh
# Build a release archive and matching SHA-256 checksum for one target.
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: scripts/package-release.sh <target-triple>" >&2
  exit 2
fi

checksum_file() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    echo "sha256sum or shasum is required to package release assets" >&2
    exit 1
  fi
}

target="$1"
argus_bin_name="argus"
arguscode_bin_name="arguscode"
case "$target" in
  *windows*|*-pc-*)
    argus_bin_name="argus.exe"
    arguscode_bin_name="arguscode.exe"
    ;;
esac

argus_bin_path="${ARGUS_BIN_PATH:-target/$target/release/$argus_bin_name}"
arguscode_bin_path="${ARGUSCODE_BIN_PATH:-target/$target/release/$arguscode_bin_name}"
dist="${ARGUS_DIST_DIR:-dist}"
archive_name="argus-$target.tar.gz"
archive_path="$dist/$archive_name"

if [ ! -f "$argus_bin_path" ]; then
  echo "release binary not found: $argus_bin_path" >&2
  exit 1
fi
if [ ! -f "$arguscode_bin_path" ]; then
  echo "release binary not found: $arguscode_bin_path" >&2
  exit 1
fi

mkdir -p "$dist"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/package"
cp "$argus_bin_path" "$tmp/package/$argus_bin_name"
cp "$arguscode_bin_path" "$tmp/package/$arguscode_bin_name"
cp README.md LICENSE-MIT LICENSE-APACHE "$tmp/package/"
chmod 755 "$tmp/package/$argus_bin_name" "$tmp/package/$arguscode_bin_name" 2>/dev/null || true

tar -czf "$archive_path" -C "$tmp/package" "$argus_bin_name" "$arguscode_bin_name" README.md LICENSE-MIT LICENSE-APACHE
sum="$(checksum_file "$archive_path")"
printf '%s  %s\n' "$sum" "$archive_name" > "$archive_path.sha256"

echo "wrote $archive_path"
echo "wrote $archive_path.sha256"
