# Differential testing harness

This directory holds the temporary C++ **reference transcriptions** used to
generate the golden vectors that prove parity between the Rust port and the
original Zero Hour algorithms (migration plan §3.1 stage A and §3.2 parity
level **P0**: byte-exact output on identical input).

## Files

| File | Purpose |
|------|---------|
| `gen_golden.cpp` | Verbatim transcription of the baseline CRC (`Common/crc.h` / `crc.cpp`), RNG (`Common/RandomValue.cpp`, non-retail branches), name-key interning (`NameKeyGenerator.{h,cpp}`) and the IEEE-754 exact WWMath subset (`wwmath.h`, `vector{2,3,4}.h`, `vector2i.h`, `Vector3i.h`, deterministic branches) at commit `571a7c5`. Emits the golden vector files. |
| `gen_gmath_golden.c` | Golden-vector driver for `rzh-math::gmath`. Not a transcription: it is compiled and linked against the REAL pinned GameMath sources (`TheSuperHackers/GameMath` @ `59f7ccd4`, the pin `cmake/gamemath.cmake` fetches), so the gmath vectors come from the very code the C++ oracle runs. |
| `gen_compress_golden.cpp` | Golden-vector driver for `rzh-compress::refpack`. Compiles the REAL EAC codec sources (`Compression/EAC/ref{decode,encode}.cpp`) from the game tree as separate translation units and calls them, so the RefPack goldens come from the original code in both directions (encode and decode). |
| `gen_ini_golden.cpp` | Golden-dump driver for `rzh-ini`: a verbatim transcription of the INI engine (`INI.cpp` reader, `strtok` tokenizing, `scanType<T>` with `USE_STD_FROM_CHARS_PARSING = 1`, `initFromINI`, the bit-string and quoted-string readers, the unit conversions) plus the three task-10 block types (complete `Armor`, complete `Locomotor`, the ported `Weapon` subset with the store semantics). Replays a fixed scenario over the committed fixtures in `crates/rzh-ini/tests/fixtures/` (base loads, a map-style override load, a CRLF-transformed edge-case load, and nine error files) and writes the canonical dump the Rust test must match byte-for-byte. |
| `gen_csf_golden.cpp` | Golden-dump driver for `rzh-formats::csf` (Phase 2, first slice): two verbatim transcriptions — the **writer** (Babylon `expimp.cpp` `generate_csf`/`writeCSFLabel`/`writeCSFString` plus the `TransDB.cpp` LANGINFO language letter, minus the tool-only `StripSpaces`/`ConvertMetaChars` content transforms) and the **reader** (runtime `GameText.cpp` `parseCSF` + `stripSpaces`, invert-until-zero decode, first-string rule, C-string views). Emits the `csf.txt` script: writer bytes (`gen`), reader verdicts (`ok`/`err`/`count`), the game-visible strings (`view`) and `getCSFInfo`'s language selection (`info`) over hand-crafted corpora and assembled edge-case blobs (empty labels, the `0x0000`/`0xFFFF` decode quirks, NUL vs language-letter waves, trailing-byte acceptance, version 1/2 language fallback). |
| `gen_tga_golden.cpp` | Golden-dump driver for `rzh-formats::tga` (Phase 2, second slice): a verbatim transcription of the WWLib `TARGA.cpp` class (the `Open` footer sniff, the `Load` switch with its missing `TGA_MONO_ENCODED` case, the truecolor-only `InvertImage`, the origin normalization, the RLE decoder and encoder, `XFlip`/`YFlip`, the `Save` path with its type bump and always-present Targa 2.0 footer), with the `FileClass` plumbing replaced by a byte source/sink. Emits the `tga.txt` script: corpus bytes, Save'd bytes (`gen`), reader verdicts (`ok`/`err`), the header/palette/image views as `Load` leaves them, and the 495-byte extension dumps. The encoder's final-iteration out-of-bounds compare is pinned to its deterministic majority outcome (a mismatch) on both sides — see the file comment. |
| `bench_baseline.cpp` + `bench_wrap_{golden,compress,ini,csf,tga}.cpp` + `bench_baseline.sh` | The §6 budget harness (Phase 1 exit criterion, extended into Phase 2): times the *same* transcriptions and the real EAC codec on the same LCG corpora and loop shapes as the Rust criterion benches, printing `bench <id> <ns> <MB/s>` lines that map 1:1 to criterion bench ids. The measured budget lives in `docs/benchmarks.md`; the three over-budget results are justified in ADR-003. `bench_wrap_csf.cpp` wraps the `gen_csf_golden.cpp` transcriptions and rebuilds the Rust bench corpus byte-identically through the transcribed writer; `bench_wrap_tga.cpp` does the same for the `tga/…` benches through the transcribed `TARGA.cpp`. Temporary like the rest of this directory (removed at M5). |
| `regenerate.sh` | Compiles the harness and regenerates `crates/rzh-util/tests/golden/{crc,rng,namekey}.txt` and `crates/rzh-math/tests/golden/{scalar,vectors,matrices}.txt`, then clones the GameMath pin (set `GAMEMATH_DIR` to reuse a checkout) and regenerates `crates/rzh-math/tests/golden/gmath.txt`, then builds the EAC codecs and regenerates `crates/rzh-compress/tests/golden/refpack.txt` and `crates/rzh-vfs/tests/golden/vfs.txt`, then builds the INI transcription and regenerates `crates/rzh-ini/tests/golden/ini.txt`, then builds the CSF transcriptions and regenerates `crates/rzh-formats/tests/golden/{csf,tga}.txt`. |

## Numerics

The WWMath transcription covers only operations that are IEEE-754 exact on
every conforming platform (add/sub/mul/div, `sqrt`, `fabs`, `floor`,
`ceil`, and integer bit work), where GameMath (the game's pinned FreeBSD
msun float library), the system libm and Rust's `f32` methods produce
bit-identical results — verified empirically against the pinned GameMath
sources (see ADR-001). The harness is compiled with `-ffp-contract=off`
so `a*b+c` never fuses into an FMA (Rust never contracts; neither does the
game's MSVC build).

The transcendental entry points (`Sin`…`Atan2`, `Pow`, `Exp`, `Log`,
`Fast_*`) are deliberately absent from `gen_golden.cpp`: the generic Rust
`libm` crate is **not** bit-identical to the game's GameMath pin for them.
They are covered by `gen_gmath_golden.c` (see above), which links the pin
itself; the `Fast_*` tables are still pending (they land with the
quaternion port).

Several quirks of the original headers are transcribed verbatim and pinned
by the vectors (they are behaviour): `Vector3::Quick_Length`'s `min = mid`
sort slip, `Vector4`'s compound subtraction adding W, the out-parameter
`Vector4::Lerp` writing the W interpolation into X, `Vector2`'s free
`Normalize` dividing by the reciprocal square root, and `Quick_Distance`
discarding its absolute values.

## Workflow

```sh
tools/differential/regenerate.sh          # regenerate the golden files
cargo test --workspace                    # Rust must match byte-for-byte
git diff crates/*/tests/golden            # review before committing

# §6 budget comparison (Phase 1 exit criterion, docs/benchmarks.md):
cargo bench --workspace                   # Rust side
tools/differential/bench_baseline.sh     # C++ side, same corpora
```

Golden files are committed; never edit them by hand. If a test fails, either
the Rust port or the C++ transcription drifted from the original: compare
against the real sources at the baseline tag `legacy-cpp-baseline` and fix
whichever side is wrong.

## Lifetime

This harness exists only while the C++ tree does. It is the minimal seed of
the oracle idea from migration plan §3.2 — the full oracle (headless replay
simulator with trace instrumentation) supersedes it in Phase 0/1, and both
are removed at the cut-over milestone M5 when Rust-vs-Rust golden traces take
over.
