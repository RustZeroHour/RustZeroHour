#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Regenerates the rzh-util golden vectors from the C++ reference
# transcription. Requires a C++ compiler (g++ or clang++).
#
# Usage:
#   tools/differential/regenerate.sh
#
# After regenerating, run:
#   cargo test -p rzh-util
# and review the git diff of crates/rzh-util/tests/golden/ before committing.

set -e

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
OUT_DIR="$REPO_ROOT/crates/rzh-util/tests/golden"

CXX=${CXX:-g++}
command -v "$CXX" >/dev/null 2>&1 || CXX=clang++
command -v "$CXX" >/dev/null 2>&1 || {
    echo "error: no C++ compiler found (tried g++ and clang++)" >&2
    exit 1
}

BUILD_DIR=$(mktemp -d)
trap 'rm -rf "$BUILD_DIR"' EXIT

echo "compiling gen_golden.cpp with $CXX..."
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -o "$BUILD_DIR/gen_golden" "$SCRIPT_DIR/gen_golden.cpp"

mkdir -p "$OUT_DIR"
"$BUILD_DIR/gen_golden" "$OUT_DIR/crc.txt" "$OUT_DIR/rng.txt"

echo "wrote $OUT_DIR/crc.txt and $OUT_DIR/rng.txt"
echo "now run: cargo test -p rzh-util"
