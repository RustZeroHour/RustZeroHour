// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing wrapper around the gen_golden.cpp transcription.
//
// The migration plan's §6 exit criterion requires the Rust foundation
// crates to be "no more than 20 % slower than the C++ baseline". This
// wrapper exposes the *same algorithms the golden vectors pin* (the
// verbatim transcription of crc.h, RandomValue.cpp, NameKeyGenerator.cpp
// and the WWMath headers at baseline 571a7c5) through a small C ABI that
// bench_baseline.cpp drives with the same corpora the Rust criterion
// benches use, so the numbers are directly comparable.
//
// Corpus fairness: the derived arrays (vectors, matrices, quaternions) are
// built ONCE per section by the `*_corpus_new` entry points (untimed,
// exactly like the Rust benches build theirs before the measured closure),
// using the identical construction formulas the Rust bench code uses, over
// the identical LCG float corpus the driver passes in. The timed `*_batch`
// entry points then run the same loop shapes the Rust closures run.
//
// The transcription's own main() is renamed out of the way; nothing in it
// is modified. Compiled with the same flags as the golden generator
// (-O2 -ffp-contract=off, plus the GameMath pin at link time).
//
// Temporary differential-testing infrastructure: removed at cut-over (M5).
//
// Copyright 2025 Electronic Arts Inc.
// (c) 2001-2003 Electronic Arts Inc.
// Rust port Copyright 2026 the RustZeroHour contributors.
//
// This program is free software; you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.

#define main gen_golden_generator_main
#include "gen_golden.cpp"
#undef main

#include <cmath>
#include <cstddef>

// ---------------------------------------------------------------------------
// Corpus containers: the derived arrays of the Rust benches, built with the
// identical formulas so both sides time the same work on the same values.
// ---------------------------------------------------------------------------

struct VecCorpus
{
        // float_corpus(256) chunks: inputs (85), others (254), inputs2 (128,
        // reversed pair), inputs4 (64, reversed pair)
        std::vector<Vector3> inputs;
        std::vector<Vector3> others;
        std::vector<Vector2> inputs2;
        std::vector<Vector2> others2;
};

struct MatCorpus
{
        std::vector<Matrix3x3> mats3;   // 21
        std::vector<Matrix3D> mats3d;   // 21
        std::vector<Matrix4x4> mats4;   // 21
        std::vector<Vector3> points;    // 21
};

struct QuatCorpus
{
        std::vector<Quaternion> quats;  // 16
        std::vector<float> alphas;      // 64 (only the first 16 are zipped)
};

struct TrigCorpus
{
        std::vector<float> angles;      // 256
        std::vector<float> unit;        // 256
};

extern "C" {

// --- crc ------------------------------------------------------------------

uint32_t bench_crc_update(const uint8_t *data, size_t len)
{
        CRC crc;
        crc.computeCRC(data, (Int)len);
        return crc.get();
}

// --- rng ------------------------------------------------------------------

// One batch, exactly like the Rust closure: seed, then 256 draws.
uint32_t bench_rng_raw_batch(uint32_t seed_value)
{
        UnsignedInt seed[6];
        seedRandom(seed_value, seed);
        uint32_t sink = 0;
        for (int i = 0; i < 256; ++i)
                sink += randomValue(seed);
        return sink;
}

int64_t bench_rng_random_int_batch(uint32_t seed_value)
{
        UnsignedInt seed[6];
        seedRandom(seed_value, seed);
        int64_t sink = 0;
        for (int i = 0; i < 256; ++i)
                sink += GetGameLogicRandomValue(seed, 0, 100);
        return sink;
}

float bench_rng_random_real_batch(uint32_t seed_value)
{
        UnsignedInt seed[6];
        seedRandom(seed_value, seed);
        float sink = 0.0f;
        for (int i = 0; i < 256; ++i)
                sink += GetGameLogicRandomValueReal(seed, 0.0f, 1.0f);
        return sink;
}

// --- namekey --------------------------------------------------------------

void *bench_namekey_create()
{
        return new NameKeyGenerator();
}

void *bench_namekey_create_prefilled(char **names, size_t count)
{
        NameKeyGenerator *keys = new NameKeyGenerator();
        for (size_t i = 0; i < count; ++i)
                keys->nameToKey(names[i]);
        return keys;
}

void bench_namekey_destroy(void *gen)
{
        delete (NameKeyGenerator *)gen;
}

// One batch: 512 intern calls on a generator the driver prepared (fresh
// for the miss path, pre-filled for the hit path) — matching the Rust
// iter_batched split of setup vs measurement.
size_t bench_namekey_intern_batch(void *gen, char **names, size_t count)
{
        NameKeyGenerator *keys = (NameKeyGenerator *)gen;
        UnsignedInt sink = 0;
        for (size_t i = 0; i < count; ++i)
                sink += keys->nameToKey(names[i]);
        return (size_t)sink;
}

// --- vector corpora --------------------------------------------------------

void *bench_vec_corpus_new(const float *xs, size_t count)
{
        VecCorpus *c = new VecCorpus();
        c->inputs.reserve(count / 3);
        for (size_t i = 0; i + 2 < count; i += 3)
                c->inputs.push_back(Vector3(xs[i], xs[i + 1], xs[i + 2] * 0.5f + 1.0f));
        // others[k] = (xs[k+2]+0.5, xs[k+1]*0.25, xs[k+1]) for k in 0..count-2
        c->others.reserve(count - 2);
        for (size_t k = 0; k + 2 < count; ++k)
                c->others.push_back(Vector3(xs[k + 2] + 0.5f, xs[k + 1] * 0.25f, xs[k + 1]));
        c->inputs2.reserve(count / 2);
        for (size_t i = 0; i + 1 < count; i += 2)
                c->inputs2.push_back(Vector2(xs[i] * 10.0f, xs[i + 1] * 10.0f));
        c->others2.reserve(c->inputs2.size());
        for (size_t i = c->inputs2.size(); i > 0; --i)
                c->others2.push_back(c->inputs2[i - 1]);
        return c;
}

float bench_v3_length_batch(void *corpus)
{
        VecCorpus *c = (VecCorpus *)corpus;
        float sink = 0.0f;
        for (const Vector3 &v : c->inputs)
                sink += v.Length();
        return sink;
}

float bench_v3_normalize_batch(void *corpus)
{
        VecCorpus *c = (VecCorpus *)corpus;
        float sink = 0.0f;
        for (const Vector3 &v0 : c->inputs)
        {
                Vector3 v = v0;
                v.Normalize();
                sink += v.X;
        }
        return sink;
}

float bench_v3_dot_batch(void *corpus)
{
        VecCorpus *c = (VecCorpus *)corpus;
        float sink = 0.0f;
        for (size_t i = 0; i < c->inputs.size(); ++i)
                sink += c->inputs[i] * c->others[i];
        return sink;
}

float bench_v3_cross_batch(void *corpus)
{
        VecCorpus *c = (VecCorpus *)corpus;
        float sink = 0.0f;
        for (size_t i = 0; i < c->inputs.size(); ++i)
        {
                Vector3 r;
                Vector3::Cross_Product(c->inputs[i], c->others[i], &r);
                sink += r.Z;
        }
        return sink;
}

float bench_v2_quick_distance_batch(void *corpus)
{
        VecCorpus *c = (VecCorpus *)corpus;
        float sink = 0.0f;
        for (size_t i = 0; i < c->inputs2.size(); ++i)
                sink += Vector2::Quick_Distance(c->inputs2[i], c->others2[i]);
        return sink;
}

void bench_vec_corpus_free(void *corpus) { delete (VecCorpus *)corpus; }

// --- matrix corpora --------------------------------------------------------

void *bench_mat_corpus_new(const float *xs, size_t count)
{
        MatCorpus *c = new MatCorpus();
        for (size_t i = 0; i + 2 < count; i += 3)
        {
                float c0 = xs[i], c1 = xs[i + 1], c2 = xs[i + 2];
                c->mats3d.push_back(Matrix3D(
                        Vector3(c0, c1, 0.0f),
                        Vector3(c1 * 0.5f + 0.1f, c2, c0),
                        Vector3(c2, 0.0f, c1),
                        Vector3(c0 * 5.0f, c1 * 5.0f, c2 * 5.0f)));
                c->mats3.push_back(Matrix3x3(
                        Vector3(c0, c1, c2),
                        Vector3(c1, c2, c0),
                        Vector3(c2, c0, c1)));
                c->points.push_back(Vector3(c0 * 32.0f, c1 * 32.0f, c2 * 32.0f));
        }
        for (const Matrix3D &m : c->mats3d)
        {
                Matrix4x4 m4;
                m4.Init(m);
                c->mats4.push_back(m4);
        }
        return c;
}

float bench_matrix3_multiply_batch(void *corpus)
{
        MatCorpus *c = (MatCorpus *)corpus;
        float sink = 0.0f;
        const size_t n = c->mats3.size();
        for (size_t i = 0; i < n; ++i)
        {
                Matrix3x3 res;
                Matrix3x3::Multiply(c->mats3[i], c->mats3[n - 1 - i], &res);
                sink += res.Row[0].X;
        }
        return sink;
}

float bench_matrix3d_multiply_batch(void *corpus)
{
        MatCorpus *c = (MatCorpus *)corpus;
        float sink = 0.0f;
        const size_t n = c->mats3d.size();
        for (size_t i = 0; i < n; ++i)
        {
                Matrix3D acc;
                acc.mul(c->mats3d[i], c->mats3d[n - 1 - i]);
                sink += acc.Row[0][0];
        }
        return sink;
}

float bench_matrix4_multiply_batch(void *corpus)
{
        MatCorpus *c = (MatCorpus *)corpus;
        float sink = 0.0f;
        const size_t n = c->mats4.size();
        for (size_t i = 0; i < n; ++i)
        {
                Matrix4x4 res = c->mats4[i] * c->mats4[n - 1 - i];
                sink += res.Row[0].X;
        }
        return sink;
}

float bench_matrix3d_transform_batch(void *corpus)
{
        MatCorpus *c = (MatCorpus *)corpus;
        float sink = 0.0f;
        for (size_t i = 0; i < c->mats3d.size(); ++i)
        {
                Vector3 out;
                Matrix3D::Transform_Vector(c->mats3d[i], c->points[i], &out);
                sink += out.X;
        }
        return sink;
}

uint32_t bench_matrix4_inverse_batch(void *corpus)
{
        MatCorpus *c = (MatCorpus *)corpus;
        uint32_t ok = 0;
        for (const Matrix4x4 &m : c->mats4)
        {
                Matrix4x4 inv;
                float det;
                if (Matrix4x4::Inverse(&inv, &det, &m) != nullptr)
                        ok++;
        }
        return ok;
}

void bench_mat_corpus_free(void *corpus) { delete (MatCorpus *)corpus; }

// --- quaternion + fast trig ------------------------------------------------

void bench_fast_tables_init() { WWMathInit(); }

void *bench_quat_corpus_new(const float *xs, size_t count)
{
        QuatCorpus *c = new QuatCorpus();
        for (size_t i = 0; i + 3 < count; i += 4)
        {
                Quaternion q(xs[i], xs[i + 1], xs[i + 2], xs[i + 3]);
                q.Normalize();
                c->quats.push_back(q);
        }
        c->alphas.reserve(count);
        for (size_t i = 0; i < count; ++i)
                c->alphas.push_back(xs[i] * 0.25f + 0.5f);
        return c;
}

float bench_quat_slerp_batch(void *corpus)
{
        QuatCorpus *c = (QuatCorpus *)corpus;
        float sink = 0.0f;
        const size_t n = c->quats.size();
        for (size_t i = 0; i < n; ++i)
        {
                Quaternion res;
                Slerp(res, c->quats[i], c->quats[n - 1 - i], c->alphas[i]);
                sink += res.W;
        }
        return sink;
}

float bench_quat_fast_slerp_batch(void *corpus)
{
        QuatCorpus *c = (QuatCorpus *)corpus;
        float sink = 0.0f;
        const size_t n = c->quats.size();
        for (size_t i = 0; i < n; ++i)
        {
                Quaternion res;
                Fast_Slerp(res, c->quats[i], c->quats[n - 1 - i], c->alphas[i]);
                sink += res.W;
        }
        return sink;
}

float bench_quat_build_matrix3_batch(void *corpus)
{
        QuatCorpus *c = (QuatCorpus *)corpus;
        float sink = 0.0f;
        for (const Quaternion &q : c->quats)
        {
                Matrix3x3 m(q);
                sink += m.Row[0].X;
        }
        return sink;
}

void bench_quat_corpus_free(void *corpus) { delete (QuatCorpus *)corpus; }

void *bench_trig_corpus_new(const float *xs, size_t count)
{
        TrigCorpus *c = new TrigCorpus();
        c->angles.reserve(count);
        c->unit.reserve(count);
        for (size_t i = 0; i < count; ++i)
        {
                c->angles.push_back(xs[i] * 3.14159265358979323846f);
                c->unit.push_back(xs[i] * 0.49f);
        }
        return c;
}

float bench_fast_sin_batch(void *corpus)
{
        TrigCorpus *c = (TrigCorpus *)corpus;
        float sink = 0.0f;
        for (float a : c->angles)
                sink += Fast_Sin(a);
        return sink;
}

float bench_fast_cos_batch(void *corpus)
{
        TrigCorpus *c = (TrigCorpus *)corpus;
        float sink = 0.0f;
        for (float a : c->angles)
                sink += Fast_Cos(a);
        return sink;
}

float bench_fast_acos_batch(void *corpus)
{
        TrigCorpus *c = (TrigCorpus *)corpus;
        float sink = 0.0f;
        for (float v : c->unit)
                sink += Fast_Acos(v);
        return sink;
}

void bench_trig_corpus_free(void *corpus) { delete (TrigCorpus *)corpus; }

} // extern "C"
