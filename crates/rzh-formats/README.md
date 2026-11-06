# rzh-formats

Zero Hour asset-format parsers for [RustZeroHour](../../README.md) — the
stage-B "Data" crate of the [migration plan](../../docs/RUST_MIGRATION_PLAN.md)
(§3.1/§3.4): parsing only, byte-exact (parity level **P0**), no
dependencies, `#![forbid(unsafe_code)]`.

## Modules

| Module | Format | Ported from (baseline `571a7c5`) | Status |
|---|---|---|---|
| [`csf`](src/csf.rs) | `.csf` compiled string tables | `GameText.cpp` (runtime reader), `Core/Tools/Babylon/expimp.cpp` (writer) | done |
| [`tga`](src/tga.rs) | `.tga` Targa images | `WWLib/TARGA.cpp` (the class behind the texture loader, the font loader and the screenshot path) | done |
| [`chunk`](src/chunk.rs) | the W3D chunk container | `WWLib/chunkio.cpp` (`ChunkSaveClass`/`ChunkLoadClass`, the IFF-style framing every W3D asset is stored in) | done |
| [`w3d_mesh`](src/w3d_mesh.rs) | W3D mesh chunk trees | `WW3D2/meshmdlio.cpp` (`MeshModelClass::Load_W3D` and every sub-reader), the geometry/AABTree/vertex-material/texture data paths, the load-path half of `meshmatdesc.cpp`, the `ShaderClass` bit packing and the WWLib CRC-32 | done |
| [`w3d_htree`](src/w3d_htree.rs) | W3D hierarchy trees (skeletons) | `WW3D2/htree.cpp` (`HTreeClass::Load_W3D`, `read_pivots`, the `stricmp` bone lookup), `WW3D2/pivot.cpp`, the WWMath `Build_Matrix3D`/`Translate`/`postMul` base-pose formulas | done |
| [`w3d_hanim`](src/w3d_hanim.rs) | W3D raw animations | `WW3D2/hrawanim.cpp` (`HRawAnimClass::Load_W3D`, the channel routing, `Get_Translation`/`Get_Orientation`/`Get_Transform`), `WW3D2/motchan.cpp` (the classic motion/bit channels), the WWMath `Fast_Slerp`/`Fast_Acos`/`Fast_Sin` tables and the pinned GameMath entry points they call | done |
| [`w3d_hcanim`](src/w3d_hcanim.rs) | W3D compressed animations | `WW3D2/hcanim.cpp` (`HCompressedAnimClass::Load_W3D`, the flavor-switched channel routing, the stateful queries), `WW3D2/motchan.cpp` (the time-coded, time-coded-bit and adaptive-delta channels with the filtertable and the two-frame decompression cache) | done |
| [`w3d_hlod`](src/w3d_hlod.rs) | W3D HLOD assemblies | `WW3D2/hlod.cpp` (`HLodDefClass::Load_W3D`, `read_header`, `read_proxy_array`, the sub-object arrays, the file's own proxy records) | done |
| [`dds`](src/dds.rs) | `.dds` DDS textures | `WW3D2/ddsfile.cpp` (the name munging, the legacy `DDSURFACEDESC2` decode, the FourCC conversion, the mip-level layout, the DXT pixel queries and block decompressor, the surface copies and the HSV recolour stack) | done |
| [`map_chunk`](src/map_chunk.rs) | the map data-chunk container | `Common/System/DataChunk.cpp` (the `CkMp` table of contents, `DataChunkInput`'s registered-parser walk, `DataChunkOutput`'s patch-in-place writer, the load-path half of `Dict` and the name-key interning) | done |
| [`map_heightmap`](src/map_heightmap.rs) | the map height-map payload | `WorldHeightMap.cpp` (`ParseHeightMapData`/`ParseSizeOnly`, the height-map readers of both constructor load modes, the `isValid*` size contracts, the version-1 in-place downsample) | done |
| [`map_blendtile`](src/map_blendtile.rs) | the map blend-tile payload | `WorldHeightMap.cpp` (`ParseBlendTileData`, the largest map payload: the four per-cell index arrays, the flip/cliff bitmaps with the version-7 row-length remap and the pre-version-7 height derivation, the texture and edge classes, the blend records with their `FLAG_VAL` sentinels, the cliff u/v tables, the version-1 downsample, the 3-way gate, the post-walk patch loop) | done |
| [`map_lighting`](src/map_lighting.rs) | the map global-lighting payload | `WorldHeightMap.cpp` (`ParseLightingDataChunk`, the full load's other payload: the raw time-of-day word, the init pass over both light tables, the interleaved per-time-of-day terrain/object light blocks with their version gates, the bytes-left-gated shadow colour) | done |
| [`map_objects`](src/map_objects.rs) | the map objects-list payload | `WorldHeightMap.cpp` (`ParseObjectsDataChunk`/`ParseObjectData`, the logical load's other payload: the location triple with the version-gated z zeroing, the `normalizeAngle` wrap in its mixed precision, the editor flags, the type name, the property dict with the well-known-key derivations, the z-range drops, the head-overwrite list semantics) | done |
| — | map payloads | the `WorldInfo`/`PolygonTriggers`/`SidesList` readers of `WorldHeightMap.cpp` on top of [`map_chunk`] | next |
| — | WND layouts | `GameWindowManager`, GUIEdit | planned |
| — | fonts | `WW3D2/font3d.cpp` (the Targa font-strike loader) | planned |

## Design

- **Format vs. consumer.** The parser returns the file's raw content,
  byte-faithful. Normalizations the original applies on top of a format
  (the string manager's space stripping, first-string-only selection, the
  language defaulting) are explicit `game_*` helpers pinned by the same
  goldens, so a future consumer crate reproduces the original behaviour
  without hiding it in the parser.
- **Byte-exact round-trips.** `parse`/`to_bytes` round-trip any accepted
  file byte-for-byte (raw bytes preserved, counts canonicalized the way
  Babylon writes them). Where the C++ reads out of bounds or reuses stale
  buffers on malformed input, the port returns `Err` — see each module's
  "Differences from the C++" notes.
- **Raw code units.** UTF-16 text is kept as `Vec<u16>` (not `String`) so
  unpaired surrogates, embedded NULs and the `0xFFFF` loader quirk survive
  round-trips; `String` views are helpers.

## Testing (plan §6)

```sh
cargo test  -p rzh-formats      # golden vectors + property tests
cargo bench -p rzh-formats      # §6 benchmarks (docs/benchmarks.md)
tools/differential/regenerate.sh            # regenerate the goldens (C++ harness)
tools/differential/bench_baseline.sh        # C++ baseline, same corpora
```

- `tests/golden/csf.txt` — generated by
  [`tools/differential/gen_csf_golden.cpp`](../../tools/differential/):
  the transcribed Babylon writer pins the byte direction, the transcribed
  `parseCSF` pins the runtime view (label/speech C-string semantics, the
  invert-until-zero truncation, `stripSpaces`).
- `tests/golden/tga.txt` — generated by
  [`tools/differential/gen_tga_golden.cpp`](../../tools/differential/):
  the transcribed `TARGA.cpp` class pins both directions — the reader
  (Open footer sniff, the Load switch with its missing mono-encoded case,
  the truecolor-only invert, the origin normalization, the RLE decoder)
  and the writer (Save with the type bump, the always-present footer,
  the stamped extension, the RLE encoder).
- `tests/golden/chunk.txt` — generated by
  [`tools/differential/gen_chunk_golden.cpp`](../../tools/differential/):
  the transcribed `chunkio.cpp` classes pin both directions — the writer
  (header patching, the sub-chunk flag, size accumulation, micro chunks)
  and the reader (the walk: the equality-based parent exhaustion test
  that lets over-declared children walk past their parent's boundary,
  the clamped reads, the unclamped close seeks, the short-read
  consumes-then-fails quirk, the 255-deep stack boundary, the
  read-from-container case and the IOVector read helpers' pointer-size
  bug).
- `tests/golden/hcanim.txt` — generated by
  [`tools/differential/gen_hcanim_golden.cpp`](../../tools/differential/):
  the transcribed `hcanim.cpp` load path pins both compression flavors'
  channel readers — the time-coded packet scan with its look-ahead cache
  and binary search, the binary-movement flag, the numeric time-code
  conversion that rounds above 2^24, the adaptive-delta nybble
  decompression over the 256-entry filtertable with the two-frame cache
  (the sliding window, the cache-continue off-by-one, the never-reset
  `done` flag), the dead bit-channel `DefaultVal`, the flavor-switched
  routing (including the unknown flavor that reads no channels and the
  cross-shape payloads whose reader follows the header's flavor), the
  multi-word `VectorLen`-2 writes, and the error verdicts.
- `tests/golden/htree.txt` — generated by
  [`tools/differential/gen_htree_golden.cpp`](../../tools/differential/):
  the transcribed `htree.cpp` load path pins the hierarchy header decode,
  the pivot reads with their mixed-precision base-transform build (the
  `f64`-promoted `Build_Matrix3D` with its `2.0f` slip, `Translate`, the
  `submul` `postMul`), the pre-3.0 root synthesis and parent-index fixup
  (including the `0xFFFFFFFE` wrap that mints a second root), the
  default-constructed pivots a missing pivots chunk leaves, the
  failing-sub-read-fails-the-load contrast with the mesh loader, and
  the `stricmp` bone lookups the skinning path resolves bones against.
- `tests/golden/mesh.txt` — generated by
  [`tools/differential/gen_mesh_golden.cpp`](../../tools/differential/):
  the transcribed mesh loader pins the header, geometry, material tree
  (passes, prelit selection, alternates), AABTree, the CRC-based UV
  dedup and the discarded sub-reader error.
- `tests/golden/hlod.txt` — generated by
  [`tools/differential/gen_hlod_golden.cpp`](../../tools/differential/):
  the transcribed HLOD loader pins the `strdup` run-through names, the
  per-LOD sub-object arrays, the discarded-error family, the unbalanced
  failure contrast, the `strlcpy` proxy truncation and the last-wins
  repeated arrays.
- `tests/golden/dds.txt` — generated by
  [`tools/differential/gen_dds_golden.cpp`](../../tools/differential/):
  the transcribed `ddsfile.cpp` reader pins the mip chains, the
  reduction family, the cube maps and volumes, the DXT pixel corpora,
  every destination format's block dump, the copy family and the name
  munging.
- `tests/golden/mapheightmap.txt` — generated by
  [`tools/differential/gen_mapheightmap_golden.cpp`](../../tools/differential/):
  the transcribed `WorldHeightMap.cpp` height-map load path (both
  constructor modes' `HeightMapData` readers, the `isValid*` size
  contracts, the version-1 downsample) pins the version family (0–5, the
  v1 mode divergence), the boundary family (raw int storage, the
  negative count refusal), every size-contract rejection (including the
  per-axis border ceiling and the data-left check's unsigned view that
  lets the under-declared chunk pass), the read-into-the-next-chunk
  samples, the zero-filled short tail, the leftover-skipping close, the
  global-scope rule, last-wins repeats, the absent payload and the walk
  refusals.
- `tests/golden/maplighting.txt` — generated by
  [`tools/differential/gen_maplighting_golden.cpp`](../../tools/differential/):
  the transcribed `WorldHeightMap.cpp` global-lighting load path
  (`ParseLightingDataChunk` over the constructor's full-load walk) pins
  the version family (0–4, the ceiling behaviour both ways), the
  interleaved per-time-of-day block order, the init values the gates
  leave behind, the raw unvalidated time-of-day word, the bytes-left
  shadow gate (a version-1 chunk with trailing bytes reads the
  colour), the raw bit-pattern transport (-0.0, subnormals, f32::MAX,
  a quiet NaN), the zero-filled short reads, the under-declared
  chunk's reads past its declared end, the leftover-skipping close,
  the global-scope rule, last-wins repeats, the absent payload and
  the walk refusals.
- `tests/golden/mapobjects.txt` — generated by
  [`tools/differential/gen_mapobjects_golden.cpp`](../../tools/differential/):
  the transcribed `WorldHeightMap.cpp` objects-list load path (the
  ObjectsList container reader, the Object reader, the MapObject
  construction) pins the version family (0–4, the gates hanging off
  the Object's own word), the exact-type derivations, the z-range
  drops, the `normalizeAngle` wrap in its mixed double/single
  precision ((float)π wrapping negative, its negation positive, the
  NaN early 0, the infinite-angle refusal), the name shapes, all five
  pair types with the duplicate-key update and the interning-order
  storage, the corrupt dict type, the head-overwrite list semantics,
  the scope rules, the leftover-skipping closes, the under-declared
  Object, the short file's zero-filled pairs and the walk refusals.
- `tests/golden/mapchunk.txt` — generated by
  [`tools/differential/gen_mapchunk_golden.cpp`](../../tools/differential/):
  the transcribed `DataChunk.cpp` classes pin both directions — the
  reader (the prepend-list TOC with its duplicate-name/duplicate-id
  lookups, the count>0-but-EOF header refusal, the registered-parser
  walk with its scope rules and last-registered-wins matching, the
  signed data-left accounting, the zero-filled short reads, the
  NUL-truncated string views) and the writer (the placeholder-patched
  headers, the reverse-order TOC, the `u16` string-length wraps, the
  `u8` name truncation), plus the load-path `Dict` and the name-key
  interning.
- `tests/properties.rs` — round-trip properties over arbitrary structures
  and a never-panic property over arbitrary bytes.

## License

GPL-3.0-or-later; original code Copyright 2025 Electronic Arts Inc., Rust
port Copyright 2026 the RustZeroHour contributors (see `LICENSE.md`).
