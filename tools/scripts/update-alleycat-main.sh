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

# This script name is retained for compatibility with existing build lanes, but
# mission-relevant Alleycat resolution must be deterministic. Keep this value in
# sync with the Cargo.toml pins and lockfiles.
ALLEYCAT_REV="4e42351d8ce670805b1c12d9dc2830ed123c9189"
ALLEYCAT_SOURCE_URL="https://github.com/notcheesex/alleycat.git"

update_shared() {
  echo "==> Resolving shared Rust Alleycat deps to $ALLEYCAT_SOURCE_URL rev $ALLEYCAT_REV..."
  for package in \
    alleycat-bridge-core \
    alleycat-pi-bridge \
    alleycat-claude-bridge \
    alleycat-opencode-bridge
  do
    cargo update \
      --quiet \
      --manifest-path "$REPO_DIR/shared/rust-bridge/Cargo.toml" \
      -p "$package" \
      --precise "$ALLEYCAT_REV"
  done
}

update_kittylitter() {
  echo "==> Resolving kittylitter Alleycat dep to $ALLEYCAT_SOURCE_URL rev $ALLEYCAT_REV..."
  cargo update \
    --quiet \
    --manifest-path "$REPO_DIR/services/kittylitter/Cargo.toml" \
    -p alleycat \
    --precise "$ALLEYCAT_REV"
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
