# Releasing Argus

This checklist is for maintainers cutting a tagged release.

## Before Tagging

1. Review the hardened sandbox policy MVP behavior before the first public binary release:
   `argus policy show --sandbox workspace-write`, `argus policy show --sandbox read-only`, and
   `argus mcp-serve --workspace <repo>` should match the documented defaults.
2. Ensure the working tree is clean.
3. Update `workspace.package.version` in `Cargo.toml`.
4. Update `CHANGELOG.md`.
5. Run local verification:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p argus-cli --test release -- --nocapture
cargo metadata --locked --format-version 1 >/dev/null
cargo package -p argus-trace --locked --allow-dirty
```

6. Check the tag matches the workspace version:

```bash
scripts/check-release-version.sh v0.1.0
```

## Tag And GitHub Release

```bash
git tag v0.1.0
git push origin v0.1.0
```

The `Release` workflow validates the tag, runs locked tests, packages each target with `scripts/package-release.sh`, uploads artifacts from the build matrix, then publishes the GitHub Release only after all target builds complete. Each archive has a sibling `.sha256` file.

## Installer Smoke

After the GitHub Release is published:

```bash
tmp="$(mktemp -d)"
curl -fsSL https://raw.githubusercontent.com/YOMXXX/argus/master/install.sh \
  | ARGUS_VERSION=v0.1.0 ARGUS_INSTALL_DIR="$tmp/bin" sh
"$tmp/bin/argus" --version
```

For local release fixtures, point the installer at a directory or HTTP server:

```bash
ARGUS_RELEASE_BASE_URL="file:///path/to/release-assets" sh install.sh
```

## crates.io

Do not block the first GitHub Release on crates.io. The package names `argus`, `argus-core`, and
`argus-cli` must be verified or renamed before public crates.io instructions are advertised.

If the package names are available or renamed, publish crates in dependency order:

```bash
cargo publish --dry-run -p argus-trace
cargo publish -p argus-trace
cargo publish --dry-run -p argus-core
cargo publish -p argus-core
cargo publish --dry-run -p argus-cli
cargo publish -p argus-cli
```

Do not publish `argus-core` until `argus-trace` is available on crates.io at the same version. Do not publish `argus-cli` until `argus-core` is available.

Before the first publish of a version, `cargo package --workspace` and package checks for `argus-core` or `argus-cli` are expected to fail because unpublished internal crates are not yet available in the crates.io index. Use the ordered dry-run/publish sequence above instead.
