#!/usr/bin/env bash
set -euo pipefail
shopt -s nullglob

for cmd in jq gh; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "This script requires '$cmd' to be installed and on PATH." >&2
    exit 1
  fi
done

if [ $# -lt 1 ]; then
  echo "Usage: scripts/release.sh <version> [--universal]" >&2
  exit 1
fi

VERSION="$1"
UNIVERSAL=false
if [ "${2:-}" = "--universal" ]; then
  UNIVERSAL=true
fi

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Version must be in the form X.Y.Z, e.g. 0.2.0" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [ -n "$(git status --porcelain)" ]; then
  echo "Working tree is not clean. Commit or stash changes before releasing." >&2
  exit 1
fi

if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ] || [ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]; then
  cat >&2 <<'EOF'
TAURI_SIGNING_PRIVATE_KEY and/or TAURI_SIGNING_PRIVATE_KEY_PASSWORD are not set.

Generate the updater signing key once (owner only, never commit the private key):
  pnpm --dir app tauri signer generate -w ~/.tauri/fettlr.key

Then export the private key and its password before running this script:
  export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/fettlr.key)"
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<the password you set>"

Paste the printed public key into app/src-tauri/tauri.conf.json ("plugins.updater.pubkey").
EOF
  exit 1
fi

pnpm run gate
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

TAURI_CONF="app/src-tauri/tauri.conf.json"
CARGO_TOML="app/src-tauri/Cargo.toml"

jq --arg v "$VERSION" '.version = $v' "$TAURI_CONF" > "$TAURI_CONF.tmp"
mv "$TAURI_CONF.tmp" "$TAURI_CONF"

sed -i '' "s/^version = \"[^\"]*\"\$/version = \"$VERSION\"/" "$CARGO_TOML"

if [ "$UNIVERSAL" = true ]; then
  BUNDLE_DIR="target/universal-apple-darwin/release/bundle"
else
  BUNDLE_DIR="target/release/bundle"
fi

rm -rf "$BUNDLE_DIR"

if [ "$UNIVERSAL" = true ]; then
  pnpm --dir app tauri build --target universal-apple-darwin
else
  pnpm --dir app tauri build
fi

app_bundle_candidates=("$BUNDLE_DIR"/macos/*.app)
if [ ${#app_bundle_candidates[@]} -eq 0 ]; then
  echo "Built Fettlr.app not found under $BUNDLE_DIR/macos" >&2
  exit 1
fi
codesign --verify --deep --strict "${app_bundle_candidates[0]}"

app_tar_gz_candidates=("$BUNDLE_DIR"/macos/*.app.tar.gz)
dmg_candidates=("$BUNDLE_DIR"/dmg/*.dmg)

if [ ${#app_tar_gz_candidates[@]} -eq 0 ] || [ ${#dmg_candidates[@]} -eq 0 ]; then
  echo "Expected build artifacts not found under $BUNDLE_DIR" >&2
  exit 1
fi

APP_TAR_GZ="${app_tar_gz_candidates[0]}"
SIG_FILE="$APP_TAR_GZ.sig"
DMG_FILE="${dmg_candidates[0]}"

if [ ! -f "$SIG_FILE" ]; then
  echo "Signature file not found: $SIG_FILE" >&2
  exit 1
fi

SIGNATURE="$(cat "$SIG_FILE")"
PUB_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
APP_TAR_GZ_NAME="$(basename "$APP_TAR_GZ")"
ASSET_URL="https://github.com/thomasguillot/fettlr/releases/download/v$VERSION/$APP_TAR_GZ_NAME"
LATEST_JSON="$BUNDLE_DIR/latest.json"

if [ "$UNIVERSAL" = true ]; then
  jq -n \
    --arg version "$VERSION" \
    --arg pub_date "$PUB_DATE" \
    --arg url "$ASSET_URL" \
    --arg signature "$SIGNATURE" \
    '{
      version: $version,
      pub_date: $pub_date,
      platforms: {
        "darwin-aarch64": { signature: $signature, url: $url },
        "darwin-x86_64": { signature: $signature, url: $url }
      }
    }' > "$LATEST_JSON"
else
  ARCH="$(uname -m)"
  if [ "$ARCH" = "arm64" ]; then
    PLATFORM="darwin-aarch64"
  else
    PLATFORM="darwin-x86_64"
  fi
  jq -n \
    --arg version "$VERSION" \
    --arg pub_date "$PUB_DATE" \
    --arg url "$ASSET_URL" \
    --arg signature "$SIGNATURE" \
    --arg platform "$PLATFORM" \
    '{
      version: $version,
      pub_date: $pub_date,
      platforms: {
        ($platform): { signature: $signature, url: $url }
      }
    }' > "$LATEST_JSON"
fi

echo "Wrote $LATEST_JSON"
echo

DMG_NAME="$(basename "$DMG_FILE")"
SIG_NAME="$(basename "$SIG_FILE")"
LATEST_JSON_NAME="$(basename "$LATEST_JSON")"

cat <<EOF
Review the version bump and the assets under $BUNDLE_DIR, commit and tag, then run:

gh release create "v$VERSION" \\
  "$APP_TAR_GZ" \\
  "$SIG_FILE" \\
  "$DMG_FILE" \\
  "$LATEST_JSON" \\
  --title "v$VERSION" \\
  --generate-notes

Assets: $APP_TAR_GZ_NAME, $SIG_NAME, $DMG_NAME, $LATEST_JSON_NAME
EOF
