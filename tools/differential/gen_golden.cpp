// SPDX-License-Identifier: GPL-3.0-or-later
//
// Differential reference harness for the rzh-util Rust port.
//
// This file transcribes, verbatim, the Zero Hour CRC and RNG algorithms from
// the baseline commit 571a7c5 so that golden vectors can be generated from
// the C++ side and compared byte-for-byte with the Rust implementation
// (parity level P0, see docs/RUST_MIGRATION_PLAN.md §3.2).
//
// Sources:
//   Core/GameEngine/Include/Common/crc.h            (inline non-debug variant)
//   Core/GameEngine/Source/Common/crc.cpp           (debug variant, same algorithm)
//   Core/GameEngine/Source/Common/RandomValue.cpp   (non-retail branches, ADR-000)
//   Core/Libraries/Source/Lib/Include/Precompiled/BaseTypes.h (type definitions)
//
// Copyright 2025 Electronic Arts Inc.
// (c) 2001-2003 Electronic Arts Inc.
// RandomValue.cpp — Pseudo-random number generators. Author: Michael S. Booth, January 1998.
// CRC.h — A class encapsulating CRC calculation. Author: Matthew D. Campbell, October 2001.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.

#include <climits>
#include <cstdint>
#include <cstdio>
#include <cstring>

// Type definitions from BaseTypes.h (the game builds with these exact widths
// on every supported platform).
typedef float Real;
typedef int32_t Int;
typedef uint32_t UnsignedInt;
typedef unsigned char UnsignedByte;

// ---------------------------------------------------------------------------
// CRC, transcribed from crc.h (the optimized inline version) and crc.cpp
// (the debug version; both implement the same shift-add algorithm).
// ---------------------------------------------------------------------------

struct CRC
{
        UnsignedInt crc;

        CRC() { crc = 0; }

        void computeCRC( const void *buf, Int len )
        {
                if (!buf || len < 1)
                        return;

                for (UnsignedByte *uintPtr = (UnsignedByte *)buf; len > 0; len--, uintPtr++)
                {
                        int hibit;
                        if (crc & 0x80000000)
                        {
                                hibit = 1;
                        }
                        else
                        {
                                hibit = 0;
                        }

                        crc <<= 1;
                        crc += *uintPtr;
                        crc += hibit;
                }
        }

        void clear() { crc = 0; }

        UnsignedInt get() const { return crc; }
};

// ---------------------------------------------------------------------------
// RNG, transcribed from RandomValue.cpp (non-retail branches).
// ---------------------------------------------------------------------------

static const Real theMultFactor = 1.0f / static_cast<float>(UINT_MAX);

static void seedRandom( UnsignedInt SEED, UnsignedInt (&seed)[6] )
{
        UnsignedInt ax;

        ax = SEED;
        ax += 0xf22d0e56;
        seed[0] = ax;
        ax += 0x883126e9 - 0xf22d0e56;
        seed[1] = ax;
        ax += 0xc624dd2f - 0x883126e9;
        seed[2] = ax;
        ax += 0x0702c49c - 0xc624dd2f;
        seed[3] = ax;
        ax += 0x9e353f7d - 0x0702c49c;
        seed[4] = ax;
        ax += 0x6fdf3b64 - 0x9e353f7d;
        seed[5] = ax;
}

// Add with carry. SUM is replaced with A + B + C, C is replaced with 1 if
// there was a carry, 0 if there wasn't.
#define ADC(SUM, A, B, C) SUM = (A) + (B) + (C); C = ((SUM < (A)) || (SUM < (B)))

static UnsignedInt randomValue( UnsignedInt (&seed)[6] )
{
        UnsignedInt ax;
        UnsignedInt c = 0;

        ADC(ax, seed[5], seed[4], c);
        seed[4] = ax;

        ADC(ax, ax, seed[3], c);
        seed[3] = ax;

        ADC(ax, ax, seed[2], c);
        seed[2] = ax;

        ADC(ax, ax, seed[1], c);
        seed[1] = ax;

        ADC(ax, ax, seed[0], c);
        seed[0] = ax;

        /* Increment seed array, bubbling up the carries. */
        if (!++seed[5])
        {
                if (!++seed[4])
                {
                        if (!++seed[3])
                        {
                                if (!++seed[2])
                                {
                                        if (!++seed[1])
                                        {
                                                ++seed[0];
                                                ++ax;
                                        }
                                }
                        }
                }
        }

        return ax;
}

// GetGameLogicRandomValue, non-retail branch (identical shape to the audio
// and client variants; the logic stream is used here).
static Int GetGameLogicRandomValue( UnsignedInt (&seed)[6], int lo, int hi )
{
        if (lo >= hi)
                return hi;

        const UnsignedInt delta = hi - lo + 1;
        const Int rval = ((Int)(randomValue(seed) % delta)) + lo;

        return rval;
}

// GetGameLogicRandomValueReal, non-retail branch.
static Real GetGameLogicRandomValueReal( UnsignedInt (&seed)[6], Real lo, Real hi )
{
        if (lo >= hi)
                return hi;

        const Real delta = hi - lo;
        const Real rval = ((Real)(randomValue(seed)) * theMultFactor) * delta + lo;

        return rval;
}

// GetGameLogicRandomValueUnchanged, non-retail branch.
static Int GetGameLogicRandomValueUnchanged( const UnsignedInt (&seed)[6], int lo, int hi )
{
        if (lo >= hi)
                return hi;

        UnsignedInt local_seed[6];
        memcpy(&local_seed[0], &seed[0], sizeof(local_seed));

        const UnsignedInt delta = hi - lo + 1;
        const Int rval = ((Int)(randomValue(local_seed) % delta)) + lo;

        return rval;
}

// GetGameLogicRandomValueRealUnchanged, non-retail branch.
static Real GetGameLogicRandomValueRealUnchanged( const UnsignedInt (&seed)[6], Real lo, Real hi )
{
        if (lo >= hi)
                return hi;

        UnsignedInt local_seed[6];
        memcpy(&local_seed[0], &seed[0], sizeof(local_seed));

        const Real delta = hi - lo;
        const Real rval = ((Real)(randomValue(local_seed)) * theMultFactor) * delta + lo;

        return rval;
}

// ---------------------------------------------------------------------------
// Golden vector generation. Deterministic: a fixed LCG produces the test
// byte sequences, so the same vectors come out on every platform.
// ---------------------------------------------------------------------------

static UnsignedInt floatBits( Real value )
{
        UnsignedInt bits;
        memcpy(&bits, &value, sizeof(bits));
        return bits;
}

// A tiny LCG so the generated byte sequences are deterministic and identical
// on every platform that runs this harness.
static UnsignedInt lcgState = 0;
static void resetLcg( UnsignedInt seed )
{
        lcgState = seed;
}
static UnsignedByte lcgByte()
{
        lcgState = lcgState * 1664525u + 1013904223u;
        return (UnsignedByte)(lcgState >> 24);
}

static void writeHex( FILE *out, const UnsignedByte *bytes, size_t len )
{
        if (len == 0)
        {
                fputc('-', out);
                return;
        }
        for (size_t i = 0; i < len; i++)
                fprintf(out, "%02x", (unsigned)bytes[i]);
}

static void generateCrcGolden( FILE *out )
{
        fprintf(out, "# CRC golden vectors (parity level P0).\n");
        fprintf(out, "# Generated by tools/differential/gen_golden.cpp, transcribed from\n");
        fprintf(out, "# Core/GameEngine/Include/Common/crc.h and Source/Common/crc.cpp @ 571a7c5.\n");
        fprintf(out, "# Format: crc <len> <input_hex_or_dash> <crc_hex>\n");
        fprintf(out, "# Regenerate with tools/differential/regenerate.sh; never edit by hand.\n");

        UnsignedByte buf[65536];

        // The empty input (len < 1 early-out; crc stays 0).
        {
                CRC c;
                fprintf(out, "crc 0 - %08x\n", c.get());
        }

        // Every single byte value.
        for (unsigned b = 0; b < 256; b++)
        {
                UnsignedByte one = (UnsignedByte)b;
                CRC c;
                c.computeCRC(&one, 1);
                fprintf(out, "crc 1 %02x %08x\n", b, c.get());
        }

        // Classic ASCII fixtures (hex-encoded like every other input).
        {
                const char *s = "123456789";
                size_t len = strlen(s);
                CRC c;
                c.computeCRC(s, (Int)len);
                fprintf(out, "crc %zu ", len);
                writeHex(out, (const UnsignedByte *)s, len);
                fprintf(out, " %08x\n", c.get());
        }
        {
                const char *s = "The quick brown fox jumps over the lazy dog";
                size_t len = strlen(s);
                CRC c;
                c.computeCRC(s, (Int)len);
                fprintf(out, "crc %zu ", len);
                writeHex(out, (const UnsignedByte *)s, len);
                fprintf(out, " %08x\n", c.get());
        }
        {
                const char *s = "RustZeroHour";
                size_t len = strlen(s);
                CRC c;
                c.computeCRC(s, (Int)len);
                fprintf(out, "crc %zu ", len);
                writeHex(out, (const UnsignedByte *)s, len);
                fprintf(out, " %08x\n", c.get());
        }

        // Deterministic pseudo-random sequences of every length 2..64.
        for (size_t len = 2; len <= 64; len++)
        {
                resetLcg(0x1234u + (UnsignedInt)len);
                for (size_t i = 0; i < len; i++)
                        buf[i] = lcgByte();
                CRC c;
                c.computeCRC(buf, (Int)len);
                fprintf(out, "crc %zu ", len);
                writeHex(out, buf, len);
                fprintf(out, " %08x\n", c.get());
        }

        // Longer sequences (also exercises the top-bit carry path deeply).
        {
                const size_t lengths[] = { 100, 1000, 4096, 65536 };
                for (size_t li = 0; li < sizeof(lengths) / sizeof(lengths[0]); li++)
                {
                        const size_t len = lengths[li];
                        resetLcg(0xBEEFu + (UnsignedInt)len);
                        for (size_t i = 0; i < len; i++)
                                buf[i] = lcgByte();
                        CRC c;
                        c.computeCRC(buf, (Int)len);
                        fprintf(out, "crc %zu ", len);
                        writeHex(out, buf, len);
                        fprintf(out, " %08x\n", c.get());
                }
        }
}

struct IntRange { int lo, hi; };
struct RealRange { Real lo, hi; };

static void generateRngGolden( FILE *out )
{
        fprintf(out, "# RNG golden vectors (parity level P0).\n");
        fprintf(out, "# Generated by tools/differential/gen_golden.cpp, transcribed from\n");
        fprintf(out, "# Core/GameEngine/Source/Common/RandomValue.cpp @ 571a7c5 (non-retail).\n");
        fprintf(out, "# Per record, one stream is driven in order:\n");
        fprintf(out, "#   raw <value_hex>                        (advances)\n");
        fprintf(out, "#   int <lo> <hi> <value>                  (advances)\n");
        fprintf(out, "#   real <lo_bits_hex> <hi_bits_hex> <value_bits_hex>  (advances)\n");
        fprintf(out, "#   unchanged_int <lo> <hi> <value>        (does NOT advance)\n");
        fprintf(out, "#   unchanged_real <lo_bits> <hi_bits> <value_bits>   (does NOT advance)\n");
        fprintf(out, "# Regenerate with tools/differential/regenerate.sh; never edit by hand.\n");

        static const UnsignedInt seeds[] = {
                0x00000000u, 0x00000001u, 0x0000002au, 0x00000063u,
                0x12345678u, 0xdeadbeefu, 0xffffffffu
        };

        static const IntRange intRanges[] = {
                { 0, 100 }, { 1, 10 }, { -50, 50 }, { 0, 1 },
                { 0, 1000000 }, { -100, 100 }, { 0, 2147483647 }
                // Note: the full i32 range (INT_MIN..INT_MAX) makes
                // hi - lo + 1 overflow to 0 and the modulo undefined in C++
                // (SIGFPE on x86); the Rust port panics there on purpose.
        };

        static const RealRange realRanges[] = {
                { 0.0f, 1.0f }, { 0.5f, 2.5f }, { -1.0f, 1.0f }, { 0.0f, 100.0f },
                { -3.25f, 42.75f }, { 0.0f, 1e30f }, { -1e20f, 1e20f }
        };

        for (size_t si = 0; si < sizeof(seeds) / sizeof(seeds[0]); si++)
        {
                UnsignedInt seed[6];
                seedRandom(seeds[si], seed);

                fprintf(out, "seq %08x\n", seeds[si]);

                for (int i = 0; i < 20; i++)
                        fprintf(out, "raw %08x\n", randomValue(seed));

                for (size_t r = 0; r < sizeof(intRanges) / sizeof(intRanges[0]); r++)
                        fprintf(out, "int %d %d %d\n", intRanges[r].lo, intRanges[r].hi,
                                GetGameLogicRandomValue(seed, intRanges[r].lo, intRanges[r].hi));

                for (size_t r = 0; r < sizeof(realRanges) / sizeof(realRanges[0]); r++)
                        fprintf(out, "real %08x %08x %08x\n",
                                floatBits(realRanges[r].lo),
                                floatBits(realRanges[r].hi),
                                floatBits(GetGameLogicRandomValueReal(seed, realRanges[r].lo, realRanges[r].hi)));

                // Unchanged variants: two identical consecutive calls, then
                // raw values prove the stream was not advanced.
                fprintf(out, "unchanged_int %d %d %d\n", 0, 100,
                        GetGameLogicRandomValueUnchanged(seed, 0, 100));
                fprintf(out, "unchanged_int %d %d %d\n", 0, 100,
                        GetGameLogicRandomValueUnchanged(seed, 0, 100));
                fprintf(out, "unchanged_real %08x %08x %08x\n",
                        floatBits(0.0f), floatBits(1.0f),
                        floatBits(GetGameLogicRandomValueRealUnchanged(seed, 0.0f, 1.0f)));
                fprintf(out, "unchanged_real %08x %08x %08x\n",
                        floatBits(0.0f), floatBits(1.0f),
                        floatBits(GetGameLogicRandomValueRealUnchanged(seed, 0.0f, 1.0f)));

                for (int i = 0; i < 5; i++)
                        fprintf(out, "raw %08x\n", randomValue(seed));

                fprintf(out, "end\n");
        }
}

int main( int argc, char **argv )
{
        if (argc != 3)
        {
                fprintf(stderr, "usage: %s <crc_out.txt> <rng_out.txt>\n", argv[0]);
                return 1;
        }

        FILE *crcOut = fopen(argv[1], "w");
        if (!crcOut)
        {
                fprintf(stderr, "cannot open %s\n", argv[1]);
                return 1;
        }
        FILE *rngOut = fopen(argv[2], "w");
        if (!rngOut)
        {
                fclose(crcOut);
                fprintf(stderr, "cannot open %s\n", argv[2]);
                return 1;
        }

        generateCrcGolden(crcOut);
        generateRngGolden(rngOut);

        fclose(crcOut);
        fclose(rngOut);
        return 0;
}
