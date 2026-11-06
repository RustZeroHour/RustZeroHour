// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/GameEngineDevice/Source/W3DDevice/GameClient/WorldHeightMap.cpp
//     (WorldHeightMap::ParseWorldDictDataChunk — the WorldInfo chunk
//     reader the constructor's logical-data-only load registers: the
//     dict read whole, the world-dict replacement and the weather
//     exists-gate; and the constructor's registerParser/parse walk
//     shape)
//   Core/GameEngine/Include/Common/MapObject.h
//     (the static TheWorldDict the reader replaces — MapObject.h line
//     112; the member is the storage, not the load, so it stays the
//     port's payload)
//   Core/GameEngine/Include/Common/GameType.h
//     (the Weather enum values WEATHER_NORMAL/WEATHER_SNOWY — the cast
//     target of the raw word)
//   Core/GameEngine/Source/Common/Dict.cpp
//     (Dict::getInt(key, &exists) — the exact-type INT query the
//     weather gate drives, over the container's findPairByKey)
//   Core/GameEngine/Include/Common/MapReaderWriterInfo.h
//     (K_WORLDDICT_VERSION_1 — the version word the reader never reads)
//   GeneralsMD/Code/GameEngine/Include/Common/WellKnownKeys.h
//     (TheKey_weather — the DEFINE_KEY(weather) startup key the query
//     consults)
//
// See the module docs for the format layout and the documented
// differences from the C++ (the two process-globals become the walk
// state; the sticky weather becomes the Option).

//! The **map world-info payload** — the `WorldInfo` chunk of a `.map`
//! file, on top of [`crate::map_chunk`].
//!
//! A Zero Hour map's global settings are one flat dict at the top level
//! of the data-chunk tree: the map's name and editor settings, and the
//! weather word the client switches its precipitation on.
//! `WorldHeightMap`'s constructor registers a reader for it in the
//! **logical-data-only** load only (the full load never looks at it),
//! and this module ports that reader over the
//! [`DataChunkInput`](crate::map_chunk::DataChunkInput) walk, quirks
//! included (parity level P0, pinned by golden vectors generated from
//! the transcription in `tools/differential/gen_mapworldinfo_golden.cpp`).
//!
//! # The chunk in one page
//!
//! ```text
//! WorldInfo chunk (any version; the reader never reads its own)
//!   u16          the dict pair count
//!   pair × n     keyAndType word + value (see crate::map_chunk::Dict)
//! ```
//!
//! # The load-path quirks (transcribed)
//!
//! - The dict **replaces the previous one whole, every chunk**: the
//!   original's `*MapObject::getWorldDict() = d` is an unconditional
//!   assignment to a static — repeated `WorldInfo` chunks each install
//!   their dict, the last one owning the final state.
//! - The weather is re-derived from the **new** dict through
//!   [`Dict::getInt`](crate::map_chunk::Dict)'s exact-type contract: a
//!   `weather` pair of type `INT` sets the word; a `weather` pair of
//!   any other type (BOOL, REAL, ASCII, UNICODE) reads as *absent* —
//!   the word keeps whatever an earlier chunk set (the original's
//!   `m_weather` is a sticky process-global the map load never resets).
//! - The word is **never range-checked**: any `i32` survives the
//!   `(Weather)` cast (the goldens pin an out-of-enum 2 and the
//!   zero-filled short-read 0 alike).
//! - The chunk's **version word is never read** — versions 0 and 9 load
//!   identically to 1.
//! - The reader never checks it consumed the chunk (the
//!   `atEndOfChunk` assert is debug-only); leftovers go to the walk's
//!   close, and a dict read past a short file mints zero-filled
//!   `keyAndType` words as empty-name `BOOL` pairs (the container's
//!   pinned stance) — a zero-filled `weather` `INT` pair still sets the
//!   weather to 0.
//! - A `WorldInfo` chunk at any **non-global scope** never fires — the
//!   registration is `("WorldInfo", "")`.
//!
//! # Differences from the C++ (documented)
//!
//! - The two globals the reader writes become the walk's state: the
//!   static `MapObject::TheWorldDict` becomes this payload's [`Dict`],
//!   and `TheWritableGlobalData->m_weather` becomes the [`Option`]
//!   below. The original's cross-load stickiness (a value set by an
//!   earlier *map* surviving into this one) is the same "keeps its
//!   prior value" rule the absent-chunk [`None`] models — a fresh
//!   walk starts at `None` where the original's process starts at
//!   `WEATHER_NORMAL`; a caller that knows the prior state can read the
//!   [`Option`] and keep its own word when it is `None`.
//! - The name-key interning: the original's `TheNameKeyGenerator`
//!   pre-interns the whole `WellKnownKeys.h` table at static
//!   initialization before any dict is read; the port's walk seeds
//!   exactly the **one** key this load consults (`weather`), in the
//!   header's declaration order. A dict mixing other well-known names
//!   orders them by file-first-seen here where the original orders them
//!   by its startup table — the same documented stance as
//!   [`crate::map_objects`] (callers can pre-seed wider tables through
//!   [`parse_world_dict_data_chunk`]'s `keys` parameter).
//! - `readDict`'s escaping `ERROR_CORRUPT_FILE_FORMAT` becomes
//!   [`WorldInfoError::Dict`] and stops the walk through the parser's
//!   `false` (the chunk stays open, as in the original); the world dict
//!   keeps the previous chunk's content — the replacement happens only
//!   after the read succeeds.
//!
//! # Examples
//!
//! ```
//! use rzh_formats::map_chunk::DataChunkOutput;
//! use rzh_formats::map_worldinfo::{
//!     WorldInfoData, WORLD_INFO_LABEL, K_WORLDDICT_VERSION_1,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A world dict with the weather set to snowy, built with the
//! // container's writer the way WorldBuilder saves one.
//! let mut out = DataChunkOutput::new();
//! out.open_data_chunk(WORLD_INFO_LABEL, K_WORLDDICT_VERSION_1);
//! // the dict is built through the writer's dict path in real maps;
//! // a zero-pair dict is written directly here
//! out.write_int(0); // the pair count — zero pairs
//! out.close_data_chunk();
//! let bytes = out.into_bytes()?;
//!
//! // The constructor's logical-load walk: the map's world dict.
//! let info = WorldInfoData::from_map(&bytes)?.expect("the chunk fired");
//! assert_eq!(info.dict.pair_count(), 0);
//! assert_eq!(info.weather, None); // no INT weather pair — nothing set
//! # Ok(())
//! # }
//! ```

use std::fmt;

use crate::map_chunk::{
    DataChunkInfo, DataChunkInput, Dict, MapChunkError, NameKeys, ParserTable, dict_type,
};

// ---------------------------------------------------------------------------
// constants (MapReaderWriterInfo.h, GameType.h, WellKnownKeys.h)
// ---------------------------------------------------------------------------

/// The chunk label the constructor registers its world-info reader
/// under, at the global scope.
pub const WORLD_INFO_LABEL: &[u8] = b"WorldInfo";

/// The world-dict chunk's version word (`K_WORLDDICT_VERSION_1`) — the
/// only version ever written, and a word the reader never reads (any
/// version loads the same).
pub const K_WORLDDICT_VERSION_1: u16 = 1;

/// The well-known world-dict key the weather gate consults
/// (`TheKey_weather` — `WellKnownKeys.h`'s `DEFINE_KEY(weather)`).
pub const WEATHER_KEY: &[u8] = b"weather";

/// The `Weather` enum values (`GameType.h`) — the cast target of the
/// raw word. The load never validates the range (any `i32` survives);
/// these are the named words the game's INI layer uses.
pub mod weather {
    /// The normal weather (`WEATHER_NORMAL`).
    pub const WEATHER_NORMAL: i32 = 0;
    /// The snowy weather (`WEATHER_SNOWY`).
    pub const WEATHER_SNOWY: i32 = 1;
}

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the world-info load path can produce.
///
/// The original has one failure code — `ERROR_CORRUPT_FILE_FORMAT`,
/// thrown by `readDict` on an impossible pair type and by the
/// constructor when the walk itself refuses. The port states each site
/// as its own variant; the dict error stops the walk the way the
/// escaping exception does (the chunk stays open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldInfoError {
    /// The walk itself failed — the header never opened as a `CkMp`
    /// table of contents (the constructor's `if (!file.parse(this))`
    /// throw).
    CorruptFileFormat,
    /// The world dict met a pair whose type byte is not one of the five
    /// `Dict::DataType` values (`readDict`'s escaping
    /// `ERROR_CORRUPT_FILE_FORMAT`, its type byte kept).
    Dict(MapChunkError),
}

impl fmt::Display for WorldInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorldInfoError::CorruptFileFormat => {
                write!(f, "map is not a chunky data-chunk file")
            }
            WorldInfoError::Dict(e) => write!(f, "world info dict: {e}"),
        }
    }
}

impl std::error::Error for WorldInfoError {}

impl From<MapChunkError> for WorldInfoError {
    fn from(e: MapChunkError) -> Self {
        WorldInfoError::Dict(e)
    }
}

// ---------------------------------------------------------------------------
// the payload (the world dict as the load leaves it)
// ---------------------------------------------------------------------------

/// One `WorldInfo` chunk as the reader leaves it: the dict read whole
/// and this chunk's own weather observation (an `INT` `weather` pair's
/// word, `None` for any other type or no pair).
#[derive(Debug, Clone, PartialEq)]
pub struct WorldDictLoad {
    /// The chunk's dict (`*MapObject::getWorldDict() = d`'s right-hand
    /// side).
    pub dict: Dict,
    /// The chunk's weather: `Some(word)` exactly when the new dict
    /// carries a `weather` pair of type `INT` (`getInt`'s exact-type
    /// exists bit).
    pub weather: Option<i32>,
}

/// The `WorldInfo` payload: the map's world dict as the last chunk left
/// it, plus the sticky weather word.
///
/// [`WorldInfoData::weather`] is `Some` when **any** chunk this load
/// carried an `INT` weather pair — a later chunk without one keeps the
/// earlier word (the original's process-global is never reset by the
/// load); the dict, in contrast, always belongs to the last chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldInfoData {
    /// The world dict (`MapObject::TheWorldDict` after the walk — the
    /// last chunk's dict, replaced whole per chunk).
    pub dict: Dict,
    /// The sticky weather word (`TheWritableGlobalData->m_weather`):
    /// the last word any chunk's exists-gate set, `None` when no chunk
    /// this load did (the original keeps its prior value — see the
    /// module docs).
    pub weather: Option<i32>,
}

// ---------------------------------------------------------------------------
// the reader (WorldHeightMap.cpp)
// ---------------------------------------------------------------------------

/// `WorldHeightMap::ParseWorldDictDataChunk` — the `WorldInfo` chunk
/// reader: one dict read whole, then the weather re-derived from the
/// new dict through the exact-type `INT` query.
///
/// The `keys` table is the walk's interning state (the well-known
/// `weather` key pre-seeded); a dict read interns its pair names
/// through it, so the dict's stored order is the interning order.
pub fn parse_world_dict_data_chunk(
    file: &mut DataChunkInput<'_>,
    keys: &mut NameKeys,
) -> Result<WorldDictLoad, WorldInfoError> {
    // Dict d = file.readDict(); — the corrupt-type escape stops the
    // walk (the world dict keeps its previous content).
    let dict = file.read_dict(keys)?;

    // Bool exists; Int theWeather = getWorldDict()->getInt(
    //     TheKey_weather, &exists); if (exists) m_weather = theWeather;
    let weather = int_pair(keys, &dict, WEATHER_KEY);

    Ok(WorldDictLoad { dict, weather })
}

/// `Dict::getInt(key, &exists)` through the walk's table: the pair
/// named `name`'s word when it is an exact-type `INT` match, `None`
/// when no such pair or any other type.
fn int_pair(keys: &mut NameKeys, dict: &Dict, name: &[u8]) -> Option<i32> {
    let key = keys.name_to_key(name);
    for i in 0..dict.pair_count() {
        if dict.nth_key(i) == key && dict.nth_type(i) == dict_type::DICT_INT {
            match dict.nth_value(i) {
                crate::map_chunk::DictValue::Int(v) => return Some(*v),
                _ => return None,
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// the walk (the constructor's logical-load shape, world info only)
// ---------------------------------------------------------------------------

impl WorldInfoData {
    /// The `WorldHeightMap` constructor's **logical-data-only** walk,
    /// restricted to this payload: `WorldInfo` registered at the global
    /// scope, each chunk replacing the world dict whole and re-deriving
    /// the weather (the sticky word survives chunks without an `INT`
    /// pair).
    ///
    /// `Ok(None)` is the map with no `WorldInfo` chunk at all — the
    /// original's globals keep whatever they held (a fresh process's
    /// empty dict and `WEATHER_NORMAL`).
    ///
    /// The other payloads the logical load registers (the size-only
    /// height map, the objects list, the polygon triggers and the
    /// sides list) arrive with their own slices — here their chunks
    /// are skipped whole, the walk's behaviour for an unmatched label.
    pub fn from_map(bytes: &[u8]) -> Result<Option<Self>, WorldInfoError> {
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, WorldInfoWalk> = ParserTable::new();
        table.register(WORLD_INFO_LABEL, b"", world_info_chunk_parser);
        let mut user = WorldInfoWalk {
            result: None,
            keys: NameKeys::new(),
            error: None,
        };
        // the well-known key seeded the way the engine's static
        // initialization does (WellKnownKeys.h's declaration order)
        user.keys.name_to_key(WEATHER_KEY);
        if !input.parse(&mut table, &mut user) {
            return Err(user.error.unwrap_or(WorldInfoError::CorruptFileFormat));
        }
        Ok(user.result)
    }
}

/// The walk's user state: the payload the last good chunk left, the
/// name-key table and the error capture.
struct WorldInfoWalk {
    result: Option<WorldInfoData>,
    keys: NameKeys,
    error: Option<WorldInfoError>,
}

/// The registered `WorldInfo` parser: dispatch to the reader, install
/// the new dict, and fold the weather (the sticky word — a chunk
/// without an `INT` pair keeps it).
fn world_info_chunk_parser(
    file: &mut DataChunkInput<'_>,
    _info: &DataChunkInfo,
    _table: &mut ParserTable<'_, WorldInfoWalk>,
    user: &mut WorldInfoWalk,
) -> bool {
    match parse_world_dict_data_chunk(file, &mut user.keys) {
        Ok(load) => {
            let weather = load.weather.or_else(|| {
                // the exists gate kept the word — the walk's sticky
                // weather survives chunks without an INT pair
                user.result.as_ref().and_then(|r| r.weather)
            });
            user.result = Some(WorldInfoData {
                dict: load.dict,
                weather,
            });
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_chunk::{DataChunkOutput, DictValue, dict_type};

    /// A test file's table of contents: the chunk labels and the dict
    /// pair names the corpora reference.
    struct Toc {
        entries: Vec<(Vec<u8>, u32)>,
    }

    impl Toc {
        fn new(names: &[&[u8]]) -> Self {
            Toc {
                entries: names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| ((*n).to_vec(), i as u32 + 1))
                    .collect(),
            }
        }

        fn id(&self, name: &[u8]) -> u32 {
            self.entries
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, id)| *id)
                .unwrap_or(0)
        }

        fn bytes(&self) -> Vec<u8> {
            let mut b = Vec::new();
            b.extend_from_slice(b"CkMp");
            b.extend_from_slice(&(self.entries.len() as i32).to_le_bytes());
            for (name, id) in &self.entries {
                b.push(name.len() as u8);
                b.extend_from_slice(name);
                b.extend_from_slice(&id.to_le_bytes());
            }
            b
        }
    }

    fn put_i32(b: &mut Vec<u8>, v: i32) {
        b.extend_from_slice(&v.to_le_bytes());
    }

    fn put_chunk(b: &mut Vec<u8>, id: u32, ver: u16, data: &[u8]) {
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&ver.to_le_bytes());
        b.extend_from_slice(&(data.len() as i32).to_le_bytes());
        b.extend_from_slice(data);
    }

    /// A dict pair: the `keyAndType` word plus the value.
    fn pair(toc: &Toc, kind: u8, key: &[u8], value: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        let key_and_type = ((toc.id(key) << 8) | kind as u32) as i32;
        put_i32(&mut p, key_and_type);
        p.extend_from_slice(value);
        p
    }

    fn pair_int(toc: &Toc, key: &[u8], v: i32) -> Vec<u8> {
        let mut p = pair(toc, 1, key, &[]);
        p.extend_from_slice(&v.to_le_bytes());
        p
    }
    fn pair_bool(toc: &Toc, key: &[u8], v: bool) -> Vec<u8> {
        let mut p = pair(toc, 0, key, &[]);
        p.push(u8::from(v));
        p
    }

    /// One WorldInfo chunk payload (the pair count and pairs).
    fn worldinfo_payload(pairs: &[Vec<u8>]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&(pairs.len() as u16).to_le_bytes());
        for pair in pairs {
            p.extend_from_slice(pair);
        }
        p
    }

    /// A map file: the TOC plus the top-level chunk bytes.
    fn build_map(toc: &Toc, chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut b = toc.bytes();
        for chunk in chunks {
            b.extend_from_slice(chunk);
        }
        b
    }

    #[test]
    fn the_version_gates_match_the_format() {
        assert_eq!(K_WORLDDICT_VERSION_1, 1);
        assert_eq!(WORLD_INFO_LABEL, b"WorldInfo");
        assert_eq!(WEATHER_KEY, b"weather");
        assert_eq!(weather::WEATHER_NORMAL, 0);
        assert_eq!(weather::WEATHER_SNOWY, 1);
    }

    #[test]
    fn a_full_dict_loads_with_its_weather() {
        let toc = Toc::new(&[b"WorldInfo", b"mapName", b"mapHeightScale", b"weather"]);
        let pairs = vec![
            pair_int(&toc, b"weather", weather::WEATHER_SNOWY),
            pair_bool(&toc, b"mapHeightScale", true),
        ];
        let mut chunk = Vec::new();
        let payload = worldinfo_payload(&pairs);
        put_chunk(&mut chunk, 1, K_WORLDDICT_VERSION_1, &payload);
        let bytes = build_map(&toc, &[chunk]);

        let info = WorldInfoData::from_map(&bytes)
            .expect("loads")
            .expect("fired");
        assert_eq!(info.dict.pair_count(), 2);
        // the stored order is the interning order: the seeded weather
        // key (ordinal 1) before mapHeightScale (first-seen 2) — the
        // mapName pair never made it into the dict
        assert_eq!(info.dict.nth_key(0).ordinal(), 1);
        assert_eq!(info.dict.nth_type(0), dict_type::DICT_INT);
        assert_eq!(
            info.dict.nth_value(0),
            &DictValue::Int(weather::WEATHER_SNOWY)
        );
        assert_eq!(info.dict.nth_type(1), dict_type::DICT_BOOL);
        assert_eq!(info.weather, Some(weather::WEATHER_SNOWY));
    }

    #[test]
    fn the_weather_gate_is_an_exact_type_match() {
        let toc = Toc::new(&[b"WorldInfo", b"weather"]);
        let mut chunks = Vec::new();
        for &kind in &[1u8, 0, 2] {
            // INT, then BOOL, then REAL — only the INT sets the word
            let pairs = vec![pair(&toc, kind, b"weather", &[9, 0, 0, 0])];
            let payload = worldinfo_payload(&pairs);
            let mut chunk = Vec::new();
            put_chunk(&mut chunk, 1, K_WORLDDICT_VERSION_1, &payload);
            chunks.push(chunk);
        }
        let bytes = build_map(&toc, &chunks);

        let info = WorldInfoData::from_map(&bytes)
            .expect("loads")
            .expect("fired");
        // the last dict's weather pair is REAL — the exists gate read
        // it as absent, so the first chunk's INT word survives
        assert_eq!(info.dict.nth_type(0), dict_type::DICT_REAL);
        assert_eq!(info.weather, Some(0x0000_0009));
    }

    #[test]
    fn an_absent_chunk_is_none_and_a_nested_one_never_fires() {
        // no WorldInfo at all
        let toc = Toc::new(&[b"HeightMapData"]);
        let mut chunk = Vec::new();
        put_chunk(&mut chunk, 1, 1, &[]);
        let bytes = build_map(&toc, &[chunk]);
        assert_eq!(WorldInfoData::from_map(&bytes).expect("loads"), None);

        // a WorldInfo inside a foreign container: the registration is
        // global-scope, so the walk skips it whole
        let toc = Toc::new(&[b"Container", b"WorldInfo", b"weather"]);
        let mut inner = Vec::new();
        let payload = worldinfo_payload(&[pair_int(&toc, b"weather", 1)]);
        put_chunk(&mut inner, 2, K_WORLDDICT_VERSION_1, &payload);
        let mut chunk = Vec::new();
        put_chunk(&mut chunk, 1, 1, &inner);
        let bytes = build_map(&toc, &[chunk]);
        assert_eq!(WorldInfoData::from_map(&bytes).expect("loads"), None);
    }

    #[test]
    fn the_walk_refuses_non_chunky_bytes() {
        assert_eq!(
            WorldInfoData::from_map(b"not a map at all"),
            Err(WorldInfoError::CorruptFileFormat)
        );
    }

    /// The example in the module docs (kept here so a doctest failure
    /// names the unit).
    #[test]
    fn the_doctest_shape_loads() {
        let mut out = DataChunkOutput::new();
        out.open_data_chunk(WORLD_INFO_LABEL, K_WORLDDICT_VERSION_1);
        out.write_int(0);
        out.close_data_chunk();
        let bytes = out.into_bytes().expect("writes");
        let info = WorldInfoData::from_map(&bytes)
            .expect("loads")
            .expect("fired");
        assert_eq!(info.dict.pair_count(), 0);
        assert_eq!(info.weather, None);
    }
}
