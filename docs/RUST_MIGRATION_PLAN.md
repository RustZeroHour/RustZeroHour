# RustZeroHour: Rust Migration Plan

| | |
|---|---|
| **Status** | Draft v1.0 |
| **Date** | 2026-11-01 |
| **Baseline** | commit `571a7c5e67ccc728843aea174365face0161f2ff` (HEAD of this fork on the date above) |
| **Lineage** | EA source release → `TheSuperHackers/GeneralsGameCode` → this fork |
| **License** | GPL-3.0 (inherited; the Rust port is a derivative work and stays GPL-3.0) |

---

## 1. Goal and non-goals

**Goal.** Replace the entire C++ code base with Rust and evolve the result into an
**independent RTS game** with its own identity, its own multiplayer, and no dependency on EA-owned
content. The game is inspired by *Command & Conquer: Generals – Zero Hour*.

**Explicit decisions (already taken).**

- The Rust game is **not** compatible with retail clients: no retail multiplayer, no retail replay
  or save compatibility. Compatibility is used only as a *temporary testing aid* (see §3.2).
- **Zero Hour is the only target.** The base-game tree (`Generals/`) is out of scope.

**Non-goals.**

| Dropped | Why |
|---|---|
| `Generals/` code tree (~440K lines) | Zero Hour is the target; shared code lives in `Core/` |
| Retail 1.04 network/replay/save compatibility | Decision above |
| DirectX 8 renderer, Miles Sound System, Bink, MaxSDK plugins, DbgHelp/MiniDumper | Proprietary or Windows-only; re-implemented, not ported |
| GameSpy, WOL, EA Browser engine | Dead services |
| VC6 / STLport support (`stlport.diff`) | Irrelevant to Rust |
| MFC-based tools (first iteration) | Deferred to Phase 8 |
| Bit-exact reproduction of retail behaviour | Not needed once compatibility is dropped |

---

## 2. What the code base looks like (measured)

All numbers come from the baseline commit and count lines in `*.cpp *.h *.c *.hpp *.inl`
(comments and blank lines included). Reproduction commands are in Appendix B.

**Total: 1,592,372 lines in 4,035 files.**

| Area | Lines | Strategy |
|---|---:|---|
| `Core/GameEngine` | 195K | **Port** (common, game logic, client, network) |
| `Core/GameEngineDevice` | 71K | **Re-implement** (W3DDevice 55K, Miles 3K, video 1.4K, Win32 2K) |
| `Core/Libraries` | 183K | **Mixed**: WW3D2 75K, WWLib 39K, WWMath 30K, WWAudio 12K, debug/profile 13K, Compression 5K |
| `GeneralsMD/Code/GameEngine` | 309K | **Port**: the heart (GameLogic 170K, GameClient 49K, Common 32K) |
| `GeneralsMD/Code/GameEngineDevice` | 24K | **Re-implement** (renderer glue) |
| `GeneralsMD/Code/Libraries` | 53K | **Mixed** (WWVegas additions) |
| `Dependencies` | 7K | **Replace** (Utility, Miles, Bink headers, etc.) |
| `Core/Tools` + `GeneralsMD/Code/Tools` | 214K + 91K | **Defer** (Phase 8) |
| `Generals/` (base game) | 440K | **Drop** |
| `Tests`, `Benchmarks` | 3.5K + 0.9K | **Reference** for the first Rust tests |

Port ≈ 500K lines of engine/game code. Re-implement ≈ 150K lines of renderer, audio, video and
platform code (the *behaviour* is rebuilt, the code is not translated).

### Key observations

1. **Shared code is conditional.** `Core/` contains `RTS_GENERALS` (117 refs) and `RTS_ZEROHOUR`
   (29 refs) branches. We port the Zero Hour branch only.
2. **Global singletons everywhere.** About 137 `TheXxx` global pointers are declared in headers
   (approximate grep). The Rust design replaces them with explicit contexts.
3. **Data-driven module system.** GameLogic/Object holds roughly 210 module source files
   (86 Update, 30 Behavior, 21 Upgrade, 20 Collide, 13 Contain, 12 Die, 11 SpecialPower,
   8 Create, 8 Body, 3 Damage). 195 `.cpp` files contain INI `FieldParse` tables.
4. **Retail-compatibility switches.** 221 files reference `RETAIL_COMPATIBLE_*` macros
   (`_CRC` 478 refs, `_AIGROUP` 137, `_PATHFINDING` 111, `_XFER_SAVE` 76, others smaller).
   They gate behaviour fixes. Because we drop retail compatibility, **the Rust port follows the
   non-retail (fixed) branch**. Each switch is verified individually in Phase 0.
5. **Platform coupling.** 155 files include `windows.h`; 13 reference `d3d8`; 12 mention Bink
   (an optional FFmpeg path already exists via vcpkg); 5 use Miles; 21 contain inline assembly.
6. **Safety nets that already exist.**
   - `GeneralsReplays` submodule: 10 Zero Hour replays (+ 6 Generals) and 4 maps.
   - A `-headless -replay` mode (`Core/GameEngine/Source/Common/ReplaySimulation.cpp`).
   - Game-state serialization/CRC layer (`Xfer.h`, `XferCRC.h`, `XferDeepCRC.h`, `XferSave.h`,
     `XferLoad.h`): a canonical description of the whole simulation state.
   - A small GoogleTest suite (AsciiString, INI, FileSystem, Compression, WWMath, ...) and
     Google Benchmark micro-benchmarks.
7. **Retail replays are toolchain-bound.** `TESTING.md` states that retail-compatible replay
   checks require a VC6 optimized build. Bit-exact floating-point equality with retail is
   therefore tied to a specific compiler. We do not chase it (see §3.2).
8. **Network layer.** `Core/GameEngine/Source/GameNetwork` is ~31K lines: frame-data lockstep,
   custom UDP transport, NAT handling, LAN API, GameSpy glue. It is a design reference, not a
   port target.

---

## 3. Strategy

### 3.1 Approach: parallel rewrite with differential testing

Module-by-module in-process FFI ("strangler") works for leaf libraries but breaks down for
game logic: ~190K lines of GameLogic are tied together through global singletons and deep class
inheritance, and interleaving them with Rust costs more than the port itself. The strategy is
therefore staged:

| Stage | What | How it is validated |
|---|---|---|
| A. Leaf libraries | math, strings/ids, RNG, CRC, compression, file system | Rust crates; a temporary C ABI shim lets C++ test harnesses compare outputs on identical inputs |
| B. Data | INI, maps, W3D, textures, string tables, UI layouts | Rust loaders; dumps compared with C++ dumps |
| C. Simulation | Standalone **headless** Rust sim | Same command streams run through the headless C++ oracle and the Rust sim; traces compared (§3.2) |
| D. Client | New renderer, UI, audio, input on top of the Rust sim | Visual/manual review, scenario tests |
| E. Cut-over | Delete C++ from the build | Rust-vs-Rust golden traces replace the oracle |

FFI is a **test tool**, not an architecture. Nothing shipped depends on it.

### 3.2 What "parity" means

Retail compatibility is dropped, but we still need a way to know the port is *right*. In an RTS,
tiny floating-point differences change pathfinding, which changes everything later, so full-match
trace equality is unrealistic. Parity is defined in levels:

| Level | Applies to | Requirement |
|---|---|---|
| **P0** exact | parsing, serialization, format conversion, CRC, RNG, compression | byte-exact output |
| **P1** discrete state | scripted scenarios, short horizon | object set, owners, health, queues, money identical |
| **P2** continuous state | same scenarios | positions/velocities within tolerance ε; identical paths for identical pathfinder inputs |
| **P3** behavioural | long AI-vs-AI matches | not trace-compared; accepted by invariants (no crash, economy flows, win/lose works, no desync Rust-vs-Rust, performance budget) |

**The oracle.** A C++ Zero Hour build compiled with the `RETAIL_COMPATIBLE_*` switches **off**
(same branches the Rust port follows) plus trace instrumentation behind a CMake option
(per-frame state hash, on-demand state snapshots, option to ignore the recorded CRC in `.rep`
files). Its acceptance test is self-consistency: two runs of the same input give identical traces.
It does **not** need to reproduce retail results.

**Inputs.** The 10 existing Zero Hour replays are too few. We also record a library of
purpose-built scenarios (one mechanic each) with the C++ oracle *before* it is deleted.
A small tool converts legacy `.rep` command streams into the new command-stream format, used
for oracle comparison only.

After cut-over (M5) the oracle is retired and **Rust-vs-Rust golden traces** take over.

### 3.3 Porting rules

1. Port the Zero Hour branch and the non-retail branch; drop dead `#if` code.
2. Preserve algorithms, data semantics and **order of operations** first; improve structure
   second. Do not mimic C++ inheritance; use traits/enums and handles.
3. Every ported file records its origin: `// Ported from <path> @ 571a7c5`. Keep the original
   copyright notices (EA, TheSuperHackers contributors) and add SPDX headers.
4. Sim crates use `#![forbid(unsafe_code)]`. `unsafe` is allowed only in platform/FFI crates,
   with `// SAFETY:` comments and review.
5. No global mutable state. State lives in explicit contexts (`Sim`, `Client`, `Services`).
6. **Sim determinism rules** (apply from day one):
   - single-threaded sim; no wall-clock, no randomness except the sim RNG;
   - no iteration over `HashMap`/`HashSet` in the sim (use `IndexMap`, `BTreeMap` or arenas with
     stable order);
   - `f32` like the original, same operation order, no fast-math, no fused multiply-add;
   - transcendental functions from the pure-Rust `libm` crate, not the platform libm;
   - fixed-width integers in anything hashed or serialized (no `usize`);
   - integer overflow semantics chosen explicitly per site (wrapping vs checked), because C++
     signed overflow and Rust debug builds differ;
   - objects referenced by handle/ID, never by pointer or `Rc<RefCell<_>>`.
7. **One-way dependencies.** `sim` never depends on client, render, audio or net. The client reads
   sim state through read-only views/snapshots and refers to objects by ID.

### 3.4 Proposed workspace layout

```
crates/
  rzh-util        CRC, sim/client RNG, name-key interning, small shared types
  rzh-math        deterministic math for the sim; conversions to render types
  rzh-vfs         virtual file system, BIG archives, loose-file/mod overlay
  rzh-compress    EA codecs (RefPack etc.)
  rzh-ini         INI lexer/parser + derive-based schema layer
  rzh-formats     W3D, map, TGA/DDS, CSF, WND, ... (parsing only)
  rzh-data        typed game data (templates, weapons, armor, locomotors, ...)
  rzh-sim         deterministic simulation (no I/O, no rendering, no threads)
  rzh-replay      command streams, snapshots, trace tools, legacy .rep import
  rzh-client      drawables, camera, selection, view-side logic
  rzh-render      wgpu renderer
  rzh-ui          window/layout/control-bar framework
  rzh-audio       audio engine
  rzh-video       video playback
  rzh-net         lockstep, transport, lobby client
  rzh-app         the game binary
  rzh-cli         dev tools: inspect, dump, trace-diff, convert
  rzh-oracle-ffi  TEMPORARY C ABI used only by differential tests (removed at M5)
```

The C++ tree stays where it is until M5, so history and diffs stay readable.

---

## 4. Phases

Sizes are **relative** (S < M < L < XL < XXL), not calendar time. Re-estimate at M1 using the
velocity actually measured (C++ lines ported per week).

### Phase 0: Groundwork and baseline (S)

- Tag the baseline (`legacy-cpp-baseline` at `571a7c5`). Keep upstream as a read-only remote.
  Policy: no automatic upstream merges; cherry-pick *logic* fixes manually until M2, none after.
- Rewrite `README`, `CONTRIBUTING`, `AI_POLICY`, `SECURITY` for this fork (they currently describe
  the upstream C++ project and its maintainers).
- Root Cargo workspace, pinned toolchain, `rustfmt`, `clippy -D warnings`, `cargo-deny`
  (license check), CI on Windows/Linux/macOS. Keep the existing C++ CI.
- ADR process in `docs/adr/` (§5).
- Developer setup: original game data is read from a user-owned install via `RZH_GAME_DIR`.
  **Never commit EA assets.**
- Inventory tooling → `docs/inventory/`: LOC census, include graph, global singleton list,
  `RETAIL_COMPATIBLE_*` switch table with the chosen branch, `RTS_GENERALS`/`RTS_ZEROHOUR` forks.
- Build the C++ oracle (Release, Zero Hour), add trace instrumentation, define **trace format v1**,
  and verify determinism (two identical runs → identical traces).
- **Exit:** CI green on both stacks; inventory published; oracle produces stable traces for all
  10 Zero Hour replays.

### Phase 1: Foundation crates (M) — **status: complete (2026-11-03)**

Exit criteria met: all P0 golden/property tests pass, criterion benchmarks
exist for every foundation crate, and the §6 budget comparison against the
C++ baseline (same corpora, same loop shapes, `tools/differential/bench_baseline.sh`)
is recorded in [`docs/benchmarks.md`](benchmarks.md) — 29 of 32 comparisons
within budget, the three exceptions justified in
[ADR-003](adr/003-benchmark-budget-exceptions.md). The budget run also
surfaced and fixed four structural deviations from the original's shape
(RNG carry chain, `Matrix3D::mul` unroll, RefPack `matchlen`, INI
`readLine` buffer reuse) — all re-verified byte-exact against the goldens.
The remaining Phase 0 oracle item is the instrumented C++ build itself
(Windows toolchain, `RZH_ORACLE_TRACE` instrumentation); its trace format
is now specified in [`docs/TRACE_FORMAT_V1.md`](TRACE_FORMAT_V1.md).

- `rzh-util`: CRC, logic RNG and client RNG (ported exactly), name keys, ids.
- `rzh-math`: vectors/matrices/quaternions/boxes/frustum subset needed by the sim (from WWMath).
- `rzh-vfs`: file systems and BIG archives; `rzh-compress`: EA codecs.
- Logging/diagnostics via `tracing` plus a panic hook (replaces debug/profile/MiniDumper);
  optional Tracy integration.
- Port the existing GoogleTests; add differential tests (C++ harness dumps vectors → committed
  golden files) and `proptest` property tests.
- **Exit:** all P0 checks pass; benchmarks within budget (§6).

### Phase 2: Data and asset formats (M–L) — **status: started (2026-11-03)**

`rzh-formats` exists and ten modules are done to the Phase 1 standard:

- the **CSF** string tables: the runtime reader (`GameText.cpp::parseCSF`)
  and the Babylon writer (`expimp.cpp::generate_csf`) are transcribed in
  `tools/differential/gen_csf_golden.cpp`, the goldens pin both directions
  (writer bytes and the game-visible view, quirks included), proptests
  round-trip arbitrary structures, and the §6 budget comparison extends to
  the `csf/…` benches (all within budget, docs/benchmarks.md);
- the **TGA** images: the WWLib `TARGA.cpp` class (the loader the texture
  and font paths drive, plus the screenshot/ImagePacker `Save` paths) is
  transcribed in `tools/differential/gen_tga_golden.cpp` — reader and
  writer directions, RLE decoder and encoder included — with the golden
  script pinning the header/palette/image views, the extension dumps and
  the re-encoded bytes; the `tga/…` benches are all within budget (the
  raw-parse check surfaced the `invert_in_place` depth specialization,
  docs/benchmarks.md finding 5).
- the **W3D chunk container**: the IFF-style framing layer every W3D asset
  is stored in (`WWLib/chunkio.cpp`, `ChunkSaveClass`/`ChunkLoadClass`)
  is transcribed in `tools/differential/gen_chunk_golden.cpp` and ported
  as `rzh-formats::chunk` — both directions, micro chunks included — with
  the goldens pinning the header patching, the equality-based parent
  exhaustion test (over-declared children walk past their parent's
  boundary, as found), the clamped reads and unclamped close seeks, the
  short-read consumes-then-fails quirk, the 255-deep reader stack
  boundary and the IOVector read helpers' pointer-size bug; the
  `chunk/…` benches are within budget on both comparisons
  (docs/benchmarks.md). This is the base the W3D mesh/skeleton/animation/
  HLOD parsers (the next slices) build on.
- the **W3D mesh chunk tree**: the data path of
  `MeshModelClass::Load_W3D` (`WW3D2/meshmdlio.cpp` — the header, the
  geometry chunks, the material tree with its passes, prelit selection
  and alternate-description routing, and the AABTree; plus the geometry,
  vertex-material, texture and AABTree readers it drives, the
  `ShaderClass` bit packing and the WWLib CRC-32 the UV dedup uses) is
  transcribed in `tools/differential/gen_mesh_golden.cpp` and ported as
  `rzh-formats::w3d_mesh` — the first real W3D model module on top of the
  container, unlocking the Phase 4 model viewer (M3). The goldens pin 30
  mesh payloads: the modern full mesh, the prelit selection matrix (the
  game's prelit mode is never changed — the multi-texture check is dead
  code), the pre-3.0 bone-link fixup, the legacy v3 materials with their
  CRC/name dedup, the alternate-material routing and merge (including
  shader-only alternates counting as empty), DCG/DIG colour-array
  ownership, the discarded sub-reader error (a failing reader aborts the
  walk, the load still succeeds), the lowercased texture identity with
  duplicate list entries, the u16 TriIndex truncation and the pass/stage
  bounds standing for the original's out-of-bounds reads; the
  `mesh` bench is within budget
  (docs/benchmarks.md).
- the **W3D hierarchy trees**: the data path of `HTreeClass::Load_W3D`
  (`WW3D2/htree.cpp` — the 36-byte header, the 60-byte pivots with the
  base-pose transforms `read_pivots` builds through the WWMath
  mixed-precision `Build_Matrix3D`/`Translate`/`postMul` formulas, the
  pre-3.0 root synthesis and parent-index fixup, and the `stricmp` bone
  lookups the skinning path resolves bones against; plus the
  `PivotClass` constructor state the pivot array is born with) is
  transcribed in `tools/differential/gen_htree_golden.cpp` and ported
  as `rzh-formats::w3d_htree` — the skeleton module the skinning path
  and the HLOD assemblies both sit on. The goldens pin 36 hierarchy
  payloads: the modern full tree and the 24-bone skeleton shape, the
  pre-3.0 fixup (with the `0xFFFFFFFE` parent wrap that mints a second
  root, and the count wrap that fails the load), the default-constructed
  pivots a missing pivots chunk leaves, the failing-sub-read-fails-the-
  load contrast with the mesh loader's discarded error, the unresolvable
  parent stated as a contract (the original stores a dangling pointer
  the first `Base_Update` would dereference) and the negative-zero
  translation rounding through the identity rows; the `htree` bench is
  within budget (docs/benchmarks.md).
- the **W3D raw animations**: the data path of `HRawAnimClass::Load_W3D`
  (`WW3D2/hrawanim.cpp` — the 44-byte header with its integer frame
  rate and the strcpy/strlcat name composition that runs through the
  header's frame words when a name field has no NUL, the pre-3.0 pivot
  shift, the channel-to-slot routing with its discard of out-of-range
  and unknown-typed channels, and the `Get_Translation` /
  `Get_Orientation` / `Get_Transform` / `Get_Visibility` queries the
  tree update samples; plus the classic `MotionChannelClass` /
  `BitChannelClass` readers from `WW3D2/motchan.cpp` with the
  exporter-junk seek and the disabled `Do_Data_Compression`, and the
  WWMath frame-math stack the orientation paths drive — `Float_To_Long`
  (the deterministic `gm_lrintf`), the `Fast_Acos`/`Fast_Sin` tables,
  `Fast_Slerp` and `Build_Matrix3D`, with the pinned GameMath
  `acosf`/`sinf`/`lrintf` transcribed privately to keep the crate
  dependency-free) is transcribed in
  `tools/differential/gen_hanim_golden.cpp` and ported as
  `rzh-formats::w3d_hanim` — the module that fills a hierarchy tree's
  transforms, on top of `w3d_htree` (the tree lookup the original's
  asset manager performs is the parse callback, mirroring
  `Get_HTree`'s lowercase exact match). The goldens pin 38 animation
  payloads: the modern full animation and the 24-bone routing corpus,
  the slerp branches (the table path, the near-parallel linear branch,
  the hemisphere flip and the `gm_acosf` fallback window), the
  pre-3.0 pivot shift with its `0xFFFF` wrap discard, the
  strcpy-run name composition (the 17-byte hierarchy key that
  resolves through the frame-count word, the 16-byte trap key that
  must miss, the unterminated animation name and the 31-character
  truncation), the numeric frame-rate conversion (a file storing the
  bits of `50.0f` loading as `1112014848.0`, the division by zero, the
  f32 rounding), the negative-as-int frame count, the identity
  blending below a channel's first frame, the exporter-junk seek, the
  newest-wins duplicate channel and the empty/negative-span stated
  contracts; the `hanim` benches are within budget on both comparisons
  (docs/benchmarks.md).
- the **W3D compressed animations** — the named follow-up slice: the
  data path of `HCompressedAnimClass::Load_W3D` (`WW3D2/hcanim.cpp` —
  the 44-byte header whose `Flavor` field picks the channel reader,
  read with the raw header's `sizeof` (the same 44, a stated
  coincidence), the strcpy-run name composition, the flavor-switched
  channel routing that reads no motion channel at all for an unknown
  flavor (the release-assert contract) and the stateful queries the
  tree update samples; plus the three compressed channel readers from
  `WW3D2/motchan.cpp` — `TimeCodedMotionChannelClass` (the packet scan
  with its look-ahead cache, the `leftIdx ^ dx` binary search and the
  binary-movement flag), `TimeCodedBitChannelClass` (the linear scan
  with its underflow clamp and the dead `DefaultVal`) and
  `AdaptiveDeltaMotionChannelClass` (the 9-byte nybble packets over the
  256-entry filtertable, the two-frame decompression cache with its
  sliding window and the cache-continue path that re-applies the delta
  for `CacheFrame+1` — the original's off-by-one — and the
  never-reset `done` flag that stops every vector index after the
  first block once the first reaches the target frame)) is transcribed
  in `tools/differential/gen_hcanim_golden.cpp` and ported as
  `rzh-formats::w3d_hcanim`, on top of `w3d_htree` through the same
  parse callback. The goldens pin 43 compressed animation payloads:
  both flavors' modern full animations, the binary-movement and
  last-packet branches, the packet-search rewind, the `VectorLen`-2 X
  channel whose multi-word write reaches the caller's second slot and
  clobbers the transform, the slerp branches, the TIMECODED_X..Q type
  values that never route, the 2^24+1 time-code whose numeric f32
  conversion rounds the interpolation ratio, the cross-shape flavor
  reads (the reader follows the header's flavor, not the payload's
  shape), the adaptive-delta filter-table halves and sign-extended
  nybbles, the getframe clamp, the sliding window and the far-jump
  off-by-one, the dead `DefaultVal` and the negative-frame bit clamp,
  and the error verdicts (the mid-chunk file truncation, the
  zero-time-code bit channel, the channel chunk smaller than the
  adaptive-delta struct); the `hcanim` benches are within budget on
  all three comparisons (docs/benchmarks.md — the budget run itself
  reshaped the port: the decompression walks are free functions over
  the channel's immutable parts so the cache branches write straight
  into their rows, and `getframe` is force-inlined).
- the **W3D HLOD assemblies** — the named follow-up slice: the
  data path of `HLodDefClass::Load_W3D` (`WW3D2/hlod.cpp` — the
  40-byte header whose two `strdup`'d names run through the
  following header words when a field has no NUL (the deterministic
  Name-into-HierarchyName run, capped at the struct on both sides),
  the version field that is never checked, the per-LOD sub-object
  arrays (8-byte array headers, 36-byte model structs with the
  strdup'd names capped at the field), and the aggregate and proxy
  arrays; plus the file's own ProxyRecordClass/ProxyArrayClass the
  proxy path fills — the `strlcpy` that truncates a 32-character
  proxy name to 31 where the model path's `strdup` caps at the
  struct, and the VectorClass `new T[size]` that births the proxy
  records zeroed) is transcribed in
  `tools/differential/gen_hlod_golden.cpp` and ported as
  `rzh-formats::w3d_hlod`, on top of the chunk container — the
  format every Zero Hour unit and building is assembled in. The
  goldens pin 39 assembly payloads: the modern full HLOD and the
  8-LOD corpus with the non-finite MaxScreenSize shapes stored
  verbatim, the zero-LOD load (legal), the discarded-error family
  (every sub-array verdict ignored by the walk — the deliberate
  contrast with the hierarchy loader — with the zeroed stance for
  the entries a failing read leaves unread), the unbalanced-failure
  contrast (a stray chunk in a non-final LOD fails the load through
  the next iteration's id check; in the final LOD, or inside the
  trailing arrays, it merely shadows the rest of the file), the
  last-wins repeated aggregate/proxy arrays (a failing second proxy
  array still replaces the first), the bone-index `int` wraps and
  the error verdicts (the first chunk and the LOD loop's open/id
  checks are the only failures); the `hlod` bench is within budget
  (docs/benchmarks.md).

- the **DDS textures** — the texture pair's compressed half: the
  data path of `DDSFileClass` (`WW3D2/ddsfile.{h,cpp}` — the
  constructor's name munging that overwrites the last three
  characters with "dds", the magic read that is never validated, the
  124-byte legacy `DDSURFACEDESC2` decode whose read size must equal
  the `Size` field, the FourCC → WW3DFormat conversion with the
  numeric small-value route through the formconv table, the
  mip-level arithmetic — the reduction factor's else branch, the two
  dropped levels the file still stores and reads, the 4/8 block
  divisor and the cube face stride's hardcoded +16 — `Load`'s read
  window, the clamp-to-four level queries, `Get_Pixel` and
  `Get_4x4_Block` with both DXT1 colour modes and the DXT5 8/6-alpha
  tables, the surface copies with the DXT1→DXT2 expansion and the
  hue-shift re-encodes, the dead volume pointer, and the HSV recolour
  stack from `colorspace.h`/`dx8wrapper.h`/`vector3.h` the
  team-colour paths drive; plus `Write_B8G8R8A8` from
  `bitmaphandler.h` with the R3G3B2 two-byte store's zero-extended
  stray byte, and `Get_Bytes_Per_Pixel` from `ww3dformat.cpp`) is
  transcribed in `tools/differential/gen_dds_golden.cpp` and ported
  as `rzh-formats::dds`, completing the texture format pair with the
  TGA slice. The goldens pin 39 texture payloads: the DXT1/DXT3/DXT5
  mip chains, the reduction family (including the overrun whose
  size arithmetic underflows past the `0x80000000` guard while Load
  still returns success with no memory — the port reports the
  original's null dereference as `NoData`), the cube maps (the DXT5
  face-stride drift of 16 bytes) and the volumes (the dead pointer
  that makes every volume copy a no-op), the numeric (22 =
  X8R8G8B8) and unknown FourCCs that load with an uncompressed
  layout, the zero-size and five-wide surfaces, the hand-built DXT1
  and DXT5 pixel corpora (both colour modes, the 8/6-alpha tables,
  a two-half index pattern that distinguishes the unrolled reader's
  byte advance, the level-1/level-2 addressing), every destination
  format's block dump, the copy family (the verbatim memcpy, the
  rewrites, the expansion, the untouched DXT3-with-shift and
  size-mismatch destinations), the name-munging corpus (the
  255-character truncation, the sub-three-character shapes refused
  where the original writes before its buffer) and the error
  verdicts (the empty and magic-only files, the short description,
  the `Size`-field mismatch whose passing shape reads uninitialised
  bytes in the original); the `dds` benches are within budget on
  both comparisons (docs/benchmarks.md — the budget run reshaped
  the block writer into a per-format inner loop, making the Rust
  conversion walk faster than the per-pixel-dispatched original).

- the **map data-chunk container** — the first map-files slice: the
  framing layer of `Common/System/DataChunk.{h,cpp}` — the `CkMp`
  table of contents (the prepend-list interning with its duplicate-name
  and duplicate-id lookups, the count>0-but-EOF header refusal, the
  NUL-truncated names, the `u8` name-length truncation), the reader
  (`DataChunkInput`'s registered-parser walk with its scope rules and
  last-registered-wins matching, the 4-byte exhaustion break, the
  EOF-header phantom chunk, the signed data-left bookkeeping that goes
  negative on over-declared children and parent-overrun opens, and the
  primitive readers with their zero-filled short reads and NUL-truncated
  string views), the writer (`DataChunkOutput`'s placeholder-patched
  headers, the interleaved chunk/dict-key/name-key TOC ids emitted in
  reverse allocation order, the `u16` string-length wraps), plus the
  load-path collaborators the primitives drive — the `Dict` half
  (`findPairByKey`'s binary search, `setPrep`'s update-or-append, the
  shellsort that orders pairs by name key, so a dict read reorders
  against the file order on a pre-seeded generator) and the name-key
  interning of `NameKeyGenerator` (first-seen sequential ids, kept as
  an explicit context per the no-globals rule, transcribed privately
  per the crate's dependency-free policy) — is transcribed in
  `tools/differential/gen_mapchunk_golden.cpp` and ported as
  `rzh-formats::map_chunk`, the map-files counterpart of the W3D chunk
  container the mesh/skeleton/animation/HLOD slices built on. The
  goldens pin 44 container payloads: the full map-load and map-save
  shapes with nested object lists and reordering dicts, the TOC edge
  family (duplicate names and ids, embedded NULs, empty names, the
  truncated count, the legacy non-chunky tag), the truncated-header and
  truncated-string shapes, the over-declared and 2 GiB-declared seeks,
  the negative declared sizes, the parser scope/order/abort/drift/reset
  behaviours, the dict duplicate-key replace and corrupt-type error,
  the name-key interning side effects, the length wraps and the
  trailing empty chunk; the `mapchunk` benches are within budget on
  both comparisons (docs/benchmarks.md — the budget run caught a
  64 KiB-memset transcription artifact before it could skew the
  comparison).
- the **map height-map payload** — the first map-files payload slice,
  on top of the data-chunk container: the data path of
  `WorldHeightMap.cpp`'s `HeightMapData` readers —
  `ParseHeightMapData` (the full load) and `ParseSizeOnly` (the
  logical-data-only load the validation and preview paths drive),
  the `isValidTotalMapSize`/`isValidPlayableMapSize`/
  `isValidHeightDataSize` size contracts (the `MAX_MAP_DIMENSION`/
  `MAX_MAP_AREA` ceilings, the per-axis `(width-1)/2` border halves,
  the sample-count check's unsigned data-left view), the
  version-gated reads (the border word from version 3, the boundary
  list from version 4 — with the synthesised pre-version-4 playable
  rectangle — and the version-1 in-place downsample whose extent
  update diverges between the two load modes while neither updates
  the declared sample count), and the constructor's
  registerParser/parse walk shape (global scope, last-wins repeated
  chunks, the absent payload as `None`) — is transcribed in
  `tools/differential/gen_mapheightmap_golden.cpp` (over the
  re-included mapchunk container transcription) and ported as
  `rzh-formats::map_heightmap`. The goldens pin 35 payload corpora
  plus a validator matrix: the version family (0 through 5), the
  boundary family (empty, raw negative/large values, the negative
  count the original's `resize` dies on), every size-contract
  rejection (including the 2-high map that refuses border 1 through
  the height axis), the under-declared chunk whose negative
  data-left passes the sample-count check and whose samples read
  into the next chunk's header, the zero-filled short sample tail,
  the leftover bytes the release close skips, the global-scope rule
  (a nested `HeightMapData` never fires), last-wins repeats and the
  walk refusals; the `mapheightmap` benches are within budget on
  both comparisons (docs/benchmarks.md).
- `rzh-ini`: lexer/parser (includes, `#define` macros, inheritance/reskin, load order) and a
  **derive-based schema layer** replacing the 195 hand-written `FieldParse` tables. Keep it
  format-agnostic so the data format can change later (ADR-003).
- `rzh-formats`: map files (the DataChunk framing container **done**,
  the height-map payload **done**; the remaining payload parsers
  follow), W3D (meshes **done**, skeletons **done**,
  raw animations **done**, compressed animations **done**, HLOD
  **done**) **on top of the ported chunk container**, TGA/DDS
  (**both done**), CSF string tables (**done**), WND layouts, fonts,
  audio containers.
- `rzh-data`: typed templates (objects, weapons, armor, locomotors, upgrades, sciences, ranks,
  special powers, FX/particles, audio events, command sets, ...).
- **Module census:** count how often each module type appears in the data → port order for Phase 3.
- `rzh-cli inspect|dump|lint` for every format. Fuzz all parsers (`cargo-fuzz`).
- **Exit (M1 "Data loads"):** the whole Zero Hour data set loads with no errors; dumps match the
  C++ oracle on sampled files (P0).

### Phase 3: Deterministic headless simulation (XXL)

Work packages, in order. Develop by **vertical slices**, not by subsystem:

1. **Kernel:** frame loop (logic rate is tied to `WWSyncPerSecond`, 30 Hz in retail, to verify),
   object model and ID allocation, partition manager (spatial queries), command (GameMessage)
   processing, players/teams/relationships, money/energy/ranks/sciences.
2. **Module framework:** Behavior/Update/Die/Damage/Contain/Collide/Create/Body/Upgrade/
   SpecialPower families, data-driven construction from templates, **preserve the sleepy-update
   scheduling semantics** (order matters for parity).
3. **Movement and terrain:** locomotors, physics, heightfield, water, bridges/layers, cliffs.
   **Pathfinding** (zones, layers, A\*) is a separate, high-risk package: quirky and parity-critical.
4. **Combat:** weapons, projectiles, armor/damage types, body modules, garrison, veterancy,
   special powers and superweapons, fire/radiation/poison.
5. **Economy and production:** queues, supply, dozer/construction, sell/repair, upgrades,
   command sets.
6. **AI:** state machines, AI groups, skirmish AI players and build lists.
7. **Script engine** (~24K lines): conditions, actions, sequential scripts, teams, triggers.
8. **Snapshots, save/load, replays:** new versioned format via `serde` (ADR-010). Snapshots double
   as desync-debugging and state-hashing tools.

Slice definitions:

| Slice | Content |
|---|---|
| S1 | Flat test map, 2 players: build power plant + barracks with a dozer, train infantry, attack and destroy a structure |
| S2 | Full economy loop: supply centre, trucks, docks, upgrades |
| S3 | Skirmish AI (all difficulties) plays a full match headless |
| S4 | Every faction/general, all special powers, remaining modules by census order |

- **Exit (M2 "Headless sim"):** S1–S4 run; P1/P2 parity on the scenario suite; AI-vs-AI soak runs
  with no crash and no Rust-vs-Rust desync across Windows/Linux/macOS.

### Phase 4: Client: rendering and UI (XL; starts in parallel after Phase 2)

Track B can begin once formats exist and runs alongside Phase 3:

1. Platform layer (window, input, timing) and wgpu context. **Model/map viewer first**: early,
   visible progress that needs no simulation.
2. Terrain (height, tile blending, water, roads, bridges, props, shroud/fog of war, shadows).
3. W3D models: skinning, animation, HLOD, team colours, damage states.
4. Materials/shaders: replace the fixed-function DX8 pipeline with WGSL that matches the look.
5. Particles, FX, decals, projectile trails, lighting.
6. Drawable layer (client-side objects and draw modules), camera, selection and picking, radar.
7. UI framework: WND/GameWindow semantics (retained mode, data-driven), control bar, shell menus,
   tooltips, hotkeys, message-stream/command translators.
- **Exit:** **M3 "First pixels"** (terrain and units on screen) → **M4 "Playable skirmish"**
  (full match vs AI on the Rust stack, basic audio).

### Phase 5: Audio and video (M)

- Audio engine replacing Miles: events, 3D positional sound, music, speech, ambience.
- Video: offline conversion of Bink to an open format; decoder choice by spike (ADR-007).
- **Exit:** feature-complete audio/video for a full match including menus and cutscenes.

### Phase 6: Cut-over: Rust-only build (S–M) → M5

- Remove C++ from the default build; delete `Generals/`, `GeneralsMD/`, `Core/`, `Dependencies/`,
  `Tests/`, `Benchmarks/`, `cmake/`, `vcpkg.json`, `stlport.diff` and the C++ CI.
  Tag `legacy-cpp-final`; archive the oracle recipe on a separate branch.
- Port remaining unit tests and benchmarks to Rust. Rust-vs-Rust golden traces become the
  regression oracle. Remove `rzh-oracle-ffi`.
- **Exit (M5 "Rust-only"):** no C++ in the build; full single-player parity checklist signed off.

### Phase 7: Networking and multiplayer (L) → M6

- New protocol; deterministic lockstep across platforms; per-N-frame **state-hash exchange** for
  desync detection with a state-diff debugging tool.
- Transport and reliability (ADR-009), NAT traversal or relay, LAN discovery, reconnect and
  disconnect handling, spectators, replays.
- New matchmaking/lobby service (separate repository).
- Cross-platform determinism CI matrix runs on every PR touching `rzh-sim`.
- **Exit (M6 "Online"):** 2–8 player matches across Windows/Linux/macOS with no desyncs in soak tests.

### Phase 8: Tools and modding (L, parallelizable)

- CLI converters first (needed early); then map editor (WorldBuilder replacement, the biggest
  item), particle editor, UI editor, model viewer; mod loading, documentation.

### Phase 9: Independence (parallel track; starts after Phase 2) → M7

- Own **data format** (ADR-003) with an INI→native converter.
- Own **content**: factions, units, names, art, audio, video, text, maps. Zero EA-owned content.
- Own **identity**: name, logo, branding; legal review (§7).
- Release engineering: installers, auto-update, crash reporting.
- **Exit (M7 "Independent"):** the game runs and ships with no EA-owned content and no
  dependency on an original install.

### Milestones

| | Milestone | Meaning |
|---|---|---|
| M0 | Groundwork | CI, inventory, oracle traces |
| M1 | Data loads | Rust loads all Zero Hour data |
| M2 | Headless sim | Full headless matches, parity levels met |
| M3 | First pixels | Terrain + units rendered |
| M4 | Playable skirmish | Complete match vs AI on Rust |
| M5 | Rust-only | C++ removed |
| M6 | Online | Cross-platform lockstep multiplayer |
| M7 | Independent | No EA content; new identity |

---

## 5. Architecture decision backlog (ADRs)

Each decision gets a short record in `docs/adr/` with context, options and consequence.
Defaults below are recommendations, to be confirmed.

| ADR | Topic | Default | Alternatives / notes |
|---|---|---|---|
| 001 | Sim numerics | `f32` + `libm` + rules in §3.3 | Fixed-point (Q16.16) if cross-platform CI shows divergence |
| 002 | Sim architecture | Module objects in arenas with stable handles | Full ECS; reconsider after M5 |
| 003 | Data format | Read INI first; native format (RON/TOML) in Phase 9 | Schema layer stays format-agnostic |
| 004 | Rendering | wgpu + WGSL | Target: same readability and layout, not pixel-exact |
| 005 | Window/input | winit | |
| 006 | Audio | Decide after a short spike | cpal-based engines (kira, rodio + symphonia) |
| 007 | Video | Convert offline to an open codec | Pure-Rust decoder vs FFmpeg binding; low priority |
| 008 | UI | Port WND/GameWindow semantics to reuse layouts | egui only for dev tools |
| 009 | Netcode | Deterministic lockstep + hash desync detection | Transport: UDP + custom reliability vs QUIC |
| 010 | Save/replay format | New, versioned, `serde`-based | Legacy `.rep` import only for the oracle |
| 011 | Threading | Sim single-threaded; client may thread loading/render/audio | |
| 012 | Branding and license | See §7 | |

Candidate crates (to be confirmed per ADR): `wgpu`, `winit`, `glam` (render side only),
`libm`, `slotmap`, `indexmap`, `serde`, `tracing`, `criterion`, `proptest`, `cargo-fuzz`, `insta`.

---

## 6. Testing, CI and definition of done

**Test layers:** unit → property tests → golden files → differential tests against the oracle
(P0–P2) → scripted scenario tests → replay trace tests → parser fuzzing → cross-platform
determinism matrix → benchmarks → headless AI-vs-AI soak tests.

**Game data in CI.** EA data cannot be committed, so tests that need it are tagged
`needs-game-data` and run only where a legitimately owned install is available (developer
machines or a private runner). Public CI uses synthetic fixtures.

**Definition of done for a ported unit:**

- [ ] builds without warnings, `clippy` clean, public API documented;
- [ ] origin recorded (`Ported from ... @ 571a7c5`) and notices preserved;
- [ ] Rust unit tests **and** a differential/golden test with its parity level (P0–P2) stated;
- [ ] no `unsafe` in sim crates; every `unsafe` elsewhere has a `SAFETY` comment and a reviewer;
- [ ] determinism rules of §3.3 respected;
- [ ] benchmark exists where relevant and is no more than 20 % slower than the C++ baseline
  unless an ADR justifies it (proposed threshold).

---

## 7. Legal, licensing and branding

*This section is planning guidance, not legal advice. Get qualified review before any public release.*

- **License.** The Rust port is a derivative of GPL-3.0 code and stays GPL-3.0. Preserve
  copyright notices; add SPDX headers; use `cargo-deny` to keep dependencies compatible.
- **Assets and data.** Art, audio, video, maps, text, INI data, faction and unit names remain
  EA's. They are read from the user's own install during development and are never committed or
  redistributed. Replacing them is the job of Phase 9.
- **Name.** "Zero Hour" and "Command & Conquer" are EA trademarks. The current project name
  contains "Zero Hour". Decide on a new name **before any public release** (ADR-012).
- **Inherited governance files.** `AI_POLICY.md` and `CONTRIBUTING.md` are written for upstream's
  maintainers and C++ workflow. Replace them in Phase 0 with this project's own rules, including
  its position on AI-assisted contributions.

---

## 8. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Sheer scale (~500K lines to port, ~150K to re-implement) | Schedule | Vertical slices, module census, early milestones, re-estimate at M1 |
| Parity cannot be proven on long matches (chaotic divergence) | Quality | Layered parity levels (§3.2), scenario suite, soak tests, then Rust-vs-Rust goldens |
| Cross-platform float determinism | Multiplayer | Rules in §3.3, `libm`, determinism CI matrix from the first sim commit, fixed-point fallback (ADR-001) |
| Global state and inheritance do not map to Rust ownership | Rewrite churn | Explicit contexts, handles, arenas; decided in ADR-002 before Phase 3 starts |
| Pathfinding and AI are quirky and parity-critical | Delay | Dedicated work package, differential tests on pathfinder inputs/outputs, early spike |
| Renderer fidelity (DX8 fixed-function → shaders) | Visual regressions | Model/map viewer early; screenshot comparisons; accept "same readability", not pixel-exact |
| Oracle disappears with the C++ tree | Lost reference | Record the scenario library and traces **before** M5; keep the oracle recipe on an archive branch |
| Scope creep ("improve while porting") | Never finishing | Behaviour-preserving first; improvements go through ADRs after M5 |
| Upstream drift (they keep fixing logic) | Divergence | Frozen baseline; manual cherry-picks of logic fixes until M2 only |
| Quality of AI-assisted ports | Subtle bugs | Every PR links its C++ origin and carries a differential test; humans review every line |
| Legal/branding | Release blocked | Phase 9 track starts early; review before first public release |
| Contributor bandwidth | Stalls | Two tracks after Phase 2 (sim vs client) with a clean one-way interface |

---

## 9. First ten tasks

1. Commit this plan to `docs/RUST_MIGRATION_PLAN.md`; tag `legacy-cpp-baseline`; open ADR-000
   (decision to migrate, retail compatibility dropped).
2. Create the Cargo workspace, `rust-toolchain.toml`, `rustfmt`/`clippy` config, `deny.toml`,
   and `rust-ci.yml` (Windows/Linux/macOS: fmt, clippy, test, deny).
3. Rewrite `README`, `CONTRIBUTING`, `AI_POLICY`, `SECURITY` for the Rust fork.
4. Write the developer-setup doc (`RZH_GAME_DIR`, no assets in git).
5. Write the inventory scripts (`tools/inventory/`) and publish `docs/inventory/`.
6. Build the C++ oracle (Release, Zero Hour) in Docker/CI; confirm two identical runs give
   identical output for the 10 Zero Hour replays.
7. Add oracle trace instrumentation (`RZH_ORACLE_TRACE`, trace format v1 in `docs/`), and a
   `RETAIL_COMPATIBLE_*`-off variant.
8. Create `rzh-util` (CRC, RNG, name keys) with differential tests and committed golden vectors.
9. Create `rzh-vfs` and `rzh-compress` (BIG archives, EA codecs), tested on real data locally.
10. Create `rzh-ini` with three simple block types end to end (for example `Armor`, `Weapon`,
    `Locomotor`) and dump comparison against the oracle. This validates the whole pipeline
    before the larger investment.

---

## Appendix A: C++ → Rust subsystem map

Line counts are for the `Source/` subtrees (headers excluded) unless noted.

| C++ source | Lines | Rust target | Notes |
|---|---:|---|---|
| `WWVegas/WWMath` | 30K | `rzh-math` | Deterministic subset for sim; render side may use `glam` |
| `WWVegas/WWLib` | 39K | `rzh-util`, `std` | Mostly replaced by std; keep CRC/RNG-like pieces |
| `Libraries/Compression` | 5K | `rzh-compress` | EA codecs |
| `WWDebug`, `debug`, `profile`, `MiniDumper` | 16K | `tracing`, panic hook, optional Tracy | Re-implemented |
| `WWVegas/WW3D2` | 75K | `rzh-formats` + `rzh-render` | Split format parsing from rendering |
| `WWVegas/WWAudio` | 12K | `rzh-audio` | |
| `WWVegas/WWDownload` | 3K | `rzh-net` (patcher, later) | |
| `GameEngine/.../Common` (Core 35K + MD 32K) | 67K | `rzh-util`, `rzh-ini`, `rzh-vfs`, `rzh-data`, `rzh-sim` | INI, file systems, name keys, Xfer, thing templates |
| `GameEngine/.../GameLogic` (Core 20K + MD 170K) | 190K | `rzh-sim` | Object 118K, ScriptEngine 24K, AI 22K, System 5K (MD) |
| `GameEngine/.../GameClient` (Core 70K + MD 49K) | 119K | `rzh-client`, `rzh-ui` | GUI ~76K, MessageStream 10K |
| `GameEngine/.../GameNetwork` (Core 31K + MD 2K) | 33K | `rzh-net` | Design reference |
| `GameEngineDevice/W3DDevice` (Core 55K + MD 24K) | 79K | `rzh-render` | Re-implement on wgpu |
| `GameEngineDevice/MilesAudioDevice`, `VideoDevice`, `Win32Device`, `StdDevice` | 7K | `rzh-audio`, `rzh-video`, `rzh-app`, `rzh-vfs` | |
| `Tools/*` | 305K | `rzh-cli`, later editors | Deferred |

## Appendix B: Reproducing the measurements

From the repository root at the baseline commit:

```sh
# total C/C++ lines and files
find . -path ./.git -prune -o -type f \( -name '*.cpp' -o -name '*.h' -o -name '*.c' \
  -o -name '*.hpp' -o -name '*.inl' \) -print0 | xargs -0 cat | wc -l

# lines in one area (example)
find GeneralsMD/Code/GameEngine -type f \( -name '*.cpp' -o -name '*.h' \) -print0 \
  | xargs -0 cat | wc -l

# retail-compatibility switches
grep -rhoE 'RETAIL_COMPATIBLE[A-Z_]*' --include='*.cpp' --include='*.h' . | sort | uniq -c | sort -rn

# global singleton declarations (approximate)
grep -rhoE '^\s*extern\s+\w+\s*\*\s*The\w+' --include='*.h' Core GeneralsMD | sort -u | wc -l

# platform coupling
grep -rl 'windows.h' --include='*.cpp' --include='*.h' Core GeneralsMD | wc -l
grep -rli 'd3d8'     --include='*.cpp' --include='*.h' Core GeneralsMD | wc -l
```

The numbers are approximate (line counts include comments and blank lines; greps are
pattern-based). Phase 0's inventory tooling replaces them with reproducible, versioned reports.
