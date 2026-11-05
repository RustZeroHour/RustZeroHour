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

The second slice, the Targa images, adds four comparisons. Measured on
2026-11-04, same setup and flags as above. The corpora are 256×256
textures built from the same pixel formula on both sides — a 24-bit raw
file in the screenshot shape and a 32-bit RLE file built through each
side's own (golden-pinned) encoder, so the parse benches read **identical
bytes**. The C++ side loads and saves through the `TARGA.cpp`
transcription in `bench_wrap_tga.cpp`; the Rust side through
`rzh-formats::tga`.

| Benchmark | Rust (criterion) | C++ baseline | Rust/C++ | Verdict |
|---|---|---:|---:|---|
| `tga/parse/rgb24_raw_x256` | 27.92 µs | 43.87 µs | 0.64x | within |
| `tga/parse/rgb32_rle_x256` | 164.09 µs | 200.15 µs | 0.82x | within |
| `tga/write/rgb24_x256` | 28.77 µs | 83.39 µs | 0.35x | within |
| `tga/encode/rgb32_x256` | 450.01 µs | 490.66 µs | 0.92x | within |

The raw-parse number is the budget check that changed the port (finding 5
below); the encode margin is thin because the RLE encoder is a
branchy per-pixel loop on both sides — the transcription's shape and the
port's are the same, and the remaining ~8 % is the bounds-checked slice
writes the C++ does not do.

The third slice, the W3D chunk container, adds two comparisons.
Measured on 2026-11-04, same setup and flags as above. The corpus is a
mesh-shaped chunk tree (65,602 bytes: a micro-chunk header record, 4096
twelve-byte vertex records written one write per record, and an index
array) built through each side's own writer — **verified byte-identical
across the two builders** — so the read bench walks identical bytes. The
C++ side builds and walks through the `chunkio.cpp` transcription in
`bench_wrap_chunk.cpp`; the Rust side through `rzh-formats::chunk`.

| Benchmark | Rust (criterion) | C++ baseline | Rust/C++ | Verdict |
|---|---|---:|---:|---|
| `chunk/write/mesh_x4096` | 70.04 µs | 112.81 µs | 0.62x | within |
| `chunk/read/walk_x4096` | 16.67 µs | 30.85 µs | 0.54x | within |

The write gap is structural: the original patches each chunk header with
a seek-back-and-rewrite through its `FileClass` (transcribed as a
position-tracked vector), while the port patches the same eight bytes in
place on its `Vec` sink; both re-walk the whole tree per call. The read
gap comes from the same shape difference in reverse (the port's
stack frames live in a `Vec` the optimizer keeps hot) plus the
bounds-checked slice reads the C++ does not do. Two reruns of each side
kept both comparisons inside budget with margins unchanged.

The fourth slice, the W3D mesh chunk tree, adds one comparison.
Measured on 2026-11-05, same setup and flags as above. The corpus is a
2048-vertex prelit-vertex mesh (180,684 bytes: the header, vertex and
normal arrays, 2048 triangles, the complete material tree inside the
prelit wrapper — one shader, one vertex material, one texture, one
material pass with DCG and UV arrays — and a 1024-node AABTree) built
from the same absolute-indexed byte formula on both sides — **verified
byte-identical across the two builders** (the corpus size assert in the
Rust bench pins the lockstep). The C++ side loads through the full
`meshmdlio.cpp` transcription in `bench_wrap_mesh.cpp`; the Rust side
through `rzh-formats::w3d_mesh`.

| Benchmark | Rust (criterion) | C++ baseline | Rust/C++ | Verdict |
|---|---|---:|---:|---|
| `mesh/parse_x2048` | 148.41 µs | 215.34 µs | 0.69x | within |

The margin is structural: the transcription's model is a web of
individually heap-allocated `ShareBufferClass` arrays joined by raw
pointers (six allocations per load plus the material description's), all
freed in the destructor, while the port's arrays are `Vec`s the
allocator serves in one batch; the per-element `Read` call shape is the
same on both sides. Two reruns of each side kept the comparison inside
budget with the margin unchanged (215.3–220.5 µs vs 148.1–149.1 µs).

The fifth slice, the W3D hierarchy trees, adds one comparison. Measured
on 2026-11-05, same setup and flags as above. The corpus is a v4.1
skeleton of 2048 chained pivots (122,932 bytes: the 36-byte header and
2048 60-byte pivot structs, every struct's bytes from the same
absolute-indexed formula with the parent index structured) — **verified
byte-identical across the two builders**, and the two loaders' loaded
state cross-checked (pivot count, resolved parents, base-transform bits
and name length agree). The C++ side loads through the `htree.cpp`
transcription in `bench_wrap_htree.cpp`; the Rust side through
`rzh-formats::w3d_htree`.

| Benchmark | Rust (criterion) | C++ baseline | Rust/C++ | Verdict |
|---|---|---:|---:|---|
| `htree/parse_x2048` | 78.28 µs | 88.32 µs | 0.89x | within |

The margin is modest and structural: the transcription allocates a
`PivotClass[]` per load (one `new[]`/`delete[]` pair of 128-byte
pointer-carrying elements with two embedded `Matrix3D` members) and
walks it through pointer-linked `Parent` fields, while the port packs
the same load-path state into one `Vec` of plain values; the
mixed-precision `Build_Matrix3D`/`Translate`/`postMul` arithmetic is
identical on both sides, so the gap is the allocation shape and the
C++ class-member call overhead the per-pivot loop pays. Three reruns
of each side kept the comparison inside budget with the margin
unchanged (88.2–88.5 µs vs 78.1–78.6 µs).

The seventh slice, the W3D raw animations, adds two comparisons.
Measured on 2026-11-05, same setup and flags as above. The corpus is a
64-bone, 128-frame animation (117,300 bytes: the 44-byte header, X/Y/Z
channels on bones 0..31 and quaternion channels on bones 32..63) built
from the same word formulas on both sides — **verified byte-identical
across the two builders** (the corpus size assert in the Rust bench
pins the lockstep). The C++ side loads and samples through the
`hrawanim.cpp`/`motchan.cpp` transcription in `bench_wrap_hanim.cpp`;
the Rust side through `rzh-formats::w3d_hanim`. The transform bench is
the per-frame `Anim_Update` shape: `Get_Transform` over all 64 bones at
16 sampled frames (1024 calls through the table-based `Fast_Slerp`),
on a preloaded animation (the load is untimed).

| Benchmark | Rust (criterion) | C++ baseline | Rust/C++ | Verdict |
|---|---:|---:|---:|---|
| `hanim/parse_x64` | 14.06 µs | 13.7 µs | 1.03x | within |
| `hanim/xforms_x1024` | 39.98 µs | 44.1 µs | 0.91x | within |

The parse margin went through the budget check the plan prescribes: the
port's first shape (a zero-filled float array plus a zero-filled byte
staging array per channel — safe Rust cannot read into an
uninitialized array the way the original's single `new[]` + file read
does) measured 1.24x, and the stack-buffer variant that replaced the
staging array with an 8 KiB zeroed buffer made it worse (5x — the
per-channel zeroing dwarfed the load). The committed shape reads
through a 512-byte stack buffer into the pre-sized array with indexed
stores; `f32::from_le_bytes` is a plain load on little-endian hosts, so
the conversion pass collapses into the copy and the comparison lands at
1.03x. The transform loop is faster than the transcription because the
C++ pays four `Get_Vector`/`Get_Vector_As_Quat` member calls plus the
`Fast_Slerp` helper per bone while the port's queries inline the same
arithmetic. Two reruns of each side kept both comparisons inside budget
with the margins unchanged (13.3–14.4 µs vs 14.0–14.1 µs and 44.0–44.3
µs vs 39.7–40.3 µs).

The eighth slice, the W3D compressed animations, adds three
comparisons — one per compression flavor's load path plus the
adaptive-delta query path. Measured on 2026-11-05, same setup and flags
as above. The time-coded corpus is a 64-bone, 128-frame animation
(182,324 bytes: the 44-byte header, time-coded X/Y/Z channels on bones
0..31 and quaternion channels on bones 32..63); the adaptive-delta
corpus carries the same shape (19,636 bytes: the 16-byte channel
structs with their base floats, eight 9-byte packet blocks per channel
and the exporter's word-alignment pad). Both are built from the same
word and packet formulas on both sides — **verified byte-identical
across the two builders** (the corpus size asserts in the Rust benches
pin the lockstep). The C++ side loads and samples through the
`hcanim.cpp`/`motchan.cpp` transcription in `bench_wrap_hcanim.cpp`;
the Rust side through `rzh-formats::w3d_hcanim`. The transform bench is
the per-frame `Anim_Update` shape on the adaptive-delta corpus:
`Get_Transform` over all 64 bones at 16 consecutive frames (1024 calls
through the decompression cache's sliding window, the table-based
`Fast_Slerp` and `Build_Matrix3D`), on a preloaded animation (the load
is untimed).

| Benchmark | Rust (criterion) | C++ baseline | Rust/C++ | Verdict |
|---|---:|---:|---:|---|
| `hcanim/parse_x64` | 19.5 µs | 20.8 µs | 0.94x | within |
| `hcanim/parse_ad_x64` | 10.5 µs | 17.2 µs | 0.61x | within |
| `hcanim/xforms_ad_x1024` | 61.0 µs | 52.7 µs | 1.16x | within |

The adaptive-delta query comparison went through its own budget
iteration: the port's first shape measured 1.46x — the cache branches
copied rows through stack scratch arrays (a borrow-checker workaround)
and `getframe` stayed a real call, so each of the eight stateful cache
probes per quaternion query paid call and copy overhead the original's
raw-pointer `getframe` does not. Writing the decompression walks as
free functions over the channel's immutable parts (so the cache
branches write straight into their rows, exactly where the original's
`decompress` writes) closed the copy half, and `#[inline(always)]` on
`getframe` — the eight probes are the entire hot path, and the compiler
would not inline the mutating method on its own — closed the rest. The
load-path comparisons are comfortably inside budget on both flavors
(the adaptive-delta parse is 0.61x: the C++ channel reader allocates a
`CacheData` float array per channel that the port keeps as a fixed
eight-float value, and the C++ corpus walk pays the adaptive-delta
corpus's smaller, allocation-dominated shape). Two reruns of each side
kept all three comparisons inside budget with the margins unchanged
(19.1–20.4 µs, 10.4–10.5 µs and 60.9–61.2 µs vs 20.8 µs, 17.2 µs and
52.7 µs).

The ninth slice, the W3D HLOD assemblies, adds the assembly-load
comparison. Measured on 2026-11-05, same setup and flags as above. The
corpus is a 64-LOD assembly — 16 models per LOD, 16 aggregates and 32
proxies, 1072 sub-objects in all (48,800 bytes: the 40-byte header, the
per-LOD wrapper/array-header pairs and 36-byte sub-object structs, the
names from the same absolute-offset formula both sides share) —
**verified byte-identical across the two builders**, with the loaded
state cross-checked (the same checksum both sides compute). The C++
side loads through the `hlod.cpp` transcription in
`bench_wrap_hlod.cpp`; the Rust side through `rzh-formats::w3d_hlod`.

| Benchmark | Rust (criterion) | C++ baseline | Rust/C++ | Verdict |
|---|---:|---:|---:|---|
| `hlod/parse_x1072` | 68.0 µs | 76.9 µs | 0.89x | within |

The assembly parse is faster than the transcription on this corpus
shape: the walk is dominated by the 1072 chunk opens and 36-byte reads,
where the port's slice-based reader keeps the bounds checks fused with
the copies while the C++ `Read` pays its stack-array bookkeeping per
call, and the C++ side's per-model `strdup`/`new[]` pair (two heap
round-trips per sub-object plus the per-LOD array allocations) costs
more than the port's two pre-sized `Vec`s. Two reruns of each side kept
the comparison inside budget with the margins unchanged.

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
5. **`tga::invert_in_place`** (rzh-formats): the pixel byte-reversal ran
   over a runtime `depth`, keeping it in a generic per-byte swap loop —
   1.44x *slower* than the C++ on the raw-texture parse. Specializing the
   depths the game ships (3/4 bytes per pixel, the generic tail kept for
   exotic headers) lets the reversal fully unroll: the raw parse went from
   1.44x slower to 0.64x faster, and the RLE parse, write and encode
   benches (all of which invert) dropped by a fifth to a quarter.

Cross-crate `#[inline]` on the hot kernels (`raw`, `multiply`, `slerp`,
`name_to_key`, …) was also required: without LTO a non-`#[inline]` public
function is a real call, and the small kernels paid up to 2.5x for the
call overhead plus argument traffic.
