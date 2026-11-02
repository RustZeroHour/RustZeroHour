// SPDX-License-Identifier: GPL-3.0-or-later
//
// Differential reference harness for the rzh-util and rzh-math Rust ports.
//
// This file transcribes, verbatim, the Zero Hour algorithms from the baseline
// commit 571a7c5 so that golden vectors can be generated from the C++ side
// and compared byte-for-byte with the Rust implementations (parity level P0,
// see docs/RUST_MIGRATION_PLAN.md §3.2).
//
// Sources:
//   Core/GameEngine/Include/Common/crc.h            (inline non-debug variant)
//   Core/GameEngine/Source/Common/crc.cpp           (debug variant, same algorithm)
//   Core/GameEngine/Source/Common/RandomValue.cpp   (non-retail branches, ADR-000)
//   Core/Libraries/Source/Lib/Include/Lib/BaseTypes.h (type definitions)
//   GeneralsMD/Code/GameEngine/Include/Common/NameKeyGenerator.h
//   GeneralsMD/Code/GameEngine/Source/Common/NameKeyGenerator.cpp
//   Core/Libraries/Source/WWVegas/WWMath/wwmath.h   (deterministic branches)
//   Core/Libraries/Source/WWVegas/WWMath/vector2.h / vector2i.h
//   Core/Libraries/Source/WWVegas/WWMath/vector3.h / Vector3i.h
//   Core/Libraries/Source/WWVegas/WWMath/vector4.h
//
// Numerics note (ADR-001): the game builds with USE_DETERMINISTIC_MATH=1,
// whose float transcendental entry points route to the GameMath (FreeBSD
// msun) functions. This harness covers only operations that are IEEE-754
// exact on every conforming platform (add/sub/mul/div/sqrt/fabs/floor/ceil
// and integer bit work), where GameMath, the system libm and Rust's f32
// methods produce bit-identical results (verified empirically against the
// pinned GameMath revision). The trigonometric entry points are deferred
// until their dedicated port; they are intentionally absent here.
//
// The transcription preserves several quirks of the original headers
// (documented inline); they are behaviour, and the Rust port keeps them.
//
// Copyright 2025 Electronic Arts Inc.
// (c) 2001-2003 Electronic Arts Inc.
// RandomValue.cpp — Pseudo-random number generators. Author: Michael S. Booth, January 1998.
// CRC.h — A class encapsulating CRC calculation. Author: Matthew D. Campbell, October 2001.
// NameKeyGenerator.{h,cpp} — Name key system. Authors: Michael Booth, Colin Day, May 2001.
// WWMath / vector*.h — Westwood math library. Author: Greg Hjelstrom et al.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.

#include <cctype>
#include <climits>
#include <math.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <utility>
#include <vector>

// The pinned GameMath (FreeBSD msun) library, exactly the revision the C++
// oracle links (cmake/gamemath.cmake, pin 59f7ccd4). Only the entry points
// the WWMath transcription needs are declared here (gmath.h itself clashes
// with the C++ standard headers' BSD macros); they link against the pin's
// sources, matching the game's USE_DETERMINISTIC_MATH = 1 build.
extern "C" {
float gm_sinf(float);
float gm_cosf(float);
float gm_asinf(float);
float gm_acosf(float);
float gm_atan2f(float, float);
}

static inline float WWSinf(float x) { return gm_sinf(x); }          // WWMath::Sinf / Sinf_Legacy
static inline float WWCosf(float x) { return gm_cosf(x); }          // WWMath::Cosf / Cosf_Legacy
static inline float WWAsinf(float x) { return gm_asinf(x); }        // WWMath::Asinf
static inline float WWAcosf(float x) { return gm_acosf(x); }        // WWMath::Acosf
static inline float WWAtan2f(float y, float x) { return gm_atan2f(y, x); } // WWMath::Atan2f

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
// NameKeyGenerator, transcribed from NameKeyGenerator.{h,cpp}. The memory
// pool becomes a plain vector; the bucket chain discipline, the hash
// functions and the id assignment are verbatim.
// ---------------------------------------------------------------------------

static const UnsignedInt NAMEKEY_SOCKET_COUNT = 45007;
static const UnsignedInt NAMEKEY_INVALID = 0;

static UnsignedInt calcHashForString(const char* p)
{
        UnsignedInt result = 0;
        UnsignedByte *pp = (UnsignedByte*)p;
        while (*pp)
                result = (result << 5) + result + *pp++;
        return result;
}

static UnsignedInt calcHashForLowercaseString(const char* p)
{
        UnsignedInt result = 0;
        UnsignedByte *pp = (UnsignedByte*)p;
        while (*pp)
                result = (result << 5) + result + tolower(*pp++);
        return result;
}

struct NameKeyBucket
{
        NameKeyBucket *nextInSocket;
        UnsignedInt m_key;
        std::string nameString;
};

struct NameKeyGenerator
{
        std::vector<NameKeyBucket*> sockets;
        std::vector<NameKeyBucket*> allBuckets; // replaces the memory pool; NOT in the original
        UnsignedInt m_nextID;

        NameKeyGenerator()
                : sockets(NAMEKEY_SOCKET_COUNT, nullptr), m_nextID(1)
        {
        }

        ~NameKeyGenerator() { freeSockets(); }

        void freeSockets()
        {
                for (size_t i = 0; i < sockets.size(); ++i)
                {
                        for (NameKeyBucket *b = sockets[i]; b; )
                        {
                                NameKeyBucket *next = b->nextInSocket;
                                delete b;
                                b = next;
                        }
                        sockets[i] = nullptr;
                }
                allBuckets.clear();
        }

        void reset()
        {
                freeSockets();
                m_nextID = 1;
        }

        UnsignedInt nameToKey(const char* name)
        {
                const UnsignedInt hash = calcHashForString(name) % NAMEKEY_SOCKET_COUNT;

                // do we have it already?
                for (NameKeyBucket *b = sockets[hash]; b; b = b->nextInSocket)
                {
                        if (strcmp(name, b->nameString.c_str()) == 0)
                                return b->m_key;
                }

                // nope, guess not. let's allocate it.
                return createNameKey(hash, name);
        }

        UnsignedInt nameToLowercaseKey(const char* name)
        {
                const UnsignedInt hash = calcHashForLowercaseString(name) % NAMEKEY_SOCKET_COUNT;

                // do we have it already?
                for (NameKeyBucket *b = sockets[hash]; b; b = b->nextInSocket)
                {
                        if (_stricmp_portable(name, b->nameString.c_str()) == 0)
                                return b->m_key;
                }

                // nope, guess not. let's allocate it.
                // TheSuperHackers @fix CryoTheRenegade 08/08/2026 Store the canonical lowercase name
                // so a regular lowercase lookup reuses this key.
                std::string lowercaseName(name);
                for (size_t i = 0; i < lowercaseName.size(); ++i)
                        lowercaseName[i] = (char)tolower((unsigned char)lowercaseName[i]);
                return createNameKey(hash, lowercaseName.c_str());
        }

        // keyToName is not transcribed: the original walks all sockets
        // linearly, which is behaviourally "return the unique stored name
        // for this key" and the Rust port answers it by index.

        UnsignedInt createNameKey(UnsignedInt hash, const char* name)
        {
                NameKeyBucket *b = new NameKeyBucket;
                b->m_key = m_nextID++;
                b->nameString = name;
                b->nextInSocket = sockets[hash];
                sockets[hash] = b;
                allBuckets.push_back(b);
                return b->m_key;
        }

private:
        // The original uses _stricmp (MSVC) / strcasecmp elsewhere; both are
        // ASCII-only case-insensitive compares, identical in the "C" locale.
        static int _stricmp_portable(const char* a, const char* b)
        {
                while (*a && *b)
                {
                        int ca = tolower((unsigned char)*a);
                        int cb = tolower((unsigned char)*b);
                        if (ca != cb)
                                return ca - cb;
                        ++a; ++b;
                }
                return tolower((unsigned char)*a) - tolower((unsigned char)*b);
        }
};

// ---------------------------------------------------------------------------
// WWMath scalar helpers, transcribed from wwmath.h, deterministic branches
// (USE_DETERMINISTIC_MATH = 1), limited to the IEEE-754 exact subset.
// ---------------------------------------------------------------------------

#define WWMATH_EPSILON      0.0001f
#define WWMATH_HALF_PI      1.570796327f
#define WWMATH_PI           3.141592654f
#define WWMATH_TWO_PI       6.283185308f
#define WWMATH_FLOAT_MAX    3.40282346638528860e+38f /* FLT_MAX */
#define WWMATH_FLOAT_MIN    (FLT_MIN)
#define WWMATH_SQRT2        1.414213562f
#define WWMATH_SQRT3        1.732050808f
#define WWMATH_OOSQRT2      0.707106781f
#define WWMATH_OOSQRT3      0.577350269f

// gm_sqrtf (FreeBSD msun software sqrt) is correctly rounded, as is the
// system sqrtf and Rust's f32::sqrt; all three agree bit-for-bit (ADR-001).
static inline float WWSqrtf(float x) { return sqrtf(x); }

// Inv_Sqrt_Legacy, deterministic branch.
static inline float WWInvSqrtLegacy(float a) { return 1.0f / WWSqrtf(a); }

// Fabsf_Legacy, deterministic branch (bit-clear, identical to fabsf).
static inline float WWFabsf(float x) { return fabsf(x); }

static inline float WWFloorf(float x) { return floorf(x); }
static inline float WWCeilf(float x) { return ceilf(x); }
static inline float WWRoundf(float x) { return WWFloorf(x + 0.5f); }

static inline float WWClamp(float val, float min, float max)
{
        if (val < min) return min;
        if (val > max) return max;
        return val;
}

static inline int WWClampInt(int val, int min_val, int max_val)
{
        if (val < min_val) return min_val;
        if (val > max_val) return max_val;
        return val;
}

static inline float WWWrap(float val, float min, float max)
{
        // Implemented as an if rather than a while, to long loops
        if ( val >= max )  val -= (max-min);
        if ( val < min )   val += (max-min);

        if ( val < min ) {
                val = min;
        }
        if ( val > max ) {
                val = max;
        }
        return val;
}

static inline float WWMin(float a, float b) { if (a<b) return a; return b; }
static inline float WWMax(float a, float b) { if (a>b) return a; return b; }

static inline float WWLerp(float a, float b, float t) { return (a + (b - a)*t); }
static inline float WWInverseLerp(float a, float b, float v) { return (v - a) / (b - a); }

static inline float WWNormalizeAngle(float angle)
{
        return angle - (WWMATH_TWO_PI * WWFloorf((angle + WWMATH_PI) / WWMATH_TWO_PI));
}

// Div_Safe, deterministic branch; `max` is the CRT macro
// ((a) > (b)) ? (a) : (b) in the game's translation units.
static inline float WWDivSafe(float dividend, float divisor, float epsilon)
{
        return dividend / ((divisor > epsilon) ? divisor : epsilon);
}

static inline bool WWIsValidFloat(float x)
{
        UnsignedInt bits;
        memcpy(&bits, &x, sizeof(bits));
        UnsignedInt exponent = (bits & 0x7F800000) >> 23;
        return exponent != 0xFF;
}

static inline int WWFloatToIntChop(float f)
{
        // NOTE: the shifts below are only defined for |f| < 2^31-ish; the
        // golden vectors stay inside that domain (the game does too).
        int a; memcpy(&a, &f, sizeof(a));                          // std::bit_cast<int>(f)
        int sign        = (a>>31);                                          // sign = 0xFFFFFFFF if original value is negative, 0 if positive
        int mantissa    = (a&((1<<23)-1))|(1<<23);                          // extract mantissa and add the hidden bit
        int exponent    = ((a&0x7fffffff)>>23)-127;                         // extract the exponent
        int r           = ((unsigned int)(mantissa)<<8)>>(31-exponent);     // ((1<<exponent)*mantissa)>>24 -- (we know that mantissa > (1<<24))
        return ((r ^ (sign)) - sign ) &~ (exponent>>31);                    // add original sign. If exponent was negative, make return value 0.
}

static inline int WWFloatToIntFloor(float f)
{
        int a; memcpy(&a, &f, sizeof(a));                          // std::bit_cast<int>(f)
        int sign        = (a>>31);                                          // sign = 0xFFFFFFFF if original value is negative, 0 if positive
        a &= 0x7fffffff;                                                    // we don't need the sign any more

        int exponent    = (a>>23)-127;                                      // extract the exponent
        int expsign     = ~(exponent>>31);                                  // 0xFFFFFFFF if exponent is positive, 0 otherwise
        int imask       = ( (1<<(31-(exponent))))-1;                        // mask for true integer values
        int mantissa    = (a&((1<<23)-1));                                  // extract mantissa (without the hidden bit)
        int r           = ((unsigned int)(mantissa|(1<<23))<<8)>>(31-exponent); // ((1<<exponent)*(mantissa|hidden bit))>>24 -- (we know that mantissa > (1<<24))

        r = ((r & expsign) ^ (sign)) + (((!((mantissa<<8)&imask))&(expsign^((a-1)>>31)))&sign); // if (fabs(value)<1.0) value = 0; copy sign; if (value < 0 && value==(int)(value)) value++;
        return r;
}

// Domain assumption of the original: f in [0, 1]; out-of-range conversions
// are undefined in C++ (and saturate in Rust), so the vectors stay in range.
static inline UnsignedByte WWUnitFloatToByte(float f) { return (UnsignedByte)(f*255.0f); }
static inline float WWByteToUnitFloat(UnsignedByte byte) { return ((float)byte) / 255.0f; }

static inline bool WWFastIsFloatPositive(const float& val)
{
        int bits;
        memcpy(&bits, &val, sizeof(bits));
        return !(bits & 0x80000000);
}

static inline bool WWIsPowerOf2(const unsigned int val)
{
        return !((val)&(val-1));
}

// Free helper from vector2.h. NOTE the quirk: the Fabsf_Legacy results are
// discarded, so negative differences stay negative. Preserved verbatim.
inline float Quick_Distance(float x1, float y1, float x2, float y2)
{
        // From Graphic Gems I.
        float x_diff = x1 - x2;
        float y_diff = y1 - y2;

        WWFabsf(x_diff);
        WWFabsf(y_diff);

        if (x_diff > y_diff)
        {
                return ((y_diff / 2) + x_diff);
        }
        else
        {
                return ((x_diff / 2) + y_diff);
        }
}

// ---------------------------------------------------------------------------
// Vector2, transcribed from vector2.h. Free functions and methods that the
// game build compiles (ALLOW_TEMPORARIES is not defined; Vector2's free
// Normalize is not gated by it).
// ---------------------------------------------------------------------------

struct Vector2
{
        float X, Y;

        Vector2() {}
        Vector2(float x, float y) { X = x; Y = y; }

        float Length() const { return WWSqrtf(Length2()); }
        float Length2() const { return (X*X + Y*Y); }

        void Normalize()
        {
                float len2 = Length2();
                if (len2 != 0.0f) {
                        float oolen = WWInvSqrtLegacy(len2);
                        X *= oolen;
                        Y *= oolen;
                }
        }

        void Update_Min (const Vector2 & a) { if (a.X < X) X = a.X; if (a.Y < Y) Y = a.Y; }
        void Update_Max (const Vector2 & a) { if (a.X > X) X = a.X; if (a.Y > Y) Y = a.Y; }

        void Scale (float a, float b) { X *= a; Y *= b; }
        void Scale (const Vector2 & a) { X *= a.X; Y *= a.Y; }

        static float Dot_Product(const Vector2 &a, const Vector2 &b) { return a*b; }

        // dot product between a and perpendicular vector to b
        static float Perp_Dot_Product(const Vector2 &a, const Vector2 &b)
        {
                return a.X * -b.Y + a.Y * b.X;
        }

        void Rotate(float s, float c)
        {
                float new_x = X * c + Y * -s;
                float new_y = X * s + Y * c;
                X = new_x;
                Y = new_y;
        }

        static void Lerp(const Vector2 & a, const Vector2 & b, float t, Vector2 * set_result)
        {
                set_result->X = (a.X + (b.X - a.X)*t);
                set_result->Y = (a.Y + (b.Y - a.Y)*t);
        }

        static float Distance(const Vector2 &a, const Vector2 &b)
        {
                Vector2 temp;
                temp = a - b;
                return (temp.Length());
        }

        static float Quick_Distance(const Vector2 &a, const Vector2 &b)
        {
                return ::Quick_Distance(a.X, a.Y, b.X, b.Y);
        }

        friend Vector2 operator * (const Vector2 &a, float k)
        {
                float a0k(a[0] * k);
                float a1k(a[1] * k);
                return Vector2(a0k, a1k);
        }
        friend Vector2 operator * (float k, const Vector2 &a)
        {
                float a0k(a[0] * k);
                float a1k(a[1] * k);
                return Vector2(a0k, a1k);
        }
        friend Vector2 operator / (const Vector2 &a, float k)
        {
                float ook = 1.0f/k;
                float a0ook(a[0] * ook);
                float a1ook(a[1] * ook);
                return Vector2(a0ook, a1ook);
        }
        friend Vector2 operator + (const Vector2 &a, const Vector2 &b)
        {
                return Vector2(a.X + b.X, a.Y + b.Y);
        }
        friend Vector2 operator - (const Vector2 &a, const Vector2 &b)
        {
                return Vector2(a.X - b.X, a.Y - b.Y);
        }
        friend float operator * (const Vector2 &a, const Vector2 &b)
        {
                return a.X*b.X + a.Y*b.Y;
        }
        float& operator [](int i) { return (&X)[i]; }
        const float& operator [](int i) const { return (&X)[i]; }
};

inline Vector2 operator-(const Vector2& v) { return Vector2(-v.X, -v.Y); }

// Free Normalize (ungated in the original): NOTE the quirk — it divides by
// the *inverse* square root instead of multiplying, which is not
// bit-equivalent to vec * sqrt(len2). Preserved verbatim.
inline Vector2 Normalize(const Vector2 & vec)
{
        float len2 = vec.Length2();
        if (len2 != 0.0f) {
                float oolen = WWInvSqrtLegacy(len2);
                return vec / oolen;
        }
        return Vector2(0.0f, 0.0f);
}

inline bool Equal_Within_Epsilon(const Vector2 &a, const Vector2 &b, float epsilon)
{
        return ( (WWFabsf(a.X - b.X) < epsilon) && (WWFabsf(a.Y - b.Y) < epsilon) );
}

// ---------------------------------------------------------------------------
// Vector3, transcribed from vector3.h. The ALLOW_TEMPORARIES-gated bodies
// (free Normalize, value-returning Lerp and Cross_Product) are NOT part of
// the game build and are not transcribed.
// ---------------------------------------------------------------------------

struct Vector3
{
        float X, Y, Z;

        Vector3() {}
        Vector3(float x, float y, float z) { X = x; Y = y; Z = z; }

        float Length() const { return WWSqrtf(Length2()); }
        float Length2() const { return X*X + Y*Y + Z*Z; }

        void Normalize()
        {
                float len2 = Length2();
                if (len2 != 0.0f)
                {
                        float oolen = WWInvSqrtLegacy(len2);
                        X *= oolen;
                        Y *= oolen;
                        Z *= oolen;
                }
        }

        float Quick_Length() const
        {
                // this method of approximating the length comes from Graphics Gems 1 and
                // supposedly gives an error of +/- 8%
                float max = WWFabsf(X);
                float mid = WWFabsf(Y);
                float min = WWFabsf(Z);
                float tmp;

                if (max < mid) { tmp = max; max = mid; mid = tmp; }
                if (max < min) { tmp = max; max = min; min = tmp; }
                // NOTE: original quirk — this line assigns `min = mid` (not
                // `min = tmp`), losing the old mid. Preserved verbatim.
                if (mid < min) { tmp = mid; mid = min; min = mid; }

                return max + (11.0f / 32.0f)*mid + (1.0f / 4.0f)*min;
        }

        void Update_Min(const Vector3 & a)
        {
                if (a.X < X) X = a.X;
                if (a.Y < Y) Y = a.Y;
                if (a.Z < Z) Z = a.Z;
        }

        void Update_Max(const Vector3 & a)
        {
                if (a.X > X) X = a.X;
                if (a.Y > Y) Y = a.Y;
                if (a.Z > Z) Z = a.Z;
        }

        void Cap_Absolute_To(const Vector3 & a)
        {
                if (X > 0) { if (a.X < X) X = a.X; } else { if (-a.X > X) X = -a.X; }
                if (Y > 0) { if (a.Y < Y) Y = a.Y; } else { if (-a.Y > Y) Y = -a.Y; }
                if (Z > 0) { if (a.Z < Z) Z = a.Z; } else { if (-a.Z > Z) Z = -a.Z; }
        }

        void Scale(const Vector3 & scale)
        {
                X *= scale.X;
                Y *= scale.Y;
                Z *= scale.Z;
        }

        static void Cross_Product(const Vector3 &a, const Vector3 &b, Vector3 * set_result)
        {
                set_result->X = (a.Y * b.Z - a.Z * b.Y);
                set_result->Y = (a.Z * b.X - a.X * b.Z);
                set_result->Z = (a.X * b.Y - a.Y * b.X);
        }

        static void Normalized_Cross_Product(const Vector3 &a, const Vector3 &b, Vector3 * set_result)
        {
                set_result->X = (a.Y * b.Z - a.Z * b.Y);
                set_result->Y = (a.Z * b.X - a.X * b.Z);
                set_result->Z = (a.X * b.Y - a.Y * b.X);
                set_result->Normalize();
        }

        static float Cross_Product_X(const Vector3 &a, const Vector3 &b) { return a.Y * b.Z - a.Z * b.Y; }
        static float Cross_Product_Y(const Vector3 &a, const Vector3 &b) { return a.Z * b.X - a.X * b.Z; }
        static float Cross_Product_Z(const Vector3 &a, const Vector3 &b) { return a.X * b.Y - a.Y * b.X; }

        static void Add(const Vector3 &a, const Vector3 &b, Vector3 * set_result)
        {
                set_result->X = a.X + b.X;
                set_result->Y = a.Y + b.Y;
                set_result->Z = a.Z + b.Z;
        }

        static void Subtract(const Vector3 &a, const Vector3 &b, Vector3 * set_result)
        {
                set_result->X = a.X - b.X;
                set_result->Y = a.Y - b.Y;
                set_result->Z = a.Z - b.Z;
        }

        static void Lerp(const Vector3 & a, const Vector3 & b, float alpha, Vector3 * set_result)
        {
                set_result->X = (a.X + (b.X - a.X)*alpha);
                set_result->Y = (a.Y + (b.Y - a.Y)*alpha);
                set_result->Z = (a.Z + (b.Z - a.Z)*alpha);
        }

        static float Find_X_At_Y(float y, const Vector3 &p1, const Vector3 &p2)
        {
                return (p1.X + ((y - p1.Y) * ((p2.X - p1.X) / (p2.Y - p1.Y))));
        }
        static float Find_Y_At_X(float x, const Vector3 &p1, const Vector3 &p2)
        {
                return (p1.Y + ((x - p1.X) * ((p2.Y - p1.Y) / (p2.X - p1.X))));
        }
        static float Find_Z_At_X(float x, const Vector3 &p1, const Vector3 &p2)
        {
                return (p1.Z + ((x - p1.X) * ((p2.Z - p1.Z) / (p2.X - p1.X))));
        }

        static float Distance(const Vector3 &p1, const Vector3 &p2)
        {
                Vector3 temp;
                temp = p1 - p2;
                return (temp.Length());
        }

        static float Quick_Distance(const Vector3 &p1, const Vector3 &p2)
        {
                Vector3 temp;
                temp = p1 - p2;
                return (temp.Quick_Length());
        }

        void Rotate_X(float s_angle, float c_angle)
        {
                float tmp_y = Y;
                float tmp_z = Z;
                Y = c_angle * tmp_y - s_angle * tmp_z;
                Z = s_angle * tmp_y + c_angle * tmp_z;
        }

        void Rotate_Y(float s_angle, float c_angle)
        {
                float tmp_x = X;
                float tmp_z = Z;
                X = c_angle * tmp_x + s_angle * tmp_z;
                Z = -s_angle * tmp_x + c_angle * tmp_z;
        }

        void Rotate_Z(float s_angle, float c_angle)
        {
                float tmp_x = X;
                float tmp_y = Y;
                X = c_angle * tmp_x - s_angle * tmp_y;
                Y = s_angle * tmp_x + c_angle * tmp_y;
        }

        // Color packing; domain assumption of the original: components in
        // [0, 1] (the (unsigned)(float) conversion is undefined outside
        // [0, 2^32), and negative values are nonsense for colours).
        unsigned long Convert_To_ABGR() const
        {
                return ((unsigned)255<<24) |
                           ((unsigned)(Z*255.0f)<<16) |
                           ((unsigned)(Y*255.0f)<<8) |
                           ((unsigned)(X*255.0f));
        }

        unsigned long Convert_To_ARGB() const
        {
                return ((unsigned)255<<24) |
                           ((unsigned)(X*255.0f)<<16) |
                           ((unsigned)(Y*255.0f)<<8) |
                           ((unsigned)(Z*255.0f));
        }

        friend Vector3 operator * (const Vector3 &a, float k) { return Vector3((a.X * k), (a.Y * k), (a.Z * k)); }
        friend Vector3 operator * (float k, const Vector3 &a) { return Vector3((a.X * k), (a.Y * k), (a.Z * k)); }
        friend Vector3 operator / (const Vector3 &a, float k)
        {
                float ook = 1.0f/k;
                return Vector3((a.X * ook), (a.Y * ook), (a.Z * ook));
        }
        friend Vector3 operator + (const Vector3 &a, const Vector3 &b)
        {
                return Vector3(a.X+b.X, a.Y+b.Y, a.Z+b.Z);
        }
        friend Vector3 operator - (const Vector3 &a, const Vector3 &b)
        {
                return Vector3(a.X-b.X, a.Y-b.Y, a.Z-b.Z);
        }
        friend float operator * (const Vector3 &a, const Vector3 &b)
        {
                return a.X*b.X + a.Y*b.Y + a.Z*b.Z;
        }
        friend bool Equal_Within_Epsilon(const Vector3 &a, const Vector3 &b, float epsilon)
        {
                return ( (WWFabsf(a.X - b.X) < epsilon) &&
                                (WWFabsf(a.Y - b.Y) < epsilon) &&
                                (WWFabsf(a.Z - b.Z) < epsilon) );
        }
        float& operator [](int i) { return (&X)[i]; }
        const float& operator [](int i) const { return (&X)[i]; }
};

inline Vector3 operator-(const Vector3& v) { return Vector3(-v.X, -v.Y, -v.Z); }

// ---------------------------------------------------------------------------
// Vector4, transcribed from vector4.h.
// ---------------------------------------------------------------------------

struct Vector4
{
        float X, Y, Z, W;

        Vector4() {}
        Vector4(float x, float y, float z, float w) { X = x; Y = y; Z = z; W = w; }

        float Length() const { return WWSqrtf(Length2()); }
        float Length2() const { return X*X + Y*Y + Z*Z + W*W; }

        void Normalize()
        {
                float len2 = Length2();
                if (len2 != 0.0f) {
                        float oolen = WWInvSqrtLegacy(len2);
                        X *= oolen;
                        Y *= oolen;
                        Z *= oolen;
                        W *= oolen;
                }
        }

        // NOTE: original quirk — the compound subtraction ADDS W instead of
        // subtracting it. Preserved verbatim.
        Vector4 & operator -= (const Vector4 & v)
        {
                X -= v.X; Y -= v.Y; Z -= v.Z; W += v.W;
                return *this;
        }

        static Vector4 Lerp(const Vector4 & a, const Vector4 & b, float alpha)
        {
                return Vector4(
                        (a.X + (b.X - a.X)*alpha),
                        (a.Y + (b.Y - a.Y)*alpha),
                        (a.Z + (b.Z - a.Z)*alpha),
                        (a.W + (b.W - a.W)*alpha)
                );
        }

        // NOTE: original quirk — the void out-parameter Lerp writes the W
        // interpolation into X and leaves W untouched. Preserved verbatim.
        static void Lerp(const Vector4 & a, const Vector4 & b, float alpha, Vector4 * set_result)
        {
                set_result->X = (a.X + (b.X - a.X)*alpha);
                set_result->Y = (a.Y + (b.Y - a.Y)*alpha);
                set_result->Z = (a.Z + (b.Z - a.Z)*alpha);
                set_result->X = (a.W + (b.W - a.W)*alpha);
        }

        friend Vector4 operator * (const Vector4 &a, float k)
        {
                return Vector4((a.X * k), (a.Y * k), (a.Z * k), (a.W * k));
        }
        friend Vector4 operator * (float k, const Vector4 &a) { return a*k; }
        friend Vector4 operator / (const Vector4 &a, float k)
        {
                float ook = 1.0f/k;
                return Vector4((a[0] * ook), (a[1] * ook), (a[2] * ook), (a[3] * ook));
        }
        friend Vector4 operator + (const Vector4 &a, const Vector4 &b)
        {
                return Vector4(a[0]+b[0], a[1]+b[1], a[2]+b[2], a[3]+b[3]);
        }
        friend Vector4 operator - (const Vector4 &a, const Vector4 &b)
        {
                return Vector4(a[0]-b[0], a[1]-b[1], a[2]-b[2], a[3]-b[3]);
        }
        friend float operator * (const Vector4 &a, const Vector4 &b)
        {
                return a[0]*b[0] + a[1]*b[1] + a[2]*b[2] + a[3]*b[3];
        }
        float& operator [](int i) { return (&X)[i]; }
        const float& operator [](int i) const { return (&X)[i]; }
};

inline Vector4 operator-(const Vector4& v) { return Vector4(-v.X, -v.Y, -v.Z, -v.W); }

inline Vector4 Normalize(const Vector4 & vec)
{
        float len2 = vec.Length2();
        if (len2 != 0.0f) {
                float oolen = WWInvSqrtLegacy(len2);
                return vec * oolen;
        }
        return Vector4(0.0f, 0.0f, 0.0f, 0.0f);
}

inline Vector4 Lerp(const Vector4 & a, const Vector4 & b, float alpha)
{
        return Vector4(
                (a.X + (b.X - a.X)*alpha),
                (a.Y + (b.Y - a.Y)*alpha),
                (a.Z + (b.Z - a.Z)*alpha),
                (a.W + (b.W - a.W)*alpha)
        );
}

// ---------------------------------------------------------------------------
// Vector2i / Vector3i, transcribed from vector2i.h / Vector3i.h.
// ---------------------------------------------------------------------------

struct Vector2i
{
        int I, J;
        Vector2i() {}
        Vector2i(int i, int j) { I = i; J = j; }
        void Set(int i, int j) { I = i; J = j; }
        // this could use MMX..
        void Swap(Vector2i & other)
        {
                I ^= other.I;
                other.I ^= I;
                I ^= other.I;

                J ^= other.J;
                other.J ^= J;
                J ^= other.J;
        }
        bool operator == (const Vector2i & v) const { return (I == v.I) && (J == v.J); }
        bool operator != (const Vector2i & v) const { return (I != v.I) || (J != v.J); }
        const int& operator[] (int n) const { return (&I)[n]; }
        int& operator[] (int n) { return (&I)[n]; }
};

struct Vector3i
{
        int I, J, K;
        Vector3i() {}
        Vector3i(int i, int j, int k) { I = i; J = j; K = k; }
        bool operator == (const Vector3i & v) const { return (I == v.I) && (J == v.J) && (K == v.K); }
        bool operator != (const Vector3i & v) const { return (I != v.I) || (J != v.J) || (K != v.K); }
        const int& operator[] (int n) const { return (&I)[n]; }
        int& operator[] (int n) { return (&I)[n]; }
};

// ---------------------------------------------------------------------------
// Fast-trig lookup tables, transcribed from wwmath.cpp (WWMath::Init, the
// non-retail branches) and wwmath.h (Fast_Acos / Fast_Asin / Fast_Sin /
// Fast_Cos / Fast_Inv_Sin / Fast_Inv_Cos).
// ---------------------------------------------------------------------------

static const int ARC_TABLE_SIZE = 1024;
static const int SIN_TABLE_SIZE = 1024;
static float _FastAcosTable[ARC_TABLE_SIZE];
static float _FastAsinTable[ARC_TABLE_SIZE];
static float _FastSinTable[SIN_TABLE_SIZE];
static float _FastInvSinTable[SIN_TABLE_SIZE];

static void WWMathInit()
{
        int a = 0;
        for (; a < ARC_TABLE_SIZE; ++a)
        {
                float cv = float(a - ARC_TABLE_SIZE / 2) * (1.0f / (ARC_TABLE_SIZE / 2));
                _FastAcosTable[a] = WWAcosf(cv);
                _FastAsinTable[a] = WWAsinf(cv);
        }

        for (a = 0; a < SIN_TABLE_SIZE; ++a)
        {
                float cv = (float)a * 2.0f * WWMATH_PI / SIN_TABLE_SIZE;
                _FastSinTable[a] = WWSinf(cv);

                if (a > 0)
                        _FastInvSinTable[a] = 1.0f / _FastSinTable[a];
                else
                        _FastInvSinTable[a] = WWMATH_FLOAT_MAX;
        }
}

static inline float Fast_Acos(float val)
{
        // Near -1 and +1, the table becomes too inaccurate
        if (WWFabsf(val) > 0.975f)
                return WWAcosf(val);

        val *= float(ARC_TABLE_SIZE / 2);

        int idx0 = WWFloatToIntFloor(val);
        int idx1 = idx0 + 1;
        float frac = val - (float)idx0;

        idx0 += ARC_TABLE_SIZE / 2;
        idx1 += ARC_TABLE_SIZE / 2;

        return (1.0f - frac) * _FastAcosTable[idx0] + frac * _FastAcosTable[idx1];
}

static inline float Fast_Asin(float val)
{
        // Near -1 and +1, the table becomes too inaccurate
        if (WWFabsf(val) > 0.975f)
                return WWAsinf(val);

        val *= float(ARC_TABLE_SIZE / 2);

        int idx0 = WWFloatToIntFloor(val);
        int idx1 = idx0 + 1;
        float frac = val - (float)idx0;

        idx0 += ARC_TABLE_SIZE / 2;
        idx1 += ARC_TABLE_SIZE / 2;

        return (1.0f - frac) * _FastAsinTable[idx0] + frac * _FastAsinTable[idx1];
}

static inline float Fast_Sin(float val)
{
        val *= float(SIN_TABLE_SIZE) / (2.0f * WWMATH_PI);

        int idx0 = WWFloatToIntFloor(val);
        int idx1 = idx0 + 1;
        float frac = val - (float)idx0;

        idx0 = ((unsigned)idx0) & (SIN_TABLE_SIZE - 1);
        idx1 = ((unsigned)idx1) & (SIN_TABLE_SIZE - 1);

        return (1.0f - frac) * _FastSinTable[idx0] + frac * _FastSinTable[idx1];
}

static inline float Fast_Cos(float val)
{
        val += (WWMATH_PI * 0.5f);
        val *= float(SIN_TABLE_SIZE) / (2.0f * WWMATH_PI);

        int idx0 = WWFloatToIntFloor(val);
        int idx1 = idx0 + 1;
        float frac = val - (float)idx0;

        idx0 = ((unsigned)idx0) & (SIN_TABLE_SIZE - 1);
        idx1 = ((unsigned)idx1) & (SIN_TABLE_SIZE - 1);

        return (1.0f - frac) * _FastSinTable[idx0] + frac * _FastSinTable[idx1];
}

static inline float Fast_Inv_Sin(float val) { return 1.0f / Fast_Sin(val); }
static inline float Fast_Inv_Cos(float val) { return 1.0f / Fast_Cos(val); }

// ---------------------------------------------------------------------------
// Matrix3x3, transcribed verbatim from matrix3.h / matrix3.cpp @ 571a7c5
// (deterministic branches). Only the Matrix3D/Matrix4x4 interconversions
// are omitted (they arrive with those types' port).
// ---------------------------------------------------------------------------

struct Quaternion; // fwd
struct Matrix3x3; // fwd
Vector3 operator * (const Matrix3x3 &a, const Vector3 &v); // fwd

struct Matrix3x3
{
        Vector3 Row[3];

        Matrix3x3() {}
        explicit Matrix3x3(bool identity)
        {
                if (identity)
                {
                        Row[0] = Vector3(1.0f, 0.0f, 0.0f);
                        Row[1] = Vector3(0.0f, 1.0f, 0.0f);
                        Row[2] = Vector3(0.0f, 0.0f, 1.0f);
                }
        }
        Matrix3x3(const Vector3 &r0, const Vector3 &r1, const Vector3 &r2)
        {
                Row[0] = r0; Row[1] = r1; Row[2] = r2;
        }
        explicit Matrix3x3(
                float m11, float m12, float m13,
                float m21, float m22, float m23,
                float m31, float m32, float m33)
        {
                Row[0] = Vector3(m11, m12, m13);
                Row[1] = Vector3(m21, m22, m23);
                Row[2] = Vector3(m31, m32, m33);
        }
        explicit Matrix3x3(const Vector3 &axis, float angle)
        {
                Set(axis, angle);
        }
        explicit Matrix3x3(const Vector3 &axis, float s_angle, float c_angle)
        {
                Set(axis, s_angle, c_angle);
        }
        explicit Matrix3x3(const Quaternion &q); // below (needs Quaternion)

        void Set(const Vector3 &r0, const Vector3 &r1, const Vector3 &r2)
        {
                Row[0] = r0; Row[1] = r1; Row[2] = r2;
        }
        void Set(
                float m11, float m12, float m13,
                float m21, float m22, float m23,
                float m31, float m32, float m33)
        {
                Row[0] = Vector3(m11, m12, m13);
                Row[1] = Vector3(m21, m22, m23);
                Row[2] = Vector3(m31, m32, m33);
        }
        void Make_Identity()
        {
                Row[0] = Vector3(1.0f, 0.0f, 0.0f);
                Row[1] = Vector3(0.0f, 1.0f, 0.0f);
                Row[2] = Vector3(0.0f, 0.0f, 1.0f);
        }

        void Set(const Vector3 &axis, float angle)
        {
                Set(axis, WWSinf(angle), WWCosf(angle));
        }

        void Set(const Vector3 &axis, float s, float c)
        {
                Row[0] = Vector3(
                        (float)(axis[0] * axis[0] + c * (1.0f - axis[0] * axis[0])),
                        (float)(axis[0] * axis[1] * (1.0f - c) - axis[2] * s),
                        (float)(axis[2] * axis[0] * (1.0f - c) + axis[1] * s)
                );

                Row[1] = Vector3(
                        (float)(axis[0] * axis[1] * (1.0f - c) + axis[2] * s),
                        (float)(axis[1] * axis[1] + c * (1.0f - axis[1] * axis[1])),
                        (float)(axis[1] * axis[2] * (1.0f - c) - axis[0] * s)
                );

                Row[2] = Vector3(
                        (float)(axis[2] * axis[0] * (1.0f - c) - axis[1] * s),
                        (float)(axis[1] * axis[2] * (1.0f - c) + axis[0] * s),
                        (float)(axis[2] * axis[2] + c * (1 - axis[2] * axis[2]))
                );
        }

        void Set(const Quaternion &q); // below

        Vector3 &operator[](int i) { return Row[i]; }
        const Vector3 &operator[](int i) const { return Row[i]; }

        Matrix3x3 Transpose() const
        {
                return Matrix3x3(
                        Vector3(Row[0][0], Row[1][0], Row[2][0]),
                        Vector3(Row[0][1], Row[1][1], Row[2][1]),
                        Vector3(Row[0][2], Row[1][2], Row[2][2])
                );
        }

        Matrix3x3 Inverse() const // Gauss-Jordan elimination with partial pivoting
        {
                Matrix3x3 a(*this);
                Matrix3x3 b(true);
                int i, j, i1;

                for (j = 0; j < 3; j++)
                {
                        i1 = j;
                        for (i = j + 1; i < 3; i++)
                        {
                                if (WWFabsf(a[i][j]) > WWFabsf(a[i1][j]))
                                        i1 = i;
                        }

                        std::swap(a.Row[i1], a.Row[j]);
                        std::swap(b.Row[i1], b.Row[j]);

                        if (a[j][j] == 0.)
                        {
                                //Matrix3x3::inverse: singular matrix; can't invert
                        }
                        b.Row[j] = b.Row[j] / a[j][j];
                        a.Row[j] = a.Row[j] / a[j][j];

                        for (i = 0; i < 3; i++)
                        {
                                if (i != j)
                                {
                                        b.Row[i] = b.Row[i] - a[i][j] * b.Row[j];
                                        a.Row[i] = a.Row[i] - a[i][j] * a.Row[j];
                                }
                        }
                }
                return b;
        }

        float Determinant() const
        {
                return Row[0][0] * (Row[1][1] * Row[2][2] - Row[1][2] * Row[2][1])
                     - Row[0][1] * (Row[1][0] * Row[2][2] - Row[1][2] * Row[2][0])
                     - Row[0][2] * (Row[1][0] * Row[2][1] - Row[1][1] * Row[2][0]);
        }

        float Get_X_Rotation() const
        {
                Vector3 v = (*this) * Vector3(0.0f, 1.0f, 0.0f);
                return WWAtan2f(v[2], v[1]);
        }

        float Get_Y_Rotation() const
        {
                Vector3 v = (*this) * Vector3(0.0f, 0.0f, 1.0f);
                return WWAtan2f(v[0], v[2]);
        }

        float Get_Z_Rotation() const
        {
                Vector3 v = (*this) * Vector3(1.0f, 0.0f, 0.0f);
                return WWAtan2f(v[1], v[0]);
        }

        Vector3 Get_X_Vector() const { return Vector3(Row[0][0], Row[1][0], Row[2][0]); }
        Vector3 Get_Y_Vector() const { return Vector3(Row[0][1], Row[1][1], Row[2][1]); }
        Vector3 Get_Z_Vector() const { return Vector3(Row[0][2], Row[1][2], Row[2][2]); }

        void Rotate_X(float theta) { Rotate_X(WWSinf(theta), WWCosf(theta)); }
        void Rotate_X(float s, float c)
        {
                float tmp1, tmp2;
                for (int r = 0; r < 3; r++)
                {
                        tmp1 = Row[r][1]; tmp2 = Row[r][2];
                        Row[r][1] = (float)(c * tmp1 + s * tmp2);
                        Row[r][2] = (float)(-s * tmp1 + c * tmp2);
                }
        }

        void Rotate_Y(float theta) { Rotate_Y(WWSinf(theta), WWCosf(theta)); }
        void Rotate_Y(float s, float c)
        {
                float tmp1, tmp2;
                for (int r = 0; r < 3; r++)
                {
                        tmp1 = Row[r][0]; tmp2 = Row[r][2];
                        Row[r][0] = (float)(c * tmp1 - s * tmp2);
                        Row[r][2] = (float)(s * tmp1 + c * tmp2);
                }
        }

        void Rotate_Z(float theta) { Rotate_Z(WWSinf(theta), WWCosf(theta)); }
        void Rotate_Z(float s, float c)
        {
                float tmp1, tmp2;
                for (int r = 0; r < 3; r++)
                {
                        tmp1 = Row[r][0]; tmp2 = Row[r][1];
                        Row[r][0] = (float)(c * tmp1 + s * tmp2);
                        Row[r][1] = (float)(-s * tmp1 + c * tmp2);
                }
        }

        // Matrix3x3::Multiply (the static, column-caching variant)
        static void Multiply(const Matrix3x3 &A, const Matrix3x3 &B, Matrix3x3 *set_res)
        {
                Matrix3x3 tmp;
                const Matrix3x3 *Aptr;
                float tmp1, tmp2, tmp3;

                if (set_res == &A)
                {
                        tmp = A;
                        Aptr = &tmp;
                }
                else
                {
                        Aptr = &A;
                }

                for (int col = 0; col < 3; col++)
                {
                        tmp1 = B[0][col];
                        tmp2 = B[1][col];
                        tmp3 = B[2][col];
                        for (int row = 0; row < 3; row++)
                        {
                                (*set_res)[row][col] = (float)((*Aptr)[row][0] * tmp1 + (*Aptr)[row][1] * tmp2 + (*Aptr)[row][2] * tmp3);
                        }
                }
        }

        static void Rotate_Vector(const Matrix3x3 &A, const Vector3 &in, Vector3 *out)
        {
                out->X = (A[0][0] * in.X + A[0][1] * in.Y + A[0][2] * in.Z);
                out->Y = (A[1][0] * in.X + A[1][1] * in.Y + A[1][2] * in.Z);
                out->Z = (A[2][0] * in.X + A[2][1] * in.Y + A[2][2] * in.Z);
        }

        static void Transpose_Rotate_Vector(const Matrix3x3 &A, const Vector3 &in, Vector3 *out)
        {
                out->X = (A[0][0] * in.X + A[1][0] * in.Y + A[2][0] * in.Z);
                out->Y = (A[0][1] * in.X + A[1][1] * in.Y + A[2][1] * in.Z);
                out->Z = (A[0][2] * in.X + A[1][2] * in.Y + A[2][2] * in.Z);
        }

        int Is_Orthogonal() const
        {
                Vector3 x(Row[0].X, Row[0].Y, Row[0].Z);
                Vector3 y(Row[1].X, Row[1].Y, Row[1].Z);
                Vector3 z(Row[2].X, Row[2].Y, Row[2].Z);

                if (x * y > WWMATH_EPSILON) return 0;
                if (y * z > WWMATH_EPSILON) return 0;
                if (z * x > WWMATH_EPSILON) return 0;

                if (WWFabsf(x.Length() - 1.0f) > WWMATH_EPSILON) return 0;
                if (WWFabsf(y.Length() - 1.0f) > WWMATH_EPSILON) return 0;
                if (WWFabsf(z.Length() - 1.0f) > WWMATH_EPSILON) return 0;

                return 1;
        }

        void Re_Orthogonalize()
        {
                Vector3 x(Row[0][0], Row[0][1], Row[0][2]);
                Vector3 y(Row[1][0], Row[1][1], Row[1][2]);
                Vector3 z;

                Vector3::Cross_Product(x, y, &z);
                Vector3::Cross_Product(z, x, &y);

                float len = x.Length();
                if (len < WWMATH_EPSILON)
                {
                        Make_Identity();
                        return;
                }
                else
                {
                        x = x / len;
                }

                len = y.Length();
                if (len < WWMATH_EPSILON)
                {
                        Make_Identity();
                        return;
                }
                else
                {
                        y = y / len;
                }

                len = z.Length();
                if (len < WWMATH_EPSILON)
                {
                        Make_Identity();
                        return;
                }
                else
                {
                        z = z / len;
                }

                Row[0][0] = x.X; Row[0][1] = x.Y; Row[0][2] = x.Z;
                Row[1][0] = y.X; Row[1][1] = y.Y; Row[1][2] = y.Z;
                Row[2][0] = z.X; Row[2][1] = z.Y; Row[2][2] = z.Z;
        }

        void Rotate_AABox_Extent(const Vector3 &extent, Vector3 *set_extent)
        {
                for (int i = 0; i < 3; i++)
                {
                        (*set_extent)[i] = 0.0f;
                        for (int j = 0; j < 3; j++)
                        {
                                (*set_extent)[i] += WWFabsf(Row[i][j] * extent[j]);
                        }
                }
        }
};

inline Matrix3x3 operator - (const Matrix3x3 &a)
{
        return Matrix3x3(-a.Row[0], -a.Row[1], -a.Row[2]);
}

inline Matrix3x3 operator * (const Matrix3x3 &a, float d)
{
        return Matrix3x3(a.Row[0] * d, a.Row[1] * d, a.Row[2] * d);
}

inline Matrix3x3 operator * (float d, const Matrix3x3 &a)
{
        return a * d;
}

inline Matrix3x3 operator / (const Matrix3x3 &a, float d)
{
        float ood = 1.0f / d;
        return Matrix3x3(a.Row[0] * ood, a.Row[1] * ood, a.Row[2] * ood);
}

inline Matrix3x3 operator * (const Matrix3x3 &a, const Matrix3x3 &b)
{
        #define ROWCOL(i,j) a[i][0]*b[0][j] + a[i][1]*b[1][j] + a[i][2]*b[2][j]

        return Matrix3x3(
                Vector3(ROWCOL(0,0), ROWCOL(0,1), ROWCOL(0,2) ),
                Vector3(ROWCOL(1,0), ROWCOL(1,1), ROWCOL(1,2) ),
                Vector3(ROWCOL(2,0), ROWCOL(2,1), ROWCOL(2,2) )
        );

        #undef ROWCOL
}

inline Vector3 operator * (const Matrix3x3 &a, const Vector3 &v)
{
        return Vector3(
                a[0][0] * v[0] + a[0][1] * v[1] + a[0][2] * v[2],
                a[1][0] * v[0] + a[1][1] * v[1] + a[1][2] * v[2],
                a[2][0] * v[0] + a[2][1] * v[1] + a[2][2] * v[2]
        );
}

// ---------------------------------------------------------------------------
// Quaternion, transcribed verbatim from quat.h / quat.cpp @ 571a7c5
// (deterministic branches; the asm Fast_Slerp and the rand()-based
// Randomize are not transcribed — the C++ build uses the plain-C
// Fast_Slerp and the port drops Randomize).
// ---------------------------------------------------------------------------

#define SLERP_EPSILON 0.001

struct Quaternion;
Quaternion operator * (const Quaternion &a, const Quaternion &b); // fwd

struct Quaternion
{
        float X;
        float Y;
        float Z;
        float W;

        Quaternion() {}
        explicit Quaternion(bool init) { if (init) { X = 0.0f; Y = 0.0f; Z = 0.0f; W = 1.0f; } }
        explicit Quaternion(float a, float b, float c, float d) { X = a; Y = b; Z = c; W = d; }
        Quaternion(const Vector3 &axis, float angle)
        {
                float s = WWSinf(angle / 2);
                float c = WWCosf(angle / 2);
                X = s * axis.X;
                Y = s * axis.Y;
                Z = s * axis.Z;
                W = c;
        }

        void Set(float a = 0.0f, float b = 0.0f, float c = 0.0f, float d = 1.0f) { X = a; Y = b; Z = c; W = d; }
        void Make_Identity() { Set(); }
        void Scale(float s) { X = (float)(s * X); Y = (float)(s * Y); Z = (float)(s * Z); W = (float)(s * W); }

        float &operator[](int i) { return (&X)[i]; }
        const float &operator[](int i) const { return (&X)[i]; }

        Quaternion operator-() const { return (Quaternion(-X, -Y, -Z, -W)); }
        Quaternion operator+() const { return *this; }

        Quaternion &Make_Closest(const Quaternion &qto)
        {
                float cos_t = qto.X * X + qto.Y * Y + qto.Z * Z + qto.W * W;
                if (cos_t < 0.0)
                {
                        X = -X; Y = -Y; Z = -Z; W = -W;
                }
                return *this;
        }

        float Length2() const { return (X * X + Y * Y + Z * Z + W * W); }
        float Length() const { return WWSqrtf(Length2()); }

        void Normalize()
        {
                float len2 = X * X + Y * Y + Z * Z + W * W;
                if (0.0f == len2)
                {
                        return;
                }
                else
                {
                        float inv_mag = WWInvSqrtLegacy(len2);
                        X *= inv_mag; Y *= inv_mag; Z *= inv_mag; W *= inv_mag;
                }
        }

        void Rotate_X(float theta) { *this = (*this) * Quaternion(Vector3(1.0f, 0.0f, 0.0f), theta); }
        void Rotate_Y(float theta) { *this = (*this) * Quaternion(Vector3(0.0f, 1.0f, 0.0f), theta); }
        void Rotate_Z(float theta) { *this = (*this) * Quaternion(Vector3(0.0f, 0.0f, 1.0f), theta); }

        Vector3 Rotate_Vector(const Vector3 &v) const
        {
                float x = W * v.X + (Y * v.Z - v.Y * Z);
                float y = W * v.Y - (X * v.Z - v.X * Z);
                float z = W * v.Z + (X * v.Y - v.X * Y);
                float w = -(X * v.X + Y * v.Y + Z * v.Z);

                return Vector3
                (
                        w * (-X) + W * x + (y * (-Z) - (-Y) * z),
                        w * (-Y) + W * y - (x * (-Z) - (-X) * z),
                        w * (-Z) + W * z + (x * (-Y) - (-X) * y)
                );
        }

        bool Is_Valid() const
        {
                UnsignedInt bx, by, bz, bw;
                memcpy(&bx, &X, 4); memcpy(&by, &Y, 4); memcpy(&bz, &Z, 4); memcpy(&bw, &W, 4);
                return ((bx & 0x7F800000) != 0x7F800000) && ((by & 0x7F800000) != 0x7F800000)
                    && ((bz & 0x7F800000) != 0x7F800000) && ((bw & 0x7F800000) != 0x7F800000);
        }
};

inline Quaternion operator + (const Quaternion &a, const Quaternion &b)
{
        return Quaternion(a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]);
}

inline Quaternion operator - (const Quaternion &a, const Quaternion &b)
{
        return Quaternion(a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]);
}

inline Quaternion operator * (float scl, const Quaternion &a)
{
        return Quaternion(scl * a[0], scl * a[1], scl * a[2], scl * a[3]);
}

inline Quaternion operator * (const Quaternion &a, float scl) { return scl * a; }

inline Quaternion operator * (const Quaternion &a, const Quaternion &b)
{
        return Quaternion
        (
                a.W * b.X + b.W * a.X + (a.Y * b.Z - b.Y * a.Z),
                a.W * b.Y + b.W * a.Y - (a.X * b.Z - b.X * a.Z),
                a.W * b.Z + b.W * a.Z + (a.X * b.Y - b.X * a.Y),
                a.W * b.W - (a.X * b.X + a.Y * b.Y + a.Z * b.Z)
        );
}

inline Quaternion Inverse(const Quaternion &a) { return Quaternion(-a[0], -a[1], -a[2], a[3]); }
inline Quaternion Conjugate(const Quaternion &a) { return Quaternion(-a[0], -a[1], -a[2], a[3]); }

inline Quaternion operator / (const Quaternion &a, const Quaternion &b) { return a * Inverse(b); }

inline Quaternion Normalize(const Quaternion &a)
{
        float mag = a.Length();
        if (0.0f == mag)
                return a;
        else
        {
                float oomag = 1.0f / mag;
                return Quaternion(a[0] * oomag, a[1] * oomag, a[2] * oomag, a[3] * oomag);
        }
}

static Quaternion Axis_To_Quat(const Vector3 &a, float phi)
{
        Quaternion q;
        Vector3 tmp = a;
        tmp.Normalize();
        q[0] = tmp[0];
        q[1] = tmp[1];
        q[2] = tmp[2];
        q.Scale(WWSinf(phi / 2.0f));
        q[3] = WWCosf(phi / 2.0f);
        return q;
}

static void Slerp(Quaternion &res, const Quaternion &p, const Quaternion &q, float alpha)
{
        float beta;
        float theta;
        float cos_t;
        float oo_sin_t;
        int qflip;

        cos_t = p.X * q.X + p.Y * q.Y + p.Z * q.Z + p.W * q.W;
        if (cos_t < 0.0f)
        {
                cos_t = -cos_t;
                qflip = true;
        }
        else
        {
                qflip = false;
        }

        if (1.0f - cos_t < WWMATH_EPSILON * WWMATH_EPSILON)
        {
                beta = 1.0f - alpha;
        }
        else
        {
                theta = WWAcosf(cos_t);
                float sin_t = WWSinf(theta);
                oo_sin_t = 1.0f / sin_t;
                beta = WWSinf(theta - alpha * theta) * oo_sin_t;
                alpha = WWSinf(alpha * theta) * oo_sin_t;
        }

        if (qflip)
        {
                alpha = -alpha;
        }

        res.X = beta * p.X + alpha * q.X;
        res.Y = beta * p.Y + alpha * q.Y;
        res.Z = beta * p.Z + alpha * q.Z;
        res.W = beta * p.W + alpha * q.W;
}

static void Fast_Slerp(Quaternion &res, const Quaternion &p, const Quaternion &q, float alpha)
{
        float beta;
        float theta;
        float cos_t;
        float oo_sin_t;
        int qflip;

        cos_t = p.X * q.X + p.Y * q.Y + p.Z * q.Z + p.W * q.W;
        if (cos_t < 0.0f)
        {
                cos_t = -cos_t;
                qflip = true;
        }
        else
        {
                qflip = false;
        }

        if (1.0f - cos_t < WWMATH_EPSILON * WWMATH_EPSILON)
        {
                beta = 1.0f - alpha;
        }
        else
        {
                theta = Fast_Acos(cos_t);
                float sin_t = Fast_Sin(theta);
                oo_sin_t = 1.0f / sin_t;
                beta = Fast_Sin(theta - alpha * theta) * oo_sin_t;
                alpha = Fast_Sin(alpha * theta) * oo_sin_t;
        }

        if (qflip)
        {
                alpha = -alpha;
        }

        res.X = beta * p.X + alpha * q.X;
        res.Y = beta * p.Y + alpha * q.Y;
        res.Z = beta * p.Z + alpha * q.Z;
        res.W = beta * p.W + alpha * q.W;
}

struct SlerpInfoStruct
{
        bool Flip;
        bool Linear;
        float Theta;
        float SinT;
};

static void Slerp_Setup(const Quaternion &p, const Quaternion &q, SlerpInfoStruct *slerpinfo)
{
        float cos_t;
        cos_t = p.X * q.X + p.Y * q.Y + p.Z * q.Z + p.W * q.W;
        if (cos_t < 0.0f)
        {
                cos_t = -cos_t;
                slerpinfo->Flip = true;
        }
        else
        {
                slerpinfo->Flip = false;
        }

        if (1.0f - cos_t < SLERP_EPSILON)
        {
                slerpinfo->Linear = true;
                slerpinfo->Theta = 0.0f;
                slerpinfo->SinT = 0.0f;
        }
        else
        {
                slerpinfo->Linear = false;
                slerpinfo->Theta = WWAcosf(cos_t);
                slerpinfo->SinT = WWSinf(slerpinfo->Theta);
        }
}

static Quaternion Cached_Slerp(const Quaternion &p, const Quaternion &q, float alpha, const SlerpInfoStruct *slerpinfo)
{
        float beta;
        float oo_sin_t;

        if (slerpinfo->Linear)
        {
                beta = 1.0f - alpha;
        }
        else
        {
                oo_sin_t = 1.0f / slerpinfo->Theta; // NOTE: original divides by Theta (bug), preserved
                beta = WWSinf(slerpinfo->Theta - alpha * slerpinfo->Theta) * oo_sin_t;
                alpha = WWSinf(alpha * slerpinfo->Theta) * oo_sin_t;
        }

        if (slerpinfo->Flip)
        {
                alpha = -alpha;
        }

        Quaternion res;
        res.X = beta * p.X + alpha * q.X;
        res.Y = beta * p.Y + alpha * q.Y;
        res.Z = beta * p.Z + alpha * q.Z;
        res.W = beta * p.W + alpha * q.W;
        return res;
}

static int _nxt[3] = { 1, 2, 0 };

static Quaternion Build_Quaternion(const Matrix3x3 &mat)
{
        float tr, s;
        int i, j, k;
        Quaternion q;
        tr = mat[0][0] + mat[1][1] + mat[2][2];
        if (tr > 0.0f)
        {
                s = WWSqrtf(tr + 1.0f);
                q[3] = s * 0.5f;
                s = 0.5f / s;
                q[0] = (mat[2][1] - mat[1][2]) * s;
                q[1] = (mat[0][2] - mat[2][0]) * s;
                q[2] = (mat[1][0] - mat[0][1]) * s;
        }
        else
        {
                i = 0;
                if (mat[1][1] > mat[0][0]) i = 1;
                if (mat[2][2] > mat[i][i]) i = 2;
                j = _nxt[i];
                k = _nxt[j];
                s = WWSqrtf((mat[i][i] - (mat[j][j] + mat[k][k])) + 1.0f);
                q[i] = s * 0.5f;
                if (s != 0.0f)
                {
                        s = 0.5f / s;
                }
                q[3] = (mat[k][j] - mat[j][k]) * s;
                q[j] = (mat[j][i] + mat[i][j]) * s;
                q[k] = (mat[k][i] + mat[i][k]) * s;
        }
        return q;
}

static Matrix3x3 Build_Matrix3(const Quaternion &q)
{
        Matrix3x3 m;

        m[0][0] = (float)(1.0 - 2.0 * (q[1] * q[1] + q[2] * q[2]));
        m[0][1] = (float)(2.0 * (q[0] * q[1] - q[2] * q[3]));
        m[0][2] = (float)(2.0 * (q[2] * q[0] + q[1] * q[3]));

        m[1][0] = (float)(2.0 * (q[0] * q[1] + q[2] * q[3]));
        m[1][1] = (float)(1.0 - 2.0f * (q[2] * q[2] + q[0] * q[0]));
        m[1][2] = (float)(2.0 * (q[1] * q[2] - q[0] * q[3]));

        m[2][0] = (float)(2.0 * (q[2] * q[0] - q[1] * q[3]));
        m[2][1] = (float)(2.0 * (q[1] * q[2] + q[0] * q[3]));
        m[2][2] = (float)(1.0 - 2.0 * (q[1] * q[1] + q[0] * q[0]));

        return m;
}

// Matrix3x3 members that need the complete Quaternion type.
Matrix3x3::Matrix3x3(const Quaternion &q) { this->Set(q); }

void Matrix3x3::Set(const Quaternion &q)
{
        Row[0][0] = (float)(1.0 - 2.0 * (q[1] * q[1] + q[2] * q[2]));
        Row[0][1] = (float)(2.0 * (q[0] * q[1] - q[2] * q[3]));
        Row[0][2] = (float)(2.0 * (q[2] * q[0] + q[1] * q[3]));

        Row[1][0] = (float)(2.0 * (q[0] * q[1] + q[2] * q[3]));
        Row[1][1] = (float)(1.0 - 2.0f * (q[2] * q[2] + q[0] * q[0]));
        Row[1][2] = (float)(2.0 * (q[1] * q[2] - q[0] * q[3]));

        Row[2][0] = (float)(2.0 * (q[2] * q[0] - q[1] * q[3]));
        Row[2][1] = (float)(2.0 * (q[1] * q[2] + q[0] * q[3]));
        Row[2][2] = (float)(1.0 - 2.0 * (q[1] * q[1] + q[0] * q[0]));
}

// ---------------------------------------------------------------------------
// Golden vector generation. Deterministic: a fixed LCG produces the test
// inputs, so the same vectors come out on every platform.
// ---------------------------------------------------------------------------

static UnsignedInt floatBits( Real value )
{
        UnsignedInt bits;
        memcpy(&bits, &value, sizeof(bits));
        return bits;
}

static Real bitsToFloat( UnsignedInt bits )
{
        Real value;
        memcpy(&value, &bits, sizeof(value));
        return value;
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
static UnsignedInt lcgWord()
{
        lcgState = lcgState * 1664525u + 1013904223u;
        return lcgState;
}

// A finite float from the raw bit pool (every magnitude class including
// subnormals; NaN and infinity are rejected because NaN payloads and
// inf-inf results are not stable across platforms).
static Real finiteFloat()
{
        for (;;)
        {
                Real f = bitsToFloat(lcgWord());
                if (f == f && f * 0.0f == 0.0f) // not NaN (x==x), not inf (x*0==0)
                        return f;
        }
}

// A game-typical float: quarter steps in [-2500, 2500] (exact binary values,
// no representation noise) mixed with the raw pool above by the caller.
static Real stepFloat()
{
        return ((int)(lcgWord() % 20001) - 10000) * 0.25f;
}

// A float in [0, 1] for the colour / unit-float conversions.
static Real unitFloat()
{
        return (float)(lcgWord() % 1001) / 1000.0f;
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

static void generateNameKeyGolden( FILE *out )
{
        fprintf(out, "# NameKey golden vectors (parity level P0).\n");
        fprintf(out, "# Generated by tools/differential/gen_golden.cpp, transcribed from\n");
        fprintf(out, "# GeneralsMD/Code/GameEngine/Source/Common/NameKeyGenerator.cpp @ 571a7c5.\n");
        fprintf(out, "# Records:\n");
        fprintf(out, "#   hash <name_hex> <hash_hex>          calcHashForString\n");
        fprintf(out, "#   lhash <name_hex> <hash_hex>         calcHashForLowercaseString\n");
        fprintf(out, "#   seq ... end                         one generator driven in order:\n");
        fprintf(out, "#     key <name_hex> <key_dec>          nameToKey\n");
        fprintf(out, "#     lkey <name_hex> <key_dec>         nameToLowercaseKey\n");
        fprintf(out, "#     name <key_dec> <name_hex_or_->    keyToName (dash = empty/unknown)\n");
        fprintf(out, "# Regenerate with tools/differential/regenerate.sh; never edit by hand.\n");

        // Hash anchors: fixed names including empty, mixed case, separators,
        // high-bit bytes and a long name.
        static const char *const names[] = {
                "",
                "a", "A", "z", "Z", "0", "_", "-", ".",
                "AmericaTank",
                "AmericaTank",
                "americatank",
                "AMERICATANK",
                "GLAInfantryRebel",
                "Data\\INI\\Default\\Object.ini",
                "Data\\Audio\\Tracks\\GLA_02.mp3",
                "AmericaAmericaWeaponFactory",
                "ObjectCreationList",
                "UnitSpecificSounds",
                "CommandSet:AmericaVehicleHumvee",
                "a\xc4\xdfname",        // high-bit bytes pass through untouched
                "VeryLongTemplateNameThatGoesOnAndOnAndOn_00123456789",
        };

        for (size_t i = 0; i < sizeof(names) / sizeof(names[0]); i++)
        {
                const char *s = names[i];
                size_t len = strlen(s);
                fprintf(out, "hash ");
                writeHex(out, (const UnsignedByte *)s, len);
                fprintf(out, " %08x\n", calcHashForString(s));
                fprintf(out, "lhash ");
                writeHex(out, (const UnsignedByte *)s, len);
                fprintf(out, " %08x\n", calcHashForLowercaseString(s));
        }

        // Deterministic pseudo-random names through both entry points,
        // exercising reuse, case-insensitive canonicalization and the
        // reverse lookup. resetLcg drives name lengths and letters.
        NameKeyGenerator gen;

        fprintf(out, "seq\n");
        for (int i = 0; i < 500; i++)
        {
                char buf[64];
                size_t len = 1 + (lcgWord() % 24);
                for (size_t c = 0; c < len; c++)
                {
                        static const char alphabet[] =
                                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-\\";
                        buf[c] = alphabet[lcgWord() % (sizeof(alphabet) - 1)];
                }
                buf[len] = 0;

                // Alternate between the case-sensitive and insensitive entry
                // points; also re-look-up the previous name sometimes.
                switch (lcgWord() % 4u)
                {
                case 0:
                        fprintf(out, "key ");
                        writeHex(out, (const UnsignedByte *)buf, len);
                        fprintf(out, " %u\n", gen.nameToKey(buf));
                        break;
                case 1:
                        fprintf(out, "lkey ");
                        writeHex(out, (const UnsignedByte *)buf, len);
                        fprintf(out, " %u\n", gen.nameToLowercaseKey(buf));
                        break;
                case 2:
                {
                        // Uppercase a copy so the insensitive path must hit
                        // previously-stored canonical lowercase names.
                        char upper[64];
                        for (size_t c = 0; c <= len; c++)
                                upper[c] = (char)toupper((unsigned char)buf[c]);
                        fprintf(out, "lkey ");
                        writeHex(out, (const UnsignedByte *)upper, len);
                        fprintf(out, " %u\n", gen.nameToLowercaseKey(upper));
                        break;
                }
                default:
                        fprintf(out, "key ");
                        writeHex(out, (const UnsignedByte *)buf, len);
                        fprintf(out, " %u\n", gen.nameToKey(buf));
                        break;
                }

                if (i % 5 == 0 && gen.m_nextID > 1)
                {
                        UnsignedInt k = 1 + (lcgWord() % (gen.m_nextID - 1));
                        const std::string &name = gen.allBuckets[k - 1]->nameString;
                        fprintf(out, "name %u ", k);
                        writeHex(out, (const UnsignedByte *)name.data(), name.size());
                        fprintf(out, "\n");
                }
        }
        // Reverse lookups of the invalid key and one past the end (the
        // original answers these with the empty string; Rust answers None).
        fprintf(out, "name %u -\n", NAMEKEY_INVALID);
        fprintf(out, "name %u -\n", gen.m_nextID);
        fprintf(out, "end\n");

        // A second sequence proves reset() restarts the id assignment.
        gen.reset();
        fprintf(out, "seq\n");
        fprintf(out, "key ");
        writeHex(out, (const UnsignedByte *)"AfterReset", 10);
        fprintf(out, " %u\n", gen.nameToKey("AfterReset"));
        fprintf(out, "end\n");
}

static void generateMathScalarGolden( FILE *out )
{
        fprintf(out, "# WWMath scalar golden vectors (parity level P0, IEEE-754 exact subset).\n");
        fprintf(out, "# Generated by tools/differential/gen_golden.cpp, transcribed from\n");
        fprintf(out, "# Core/Libraries/Source/WWVegas/WWMath/wwmath.h @ 571a7c5 (deterministic\n");
        fprintf(out, "# branches; trig/pow/exp/log are deferred until their dedicated port).\n");
        fprintf(out, "# Format: scalar <op> <a_bits> [<b_bits>] <result_bits>\n");
        fprintf(out, "# Integer-valued ops emit the integer in decimal instead of hex bits.\n");
        fprintf(out, "# Operands are finite floats (NaN/inf payloads are not portable).\n");
        fprintf(out, "# Regenerate with tools/differential/regenerate.sh; never edit by hand.\n");

        resetLcg(0x5ca1ab1eu);

        for (int i = 0; i < 400; i++)
        {
                float a = (i & 1) ? finiteFloat() : stepFloat();
                float b = (i & 2) ? finiteFloat() : stepFloat();

                fprintf(out, "scalar sqrt %08x %08x\n", floatBits(a), floatBits(WWSqrtf(a)));
                fprintf(out, "scalar inv_sqrt %08x %08x\n", floatBits(a), floatBits(WWInvSqrtLegacy(a)));
                fprintf(out, "scalar fabs %08x %08x\n", floatBits(a), floatBits(WWFabsf(a)));
                fprintf(out, "scalar floor %08x %08x\n", floatBits(a), floatBits(WWFloorf(a)));
                fprintf(out, "scalar ceil %08x %08x\n", floatBits(a), floatBits(WWCeilf(a)));
                fprintf(out, "scalar round %08x %08x\n", floatBits(a), floatBits(WWRoundf(a)));
                fprintf(out, "scalar clamp %08x %08x\n", floatBits(a), floatBits(WWClamp(a, 0.0f, 1.0f)));
                fprintf(out, "scalar clamp2 %08x %08x %08x\n", floatBits(a), floatBits(b), floatBits(WWClamp(a, -2.5f, 7.5f)));
                fprintf(out, "scalar wrap %08x %08x\n", floatBits(a), floatBits(WWWrap(a, 0.0f, 1.0f)));
                fprintf(out, "scalar min %08x %08x %08x\n", floatBits(a), floatBits(b), floatBits(WWMin(a, b)));
                fprintf(out, "scalar max %08x %08x %08x\n", floatBits(a), floatBits(b), floatBits(WWMax(a, b)));
                fprintf(out, "scalar lerp %08x %08x %08x %08x\n", floatBits(a), floatBits(b), floatBits(b), floatBits(WWLerp(a, b, b)));
                fprintf(out, "scalar inverse_lerp %08x %08x %08x %08x\n", floatBits(a), floatBits(b), floatBits(a), floatBits(WWInverseLerp(a, b, a)));
                fprintf(out, "scalar normalize_angle %08x %08x\n", floatBits(a), floatBits(WWNormalizeAngle(a)));
                fprintf(out, "scalar div_safe %08x %08x %08x\n", floatBits(a), floatBits(b), floatBits(WWDivSafe(a, b, WWMATH_EPSILON)));
                UnsignedByte by = (UnsignedByte)(lcgWord() % 256);
                fprintf(out, "scalar byte_to_unit %08x %08x\n", floatBits((float)by), floatBits(WWByteToUnitFloat(by)));

                // Conversions: stay inside the domain the original defines.
                float small = (i & 1) ? ((int)(lcgWord() % 2000001) - 1000000) * 0.5f
                                      : (float)((int)(lcgWord() % 2001) - 1000) + 0.25f;
                fprintf(out, "scalar float_to_int_chop %08x %d\n", floatBits(small), WWFloatToIntChop(small));
                fprintf(out, "scalar float_to_int_floor %08x %d\n", floatBits(small), WWFloatToIntFloor(small));

                float unit = unitFloat();
                fprintf(out, "scalar unit_float_to_byte %08x %u\n", floatBits(unit), (unsigned)WWUnitFloatToByte(unit));

                if (WWIsValidFloat(a))
                        fprintf(out, "scalar is_valid_float %08x 1\n", floatBits(a));
                else
                        fprintf(out, "scalar is_valid_float %08x 0\n", floatBits(a));
                fprintf(out, "scalar is_valid_float %08x %d\n", floatBits(bitsToFloat(0x7f800000u)), 0); // +inf
                fprintf(out, "scalar is_valid_float %08x %d\n", floatBits(bitsToFloat(0xffc00000u)), 0); // -nan

                UnsignedInt p2 = lcgWord() % 4096;
                fprintf(out, "scalar is_power_of_2 %u %d\n", p2, (int)WWIsPowerOf2(p2));
                fprintf(out, "scalar is_power_of_2 %u %d\n", 1u << (p2 % 31), (int)WWIsPowerOf2(1u << (p2 % 31)));

                fprintf(out, "scalar fast_is_float_positive %08x %d\n", floatBits(a), (int)WWFastIsFloatPositive(a));

                int ia = (int)(lcgWord() % 4001) - 2000;
                int ib = (int)(lcgWord() % 4001) - 2000;
                fprintf(out, "scalar clamp_int %d %d %d %d\n", ia, ib, ia + 1, WWClampInt(ia, ib, ia + 1));
        }
}

static void generateMathVectorsGolden( FILE *out )
{
        fprintf(out, "# WWMath vector golden vectors (parity level P0, IEEE-754 exact subset).\n");
        fprintf(out, "# Generated by tools/differential/gen_golden.cpp, transcribed from\n");
        fprintf(out, "# Core/Libraries/Source/WWVegas/WWMath/vector{2,3,4}.h @ 571a7c5.\n");
        fprintf(out, "# All float fields are bit patterns in hex; operands are finite.\n");
        fprintf(out, "# Regenerate with tools/differential/regenerate.sh; never edit by hand.\n");

        resetLcg(0x0badf00du);

        for (int i = 0; i < 250; i++)
        {
                // Two operand vectors and two scalars per round; alternate
                // raw bit patterns with exact quarter steps.
                Vector2 a2((i & 1) ? finiteFloat() : stepFloat(), (i & 2) ? finiteFloat() : stepFloat());
                Vector2 b2((i & 4) ? finiteFloat() : stepFloat(), (i & 8) ? finiteFloat() : stepFloat());
                float k = finiteFloat();
                float t = unitFloat();

                Vector2 r2;
                r2 = a2 + b2;
                fprintf(out, "v2 add %08x %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), floatBits(r2.X), floatBits(r2.Y));
                r2 = a2 - b2;
                fprintf(out, "v2 sub %08x %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), floatBits(r2.X), floatBits(r2.Y));
                r2 = a2 * k;
                fprintf(out, "v2 mul %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(k), floatBits(r2.X), floatBits(r2.Y));
                r2 = k * a2;
                fprintf(out, "v2 rmul %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(k), floatBits(r2.X), floatBits(r2.Y));
                r2 = a2 / k;
                fprintf(out, "v2 div %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(k), floatBits(r2.X), floatBits(r2.Y));
                r2 = -a2;
                fprintf(out, "v2 neg %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(r2.X), floatBits(r2.Y));
                fprintf(out, "v2 dot %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), floatBits(a2 * b2));
                fprintf(out, "v2 perp_dot %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), floatBits(Vector2::Perp_Dot_Product(a2, b2)));
                fprintf(out, "v2 length %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(a2.Length()));
                fprintf(out, "v2 length2 %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(a2.Length2()));

                r2 = a2; r2.Normalize();
                fprintf(out, "v2 normalize %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(r2.X), floatBits(r2.Y));
                r2 = Normalize(a2); // the / oolen quirk
                fprintf(out, "v2 normalize_free %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(r2.X), floatBits(r2.Y));

                fprintf(out, "v2 distance %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), floatBits(Vector2::Distance(a2, b2)));
                fprintf(out, "v2 quick_distance %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), floatBits(Vector2::Quick_Distance(a2, b2)));

                Vector2::Lerp(a2, b2, t, &r2);
                fprintf(out, "v2 lerp %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), floatBits(t), floatBits(r2.X), floatBits(r2.Y));

                r2 = a2; r2.Update_Min(b2);
                fprintf(out, "v2 update_min %08x %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), floatBits(r2.X), floatBits(r2.Y));
                r2 = a2; r2.Update_Max(b2);
                fprintf(out, "v2 update_max %08x %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), floatBits(r2.X), floatBits(r2.Y));

                float sx = stepFloat(), sy = stepFloat();
                r2 = a2; r2.Scale(sx, sy);
                fprintf(out, "v2 scale %08x %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(sx), floatBits(sy), floatBits(r2.X), floatBits(r2.Y));
                r2 = a2; r2.Scale(b2);
                fprintf(out, "v2 scale_v %08x %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), floatBits(r2.X), floatBits(r2.Y));

                float s = stepFloat(), c = stepFloat();
                r2 = a2; r2.Rotate(s, c);
                fprintf(out, "v2 rotate_sc %08x %08x %08x %08x %08x %08x\n", floatBits(a2.X), floatBits(a2.Y), floatBits(s), floatBits(c), floatBits(r2.X), floatBits(r2.Y));

                int eps_hit = Equal_Within_Epsilon(a2, b2, WWMATH_EPSILON);
                fprintf(out, "v2 equal_eps %08x %08x %08x %08x %d\n", floatBits(a2.X), floatBits(a2.Y), floatBits(b2.X), floatBits(b2.Y), eps_hit);

                // Vector3
                Vector3 a3((i & 1) ? finiteFloat() : stepFloat(), (i & 2) ? finiteFloat() : stepFloat(), (i & 4) ? finiteFloat() : stepFloat());
                Vector3 b3((i & 8) ? finiteFloat() : stepFloat(), (i & 16) ? finiteFloat() : stepFloat(), (i & 32) ? finiteFloat() : stepFloat());
                Vector3 r3;

                r3 = a3 + b3;
                fprintf(out, "v3 add %08x %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                r3 = a3 - b3;
                fprintf(out, "v3 sub %08x %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                r3 = a3 * k;
                fprintf(out, "v3 mul %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(k), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                r3 = a3 / k;
                fprintf(out, "v3 div %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(k), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                r3 = -a3;
                fprintf(out, "v3 neg %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                fprintf(out, "v3 dot %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(a3 * b3));

                Vector3::Cross_Product(a3, b3, &r3);
                fprintf(out, "v3 cross %08x %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                Vector3::Normalized_Cross_Product(a3, b3, &r3);
                fprintf(out, "v3 normalized_cross %08x %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                fprintf(out, "v3 cross_x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(Vector3::Cross_Product_X(a3, b3)));
                fprintf(out, "v3 cross_y %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(Vector3::Cross_Product_Y(a3, b3)));
                fprintf(out, "v3 cross_z %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(Vector3::Cross_Product_Z(a3, b3)));

                fprintf(out, "v3 length %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(a3.Length()));
                fprintf(out, "v3 length2 %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(a3.Length2()));
                r3 = a3; r3.Normalize();
                fprintf(out, "v3 normalize %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                fprintf(out, "v3 quick_length %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(a3.Quick_Length()));
                fprintf(out, "v3 distance %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(Vector3::Distance(a3, b3)));
                fprintf(out, "v3 quick_distance %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(Vector3::Quick_Distance(a3, b3)));

                Vector3::Lerp(a3, b3, t, &r3);
                fprintf(out, "v3 lerp %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(t), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));

                r3 = a3; r3.Update_Min(b3);
                fprintf(out, "v3 update_min %08x %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                r3 = a3; r3.Update_Max(b3);
                fprintf(out, "v3 update_max %08x %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                r3 = a3; r3.Cap_Absolute_To(b3);
                fprintf(out, "v3 cap_absolute_to %08x %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                r3 = a3; r3.Scale(b3);
                fprintf(out, "v3 scale_v %08x %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));

                r3 = a3; r3.Rotate_X(s, c);
                fprintf(out, "v3 rotate_x %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(s), floatBits(c), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                r3 = a3; r3.Rotate_Y(s, c);
                fprintf(out, "v3 rotate_y %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(s), floatBits(c), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));
                r3 = a3; r3.Rotate_Z(s, c);
                fprintf(out, "v3 rotate_z %08x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(s), floatBits(c), floatBits(r3.X), floatBits(r3.Y), floatBits(r3.Z));

                fprintf(out, "v3 find_x_at_y %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(Vector3::Find_X_At_Y(a3.Y, a3, b3)));
                fprintf(out, "v3 find_y_at_x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(Vector3::Find_Y_At_X(a3.X, a3, b3)));
                fprintf(out, "v3 find_z_at_x %08x %08x %08x %08x %08x %08x %08x\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), floatBits(Vector3::Find_Z_At_X(a3.X, a3, b3)));

                int eq3 = Equal_Within_Epsilon(a3, b3, WWMATH_EPSILON);
                fprintf(out, "v3 equal_eps %08x %08x %08x %08x %08x %08x %d\n", floatBits(a3.X), floatBits(a3.Y), floatBits(a3.Z), floatBits(b3.X), floatBits(b3.Y), floatBits(b3.Z), eq3);

                // Colour packing: components in [0, 1] (domain of the original).
                Vector3 col(unitFloat(), unitFloat(), unitFloat());
                fprintf(out, "v3 convert_abgr %08x %08x %08x %08x\n", floatBits(col.X), floatBits(col.Y), floatBits(col.Z), (UnsignedInt)col.Convert_To_ABGR());
                fprintf(out, "v3 convert_argb %08x %08x %08x %08x\n", floatBits(col.X), floatBits(col.Y), floatBits(col.Z), (UnsignedInt)col.Convert_To_ARGB());

                // Vector4
                Vector4 a4((i & 1) ? finiteFloat() : stepFloat(), (i & 2) ? finiteFloat() : stepFloat(),
                           (i & 4) ? finiteFloat() : stepFloat(), (i & 8) ? finiteFloat() : stepFloat());
                Vector4 b4((i & 16) ? finiteFloat() : stepFloat(), (i & 32) ? finiteFloat() : stepFloat(),
                           (i & 64) ? finiteFloat() : stepFloat(), (i & 128) ? finiteFloat() : stepFloat());
                Vector4 r4;

                r4 = a4 + b4;
                fprintf(out, "v4 add %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(b4.X), floatBits(b4.Y), floatBits(b4.Z), floatBits(b4.W),
                        floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));
                r4 = a4 - b4;
                fprintf(out, "v4 sub %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(b4.X), floatBits(b4.Y), floatBits(b4.Z), floatBits(b4.W),
                        floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));
                r4 = a4;
                r4 -= b4; // the W += quirk
                fprintf(out, "v4 sub_assign %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(b4.X), floatBits(b4.Y), floatBits(b4.Z), floatBits(b4.W),
                        floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));
                r4 = a4 * k;
                fprintf(out, "v4 mul %08x %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(k), floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));
                r4 = a4 / k;
                fprintf(out, "v4 div %08x %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(k), floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));
                r4 = -a4;
                fprintf(out, "v4 neg %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));
                fprintf(out, "v4 dot %08x %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(b4.X), floatBits(b4.Y), floatBits(b4.Z), floatBits(b4.W),
                        floatBits(a4 * b4));
                fprintf(out, "v4 length %08x %08x %08x %08x %08x\n", floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W), floatBits(a4.Length()));
                fprintf(out, "v4 length2 %08x %08x %08x %08x %08x\n", floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W), floatBits(a4.Length2()));
                r4 = a4; r4.Normalize();
                fprintf(out, "v4 normalize %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));
                r4 = Normalize(a4);
                fprintf(out, "v4 normalize_free %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));
                r4 = Lerp(a4, b4, t);
                fprintf(out, "v4 lerp_free %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(b4.X), floatBits(b4.Y), floatBits(b4.Z), floatBits(b4.W),
                        floatBits(t), floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));
                r4 = Vector4::Lerp(a4, b4, t);
                fprintf(out, "v4 lerp_static %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(b4.X), floatBits(b4.Y), floatBits(b4.Z), floatBits(b4.W),
                        floatBits(t), floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));
                Vector4::Lerp(a4, b4, t, &r4); // the X/W quirk
                fprintf(out, "v4 lerp_out %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x %08x\n",
                        floatBits(a4.X), floatBits(a4.Y), floatBits(a4.Z), floatBits(a4.W),
                        floatBits(b4.X), floatBits(b4.Y), floatBits(b4.Z), floatBits(b4.W),
                        floatBits(t), floatBits(r4.X), floatBits(r4.Y), floatBits(r4.Z), floatBits(r4.W));

                // Integer vectors
                Vector2i i2a((int)(lcgWord() % 100) - 50, (int)(lcgWord() % 100) - 50);
                Vector2i i2b((int)(lcgWord() % 100) - 50, (int)(lcgWord() % 100) - 50);
                fprintf(out, "vi2 eq %d %d %d %d %d\n", i2a.I, i2a.J, i2b.I, i2b.J, (int)(i2a == i2b));
                Vector3i i3a((int)(lcgWord() % 100) - 50, (int)(lcgWord() % 100) - 50, (int)(lcgWord() % 100) - 50);
                Vector3i i3b((int)(lcgWord() % 100) - 50, (int)(lcgWord() % 100) - 50, (int)(lcgWord() % 100) - 50);
                fprintf(out, "vi3 eq %d %d %d %d %d %d %d\n", i3a.I, i3a.J, i3a.K, i3b.I, i3b.J, i3b.K, (int)(i3a == i3b));
        }
}

// ---------------------------------------------------------------------------
// Matrix3 / Quaternion golden vectors.
// ---------------------------------------------------------------------------

static void writeM3In( FILE *out, const Matrix3x3 &m )
{
        fprintf(out, "%08x %08x %08x %08x %08x %08x %08x %08x %08x",
                floatBits(m[0][0]), floatBits(m[0][1]), floatBits(m[0][2]),
                floatBits(m[1][0]), floatBits(m[1][1]), floatBits(m[1][2]),
                floatBits(m[2][0]), floatBits(m[2][1]), floatBits(m[2][2]));
}

static void writeV3( FILE *out, const Vector3 &v )
{
        fprintf(out, "%08x %08x %08x", floatBits(v.X), floatBits(v.Y), floatBits(v.Z));
}

static void writeQuat( FILE *out, const Quaternion &q )
{
        fprintf(out, "%08x %08x %08x %08x", floatBits(q.X), floatBits(q.Y), floatBits(q.Z), floatBits(q.W));
}

static void generateMatrixQuatGolden( FILE *out )
{
        fprintf(out, "# WWMath Matrix3x3 / Quaternion golden vectors (parity level P0).\n");
        fprintf(out, "# Generated by tools/differential/gen_golden.cpp, transcribed from\n");
        fprintf(out, "# Core/Libraries/Source/WWVegas/WWMath/matrix3.{h,cpp} and quat.{h,cpp}\n");
        fprintf(out, "# @ 571a7c5 (deterministic branches; trig via the pinned GameMath).\n");
        fprintf(out, "# Regenerate with tools/differential/regenerate.sh; never edit by hand.\n");

        WWMathInit(); // the Fast_* tables, exactly like engine startup

        resetLcg(0xfeedfaceu);

        // Fixed axes: exact unit axes, the normalized diagonals, and a few
        // non-unit axes (the original does not normalize either).
        Vector3 axes[] = {
                Vector3(1.0f, 0.0f, 0.0f),
                Vector3(0.0f, 1.0f, 0.0f),
                Vector3(0.0f, 0.0f, 1.0f),
                Vector3(0.0f, -1.0f, 0.0f),
                Vector3(1.0f, 1.0f, 0.0f),
                Vector3(0.0f, 1.0f, 1.0f),
                Vector3(1.0f, 1.0f, 1.0f),
                Vector3(-1.0f, 2.0f, -0.5f),
        };
        const int NUM_AXES = sizeof(axes) / sizeof(axes[0]);

        float angles[] = {
                0.0f, 0.25f, -0.25f, 1.0f, -1.0f, 1.5f, -2.5f,
                3.14159265f, 1.5707963f, -3.14159265f, 6.2831853f,
                100.0f, -100.0f, 12345.6f,
        };
        const int NUM_ANGLES = sizeof(angles) / sizeof(angles[0]);

        for (int a = 0; a < NUM_AXES; a++)
        {
                for (int t = 0; t < NUM_ANGLES; t++)
                {
                        const Vector3 &axis = axes[a];
                        float angle = angles[t];

                        // Matrix3x3(axis, angle)
                        Matrix3x3 m(axis, angle);
                        fprintf(out, "m3 from_axis_angle ");
                        writeV3(out, axis);
                        fprintf(out, " %08x ", floatBits(angle));
                        writeM3In(out, m);
                        fprintf(out, "\n");

                        // Matrix3x3(axis, s, c)
                        Matrix3x3 msc(axis, WWSinf(angle), WWCosf(angle));
                        fprintf(out, "m3 from_axis_angle_sc ");
                        writeV3(out, axis);
                        fprintf(out, " %08x %08x ", floatBits(WWSinf(angle)), floatBits(WWCosf(angle)));
                        writeM3In(out, msc);
                        fprintf(out, "\n");

                        // Quaternion(axis, angle)
                        Quaternion q(axis, angle);
                        fprintf(out, "quat from_axis_angle ");
                        writeV3(out, axis);
                        fprintf(out, " %08x ", floatBits(angle));
                        writeQuat(out, q);
                        fprintf(out, "\n");

                        // quaternion -> matrix and back
                        Matrix3x3 qm(q);
                        fprintf(out, "m3 from_quaternion ");
                        writeQuat(out, q);
                        fprintf(out, " ");
                        writeM3In(out, qm);
                        fprintf(out, "\n");

                        Quaternion bq = Build_Quaternion(m);
                        fprintf(out, "quat build_quaternion ");
                        writeM3In(out, m);
                        fprintf(out, " ");
                        writeQuat(out, bq);
                        fprintf(out, "\n");

                        // in-place rotations on the axis-angle matrix
                        Matrix3x3 rm = m;
                        rm.Rotate_X(angle);
                        fprintf(out, "m3 rotate_x ");
                        writeM3In(out, m);
                        fprintf(out, " %08x ", floatBits(angle));
                        writeM3In(out, rm);
                        fprintf(out, "\n");

                        rm = m;
                        rm.Rotate_Y(angle);
                        fprintf(out, "m3 rotate_y ");
                        writeM3In(out, m);
                        fprintf(out, " %08x ", floatBits(angle));
                        writeM3In(out, rm);
                        fprintf(out, "\n");

                        rm = m;
                        rm.Rotate_Z(angle);
                        fprintf(out, "m3 rotate_z ");
                        writeM3In(out, m);
                        fprintf(out, " %08x ", floatBits(angle));
                        writeM3In(out, rm);
                        fprintf(out, "\n");

                        // axis_to_quat (normalizes the axis)
                        Quaternion aq = Axis_To_Quat(axis, angle);
                        fprintf(out, "quat axis_to_quat ");
                        writeV3(out, axis);
                        fprintf(out, " %08x ", floatBits(angle));
                        writeQuat(out, aq);
                        fprintf(out, "\n");
                }
        }

        // Raw matrices from the LCG (both well-conditioned and wild),
        // exercising the non-trig code paths.
        for (int i = 0; i < 220; i++)
        {
                Matrix3x3 a, b;
                for (int r = 0; r < 3; r++)
                {
                        for (int c = 0; c < 3; c++)
                        {
                                if (i & 1)
                                {
                                        a[r][c] = finiteFloat();
                                        b[r][c] = stepFloat();
                                }
                                else
                                {
                                        a[r][c] = stepFloat();
                                        b[r][c] = finiteFloat();
                                }
                        }
                }
                float k = finiteFloat();
                Vector3 v(finiteFloat(), finiteFloat(), finiteFloat());

                fprintf(out, "m3 transpose ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeM3In(out, a.Transpose());
                fprintf(out, "\n");

                fprintf(out, "m3 inverse ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeM3In(out, a.Inverse());
                fprintf(out, "\n");

                fprintf(out, "m3 determinant ");
                writeM3In(out, a);
                fprintf(out, " %08x\n", floatBits(a.Determinant()));

                fprintf(out, "m3 get_x_rotation ");
                writeM3In(out, a);
                fprintf(out, " %08x\n", floatBits(a.Get_X_Rotation()));
                fprintf(out, "m3 get_y_rotation ");
                writeM3In(out, a);
                fprintf(out, " %08x\n", floatBits(a.Get_Y_Rotation()));
                fprintf(out, "m3 get_z_rotation ");
                writeM3In(out, a);
                fprintf(out, " %08x\n", floatBits(a.Get_Z_Rotation()));

                fprintf(out, "m3 get_x_vector ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeV3(out, a.Get_X_Vector());
                fprintf(out, "\n");

                fprintf(out, "m3 mul ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeM3In(out, b);
                fprintf(out, " ");
                writeM3In(out, a * b);
                fprintf(out, "\n");

                Matrix3x3 res;
                Matrix3x3::Multiply(a, b, &res);
                fprintf(out, "m3 multiply_static ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeM3In(out, b);
                fprintf(out, " ");
                writeM3In(out, res);
                fprintf(out, "\n");

                fprintf(out, "m3 mul_scalar ");
                writeM3In(out, a);
                fprintf(out, " %08x ", floatBits(k));
                writeM3In(out, a * k);
                fprintf(out, "\n");

                fprintf(out, "m3 div_scalar ");
                writeM3In(out, a);
                fprintf(out, " %08x ", floatBits(k));
                writeM3In(out, a / k);
                fprintf(out, "\n");

                Matrix3x3 sum = a; // operator+ via components
                {
                        Matrix3x3 s2(
                                Vector3(a[0][0] + b[0][0], a[0][1] + b[0][1], a[0][2] + b[0][2]),
                                Vector3(a[1][0] + b[1][0], a[1][1] + b[1][1], a[1][2] + b[1][2]),
                                Vector3(a[2][0] + b[2][0], a[2][1] + b[2][1], a[2][2] + b[2][2]));
                        sum = s2;
                }
                fprintf(out, "m3 add ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeM3In(out, b);
                fprintf(out, " ");
                writeM3In(out, sum);
                fprintf(out, "\n");

                {
                        Matrix3x3 d2(
                                Vector3(a[0][0] - b[0][0], a[0][1] - b[0][1], a[0][2] - b[0][2]),
                                Vector3(a[1][0] - b[1][0], a[1][1] - b[1][1], a[1][2] - b[1][2]),
                                Vector3(a[2][0] - b[2][0], a[2][1] - b[2][1], a[2][2] - b[2][2]));
                        fprintf(out, "m3 sub ");
                        writeM3In(out, a);
                        fprintf(out, " ");
                        writeM3In(out, b);
                        fprintf(out, " ");
                        writeM3In(out, d2);
                        fprintf(out, "\n");
                }

                fprintf(out, "m3 neg ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeM3In(out, Matrix3x3(-a[0], -a[1], -a[2]));
                fprintf(out, "\n");

                fprintf(out, "m3 mul_vector ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeV3(out, v);
                fprintf(out, " ");
                writeV3(out, a * v);
                fprintf(out, "\n");

                Vector3 rv;
                Matrix3x3::Rotate_Vector(a, v, &rv);
                fprintf(out, "m3 rotate_vector ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeV3(out, v);
                fprintf(out, " ");
                writeV3(out, rv);
                fprintf(out, "\n");

                Matrix3x3::Transpose_Rotate_Vector(a, v, &rv);
                fprintf(out, "m3 transpose_rotate_vector ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeV3(out, v);
                fprintf(out, " ");
                writeV3(out, rv);
                fprintf(out, "\n");

                fprintf(out, "m3 is_orthogonal ");
                writeM3In(out, a);
                fprintf(out, " %d\n", a.Is_Orthogonal());

                Matrix3x3 ro = a;
                ro.Re_Orthogonalize();
                fprintf(out, "m3 re_orthogonalize ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeM3In(out, ro);
                fprintf(out, "\n");

                Vector3 ext(finiteFloat(), finiteFloat(), finiteFloat());
                Vector3 newext;
                a.Rotate_AABox_Extent(ext, &newext);
                fprintf(out, "m3 rotate_aabox_extent ");
                writeM3In(out, a);
                fprintf(out, " ");
                writeV3(out, ext);
                fprintf(out, " ");
                writeV3(out, newext);
                fprintf(out, "\n");

                // Quaternion algebra on raw components.
                Quaternion qa(a[0][0], a[0][1], a[0][2], a[0][2]);
                Quaternion qb(b[1][0], b[1][1], b[1][2], b[2][2]);

                fprintf(out, "quat mul ");
                writeQuat(out, qa);
                fprintf(out, " ");
                writeQuat(out, qb);
                fprintf(out, " ");
                writeQuat(out, qa * qb);
                fprintf(out, "\n");

                fprintf(out, "quat div ");
                writeQuat(out, qa);
                fprintf(out, " ");
                writeQuat(out, qb);
                fprintf(out, " ");
                writeQuat(out, qa / qb);
                fprintf(out, "\n");

                fprintf(out, "quat inverse ");
                writeQuat(out, qa);
                fprintf(out, " ");
                writeQuat(out, Inverse(qa));
                fprintf(out, "\n");

                Quaternion nq = qa;
                nq.Normalize();
                fprintf(out, "quat normalize ");
                writeQuat(out, qa);
                fprintf(out, " ");
                writeQuat(out, nq);
                fprintf(out, "\n");

                fprintf(out, "quat normalize_free ");
                writeQuat(out, qa);
                fprintf(out, " ");
                writeQuat(out, Normalize(qa));
                fprintf(out, "\n");

                Quaternion cq = qa;
                cq.Make_Closest(qb);
                fprintf(out, "quat make_closest ");
                writeQuat(out, qa);
                fprintf(out, " ");
                writeQuat(out, qb);
                fprintf(out, " ");
                writeQuat(out, cq);
                fprintf(out, "\n");

                fprintf(out, "quat rotate_vector ");
                writeQuat(out, qa);
                fprintf(out, " ");
                writeV3(out, v);
                fprintf(out, " ");
                writeV3(out, qa.Rotate_Vector(v));
                fprintf(out, "\n");

                // slerps across the interpolation parameter
                float alphas[] = { 0.0f, 0.1f, 0.25f, 0.5f, 0.75f, 0.9f, 1.0f };
                for (int t = 0; t < 7; t++)
                {
                        Quaternion sr;
                        Slerp(sr, qa, qb, alphas[t]);
                        fprintf(out, "quat slerp ");
                        writeQuat(out, qa);
                        fprintf(out, " ");
                        writeQuat(out, qb);
                        fprintf(out, " %08x ", floatBits(alphas[t]));
                        writeQuat(out, sr);
                        fprintf(out, "\n");

                        Quaternion fr;
                        Fast_Slerp(fr, qa, qb, alphas[t]);
                        fprintf(out, "quat fast_slerp ");
                        writeQuat(out, qa);
                        fprintf(out, " ");
                        writeQuat(out, qb);
                        fprintf(out, " %08x ", floatBits(alphas[t]));
                        writeQuat(out, fr);
                        fprintf(out, "\n");
                }

                SlerpInfoStruct info;
                Slerp_Setup(qa, qb, &info);
                Quaternion cs = Cached_Slerp(qa, qb, 0.4f, &info);
                fprintf(out, "quat slerp_setup %d %d %08x %08x\n",
                        info.Flip ? 1 : 0, info.Linear ? 1 : 0,
                        floatBits(info.Theta), floatBits(info.SinT));
                fprintf(out, "quat cached_slerp ");
                writeQuat(out, qa);
                fprintf(out, " ");
                writeQuat(out, qb);
                fprintf(out, " 3ecd353f "); // 0.4f
                writeQuat(out, cs);
                fprintf(out, "\n");

                // quaternion rotations about the coordinate axes
                Quaternion rx = qa;
                rx.Rotate_X(0.7f);
                fprintf(out, "quat rotate_x ");
                writeQuat(out, qa);
                fprintf(out, " %08x ", floatBits(0.7f));
                writeQuat(out, rx);
                fprintf(out, "\n");

                Quaternion ry = qa;
                ry.Rotate_Y(0.7f);
                fprintf(out, "quat rotate_y ");
                writeQuat(out, qa);
                fprintf(out, " %08x ", floatBits(0.7f));
                writeQuat(out, ry);
                fprintf(out, "\n");

                Quaternion rz = qa;
                rz.Rotate_Z(0.7f);
                fprintf(out, "quat rotate_z ");
                writeQuat(out, qa);
                fprintf(out, " %08x ", floatBits(0.7f));
                writeQuat(out, rz);
                fprintf(out, "\n");

                fprintf(out, "quat build_matrix3 ");
                writeQuat(out, qa);
                fprintf(out, " ");
                writeM3In(out, Build_Matrix3(qa));
                fprintf(out, "\n");
        }
}

int main( int argc, char **argv )
{
        if (argc != 7)
        {
                fprintf(stderr, "usage: %s <crc_out.txt> <rng_out.txt> <namekey_out.txt> <math_scalar_out.txt> <math_vectors_out.txt> <math_matrix_quat_out.txt>\n", argv[0]);
                return 1;
        }

        FILE *outs[6];
        const char *const names[6] = { argv[1], argv[2], argv[3], argv[4], argv[5], argv[6] };
        for (int i = 0; i < 6; i++)
        {
                outs[i] = fopen(names[i], "w");
                if (!outs[i])
                {
                        fprintf(stderr, "cannot open %s\n", names[i]);
                        for (int j = 0; j < i; j++)
                                fclose(outs[j]);
                        return 1;
                }
        }

        generateCrcGolden(outs[0]);
        generateRngGolden(outs[1]);
        generateNameKeyGolden(outs[2]);
        generateMathScalarGolden(outs[3]);
        generateMathVectorsGolden(outs[4]);
        generateMatrixQuatGolden(outs[5]);

        for (int i = 0; i < 6; i++)
                fclose(outs[i]);
        return 0;
}
