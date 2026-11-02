// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-vector generator for the rzh-math::gmath port (ADR-001 option C).
//
// Unlike gen_golden.cpp (a hand transcription of the game's own headers),
// this driver links against the REAL pinned GameMath sources fetched from
// TheSuperHackers/GameMath at the exact commit the C++ oracle builds with
// (pin 59f7ccd494f7e7c916a784ac26ef266f9f09d78d, cmake/gamemath.cmake).
// The golden vectors therefore come from the very code the oracle runs —
// there is no transcription layer on the C side at all.
//
// The pin's float routines are Copyright (C) 1993 by Sun Microsystems, Inc.
// (SunPro), developed at SunPro; permission to use, copy, modify, and
// distribute this software is freely granted, provided that this notice is
// preserved.
//
// Usage (wired up by tools/differential/regenerate.sh):
//   gen_gmath_golden <out.txt>
//
// Output format: one record per line, "name x_bits y_bits result_bits"
// (hex, no prefix). NaNs compare by bit pattern; the Rust test treats a
// mismatch as failure (the msun routines produce deterministic NaN payloads
// where they produce NaN at all).

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "gmath.h"

// ---------------------------------------------------------------- input sets

// Deterministic LCG so the vector set is reproducible across platforms.
static uint32_t lcg_state = 0x12345678u;
static uint32_t lcg(void) {
    lcg_state = lcg_state * 1664525u + 1013904223u;
    return lcg_state;
}

// A hand-picked set of interesting bit patterns (exercising every branch
// of every algorithm: reduction paths, subnormals, boundaries, signs).
static const uint32_t anchors[] = {
    0x00000000, // +0
    0x80000000, // -0
    0x3f800000, // 1
    0xbf800000, // -1
    0x3f000000, // 0.5
    0x3eaaaaab, // ~1/3
    0x3f490fdb, // ~pi/4
    0x3fc90fdb, // ~pi/2
    0x40490fdb, // ~pi
    0x40c90fdb, // ~3pi/2
    0x41100000, // 9
    0x42b17218, // ~88.73 (expf overflow boundary)
    0x42b2d4fc, // cosh/sinh overflow threshold
    0x43490f80, // 2^7*pi/2 boundary (rem_pio2f medium/huge)
    0x42ffffff, // just below the medium/huge boundary
    0x43500000, // just above (huge path)
    0x46000000, // 8192
    0x4b000000, // 2^23
    0x4b800000, // 2^24 (yisint boundary for powf)
    0x4d000000, // 2^27 (powf huge-y boundary)
    0x50800000, // 2^34 (atanf huge boundary)
    0x5f000000, // ~2^63
    0x6a000000, // huge (kernel_rem_pio2f path)
    0x7a000000, // ~2^120
    0x7ee00000, // near maxfloat
    0x7f7fffff, // maxfloat
    0x7f800000, // +inf
    0xff800000, // -inf
    0x7fc00000, // qNaN
    0xffc00000, // -qNaN
    0x00800000, // smallest normal
    0x00000001, // smallest subnormal
    0x007fffff, // largest subnormal
    0x33a22168, // pio2_lo
    0x3eed6338, // atan(0.5) hi
    0x3f79999a, // 0.975 (asinf/acosf branch)
    0x3f2ca140, // 0.67434 (kernel_tanf branch)
    0x3f851592, // 1.5*ln2
    0x3eb17218, // 0.5*ln2
    0x3ee00000, // 0.4375 (atanf reduction boundary)
    0x3f300000, // 0.6875 (atanf reduction boundary)
    0x3f980000, // 1.1875 (atanf reduction boundary)
    0x401c0000, // 2.4375 (atanf reduction boundary)
    0x4016cbe4, // 3*pi/4 (rem_pio2f boundary)
    0x39800000, // 2^-12
    0x32000000, // 2^-27
    0x31000000, // 2^-29
    0x23000000, // 2^-57
    0x1c000000, // ~2^-115
    0x4195b844, // 27*ln2
    0x41b00000, // 22
};

// A dense deterministic set of bit patterns (every 97th pattern from the
// LCG) so the committed vector file stays a few hundred KB.
#define NUM_RANDOM 4096

static uint32_t random_patterns[NUM_RANDOM];

static void init_random(void) {
    for (int i = 0; i < NUM_RANDOM; i++) {
        random_patterns[i] = lcg();
    }
}

// Game-range angles: the simulation feeds radians with magnitudes from
// 0 to a few hundred; make sure the common range is dense.
static uint32_t game_angles[2048];
static void init_game_angles(void) {
    for (int i = 0; i < 2048; i++) {
        // angle = i/8 - 128, covering [-128, 127.875] in 1/8 steps
        float a = (float)(i) * 0.125f - 128.0f;
        memcpy(&game_angles[i], &a, 4);
    }
}

// ---------------------------------------------------------------- emit code

static FILE *out;

static void emit1(const char *name, uint32_t xb, float (*fn)(float)) {
    float x;
    memcpy(&x, &xb, 4);
    float r = fn(x);
    uint32_t rb;
    memcpy(&rb, &r, 4);
    fprintf(out, "%s %08x %08x\n", name, xb, rb);
}

static void emit2(const char *name, uint32_t xb, uint32_t yb,
                  float (*fn)(float, float)) {
    float x, y;
    memcpy(&x, &xb, 4);
    memcpy(&y, &yb, 4);
    float r = fn(x, y);
    uint32_t rb;
    memcpy(&rb, &r, 4);
    fprintf(out, "%s %08x %08x %08x\n", name, xb, yb, rb);
}

// Wrappers keep the emit helpers' function-pointer types uniform.
static float w_sinf(float x) { return gm_sinf(x); }
static float w_cosf(float x) { return gm_cosf(x); }
static float w_tanf(float x) { return gm_tanf(x); }
static float w_asinf(float x) { return gm_asinf(x); }
static float w_acosf(float x) { return gm_acosf(x); }
static float w_atanf(float x) { return gm_atanf(x); }
static float w_expf(float x) { return gm_expf(x); }
static float w_expm1f(float x) { return gm_expm1f(x); }
static float w_logf(float x) { return gm_logf(x); }
static float w_log10f(float x) { return gm_log10f(x); }
static float w_sinhf(float x) { return gm_sinhf(x); }
static float w_coshf(float x) { return gm_coshf(x); }
static float w_tanhf(float x) { return gm_tanhf(x); }
static float w_sqrtf(float x) { return gm_sqrtf(x); }
static float w_scalbnf(float x) { return gm_scalbnf(x, 3); }

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <gmath_out.txt>\n", argv[0]);
        return 1;
    }
    out = fopen(argv[1], "w");
    if (!out) {
        fprintf(stderr, "cannot open %s\n", argv[1]);
        return 1;
    }

    init_random();
    init_game_angles();

    /* Unary functions over anchors + random + game angles. asinf/acosf
       only make sense in [-1, 1]; the anchors already cover their domain
       boundaries and the random sweep exercises NaN propagation through
       them. */
    for (size_t i = 0; i < sizeof(anchors) / sizeof(anchors[0]); i++) {
        uint32_t xb = anchors[i];
        emit1("sinf", xb, w_sinf);
        emit1("cosf", xb, w_cosf);
        emit1("tanf", xb, w_tanf);
        emit1("asinf", xb, w_asinf);
        emit1("acosf", xb, w_acosf);
        emit1("atanf", xb, w_atanf);
        emit1("expf", xb, w_expf);
        emit1("expm1f", xb, w_expm1f);
        emit1("logf", xb, w_logf);
        emit1("log10f", xb, w_log10f);
        emit1("sinhf", xb, w_sinhf);
        emit1("coshf", xb, w_coshf);
        emit1("tanhf", xb, w_tanhf);
        emit1("sqrtf", xb, w_sqrtf);
        emit1("scalbnf", xb, w_scalbnf);
    }

    for (int i = 0; i < NUM_RANDOM; i++) {
        uint32_t xb = random_patterns[i];
        emit1("sinf", xb, w_sinf);
        emit1("cosf", xb, w_cosf);
        emit1("tanf", xb, w_tanf);
        emit1("asinf", xb, w_asinf);
        emit1("acosf", xb, w_acosf);
        emit1("atanf", xb, w_atanf);
        emit1("expf", xb, w_expf);
        emit1("expm1f", xb, w_expm1f);
        emit1("logf", xb, w_logf);
        emit1("log10f", xb, w_log10f);
        emit1("sinhf", xb, w_sinhf);
        emit1("coshf", xb, w_coshf);
        emit1("tanhf", xb, w_tanhf);
        emit1("sqrtf", xb, w_sqrtf);
        emit1("scalbnf", xb, w_scalbnf);
    }

    for (int i = 0; i < 2048; i++) {
        uint32_t xb = game_angles[i];
        emit1("sinf", xb, w_sinf);
        emit1("cosf", xb, w_cosf);
        emit1("tanf", xb, w_tanf);
    }

    /* atan2f(y, x): both operands vary. Grid: anchors x anchors, plus
       (random, random) pairs and (game angle, game angle) pairs. */
    for (size_t i = 0; i < sizeof(anchors) / sizeof(anchors[0]); i++) {
        for (size_t j = 0; j < sizeof(anchors) / sizeof(anchors[0]); j++) {
            emit2("atan2f", anchors[i], anchors[j], gm_atan2f);
        }
    }
    for (int i = 0; i < 1024; i++) {
        emit2("atan2f", random_patterns[i], random_patterns[i + 1024],
              gm_atan2f);
        emit2("atan2f", random_patterns[i + 2048], random_patterns[i + 3072],
              gm_atan2f);
    }

    /* powf(x, y): anchors x anchors (covers 0/1/2/0.5/negative/inf/NaN
       special cases and both branches of the reduction), random pairs,
       and a base sweep against interesting exponents. */
    for (size_t i = 0; i < sizeof(anchors) / sizeof(anchors[0]); i++) {
        for (size_t j = 0; j < sizeof(anchors) / sizeof(anchors[0]); j++) {
            emit2("powf", anchors[i], anchors[j], gm_powf);
        }
    }
    for (int i = 0; i < 1024; i++) {
        emit2("powf", random_patterns[i], random_patterns[i + 1024], gm_powf);
        emit2("powf", random_patterns[i + 2048], random_patterns[i + 3072],
              gm_powf);
    }

    /* lrintf: in-range floats only. The C pin compiled with intrinsics
       returns the platform "integer indefinite" value for NaN/overflow
       (which C leaves undefined on the generic path), so the golden file
       pins the defined domain; the Rust unit tests pin the sentinel
       behaviour separately (see gmath::rint). */
    {
        static const uint32_t lrng[] = {
            0x00000000, 0x80000000, 0x3f800000, 0xbf800000, 0x40000000,
            0xc0000000, 0x3f000000, 0xbf000000, 0x3eaaaaab, 0x40200000,
            0xc0200000, 0x4b7fffff, 0xcb7fffff, 0x4b000000, 0xcb000000,
            0x4effffff, 0xceffffff, 0x4f000000, 0xcf000000, 0x5effffff,
            0x41200000, 0xc1200000, 0x3fc00000, 0xbfc00000, 0x40b33334,
            0xc0b33334, 0x477fff00, 0xc77fff00,
        };
        for (size_t i = 0; i < sizeof(lrng) / sizeof(lrng[0]); i++) {
            float x;
            memcpy(&x, &lrng[i], 4);
            long r = gm_lrintf(x);
            fprintf(out, "lrintf %08x %ld\n", lrng[i], r);
        }
        for (int i = 0; i < 512; i++) {
            float x = (float)((int)random_patterns[i] % 20000) - 10000.0f;
            uint32_t xb;
            memcpy(&xb, &x, 4);
            long r = gm_lrintf(x);
            fprintf(out, "lrintf %08x %ld\n", xb, r);
        }
    }

    fclose(out);
    return 0;
}
