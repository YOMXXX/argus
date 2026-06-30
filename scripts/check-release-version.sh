#!/bin/sh
# Verify that a release tag such as v0.1.0 matches workspace.package.version.
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: scripts/check-release-version.sh <tag>" >&2
  exit 2
fi

tag="$1"
tag_version="${tag#v}"
workspace_version="$(
  awk '
    $0 ~ /^\[workspace.package\]/ { in_workspace_package = 1; next }
    $0 ~ /^\[/ && in_workspace_package { exit }
    in_workspace_package && $1 == "version" {
      value = $3
      gsub(/"/, "", value)
      print value
      exit
    }
  ' Cargo.toml
)"

if [ -z "$workspace_version" ]; then
  echo "could not find workspace.package.version in Cargo.toml" >&2
  exit 1
fi

if [ "$tag_version" != "$workspace_version" ]; then
  echo "release tag $tag does not match workspace version $workspace_version" >&2
  exit 1
fi

echo "release tag $tag matches workspace version $workspace_version"
