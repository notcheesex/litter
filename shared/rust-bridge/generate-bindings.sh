#!/usr/bin/env bash
#
# Generate Swift/Kotlin bindings from codex-mobile-client.
#
# Usage:  ./generate-bindings.sh [--release] [--swift-only] [--kotlin-only]
#
# Outputs:
#   generated/swift/   — Swift source files
#   generated/kotlin/  — Kotlin source files

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$SCRIPT_DIR"
source "$WORKSPACE_DIR/../../tools/scripts/load-sccache-aws-creds.sh"
CRATE_DIR="$WORKSPACE_DIR/codex-mobile-client"
OUT_SWIFT="$WORKSPACE_DIR/generated/swift"
OUT_KOTLIN="$WORKSPACE_DIR/generated/kotlin"
REPRO_TMP_DIR=""

cleanup() {
    if [[ -n "${REPRO_TMP_DIR:-}" && -d "$REPRO_TMP_DIR" ]]; then
        rm -rf "$REPRO_TMP_DIR"
    fi
}
trap cleanup EXIT

cd "$WORKSPACE_DIR"

if [[ -z "${RUSTC_WRAPPER:-}" ]] && command -v sccache >/dev/null 2>&1; then
    export RUSTC_WRAPPER="$(command -v sccache)"
fi

"$WORKSPACE_DIR/../../tools/scripts/update-alleycat-main.sh" --shared

PROFILE="debug"
GENERATE_SWIFT=1
GENERATE_KOTLIN=1

for arg in "$@"; do
    case "$arg" in
        --release)
            PROFILE="release"
            ;;
        --swift-only)
            GENERATE_KOTLIN=0
            ;;
        --kotlin-only)
            GENERATE_SWIFT=0
            ;;
        *)
            echo "usage: $(basename "$0") [--release] [--swift-only] [--kotlin-only]" >&2
            exit 1
            ;;
    esac
done

if [[ "$GENERATE_SWIFT" -eq 0 && "$GENERATE_KOTLIN" -eq 0 ]]; then
    echo "error: nothing to generate" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# 1. Build the cdylib so uniffi-bindgen can read its metadata
# ---------------------------------------------------------------------------
echo "==> Building codex-mobile-client cdylib ($PROFILE)..."

if [[ "$PROFILE" == "release" ]]; then
    cargo build -p codex-mobile-client --release
else
    cargo build -p codex-mobile-client
fi

DYLIB_PATH="${CARGO_TARGET_DIR:-$WORKSPACE_DIR/target}/$PROFILE"

# Resolve the dynamic library name per platform
if [[ "$(uname)" == "Darwin" ]]; then
    DYLIB_FILE="$DYLIB_PATH/libcodex_mobile_client.dylib"
else
    DYLIB_FILE="$DYLIB_PATH/libcodex_mobile_client.so"
fi

if [[ ! -f "$DYLIB_FILE" ]]; then
    echo "ERROR: Could not find built library at $DYLIB_FILE" >&2
    exit 1
fi

generate_swift_bindings() {
    local out_dir="$1"

    echo "==> Generating Swift bindings -> $out_dir"
    mkdir -p "$out_dir"
    cargo run -p uniffi-bindgen -- generate \
        --library "$DYLIB_FILE" \
        --language swift \
        --out-dir "$out_dir"
    cp "$out_dir/codex_mobile_clientFFI.modulemap" "$out_dir/module.modulemap"
}

generate_kotlin_bindings() {
    local out_dir="$1"

    echo "==> Generating Kotlin bindings -> $out_dir"
    mkdir -p "$out_dir"
    cargo run -p uniffi-bindgen -- generate \
        --library "$DYLIB_FILE" \
        --language kotlin \
        --out-dir "$out_dir"
}

verify_regeneration_idempotence() {
    if [[ "${VERIFY_BINDING_REPRODUCIBILITY:-1}" == "0" ]]; then
        echo "==> Skipping binding regeneration idempotence check (VERIFY_BINDING_REPRODUCIBILITY=0)"
        return
    fi

    REPRO_TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/codex-mobile-bindings.XXXXXX")"
    echo "==> Verifying binding regeneration idempotence (second pass -> $REPRO_TMP_DIR)"

    if [[ "$GENERATE_SWIFT" -eq 1 ]]; then
        generate_swift_bindings "$REPRO_TMP_DIR/swift"
        if ! diff -ruN "$OUT_SWIFT" "$REPRO_TMP_DIR/swift"; then
            echo "ERROR: Swift binding regeneration is not idempotent" >&2
            exit 1
        fi
    fi

    if [[ "$GENERATE_KOTLIN" -eq 1 ]]; then
        generate_kotlin_bindings "$REPRO_TMP_DIR/kotlin"
        if ! diff -ruN "$OUT_KOTLIN" "$REPRO_TMP_DIR/kotlin"; then
            echo "ERROR: Kotlin binding regeneration is not idempotent" >&2
            exit 1
        fi
    fi

    echo "==> Binding regeneration idempotence verified"
}

run_artifact_policy_check() {
    if [[ "${VERIFY_BINDING_ARTIFACT_POLICY:-1}" == "0" ]]; then
        echo "==> Skipping generated/build artifact policy check (VERIFY_BINDING_ARTIFACT_POLICY=0)"
        return
    fi

    bash "$WORKSPACE_DIR/verify-artifact-policy.sh"
}

if [[ "$GENERATE_SWIFT" -eq 1 ]]; then
    rm -rf "$OUT_SWIFT"
    generate_swift_bindings "$OUT_SWIFT"
fi

if [[ "$GENERATE_KOTLIN" -eq 1 ]]; then
    rm -rf "$OUT_KOTLIN"
    generate_kotlin_bindings "$OUT_KOTLIN"
fi

verify_regeneration_idempotence
run_artifact_policy_check

echo "==> Done. Generated bindings:"
if [[ "$GENERATE_SWIFT" -eq 1 && "$GENERATE_KOTLIN" -eq 1 ]]; then
    find "$OUT_SWIFT" "$OUT_KOTLIN" -type f | sort
elif [[ "$GENERATE_SWIFT" -eq 1 ]]; then
    find "$OUT_SWIFT" -type f | sort
else
    find "$OUT_KOTLIN" -type f | sort
fi
