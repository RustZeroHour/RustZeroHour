#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Regenerates the rzh-util and rzh-math golden vectors from the C++ reference
# transcription. Requires a C++ compiler (g++ or clang++) on an IEEE-754
# platform (x86-64, arm64, ...).
#
# Usage:
#   tools/differential/regenerate.sh
#
# After regenerating, run:
#   cargo test --workspace
# and review the git diff of crates/*/tests/golden before committing.

set -e

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
UTIL_GOLDEN="$REPO_ROOT/crates/rzh-util/tests/golden"
MATH_GOLDEN="$REPO_ROOT/crates/rzh-math/tests/golden"

CXX=${CXX:-g++}
command -v "$CXX" >/dev/null 2>&1 || CXX=clang++
command -v "$CXX" >/dev/null 2>&1 || {
    echo "error: no C++ compiler found (tried g++ and clang++)" >&2
    exit 1
}

BUILD_DIR=$(mktemp -d)
trap 'rm -rf "$BUILD_DIR"' EXIT

# -ffp-contract=off: keep a*b+c as two IEEE operations. The C++ default for
# GCC/clang is "fast", which may fuse them into an FMA where the target
# supports it; Rust never contracts, and the game's MSVC build does not
# either (fp:precise). Without this flag the vector goldens would only be
# reproducible on baseline x86-64.
echo "compiling gen_golden.cpp with $CXX..."
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -o "$BUILD_DIR/gen_golden" "$SCRIPT_DIR/gen_golden.cpp"

mkdir -p "$UTIL_GOLDEN" "$MATH_GOLDEN"
"$BUILD_DIR/gen_golden" \
    "$UTIL_GOLDEN/crc.txt" \
    "$UTIL_GOLDEN/rng.txt" \
    "$UTIL_GOLDEN/namekey.txt" \
    "$MATH_GOLDEN/scalar.txt" \
    "$MATH_GOLDEN/vectors.txt"

echo "wrote $UTIL_GOLDEN/{crc,rng,namekey}.txt"
echo "wrote $MATH_GOLDEN/{scalar,vectors}.txt"
echo "now run: cargo test --workspace"
