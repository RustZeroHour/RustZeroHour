#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Builds and runs the C++ baseline timing harness for the migration plan's
# §6 exit criterion ("benchmarks within budget"): the Rust foundation
# crates must be no more than 20 % slower than the C++ baseline on the same
# corpora, or an ADR must justify the exception.
#
# What is timed (all P0-exact, golden-pinned code paths):
#   - the gen_golden.cpp transcription (crc.h, RandomValue.cpp,
#     NameKeyGenerator.cpp, the WWMath headers) at baseline 571a7c5,
#   - the REAL EAC RefPack codec (refdecode.cpp / refencode.cpp),
#   - the gen_ini_golden.cpp INI-engine transcription,
#   - the gen_csf_golden.cpp CSF transcriptions (Babylon writer,
#     GameText.cpp parseCSF + stripSpaces),
#   - the gen_mapchunk_golden.cpp map data-chunk transcription
#     (DataChunk.cpp: the TOC, the parser walk, the writer),
# compiled exactly the way tools/differential/regenerate.sh compiles them
# (-O2 -ffp-contract=off, GameMath pin linked for the transcription).
#
# Usage:
#   tools/differential/bench_baseline.sh          # prints `bench <id> <ns> <MB/s>` lines
#   BENCH_BATCHES=51 tools/differential/bench_baseline.sh
#
# Then run the Rust side and compare:
#   cargo bench --workspace
#
# Temporary differential-testing infrastructure: removed at cut-over (M5).

set -e

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)

CXX=${CXX:-g++}
command -v "$CXX" >/dev/null 2>&1 || CXX=clang++
command -v "$CXX" >/dev/null 2>&1 || {
    echo "error: no C++ compiler found (tried g++ and clang++)" >&2
    exit 1
}
CC=${CC:-cc}

BUILD_DIR=$(mktemp -d)
GAMEMATH_DIR=${GAMEMATH_DIR:-$BUILD_DIR/gamemath}
trap 'rm -rf "$BUILD_DIR"' EXIT

# --- the pinned GameMath sources (needed by the gen_golden wrapper) --------
GAMEMATH_PIN=59f7ccd494f7e7c916a784ac26ef266f9f09d78d
if [ ! -d "$GAMEMATH_DIR/.git" ]; then
    echo "cloning GameMath pin $GAMEMATH_PIN ..."
    git clone -q https://github.com/TheSuperHackers/GameMath.git "$GAMEMATH_DIR"
    git -C "$GAMEMATH_DIR" checkout -q "$GAMEMATH_PIN"
fi
PIN_ACTUAL=$(git -C "$GAMEMATH_DIR" rev-parse HEAD)
if [ "$PIN_ACTUAL" != "$GAMEMATH_PIN" ]; then
    echo "error: $GAMEMATH_DIR is at $PIN_ACTUAL, expected $GAMEMATH_PIN" >&2
    exit 1
fi

# Same flags as regenerate.sh: the msun-era C sources without -Werror, the
# drivers with it; -ffp-contract=off keeps a*b+c as two IEEE operations.
PIN_SOURCES="$GAMEMATH_DIR/float/s_sinf.c $GAMEMATH_DIR/float/s_cosf.c \
    $GAMEMATH_DIR/float/s_tanf.c $GAMEMATH_DIR/float/k_sinf.c \
    $GAMEMATH_DIR/float/k_cosf.c $GAMEMATH_DIR/float/k_tanf.c \
    $GAMEMATH_DIR/float/k_rem_pio2f.c $GAMEMATH_DIR/float/e_asinf.c \
    $GAMEMATH_DIR/float/e_acosf.c $GAMEMATH_DIR/float/s_atanf.c \
    $GAMEMATH_DIR/float/e_atan2f.c $GAMEMATH_DIR/float/e_powf.c \
    $GAMEMATH_DIR/float/e_expf.c $GAMEMATH_DIR/float/e_logf.c \
    $GAMEMATH_DIR/float/e_log10f.c $GAMEMATH_DIR/float/e_sinhf.c \
    $GAMEMATH_DIR/float/e_coshf.c $GAMEMATH_DIR/float/s_tanhf.c \
    $GAMEMATH_DIR/float/s_expm1f.c $GAMEMATH_DIR/float/k_expf.c \
    $GAMEMATH_DIR/float/s_scalbnf.c $GAMEMATH_DIR/float/s_lrintf.c \
    $GAMEMATH_DIR/float/s_fabsf.c $GAMEMATH_DIR/float/s_floorf.c \
    $GAMEMATH_DIR/float/s_copysignf.c $GAMEMATH_DIR/float/e_sqrtf.c \
    $GAMEMATH_DIR/common/infinity.c"
PIN_CFLAGS="-ffp-contract=off -fno-builtin -fno-strict-aliasing -I$GAMEMATH_DIR/include -I$GAMEMATH_DIR/common -I$GAMEMATH_DIR/float"

echo "compiling the baseline timing harness with $CXX ..."
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror $PIN_CFLAGS \
    -c -o "$BUILD_DIR/bench_wrap_golden.o" "$SCRIPT_DIR/bench_wrap_golden.cpp"
for src in $PIN_SOURCES; do
    obj="$BUILD_DIR/$(basename "$src").o"
    "$CC" -O2 $PIN_CFLAGS -DBUILD_WITH_INTRINSICS -c -o "$obj" "$src"
done

# The real EAC codec sources, compiled as their own translation units
# (exactly like the game build and the golden generator).
EAC_DIR="$REPO_ROOT/Core/Libraries/Source/Compression/EAC"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off -I"$EAC_DIR" \
    -c -o "$BUILD_DIR/bench_wrap_compress.o" "$SCRIPT_DIR/bench_wrap_compress.cpp"
"$CXX" -O2 -ffp-contract=off -I"$EAC_DIR" -c -o "$BUILD_DIR/refdecode.o" "$EAC_DIR/refdecode.cpp"
"$CXX" -O2 -ffp-contract=off -I"$EAC_DIR" -c -o "$BUILD_DIR/refencode.o" "$EAC_DIR/refencode.cpp"

"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_ini.o" "$SCRIPT_DIR/bench_wrap_ini.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_csf.o" "$SCRIPT_DIR/bench_wrap_csf.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_tga.o" "$SCRIPT_DIR/bench_wrap_tga.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_chunk.o" "$SCRIPT_DIR/bench_wrap_chunk.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_mesh.o" "$SCRIPT_DIR/bench_wrap_mesh.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_htree.o" "$SCRIPT_DIR/bench_wrap_htree.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_hanim.o" "$SCRIPT_DIR/bench_wrap_hanim.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_hcanim.o" "$SCRIPT_DIR/bench_wrap_hcanim.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_hlod.o" "$SCRIPT_DIR/bench_wrap_hlod.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_dds.o" "$SCRIPT_DIR/bench_wrap_dds.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_mapchunk.o" "$SCRIPT_DIR/bench_wrap_mapchunk.cpp"
# -Wno-stringop-overflow -Wno-array-bounds -Wno-restrict: this wrapper
# is the first whose parsers feed a DATA-DEPENDENT length into the
# verbatim transcription's readArrayOfBytes (m_dataSize). GCC 14's IPA
# range analysis cannot track the payload reader's throw-guarded
# `dataSize > 0` contract through the exception edge, so the union of
# inlined memcpy bounds picks up impossible negative lengths and fires
# the three diagnostics. The standalone generator compile (its main()
# path) and the mapchunk wrapper (constant-length reads) stay
# warning-free; the guards are real, the ranges are not.
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -Wno-stringop-overflow -Wno-array-bounds -Wno-restrict \
    -c -o "$BUILD_DIR/bench_wrap_mapheightmap.o" "$SCRIPT_DIR/bench_wrap_mapheightmap.cpp"
# the same three suppressions as the mapheightmap wrapper: the blend
# reader feeds data-dependent lengths into the transcription's
# readArrayOfBytes (m_dataSize) and the record loops are count-driven.
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -Wno-stringop-overflow -Wno-array-bounds -Wno-restrict \
    -c -o "$BUILD_DIR/bench_wrap_mapblendtile.o" "$SCRIPT_DIR/bench_wrap_mapblendtile.cpp"
# the same three suppressions for the lighting wrapper: its parse walk
# inlines the transcription's TOC reads, where GCC 14's IPA range
# analysis picks up impossible negative lengths (see the mapheightmap
# note above — the guards are real, the ranges are not).
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -Wno-stringop-overflow -Wno-array-bounds -Wno-restrict \
    -c -o "$BUILD_DIR/bench_wrap_maplighting.o" "$SCRIPT_DIR/bench_wrap_maplighting.cpp"
# the objects wrapper stays warning-free without the suppressions (its
# fixed-shape reads and per-object parses keep the inlining ranges
# tractable).
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_mapobjects.o" "$SCRIPT_DIR/bench_wrap_mapobjects.cpp"
# the same three suppressions for the world-info wrapper: like the
# lighting wrapper, its parse walk inlines the transcription's TOC
# reads, where GCC 14's IPA range analysis picks up impossible negative
# lengths (see the mapheightmap note above — the guards are real, the
# ranges are not).
# the same three suppressions as the world-info wrapper (its parse walk
# inlines the transcription's TOC reads the same way).
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -Wno-stringop-overflow -Wno-array-bounds -Wno-restrict \
    -c -o "$BUILD_DIR/bench_wrap_mapworldinfo.o" "$SCRIPT_DIR/bench_wrap_mapworldinfo.cpp"
# the same three suppressions as the other map wrappers (the sides
# walk inlines the transcription's TOC reads the same way).
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -Wno-stringop-overflow -Wno-array-bounds -Wno-restrict \
    -c -o "$BUILD_DIR/bench_wrap_mappolygons.o" "$SCRIPT_DIR/bench_wrap_mappolygons.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -Wno-stringop-overflow -Wno-array-bounds -Wno-restrict \
    -c -o "$BUILD_DIR/bench_wrap_mapsides.o" "$SCRIPT_DIR/bench_wrap_mapsides.cpp"
# the same three suppressions for the scripts wrapper (its parse walk
# inlines the transcription's TOC reads and count-driven loops the same
# way).
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -Wno-stringop-overflow -Wno-array-bounds -Wno-restrict \
    -c -o "$BUILD_DIR/bench_wrap_mapscripts.o" "$SCRIPT_DIR/bench_wrap_mapscripts.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_wnd.o" "$SCRIPT_DIR/bench_wrap_wnd.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_font.o" "$SCRIPT_DIR/bench_wrap_font.cpp"
# the kernel wrapper links the pinned GameMath sources like the golden
# generator (the deterministic trig the rect fill drives)
for src in $PIN_SOURCES; do
    obj="$BUILD_DIR/$(basename "$src").o"
    if [ ! -f "$obj" ]; then
        "$CC" -O2 $PIN_CFLAGS -DBUILD_WITH_INTRINSICS -c -o "$obj" "$src"
    fi
done
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_wrap_kernel.o" "$SCRIPT_DIR/bench_wrap_kernel.cpp"
"$CXX" -O2 -std=c++17 -Wall -Wextra -Werror -ffp-contract=off \
    -c -o "$BUILD_DIR/bench_baseline.o" "$SCRIPT_DIR/bench_baseline.cpp"

"$CXX" -o "$BUILD_DIR/bench_baseline" \
    "$BUILD_DIR/bench_baseline.o" \
    "$BUILD_DIR/bench_wrap_golden.o" \
    "$BUILD_DIR/bench_wrap_compress.o" \
    "$BUILD_DIR/bench_wrap_ini.o" \
    "$BUILD_DIR/bench_wrap_csf.o" \
    "$BUILD_DIR/bench_wrap_tga.o" \
    "$BUILD_DIR/bench_wrap_chunk.o" \
    "$BUILD_DIR/bench_wrap_mesh.o" \
    "$BUILD_DIR/bench_wrap_htree.o" \
    "$BUILD_DIR/bench_wrap_hanim.o" \
    "$BUILD_DIR/bench_wrap_hcanim.o" \
    "$BUILD_DIR/bench_wrap_hlod.o" \
    "$BUILD_DIR/bench_wrap_dds.o" \
    "$BUILD_DIR/bench_wrap_mapchunk.o" \
    "$BUILD_DIR/bench_wrap_mapheightmap.o" \
    "$BUILD_DIR/bench_wrap_mapblendtile.o" \
    "$BUILD_DIR/bench_wrap_maplighting.o" \
    "$BUILD_DIR/bench_wrap_mapobjects.o" \
    "$BUILD_DIR/bench_wrap_mapworldinfo.o" \
    "$BUILD_DIR/bench_wrap_mappolygons.o" \
    "$BUILD_DIR/bench_wrap_mapsides.o" \
    "$BUILD_DIR/bench_wrap_mapscripts.o" \
    "$BUILD_DIR/bench_wrap_wnd.o" \
    "$BUILD_DIR/bench_wrap_font.o" \
    "$BUILD_DIR/bench_wrap_kernel.o" \
    "$BUILD_DIR/refdecode.o" \
    "$BUILD_DIR/refencode.o" \
    "$BUILD_DIR"/*.c.o -lm

echo "running the baseline (fixtures: $REPO_ROOT/crates/rzh-ini/tests/fixtures) ..."
"$BUILD_DIR/bench_baseline" "$REPO_ROOT/crates/rzh-ini/tests/fixtures"
echo "done — compare with: cargo bench --workspace"
