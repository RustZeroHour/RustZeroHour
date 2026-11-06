// SPDX-License-Identifier: GPL-3.0-or-later
//
// C++ baseline timing driver for the Rust migration's §6 exit criterion.
//
// Measures the P0-exact foundation algorithms (CRC, RNG, name-key
// interning, RefPack, the INI engine and the WWMath kernels) on the same
// corpora as the Rust criterion benches, and prints one line per benchmark:
//
//     bench <name> <ns_per_batch> <mb_per_s_or_dash>
//
// The `name` matches the criterion bench id (group/function), so a diff
// against `cargo bench` output is direct. Batch semantics match the Rust
// closures (see the comments in the wrappers): e.g. `rng/raw` times
// seed + 256 draws, `namekey/intern_new/x512` times 512 intern calls on a
// freshly built generator, and so on.
//
// Usage: bench_baseline <fixtures-dir>
//
// Temporary differential-testing infrastructure: removed at cut-over (M5).
// The measured results are recorded in docs/benchmarks.md.
//
// Rust port Copyright 2026 the RustZeroHour contributors.
//
// This program is free software; you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

// --- the wrappers -----------------------------------------------------------

extern "C" {
// gen_golden.cpp transcription
uint32_t bench_crc_update(const uint8_t *data, size_t len);
uint32_t bench_rng_raw_batch(uint32_t seed_value);
int64_t bench_rng_random_int_batch(uint32_t seed_value);
float bench_rng_random_real_batch(uint32_t seed_value);
void *bench_namekey_create();
void *bench_namekey_create_prefilled(char **names, size_t count);
void bench_namekey_destroy(void *gen);
size_t bench_namekey_intern_batch(void *gen, char **names, size_t count);
void *bench_vec_corpus_new(const float *xs, size_t count);
float bench_v3_length_batch(void *corpus);
float bench_v3_normalize_batch(void *corpus);
float bench_v3_dot_batch(void *corpus);
float bench_v3_cross_batch(void *corpus);
float bench_v2_quick_distance_batch(void *corpus);
void bench_vec_corpus_free(void *corpus);
void *bench_mat_corpus_new(const float *xs, size_t count);
float bench_matrix3_multiply_batch(void *corpus);
float bench_matrix3d_multiply_batch(void *corpus);
float bench_matrix4_multiply_batch(void *corpus);
float bench_matrix3d_transform_batch(void *corpus);
uint32_t bench_matrix4_inverse_batch(void *corpus);
void bench_mat_corpus_free(void *corpus);
void bench_fast_tables_init();
void *bench_quat_corpus_new(const float *xs, size_t count);
float bench_quat_slerp_batch(void *corpus);
float bench_quat_fast_slerp_batch(void *corpus);
float bench_quat_build_matrix3_batch(void *corpus);
void bench_quat_corpus_free(void *corpus);
void *bench_trig_corpus_new(const float *xs, size_t count);
float bench_fast_sin_batch(void *corpus);
float bench_fast_cos_batch(void *corpus);
float bench_fast_acos_batch(void *corpus);
void bench_trig_corpus_free(void *corpus);

// the real EAC codec
long bench_refpack_encode(const uint8_t *source, size_t len, uint8_t *dest, size_t dest_cap);
long bench_refpack_decode(const uint8_t *packed, size_t packed_len, uint8_t *dest, size_t ulen);
int bench_refpack_ref_size(const uint8_t *packed);

// the INI transcription
uint32_t bench_ini_parse_scenario(
        const uint8_t *armor, size_t armor_len,
        const uint8_t *loco, size_t loco_len,
        const uint8_t *weapon, size_t weapon_len,
        const uint8_t *over, size_t over_len,
        const uint8_t *edge, size_t edge_len);
uint32_t bench_ini_parse_synthetic(const uint8_t *data, size_t len);

// the CSF transcription (Babylon writer + runtime reader)
void *bench_csf_db_new();
void bench_csf_db_free(void *db);
std::uint64_t bench_csf_write(void *db);
long bench_csf_materialize(void *db, uint8_t *out, size_t cap);
std::uint32_t bench_csf_parse(const uint8_t *data, size_t len);
std::uint32_t bench_csf_game_text(void *db);

// the TGA transcription (TARGA.cpp: load + save/encode)
void *bench_tga_targa_new(int depth);
void bench_tga_targa_free(void *handle);
long bench_tga_write(void *handle);
long bench_tga_encode(void *handle);
long bench_tga_materialize(void *handle, int compress, uint8_t *out, size_t cap);
std::uint32_t bench_tga_parse(const uint8_t *data, size_t len);

// the chunk transcription (chunkio.cpp: ChunkSaveClass + ChunkLoadClass)
long bench_chunk_write();
long bench_chunk_materialize(uint8_t *out, size_t cap);
std::uint64_t bench_chunk_walk(const uint8_t *data, size_t len);
long bench_mesh_materialize(uint8_t *out, size_t cap);
std::uint64_t bench_mesh_load(const uint8_t *data, size_t len);
long bench_htree_materialize(uint8_t *out, size_t cap);
std::uint64_t bench_htree_load(const uint8_t *data, size_t len);
long bench_hanim_materialize(uint8_t *out, size_t cap);
std::uint64_t bench_hanim_load(const uint8_t *data, size_t len);
void *bench_hanim_anim_new(const uint8_t *data, size_t len);
void bench_hanim_anim_free(void *handle);
std::uint64_t bench_hanim_xforms(void *handle);
long bench_hcanim_materialize(uint8_t *out, size_t cap);
long bench_hcanim_ad_materialize(uint8_t *out, size_t cap);
std::uint64_t bench_hcanim_load(const uint8_t *data, size_t len);
void *bench_hcanim_anim_new(const uint8_t *data, size_t len);
void bench_hcanim_anim_free(void *handle);
std::uint64_t bench_hcanim_xforms(void *handle);
long bench_hlod_materialize(uint8_t *out, size_t cap);
std::uint64_t bench_hlod_load(const uint8_t *data, size_t len);
long bench_dds_parse_materialize(uint8_t *out, size_t cap);
long bench_mapchunk_write();
long bench_mapchunk_materialize(uint8_t *out, size_t cap);
std::uint64_t bench_mapchunk_parse(const uint8_t *data, size_t len);
long bench_mapheightmap_materialize(uint8_t *out, size_t cap);
long bench_mapheightmap_materialize_v1(uint8_t *out, size_t cap);
std::uint64_t bench_mapheightmap_parse(const uint8_t *data, size_t len);
std::uint64_t bench_mapheightmap_parse_v1(const uint8_t *data, size_t len);
unsigned bench_dds_parse(const uint8_t *data, size_t len);
long bench_dds_convert_materialize(uint8_t *out, size_t cap);
unsigned bench_dds_convert(const uint8_t *data, size_t len, uint8_t *dest, size_t dest_cap);
}

// --- corpora (identical to the Rust benches) ---------------------------------

static uint32_t lcg(uint32_t &state)
{
        state = state * 1664525u + 1013904223u;
        return state;
}

// The byte corpus of the Rust crc benches: two draws per byte.
static std::vector<uint8_t> byte_corpus(size_t len)
{
        std::vector<uint8_t> data;
        data.reserve(len);
        uint32_t state = 0xc0ffee11u;
        for (size_t i = 0; i < len; ++i)
        {
                uint32_t hi = lcg(state);
                uint32_t lo = lcg(state);
                data.push_back((uint8_t)((hi >> 24) ^ lo));
        }
        return data;
}

// The float corpus of the Rust math benches: (-2, 2).
static std::vector<float> float_corpus(size_t count)
{
        std::vector<float> out;
        out.reserve(count);
        uint32_t state = 0xc0ffee11u;
        for (size_t i = 0; i < count; ++i)
        {
                uint32_t bits = lcg(state);
                float unit = (float)(bits >> 8) / 16777216.0f;
                out.push_back(unit * 4.0f - 2.0f);
        }
        return out;
}

// The RefPack corpora: zeros / text / random at 64 KiB.
struct RefCorpora
{
        std::vector<uint8_t> zeros;
        std::vector<uint8_t> text;
        std::vector<uint8_t> random;
};

static RefCorpora ref_corpora()
{
        const size_t size = 65536;
        RefCorpora c;
        c.zeros.assign(size, 0);

        c.text.reserve(size);
        uint32_t line = 0;
        char buf[128];
        while (c.text.size() < size)
        {
                int n = snprintf(buf, sizeof(buf),
                        "  Damage = %u\n  Radius = 25.0\n  Weapon = SmallWeapon_%05u\n  End\n",
                        line, line);
                c.text.insert(c.text.end(), buf, buf + n);
                line++;
        }
        c.text.resize(size);

        c.random.reserve(size);
        uint32_t state = 0xc0ffee11u;
        for (size_t i = 0; i < size; ++i)
                c.random.push_back((uint8_t)lcg(state));
        return c;
}

// The name corpus of the Rust namekey benches.
static std::vector<std::string> name_corpus()
{
        std::vector<std::string> names;
        names.reserve(512);
        char buf[64];
        for (uint32_t i = 0; i < 512; ++i)
        {
                snprintf(buf, sizeof(buf), "AmericaVehicleDozer_%05u", i);
                names.push_back(buf);
        }
        return names;
}

// The synthetic Armor corpus of the Rust `parse/synthetic_armor_x512`
// bench: 512 blocks, 20 damage coefficients each, one comment line, every
// 16th block blank — byte-identical to the Rust generator.
static std::vector<uint8_t> synthetic_armor_ini()
{
        static const char *const damage_names[] = {
                "EXPLOSION", "CRUSH", "ARMOR_PIERCING", "SMALL_ARMS", "GATTLING",
                "RADIATION", "FLAME", "LASER", "SNIPER", "POISON", "HEALING",
                "UNRESISTABLE", "WATER", "DEPLOY", "SURRENDER", "HACK",
                "KILL_PILOT", "PENALTY", "FALLING", "MELEE",
        };
        std::string text;
        text.reserve(300000);
        char buf[96];
        for (uint32_t index = 0; index < 512; ++index)
        {
                snprintf(buf, sizeof(buf), "Armor TestArmor%05u\n", index);
                text += buf;
                if (index % 16 != 0)
                {
                        for (size_t slot = 0; slot < 20; ++slot)
                        {
                                int percent = (int)((index + slot * 7) % 250) + 1;
                                snprintf(buf, sizeof(buf), "  Armor = %s %d%%\n", damage_names[slot], percent);
                                text += buf;
                        }
                        text += "  ; a comment line the tokenizer must skip\n";
                }
                text += "End\n\n";
        }
        return std::vector<uint8_t>(text.begin(), text.end());
}

// --- timing framework ---------------------------------------------------------

// Times one batch body: `batches` runs, each preceded by `setup` (untimed,
// matching the Rust iter_batched semantics). Returns the MEDIAN batch time
// in nanoseconds. The batch count is high enough for a stable median on a
// noisy developer machine; the warmup run also touches every input.
template <typename Setup, typename Body>
static double measure_ns(Setup setup, Body body, int batches = 31)
{
        volatile std::uint64_t sink = 0;
        // warmup + correctness touch
        setup();
        sink = sink + (std::uint64_t)body();

        std::vector<double> times;
        times.reserve((size_t)batches);
        for (int i = 0; i < batches; ++i)
        {
                setup();
                auto t0 = std::chrono::steady_clock::now();
                sink = sink + (std::uint64_t)body();
                auto t1 = std::chrono::steady_clock::now();
                double ns = std::chrono::duration<double, std::nano>(t1 - t0).count();
                times.push_back(ns);
        }
        std::sort(times.begin(), times.end());
        (void)sink;
        return times[times.size() / 2];
}

// Context form: `setup` builds a context (a fresh generator, a corpus)
// that `body` receives, and `teardown` disposes of it. Only `body` is
// timed — the Rust iter_batched semantics.
template <typename Setup, typename Body, typename Teardown>
static double measure_ns_ctx(Setup setup, Body body, Teardown teardown, int batches = 31)
{
        volatile std::uint64_t sink = 0;
        {
                void *ctx = setup();
                sink = sink + (std::uint64_t)body(ctx);
                teardown(ctx);
        }

        std::vector<double> times;
        times.reserve((size_t)batches);
        for (int i = 0; i < batches; ++i)
        {
                void *ctx = setup();
                auto t0 = std::chrono::steady_clock::now();
                sink = sink + (std::uint64_t)body(ctx);
                auto t1 = std::chrono::steady_clock::now();
                teardown(ctx);
                double ns = std::chrono::duration<double, std::nano>(t1 - t0).count();
                times.push_back(ns);
        }
        std::sort(times.begin(), times.end());
        (void)sink;
        return times[times.size() / 2];
}

static void report(const char *name, double ns, double bytes = -1.0)
{
        if (bytes > 0.0)
        {
                double mbps = bytes / (ns / 1e9) / (1024.0 * 1024.0);
                printf("bench %s %.2f %.1f\n", name, ns, mbps);
        }
        else
        {
                printf("bench %s %.2f -\n", name, ns);
        }
}

static std::vector<uint8_t> read_file(const std::string &path)
{
        FILE *f = fopen(path.c_str(), "rb");
        if (!f)
        {
                fprintf(stderr, "cannot open %s\n", path.c_str());
                exit(1);
        }
        fseek(f, 0, SEEK_END);
        long size = ftell(f);
        fseek(f, 0, SEEK_SET);
        std::vector<uint8_t> data((size_t)size);
        if (size > 0 && fread(data.data(), 1, (size_t)size, f) != (size_t)size)
        {
                fprintf(stderr, "cannot read %s\n", path.c_str());
                exit(1);
        }
        fclose(f);
        return data;
}

int main(int argc, char **argv)
{
        std::string fixturesDir = argc > 1 ? argv[1] : "crates/rzh-ini/tests/fixtures";

        // --- crc -------------------------------------------------------------
        for (size_t size : {64u, 4096u, 65536u})
        {
                std::vector<uint8_t> data = byte_corpus(size);
                double ns = measure_ns([] {}, [&] { return bench_crc_update(data.data(), data.size()); });
                char name[64];
                snprintf(name, sizeof(name), "crc/update/%zu", size);
                report(name, ns, (double)size);
        }

        // --- rng ---------------------------------------------------------------
        report("rng/raw", measure_ns([] {}, [] { return bench_rng_raw_batch(42); }));
        report("rng/random_int/0_100", measure_ns([] {}, [] { return bench_rng_random_int_batch(42); }));
        report("rng/random_real/0_1", measure_ns([] {}, [] { return bench_rng_random_real_batch(42); }));

        // --- namekey ------------------------------------------------------------
        {
                std::vector<std::string> names_str = name_corpus();
                std::vector<char *> names;
                for (std::string &s : names_str)
                        names.push_back(&s[0]);
                const size_t count = names.size();

                // miss path: a fresh generator per batch, setup untimed
                report("namekey/intern_new/x512", measure_ns_ctx(
                        [&] { return bench_namekey_create(); },
                        [&](void *gen) { return bench_namekey_intern_batch(gen, names.data(), count); },
                        [](void *gen) { bench_namekey_destroy(gen); }));

                // hit path: a pre-filled generator per batch, setup untimed
                report("namekey/intern_hit/x512", measure_ns_ctx(
                        [&] { return bench_namekey_create_prefilled(names.data(), count); },
                        [&](void *gen) { return bench_namekey_intern_batch(gen, names.data(), count); },
                        [](void *gen) { bench_namekey_destroy(gen); }));
        }

        // --- refpack -------------------------------------------------------------
        {
                RefCorpora c = ref_corpora();
                struct Item
                {
                        const char *tag;
                        std::vector<uint8_t> *raw;
                };
                // decode: pack once (untimed), then decode per batch
                for (Item item : {Item{"zeros", &c.zeros}, Item{"text", &c.text}, Item{"random", &c.random}})
                {
                        std::vector<uint8_t> dest(item.raw->size() + 4096, 0);
                        long plen = bench_refpack_encode(item.raw->data(), item.raw->size(), dest.data(), dest.size());
                        std::vector<uint8_t> packed(dest.begin(), dest.begin() + plen);
                        std::vector<uint8_t> out(item.raw->size(), 0);

                        char name[64];
                        snprintf(name, sizeof(name), "refpack/decode/%s", item.tag);
                        double ns = measure_ns([] {}, [&] {
                                return bench_refpack_decode(packed.data(), packed.size(), out.data(), out.size());
                        });
                        report(name, ns, (double)item.raw->size());

                        snprintf(name, sizeof(name), "refpack/encode/%s", item.tag);
                        ns = measure_ns([] {}, [&] {
                                return bench_refpack_encode(item.raw->data(), item.raw->size(), dest.data(), dest.size());
                        });
                        report(name, ns, (double)item.raw->size());
                }
        }

        // --- ini -------------------------------------------------------------------
        {
                // the golden scenario: five fixtures, one context, the golden
                // load types, edge.ini CRLF-transformed (the wrapper handles
                // the transform and the CREATE_OVERRIDES load type)
                std::vector<uint8_t> armor = read_file(fixturesDir + "/armor.ini");
                std::vector<uint8_t> loco = read_file(fixturesDir + "/locomotor.ini");
                std::vector<uint8_t> weapon = read_file(fixturesDir + "/weapon.ini");
                std::vector<uint8_t> over = read_file(fixturesDir + "/override.ini");
                std::vector<uint8_t> edge = read_file(fixturesDir + "/edge.ini");
                double total = (double)(armor.size() + loco.size() + weapon.size() + over.size() + edge.size());

                report("parse/fixtures_all", measure_ns([] {}, [&] {
                        return bench_ini_parse_scenario(
                                armor.data(), armor.size(),
                                loco.data(), loco.size(),
                                weapon.data(), weapon.size(),
                                over.data(), over.size(),
                                edge.data(), edge.size());
                }), total);

                std::vector<uint8_t> synthetic = synthetic_armor_ini();
                report("parse/synthetic_armor_x512", measure_ns([] {}, [&] {
                        return bench_ini_parse_synthetic(synthetic.data(), synthetic.size());
                }), (double)synthetic.size());
        }

        // --- csf ---------------------------------------------------------------------
        {
                // the corpus bytes are materialized once (untimed) through the
                // transcribed writer; the parse bench reads the identical
                // bytes the Rust bench builds with its own writer. The write
                // bench is the transcription into a fresh sink — the exact
                // shape of the Rust `to_bytes()` closure.
                void *db = bench_csf_db_new();
                std::vector<uint8_t> bytes(4 * 1024 * 1024);
                long total = bench_csf_materialize(db, bytes.data(), bytes.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: csf corpus write failed\n");
                        exit(1);
                }
                bytes.resize((size_t)total);

                report("csf/write/ui_x4096", measure_ns([] {}, [&] {
                        return bench_csf_write(db);
                }), (double)total);

                report("csf/parse/ui_x4096", measure_ns([] {}, [&] {
                        return (std::uint64_t)bench_csf_parse(bytes.data(), bytes.size());
                }), (double)total);

                report("csf/game_text/ui_x4096", measure_ns([] {}, [&] {
                        return (std::uint64_t)bench_csf_game_text(db);
                }), (double)total);

                bench_csf_db_free(db);
        }

        // --- tga ----------------------------------------------------------------------
        {
                // The corpus Targas are materialized once (untimed): the raw
                // screenshot file and the RLE texture file, built through the
                // transcribed Save — identical bytes to the Rust side's.
                void *raw24 = bench_tga_targa_new(3);
                void *tex32 = bench_tga_targa_new(4);

                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long raw24_len = bench_tga_materialize(raw24, 0, scratch.data(), scratch.size());
                std::vector<uint8_t> raw24_bytes(scratch.begin(), scratch.begin() + raw24_len);
                long rle32_len = bench_tga_materialize(tex32, 1, scratch.data(), scratch.size());
                std::vector<uint8_t> rle32_bytes(scratch.begin(), scratch.begin() + rle32_len);
                if (raw24_len <= 0 || rle32_len <= 0)
                {
                        fprintf(stderr, "error: tga corpus write failed\n");
                        exit(1);
                }

                report("tga/write/rgb24_x256", measure_ns([] {}, [&] {
                        return (std::uint64_t)bench_tga_write(raw24);
                }), (double)raw24_len);

                report("tga/encode/rgb32_x256", measure_ns([] {}, [&] {
                        return (std::uint64_t)bench_tga_encode(tex32);
                }), (double)rle32_len);

                report("tga/parse/rgb24_raw_x256", measure_ns([] {}, [&] {
                        return (std::uint64_t)bench_tga_parse(raw24_bytes.data(), raw24_bytes.size());
                }), (double)raw24_len);

                report("tga/parse/rgb32_rle_x256", measure_ns([] {}, [&] {
                        return (std::uint64_t)bench_tga_parse(rle32_bytes.data(), rle32_bytes.size());
                }), (double)rle32_len);

                bench_tga_targa_free(raw24);
                bench_tga_targa_free(tex32);
        }

        // --- chunk ---------------------------------------------------------------------
        {
                // The corpus is materialized once (untimed) through the
                // transcribed writer; the Rust bench builds the identical
                // bytes through the port's own writer (the golden writer
                // corpora pin the two byte-identical).
                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long total = bench_chunk_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: chunk corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);

                report("chunk/write/mesh_x4096", measure_ns([] {}, [&] {
                        return (std::uint64_t)bench_chunk_write();
                }), (double)total);

                report("chunk/read/walk_x4096", measure_ns([] {}, [&] {
                        return bench_chunk_walk(bytes.data(), bytes.size());
                }), (double)total);
        }

        // --- w3d mesh --------------------------------------------------------------------
        {
                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long total = bench_mesh_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: mesh corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);

                report("mesh/parse_x2048", measure_ns([] {}, [&] {
                        return bench_mesh_load(bytes.data(), bytes.size());
                }), (double)total);
        }

        // --- w3d hierarchy tree --------------------------------------------------------------
        {
                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long total = bench_htree_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: htree corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);

                report("htree/parse_x2048", measure_ns([] {}, [&] {
                        return bench_htree_load(bytes.data(), bytes.size());
                }), (double)total);
        }

        // --- w3d raw animation ---------------------------------------------------------------
        {
                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long total = bench_hanim_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: hanim corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);

                report("hanim/parse_x64", measure_ns([] {}, [&] {
                        return bench_hanim_load(bytes.data(), bytes.size());
                }), (double)total);

                // the transform loop covers 64 bones x 16 frames = 1024
                // Get_Transform calls; the element count is the throughput
                // column (transforms/s). The setup loads the animation
                // (untimed), the body times the transform loop.
                report("hanim/xforms_x1024", measure_ns_ctx(
                        [&] { return bench_hanim_anim_new(bytes.data(), bytes.size()); },
                        [](void *h) { return bench_hanim_xforms(h); },
                        [](void *h) { bench_hanim_anim_free(h); }),
                        1024.0);
        }

        // --- w3d compressed animation (time-coded) ------------------------------------------
        {
                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long total = bench_hcanim_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: hcanim corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);

                report("hcanim/parse_x64", measure_ns([] {}, [&] {
                        return bench_hcanim_load(bytes.data(), bytes.size());
                }), (double)total);
        }

        // --- w3d compressed animation (adaptive delta) ---------------------------------------
        {
                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long total = bench_hcanim_ad_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: hcanim AD corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);

                report("hcanim/parse_ad_x64", measure_ns([] {}, [&] {
                        return bench_hcanim_load(bytes.data(), bytes.size());
                }), (double)total);

                // the transform loop covers 64 bones x 16 frames = 1024
                // Get_Transform calls on the adaptive-delta corpus: the
                // decompression cache's slide path plus the slerp and
                // Build_Matrix3D. The stateful caches make each iteration
                // rewind once before the walk — the same work the Rust
                // bench performs.
                report("hcanim/xforms_ad_x1024", measure_ns_ctx(
                        [&] { return bench_hcanim_anim_new(bytes.data(), bytes.size()); },
                        [](void *h) { return bench_hcanim_xforms(h); },
                        [](void *h) { bench_hcanim_anim_free(h); }),
                        1024.0);
        }

        // --- w3d HLOD assembly ---------------------------------------------------------------
        {
                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long total = bench_hlod_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: hlod corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);

                report("hlod/parse_x1072", measure_ns([] {}, [&] {
                        return bench_hlod_load(bytes.data(), bytes.size());
                }), (double)total);
        }

        // --- map data-chunk container ----------------------------------------------------
        {
                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long total = bench_mapchunk_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: mapchunk corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);

                report("mapchunk/write_x2048", measure_ns([] {}, [&] {
                        return bench_mapchunk_write();
                }), (double)total);

                report("mapchunk/parse_x2048", measure_ns([] {}, [&] {
                        return bench_mapchunk_parse(bytes.data(), bytes.size());
                }), (double)total);
        }

        // --- map height-map payload -------------------------------------------------------
        {
                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long total = bench_mapheightmap_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: mapheightmap v4 corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);

                report("mapheightmap/parse_x64k", measure_ns([] {}, [&] {
                        return bench_mapheightmap_parse(bytes.data(), bytes.size());
                }), (double)total);

                long total_v1 = bench_mapheightmap_materialize_v1(scratch.data(), scratch.size());
                if (total_v1 <= 0)
                {
                        fprintf(stderr, "error: mapheightmap v1 corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes_v1(scratch.begin(), scratch.begin() + total_v1);

                report("mapheightmap/parse_v1_x64k", measure_ns([] {}, [&] {
                        return bench_mapheightmap_parse_v1(bytes_v1.data(), bytes_v1.size());
                }), (double)total_v1);
        }

        // --- dds textures ---------------------------------------------------------------
        {
                std::vector<uint8_t> scratch(4 * 1024 * 1024);
                long total = bench_dds_parse_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: dds parse corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);

                report("dds/parse_x16k", measure_ns([] {}, [&] {
                        return bench_dds_parse(bytes.data(), bytes.size());
                }), (double)total);
        }
        {
                std::vector<uint8_t> scratch(1 * 1024 * 1024);
                long total = bench_dds_convert_materialize(scratch.data(), scratch.size());
                if (total <= 0)
                {
                        fprintf(stderr, "error: dds convert corpus write failed\n");
                        exit(1);
                }
                std::vector<uint8_t> bytes(scratch.begin(), scratch.begin() + total);
                std::vector<uint8_t> dest(64 * 64 * 4 + 16);

                report("dds/convert_dxt5_x256", measure_ns([] {}, [&] {
                        return bench_dds_convert(bytes.data(), bytes.size(), dest.data(), dest.size());
                }), (double)total);
        }

        // --- math --------------------------------------------------------------------
        {
                bench_fast_tables_init();

                std::vector<float> xs256 = float_corpus(256);
                void *vec = bench_vec_corpus_new(xs256.data(), xs256.size());
                report("vector3/length/x256", measure_ns([] {}, [&] { return bench_v3_length_batch(vec); }));
                report("vector3/normalize/x256", measure_ns([] {}, [&] { return bench_v3_normalize_batch(vec); }));
                report("vector3/dot/x256", measure_ns([] {}, [&] { return bench_v3_dot_batch(vec); }));
                report("vector3/cross/x256", measure_ns([] {}, [&] { return bench_v3_cross_batch(vec); }));
                report("vector3/quick_distance2d/x256",
                        measure_ns([] {}, [&] { return bench_v2_quick_distance_batch(vec); }));
                bench_vec_corpus_free(vec);

                std::vector<float> xs64 = float_corpus(64);
                void *mat = bench_mat_corpus_new(xs64.data(), xs64.size());
                report("matrix/matrix3_multiply/x64",
                        measure_ns([] {}, [&] { return bench_matrix3_multiply_batch(mat); }));
                report("matrix/matrix3d_multiply/x64",
                        measure_ns([] {}, [&] { return bench_matrix3d_multiply_batch(mat); }));
                report("matrix/matrix4_multiply/x64",
                        measure_ns([] {}, [&] { return bench_matrix4_multiply_batch(mat); }));
                report("matrix/matrix3d_transform_vector/x64",
                        measure_ns([] {}, [&] { return bench_matrix3d_transform_batch(mat); }));
                report("matrix/matrix4_inverse/x64",
                        measure_ns([] {}, [&] { return bench_matrix4_inverse_batch(mat); }));
                bench_mat_corpus_free(mat);

                void *quat = bench_quat_corpus_new(xs64.data(), xs64.size());
                report("quat/slerp/x64", measure_ns([] {}, [&] { return bench_quat_slerp_batch(quat); }));
                report("quat/fast_slerp/x64", measure_ns([] {}, [&] { return bench_quat_fast_slerp_batch(quat); }));
                report("quat/build_matrix3/x64",
                        measure_ns([] {}, [&] { return bench_quat_build_matrix3_batch(quat); }));
                bench_quat_corpus_free(quat);

                void *trig = bench_trig_corpus_new(xs256.data(), xs256.size());
                report("fast/fast_sin/x256", measure_ns([] {}, [&] { return bench_fast_sin_batch(trig); }));
                report("fast/fast_cos/x256", measure_ns([] {}, [&] { return bench_fast_cos_batch(trig); }));
                report("fast/fast_acos/x256", measure_ns([] {}, [&] { return bench_fast_acos_batch(trig); }));
                bench_trig_corpus_free(trig);
        }

        return 0;
}
