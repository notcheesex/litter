#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

MODE="all"
case "${1:-}" in
  "")
    ;;
  --all|--shared|--kittylitter)
    MODE="${1#--}"
    ;;
  *)
    echo "usage: $(basename "$0") [--all|--shared|--kittylitter]" >&2
    exit 1
    ;;
esac

if [ "${LITTER_SKIP_ALLEYCAT_UPDATE:-0}" = "1" ]; then
  echo "==> Skipping pinned Alleycat resolution (LITTER_SKIP_ALLEYCAT_UPDATE=1)"
  exit 0
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is required" >&2
  exit 1
fi

# This script name is retained for compatibility with existing build lanes. It
# now verifies the pinned Alleycat lockfile state instead of refreshing a
# floating branch. Keep this value in sync with the Cargo.toml pins and
# lockfiles.
ALLEYCAT_REV="4e42351d8ce670805b1c12d9dc2830ed123c9189"
ALLEYCAT_SOURCE_URL="https://github.com/notcheesex/alleycat.git"

verify_metadata_locked() {
  local label="$1"
  local manifest_path="$2"
  local expected_source
  local metadata_file

  expected_source="git+$ALLEYCAT_SOURCE_URL?rev=$ALLEYCAT_REV#$ALLEYCAT_REV"
  metadata_file="$(mktemp)"
  echo "==> Verifying $label Alleycat deps with cargo metadata --locked --manifest-path $manifest_path --format-version 1"
  cargo metadata \
    --locked \
    --manifest-path "$manifest_path" \
    --format-version 1 \
    >"$metadata_file"

  echo "==> Resolved $label Alleycat sources:"
  if ! grep -F -o "$expected_source" "$metadata_file" | sort -u; then
    rm -f "$metadata_file"
    echo "error: $label did not resolve Alleycat from $expected_source" >&2
    exit 1
  fi
  rm -f "$metadata_file"
}

update_shared() {
  verify_metadata_locked "shared Rust" "$REPO_DIR/shared/rust-bridge/Cargo.toml"
}

update_kittylitter() {
  verify_metadata_locked "kittylitter" "$REPO_DIR/services/kittylitter/Cargo.toml"
}

case "$MODE" in
  all)
    update_shared
    update_kittylitter
    ;;
  shared)
    update_shared
    ;;
  kittylitter)
    update_kittylitter
    ;;
esac
