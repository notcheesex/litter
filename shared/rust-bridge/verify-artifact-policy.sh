#!/usr/bin/env bash
#
# Verify that generated UniFFI sources and platform build products obey the
# repository artifact policy. This script is intentionally lightweight: it
# checks tracked paths and git status only; it does not build or generate.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

artifact_paths=(
    "shared/rust-bridge/generated"
    "apps/ios/GeneratedRust"
    "apps/ios/Frameworks"
    "apps/ios/Resources/fs"
    "shared/rust-bridge/target"
    "apps/android/core/bridge/src/main/jniLibs"
    "apps/android/core/bridge/.cxx"
    "apps/android/core/bridge/src/main/cpp/include/ghostty.h"
    "apps/android/app/src/main/jniLibs"
    "apps/android/app/src/main/assets/alpine-fs.tgz"
    "apps/android/app/src/main/assets/alpine-fs.version"
    "apps/android/app/src/main/assets/licenses/proot-COPYING.txt"
    "apps/android/app/src/main/assets/licenses/talloc-COPYING.txt"
    "apps/android/app/src/main/assets/proot.version"
    "apps/android/app/src/main/assets/bundled_env"
)

violations=()
while IFS= read -r -d '' path; do
    case "$path" in
        shared/rust-bridge/generated/*|\
        apps/ios/GeneratedRust/*|\
        apps/ios/Frameworks/*|\
        apps/ios/Resources/fs/*|\
        apps/android/core/bridge/src/main/jniLibs/*|\
        apps/android/core/bridge/.cxx/*|\
        apps/android/core/bridge/src/main/cpp/include/ghostty.h|\
        apps/android/app/src/main/jniLibs/*|\
        apps/android/app/src/main/assets/alpine-fs.tgz|\
        apps/android/app/src/main/assets/alpine-fs.version|\
        apps/android/app/src/main/assets/licenses/proot-COPYING.txt|\
        apps/android/app/src/main/assets/licenses/talloc-COPYING.txt|\
        apps/android/app/src/main/assets/proot.version|\
        apps/android/app/src/main/assets/bundled_env/*|\
        target/*|\
        */target/*|\
        *.generated.rs|\
        *.generated.swift|\
        *.a|\
        *.so|\
        *.xcframework|\
        *.xcframework/*)
            violations+=("$path")
            ;;
    esac
done < <(git -C "$REPO_ROOT" ls-files -z)

if [[ "${#violations[@]}" -gt 0 ]]; then
    echo "ERROR: generated/build artifacts are tracked but must stay local-only:" >&2
    printf '  %s\n' "${violations[@]}" >&2
    exit 1
fi

echo "==> Artifact policy tracked-file denylist passed"

unignored_status="$(
    git -C "$REPO_ROOT" status --porcelain --untracked-files=all -- "${artifact_paths[@]}"
)"
if [[ -n "$unignored_status" ]]; then
    echo "ERROR: generated/build artifact paths contain unignored dirty files:" >&2
    printf '%s\n' "$unignored_status" >&2
    exit 1
fi

echo "==> No unignored generated/build artifacts detected"
echo "==> Ignored generated/build artifact status (expected local artifacts may appear with !!):"
git -C "$REPO_ROOT" status --porcelain --ignored=matching --untracked-files=all -- "${artifact_paths[@]}"
