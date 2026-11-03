# Benchmark budget (plan §6)

## Phase 1 (foundation crates)

Status: **complete** — all Phase 1 foundation crates carry criterion
benchmarks; 29 of 32 Rust-vs-C++ comparisons are within the §6 budget
(no more than 20 % slower than the C++ baseline), the remaining three are
accepted exceptions justified in [ADR-003](adr/003-benchmark-budget-exceptions.md).

Numbers below were measured on 2026-11-03, Linux x86-64 (Debian 12),
`rustc 1.99.1` (`-C opt-level=3`, the workspace release profile) vs
`g++ 14.2.0` (`-O2 -ffp-contract=off`, the flags the differential harness
uses for the transcriptions). They are machine- and compiler-specific; the
**ratios** are the durable result. Regenerate with:

```sh
cargo bench --workspace                      # Rust side (criterion)
tools/differential/bench_baseline.sh         # C++ side (same corpora)
```

### Phase 2 (format crates)

The §6 discipline carries into Phase 2: every new `rzh-formats` module
extends the same harness. The first slice, the CSF string tables, adds
three comparisons. Measured on 2026-11-03, Linux x86-64 (sandboxed
container; note the wider run-to-run noise than the Phase 1 numbers — the
**ratios** are the durable result and every rerun kept all three inside
budget), same `rustc 1.99.1` vs `g++ 14.2.0` flags as above. The corpus is
the formula-generated UI table both sides build in lockstep (4096 labels;
every 8th label a Babylon-style wave, every 16th a second string; 636,608
bytes — verified byte-identical across the two builders). The C++ side
parses and writes through the `gen_csf_golden.cpp` transcriptions; the
Rust side through `rzh-formats::csf`.

| Benchmark | Rust (criterion) | C++ baseline | Rust/C++ | Verdict |
|---|---|---:|---:|---|
| `csf/parse/ui_x4096` | 631.93 µs | 1.33 ms | 0.48x | within |
| `csf/write/ui_x4096` | 208.94 µs | 2.65 ms | 0.08x | within |
| `csf/game_text/ui_x4096` | 344.86 µs | 453.88 µs | 0.76x | within |

The write gap is structural, not accidental: Babylon's writer pushes its
bytes through per-field `fwrite`s (transcribed here as per-field vector
inserts), while the port batches whole fields into one `Vec`. The reader
gap comes from the same shape difference in reverse plus bounds checking
the C++ does not do.

## Methodology

- **Same code, same corpora.** The C++ side times the *verbatim
  transcriptions the golden vectors pin* (`tools/differential/gen_golden.cpp`
  — crc, RNG, name keys, WWMath —, `gen_ini_golden.cpp`, and the **real**
  EAC `refencode.cpp`/`refdecode.cpp` sources), compiled exactly the way
  `tools/differential/regenerate.sh` compiles them.
- **Identical inputs.** Corpora are generated from the differential
  harness's LCG (seed `0xc0ffee11`, Numerical Recipes constants) with the
  same construction formulas on both sides.
- **Identical loop shapes.** Batch bodies mirror the criterion closures
  one-to-one (e.g. `rng/raw` = seed + 256 draws; name-key interning setup
  is untimed on both sides, matching `iter_batched`). Criterion consumes
  (and black-boxes) each closure's return value, so the Rust loops are
  written without per-operand opacity — both compilers are free to
  vectorize, and neither side is artificially de-optimized.
- **Median statistics** on both sides (criterion estimates; the C++
  harness reports the median of 31 timed batches).

### Budget table

| Benchmark | Rust (criterion) | C++ baseline | Rust/C++ | Verdict |
|---|---|---:|---:|---|
| `crc/update/64` | 29.4 ns | 76.0 ns | 0.39x | within |
| `crc/update/4096` | 2.30 µs | 3.44 µs | 0.67x | within |
| `crc/update/65536` | 36.92 µs | 54.62 µs | 0.68x | within |
| `rng/raw` | 203.3 ns | 317.0 ns | 0.64x | within |
| `rng/random_int/0_100` | 278.6 ns | 460.0 ns | 0.61x | within |
| `rng/random_real/0_1` | 233.9 ns | 395.0 ns | 0.59x | within |
| `namekey/intern_new/x512` | 21.03 µs | 24.11 µs | 0.87x | within |
| `namekey/intern_hit/x512` | 13.89 µs | 10.20 µs | 1.36x | exception ([ADR-003](adr/003-benchmark-budget-exceptions.md)) |
| `refpack/decode/zeros` | 95.16 µs | 92.06 µs | 1.03x | within |
| `refpack/encode/zeros` | 15.34 ms | 15.12 ms | 1.01x | within |
| `refpack/decode/text` | 26.37 µs | 29.08 µs | 0.91x | within |
| `refpack/encode/text` | 1.70 ms | 1.48 ms | 1.15x | within |
| `refpack/decode/random` | 3.05 µs | 13.00 µs | 0.23x | within |
| `refpack/encode/random` | 392.23 µs | 267.84 µs | 1.46x | exception ([ADR-003](adr/003-benchmark-budget-exceptions.md)) |
| `parse/fixtures_all` | 38.51 µs | 32.41 µs | 1.19x | within |
| `parse/synthetic_armor_x512` | 2.04 ms | 1.85 ms | 1.10x | within |
| `vector3/length/x256` | 86.8 ns | 219.0 ns | 0.40x | within |
| `vector3/normalize/x256` | 153.7 ns | 223.0 ns | 0.69x | within |
| `vector3/dot/x256` | 52.6 ns | 82.0 ns | 0.64x | within |
| `vector3/cross/x256` | 51.1 ns | 75.0 ns | 0.68x | within |
| `vector3/quick_distance2d/x256` | 144.1 ns | 106.0 ns | 1.36x | exception ([ADR-003](adr/003-benchmark-budget-exceptions.md)) |
| `matrix/matrix3_multiply/x64` | 19.4 ns | 161.0 ns | 0.12x | within |
| `matrix/matrix3d_multiply/x64` | 22.1 ns | 42.0 ns | 0.53x | within |
| `matrix/matrix4_multiply/x64` | 33.5 ns | 121.0 ns | 0.28x | within |
| `matrix/matrix3d_transform_vector/x64` | 16.2 ns | 44.0 ns | 0.37x | within |
| `matrix/matrix4_inverse/x64` | 318.9 ns | 415.0 ns | 0.77x | within |
| `quat/slerp/x64` | 409.6 ns | 436.0 ns | 0.94x | within |
| `quat/fast_slerp/x64` | 438.3 ns | 584.0 ns | 0.75x | within |
| `quat/build_matrix3/x64` | 16.5 ns | 106.0 ns | 0.16x | within |
| `fast/fast_sin/x256` | 1.22 µs | 1.22 µs | 1.01x | within |
| `fast/fast_cos/x256` | 1.22 µs | 1.21 µs | 1.01x | within |
| `fast/fast_acos/x256` | 1.04 µs | 1.46 µs | 0.71x | within |

### Rust-only regression baselines

These have no 1:1 C++ counterpart (either the C++ API differs structurally
— `key_to_name` was a linear scan in the original and is O(1) in the port —
or the layer is a Rust re-implementation like the VFS backing store). They
serve as the regression baseline future changes are tracked against.

| Benchmark | Rust (criterion) |
|---|---|
| `namekey/key_to_name/x512` | 6.69 µs |
| `refpack/header/is_refpack_and_size` | 3.49 ns |
| `scan/scan_int/x256` | 1.60 µs |
| `scan/scan_real/x256` | 5.26 µs |
| `scan/scan_bool/x256` | 500.8 ns |
| `vector3/lerp4/x256` | 31.44 ns |
| `overlap/aabox_aabox/x256` | 136.37 ns |
| `big/open_memory/entries/1000` | 421.32 µs |
| `big/open_memory/entries/10000` | 4.90 ms |
| `big/lookup/file_info_hit` | 90.31 ns |
| `big/lookup/file_info_miss` | 136.94 ns |
| `big/lookup/listing/data_dir` | 4.96 ms |
| `big/read/read_entry/plain_1k` | 35.27 ns |
| `big/read/read_entry/refpack_1k` | 24.56 ns |
| `big/read/streaming/256b_windows` | 51.90 ns |
| `vfs/does_file_exist/hit` | 412.78 ns |
| `vfs/read_and_decompress/compressed` | 1.66 µs |

### Findings that changed the port

Running the budget comparison surfaced four structural issues where the
port deviated from the original's (compiler-friendly) shape; each fix was
verified byte-exact against the committed golden vectors before landing:

1. **`GameRng` carry chain** (rzh-util): the `ADC` macro's comparison form
   compiled to compare+setcc chains; writing it with `u32::carrying_add`
   emits the `add`/`adc` chain the original's asm comments describe —
   `rng/raw` went from 2.1x *slower* to 0.64x faster than the C++.
2. **`Matrix3D::mul`** (rzh-math): a loop over a dynamically indexed column
   forced the rows through memory; the original's explicit field unroll
   runs 7x faster.
3. **`refpack::refcompress`'s `matchlen`** (rzh-compress): index-based
   byte compares kept bounds checks in the encoder's inner loop; sub-slice
   form made the degenerate `zeros` corpus 1.93x → 1.01x.
4. **`Ini::read_line`** (rzh-ini): a fresh `Vec` per line where the C++
   reuses its fixed member buffer; reusing the allocation took the full
   fixture parse from 1.29x to 1.19x and the synthetic bulk parse to 1.10x.

Cross-crate `#[inline]` on the hot kernels (`raw`, `multiply`, `slerp`,
`name_to_key`, …) was also required: without LTO a non-`#[inline]` public
function is a real call, and the small kernels paid up to 2.5x for the
call overhead plus argument traffic.
