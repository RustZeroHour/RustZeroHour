// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/GameEngine/Source/GameLogic/Map/SidesList.cpp
//     (SidesList::ParseSidesDataChunk — the SidesList chunk reader with
//     its bounded side loop, build lists, team list and nested
//     PlayerScriptsList walk; SidesList::validateSides and its
//     collaborators — the neutral ensure through addPlayerByTemplate,
//     the default-team ensure with its owner fix and singleton force,
//     validateAllyEnemyList's token cleanup, the collision removal,
//     the owner reparent; SidesInfo::addToBuildList's positional
//     append; SidesList::addSide/addTeam/emptySides/emptyTeams;
//     TeamsInfoRec::removeTeam with its last-team refusal; and the
//     constructor's registration-time emptySides)
//   Core/GameEngine/Include/GameLogic/SidesList.h
//     (BuildListInfo's saved members and constructor defaults, the
//     MAX_PLAYER_COUNT sides bound)
//   Core/GameEngine/Include/Common/GameCommon.h
//     (MAX_PLAYER_COUNT = 16)
//   Core/GameEngine/Source/Common/Dict.cpp
//     (Dict::getAsciiString/getBool — the typed getters the validation
//     drives, over the container's findPairByKey)
//   Core/GameEngine/Source/Common/System/AsciiString.cpp
//     (nextToken — the " \n\r\t" tokenizing validateAllyEnemyList
//     drives)
//   GeneralsMD/Code/GameEngine/Include/Common/WellKnownKeys.h
//     (the nine well-known keys this load consults, in declaration
//     order)
//
// See the module docs for the format layout and the documented
// differences from the C++ (the scripts boundary; the unreachable
// removal-loop refusal; the vectors).

//! The **map sides-list payload** — the `SidesList` chunk of a `.map`
//! file, on top of [`crate::map_chunk`].
//!
//! A Zero Hour map's player setup: one record per side (the player
//! dict — name, faction, allies, enemies — plus the side's starting
//! build list), then the team list, then the map's script chunk. The
//! reader ends with `validateSides`, the cleanup pass that repairs
//! player/team inconsistencies before the game logic ever sees them.
//! `WorldHeightMap`'s constructor registers the reader in the
//! **logical-data-only** load, and this module ports it over the
//! [`DataChunkInput`](crate::map_chunk::DataChunkInput) walk, quirks
//! included (parity level P0, pinned by golden vectors generated from
//! the transcription in `tools/differential/gen_mapsides_golden.cpp`).
//!
//! # The chunk in one page
//!
//! ```text
//! SidesList chunk (the gates hang off its own version word)
//!   i32                     the side count (the loop breaks at 16)
//!   side × n
//!     u16 + pairs            the player dict
//!     i32                    the build-list count
//!     entry × m
//!       str16, str16           building + template names
//!       f32 × 3                the location (z forced to 0 — ground)
//!       f32                    the angle
//!       u8                     initially built
//!       i32                    the rebuild count (stored as u32)
//!       [v >= 3] str16         the script name
//!       [v >= 3] i32           the health
//!       [v >= 3] u8 × 3        whiner, unsellable, repairable
//!   [v >= 2] i32              the team count
//!   [v >= 2] u16 + pairs × n  the team dicts
//!   PlayerScriptsList chunk   the script-engine slice's payload
//!                             (skipped whole here — see below)
//! ```
//!
//! # The load-path quirks (transcribed)
//!
//! - **The side loop breaks at [`MAX_PLAYER_COUNT`] mid-side**: a map
//!   declaring more than 16 sides leaves the 17th side's bytes
//!   unconsumed, and the version-2 team-count word reads the 17th
//!   side's dict count instead — the "teams" that follow are garbage
//!   read from the wrong offset (the goldens pin one such cascade).
//! - The build entry's **z is forced to ground level** (`loc.z = 0`
//!   after the read — the stored word is always 0), and the rebuild
//!   count wraps through the int word into a `u32`.
//! - Version 2 and lower **skip the build tail**, leaving the
//!   constructor defaults as load observables: an empty script, health
//!   100, whiner `true`, unsellable `false`, repairable `true`.
//! - **`validateSides` repairs the payload**: a map with no
//!   empty-named side gains the neutral player (the `""` name, the
//!   `L"Neutral"` display name, the `"team"` default); every player
//!   gains its default team (`"team" + playerName`) with the owner
//!   pointed back at the player and the singleton flag forced; the
//!   allies/enemies lists drop self-references and unknown player
//!   names (tokenized on `" \n\r\t"`, rejoined with single spaces); a
//!   team whose name matches a player's is removed; an orphaned or
//!   self-owned team's owner resets to the neutral's `""`.
//! - The neutral add is **bounded too**: with 16 sides already loaded
//!   the added side is dropped (the [`MAX_PLAYER_COUNT`] guard) while
//!   its default team is still appended — the goldens pin the
//!   orphaned `"team"`.
//! - A repeated `SidesList` chunk **clears at parse entry** and owns
//!   the final state outright.
//! - The nested `PlayerScriptsList` chunk is the **script-engine
//!   slice's** payload (plan §3.4 work package 7): the walk here
//!   skips it whole — nothing is registered in this scope. The
//!   scripts-into-sides read-back arrives with that slice.
//! - The reader never checks it consumed the chunk (the
//!   `atEndOfChunk` assert is debug-only); leftovers go to the walk's
//!   close, and a dict read past a short file mints zero-filled
//!   `keyAndType` words as empty-name `BOOL` pairs (the container's
//!   pinned stance).
//!
//! # Differences from the C++ (documented)
//!
//! - The linked lists and fixed arrays (the build list, the 16 side
//!   slots, the team array) are vectors: the build list's
//!   `addToBuildList(entry, j)` appends at the j-th position of a
//!   j-entry list (file order), the sides keep the same
//!   [`MAX_PLAYER_COUNT`] bound, and the team array's allocation
//!   growth is not an observable.
//! - The single-team removal loop: `removeTeam` refuses the last team
//!   and `validateSides`' restart would spin forever — but the
//!   default-team ensure pads the list first (every colliding team
//!   implies the player's distinct default team exists, and the
//!   deepest default never collides so it always survives), so the
//!   refusal is **unreachable through this load path**. The port keeps
//!   [`SidesError::TeamRemovalLoop`] for the latent contract, the
//!   same way the original keeps its own dead `parse`-false throw.
//! - The name-key interning: the original pre-interns the whole
//!   `WellKnownKeys.h` table at startup; the port's walk seeds exactly
//!   the **nine** keys this load consults, in the header's declaration
//!   order — the same relative order the original's table gives them,
//!   so the dict storage order agrees for any dict over these keys.
//!   A dict mixing other well-known names orders them by
//!   file-first-seen here where the original orders them by its
//!   startup table — the same documented stance as
//!   [`crate::map_objects`].
//! - The build entry's editor/solo-AI half (the render object, the
//!   object id, the gatherers, the rally-point offset the load never
//!   writes) is not map-file state.
//!
//! # Examples
//!
//! ```
//! use rzh_formats::map_chunk::DataChunkOutput;
//! use rzh_formats::map_sideslist::{
//!     SidesListData, SIDES_LIST_LABEL, K_SIDES_DATA_VERSION_3,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A one-side map (an unnamed neutral), built with the container's
//! // writer the way WorldBuilder saves one.
//! let mut out = DataChunkOutput::new();
//! out.open_data_chunk(SIDES_LIST_LABEL, K_SIDES_DATA_VERSION_3);
//! out.write_int(1);              // the side count
//! out.write_int(0);              // the player dict's pair count
//! out.write_int(0);              // the build-list count
//! out.write_int(0);              // the team count (version 2)
//! out.close_data_chunk();
//! let bytes = out.into_bytes()?;
//!
//! // The constructor's logical-load walk: the map's sides.
//! let sides = SidesListData::from_map(&bytes)?.expect("the chunk fired");
//! assert_eq!(sides.sides.len(), 1); // the neutral, name ""
//! assert_eq!(sides.teams.len(), 1); // its default "team"
//! assert!(sides.modified);          // validateSides added the default
//! # Ok(())
//! # }
//! ```

use std::fmt;

use crate::map_chunk::{
    DataChunkInfo, DataChunkInput, Dict, DictValue, MapChunkError, NameKeys, ParserTable,
};
use crate::map_lighting::Coord3d;

// ---------------------------------------------------------------------------
// constants (SidesList.cpp, GameCommon.h, WellKnownKeys.h)
// ---------------------------------------------------------------------------

/// The chunk label the constructor registers its sides-list reader
/// under, at the global scope.
pub const SIDES_LIST_LABEL: &[u8] = b"SidesList";

/// The nested chunk label the sides reader registers the script-engine
/// parser under, in its own scope — the script-engine slice's payload
/// (plan §3.4 work package 7); this slice's walk skips it whole.
pub const PLAYER_SCRIPTS_LIST_LABEL: &[u8] = b"PlayerScriptsList";

/// The sides chunk version words (`K_SIDES_DATA_VERSION_*`) — the
/// gates the build tail and the team list hang on.
pub mod sides_version {
    /// Version 1: the sides and build lists only.
    pub const K_SIDES_DATA_VERSION_1: u16 = 1;
    /// Version 2: adds the team list.
    pub const K_SIDES_DATA_VERSION_2: u16 = 2;
    /// Version 3: adds the build tail (script, health, whiner,
    /// unsellable, repairable) — the version Zero Hour's WorldBuilder
    /// writes.
    pub const K_SIDES_DATA_VERSION_3: u16 = 3;
}

pub use sides_version::{K_SIDES_DATA_VERSION_1, K_SIDES_DATA_VERSION_2, K_SIDES_DATA_VERSION_3};

/// The player-count ceiling (`MAX_PLAYER_COUNT`): the side loop breaks
/// at it mid-side, and the added neutral side is dropped at it.
pub const MAX_PLAYER_COUNT: usize = 16;

/// The well-known team-dict keys (`TheKey_teamName` and friends —
/// `WellKnownKeys.h`'s `DEFINE_KEY` declarations, in the header's
/// order; the interning note is in the module docs).
pub const TEAM_NAME_KEY: &[u8] = b"teamName";
/// `TheKey_teamOwner`.
pub const TEAM_OWNER_KEY: &[u8] = b"teamOwner";
/// `TheKey_teamIsSingleton`.
pub const TEAM_IS_SINGLETON_KEY: &[u8] = b"teamIsSingleton";
/// `TheKey_playerName` (an empty name is the reserved Neutral marker).
pub const PLAYER_NAME_KEY: &[u8] = b"playerName";
/// `TheKey_playerIsHuman`.
pub const PLAYER_IS_HUMAN_KEY: &[u8] = b"playerIsHuman";
/// `TheKey_playerDisplayName`.
pub const PLAYER_DISPLAY_NAME_KEY: &[u8] = b"playerDisplayName";
/// `TheKey_playerFaction`.
pub const PLAYER_FACTION_KEY: &[u8] = b"playerFaction";
/// `TheKey_playerEnemies`.
pub const PLAYER_ENEMIES_KEY: &[u8] = b"playerEnemies";
/// `TheKey_playerAllies`.
pub const PLAYER_ALLIES_KEY: &[u8] = b"playerAllies";

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the sides-list load path can produce.
///
/// The original has one failure code — `ERROR_CORRUPT_FILE_FORMAT`,
/// thrown by `readDict` on an impossible pair type and by the
/// constructor when the walk itself refuses — plus the process deaths
/// its unchecked operations reach. The port states each site as its
/// own variant; the dict errors stop the walk the way the escaping
/// exception does (the chunk stays open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidesError {
    /// The walk itself failed — the header never opened as a `CkMp`
    /// table of contents (the constructor's `if (!file.parse(this))`
    /// throw).
    CorruptFileFormat,
    /// A side or team dict met a pair whose type byte is not one of
    /// the five `Dict::DataType` values (`readDict`'s escaping
    /// `ERROR_CORRUPT_FILE_FORMAT`, its type byte kept).
    Dict(MapChunkError),
    /// The single-team collision the original's `removeTeam` refusal
    /// turns into an infinite `validateSides` restart — unreachable
    /// through this load path (the default-team ensure pads the list
    /// first; see the module docs), kept for the latent contract.
    TeamRemovalLoop,
}

impl fmt::Display for SidesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SidesError::CorruptFileFormat => {
                write!(f, "map is not a chunky data-chunk file")
            }
            SidesError::Dict(e) => write!(f, "sides list dict: {e}"),
            SidesError::TeamRemovalLoop => {
                write!(
                    f,
                    "team-name collision on the last team (the removal never terminates)"
                )
            }
        }
    }
}

impl std::error::Error for SidesError {}

impl From<MapChunkError> for SidesError {
    fn from(e: MapChunkError) -> Self {
        SidesError::Dict(e)
    }
}

// ---------------------------------------------------------------------------
// the payload (the sides and teams as the load leaves them)
// ---------------------------------------------------------------------------

/// One build-list entry — `BuildListInfo`'s saved members as the load
/// leaves them.
///
/// The location's z is always 0 (forced to ground level after the
/// read), and below version 3 the tail keeps the constructor's
/// defaults (an empty script, health 100, whiner `true`, unsellable
/// `false`, repairable `true`).
#[derive(Debug, Clone, PartialEq)]
pub struct BuildListEntryData {
    /// The building's instance name (`m_buildingName`).
    pub building_name: Vec<u8>,
    /// The `ThingTemplate` name (`m_templateName` — resolving it is
    /// the consumer's job).
    pub template_name: Vec<u8>,
    /// The location (`m_location` — z forced to 0 by the load).
    pub location: Coord3d,
    /// The initial orientation (`m_angle`).
    pub angle: f32,
    /// Whether the building starts built (`m_isInitiallyBuilt`).
    pub initially_built: bool,
    /// The rebuild allowance (`m_numRebuilds` — the file's `i32` word
    /// read into the unsigned member).
    pub num_rebuilds: u32,
    /// The rebuild script (`m_script` — empty below version 3).
    pub script: Vec<u8>,
    /// The initial health percentage (`m_health` — 100 below v3).
    pub health: i32,
    /// Whether the AI complains about the building (`m_whiner` — true
    /// below v3).
    pub whiner: bool,
    /// Whether the building can be sold (`m_unsellable` — false below
    /// v3).
    pub unsellable: bool,
    /// Whether the building can be repaired (`m_repairable` — true
    /// below v3).
    pub repairable: bool,
}

/// One side — the player dict and the starting build list, in file
/// order.
#[derive(Debug, Clone, PartialEq)]
pub struct SidesInfoData {
    /// The player properties (`m_dict` — the name, the human flag, the
    /// faction, the allies and enemies).
    pub dict: Dict,
    /// The starting buildings (`m_pBuildList`).
    pub build_list: Vec<BuildListEntryData>,
}

/// The `SidesList` payload: the sides (at most [`MAX_PLAYER_COUNT`]),
/// the teams and `validateSides`' modified flag, as the load leaves
/// them.
#[derive(Debug, Clone, PartialEq)]
pub struct SidesListData {
    /// The sides, in file order.
    pub sides: Vec<SidesInfoData>,
    /// The team dicts, in file order (from version 2), plus whatever
    /// `validateSides` added or repaired.
    pub teams: Vec<Dict>,
    /// Whether `validateSides` changed anything (the original's
    /// return value — the "had to clean up sideslist" trace).
    pub modified: bool,
}

// ---------------------------------------------------------------------------
// the dict helpers (Dict.cpp's typed getters, over the walk's table)
// ---------------------------------------------------------------------------

/// `Dict::getAsciiString(key)`: the pair named `name`'s bytes when it
/// is an exact-type ASCII match, empty for any other type or no pair.
fn dict_ascii_string(keys: &mut NameKeys, dict: &Dict, name: &[u8]) -> Vec<u8> {
    let key = keys.name_to_key(name);
    for i in 0..dict.pair_count() {
        if dict.nth_key(i) == key {
            if let DictValue::AsciiString(s) = dict.nth_value(i) {
                return s.clone();
            }
            return Vec::new();
        }
    }
    Vec::new()
}

/// `Dict::getBool(key)`: the pair named `name`'s flag when it is an
/// exact-type BOOL match, false otherwise.
fn dict_bool(keys: &mut NameKeys, dict: &Dict, name: &[u8]) -> bool {
    let key = keys.name_to_key(name);
    for i in 0..dict.pair_count() {
        if dict.nth_key(i) == key {
            if let DictValue::Bool(b) = dict.nth_value(i) {
                return *b;
            }
            return false;
        }
    }
    false
}

/// `Dict::setAsciiString(key, value)` through the walk's table.
fn set_ascii_string(keys: &mut NameKeys, dict: &mut Dict, name: &[u8], value: &[u8]) {
    let key = keys.name_to_key(name);
    dict.set(key, DictValue::AsciiString(value.to_vec()));
}

/// `Dict::setBool(key, value)` through the walk's table.
fn set_bool(keys: &mut NameKeys, dict: &mut Dict, name: &[u8], value: bool) {
    let key = keys.name_to_key(name);
    dict.set(key, DictValue::Bool(value));
}

// ---------------------------------------------------------------------------
// the reader (SidesList.cpp)
// ---------------------------------------------------------------------------

/// `SidesList::ParseSidesDataChunk` — the `SidesList` chunk reader:
/// the bounded side loop (a dict and a build list per side), the
/// version-2 team list, the nested scripts walk (nothing registered —
/// the script slice's chunks are skipped whole) and `validateSides`.
///
/// The `keys` table is the walk's interning state (the nine well-known
/// keys pre-seeded); dict reads intern their pair names through it, so
/// the dicts' stored order is the interning order.
pub fn parse_sides_data_chunk(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    keys: &mut NameKeys,
) -> Result<SidesListData, SidesError> {
    // TheSidesList->clear(); — the parser resets the whole payload at
    // entry (a repeated chunk owns the state outright).
    let mut sides: Vec<SidesInfoData> = Vec::new();
    let mut teams: Vec<Dict> = Vec::new();

    let count = file.read_int();
    for i in 0..count {
        if i >= MAX_PLAYER_COUNT as i32 {
            // the mid-side break: the 17th side's bytes stay unconsumed
            // (the version-2 team count reads them)
            break;
        }
        let dict = file.read_dict(keys)?;
        // addSide: the bounded append
        if sides.len() < MAX_PLAYER_COUNT {
            sides.push(SidesInfoData {
                dict,
                build_list: Vec::new(),
            });
        }
        let build_count = file.read_int();
        for _ in 0..build_count {
            // newInstance(BuildListInfo) — the constructor defaults
            // survive the version gates below
            let mut entry = BuildListEntryData {
                building_name: file.read_ascii_string(),
                template_name: file.read_ascii_string(),
                location: Coord3d {
                    x: file.read_real(),
                    y: file.read_real(),
                    z: file.read_real(),
                },
                angle: 0.0,
                initially_built: false,
                num_rebuilds: 0,
                script: Vec::new(),
                health: 100,
                whiner: true,
                unsellable: false,
                repairable: true,
            };
            entry.location.z = 0.0; // force to ground level
            entry.angle = file.read_real();
            entry.initially_built = file.read_byte() != 0;
            entry.num_rebuilds = file.read_int() as u32;
            if info.version >= K_SIDES_DATA_VERSION_3 {
                entry.script = file.read_ascii_string();
                entry.health = file.read_int();
                entry.whiner = file.read_byte() != 0;
                entry.unsellable = file.read_byte() != 0;
                entry.repairable = file.read_byte() != 0;
            }
            // addToBuildList(entry, j) — the j-th position of a
            // j-entry list: the append (file order). The side was just
            // added (the loop's break keeps the index in bounds).
            if let Some(side) = sides.last_mut() {
                side.build_list.push(entry);
            }
        }
    }

    if info.version >= K_SIDES_DATA_VERSION_2 {
        // (after the mid-side break: the 17th side's dict words)
        let team_count = file.read_int();
        teams.clear();
        for _ in 0..team_count {
            let dict = file.read_dict(keys)?;
            teams.push(dict); // addTeam
        }
    }

    // file.registerParser("PlayerScriptsList", info->label,
    //     ScriptList::ParseScriptsDataChunk); if (!file.parse(nullptr))
    //     throw ... — the script-engine slice's parser (plan §3.4
    //     WP7): nothing is registered here, so the nested walk skips
    //     those chunks whole and never fails. The port's parser makes
    //     the same nested walk (see the walk below).

    // validateSides — the post-load cleanup
    let mut data = SidesListData {
        sides,
        teams,
        modified: false,
    };
    validate_sides(&mut data, keys)?;
    Ok(data)
}

/// `Dict::setUnicodeString(key, value)` through the walk's table.
fn set_unicode_string(keys: &mut NameKeys, dict: &mut Dict, name: &[u8], units: &[u16]) {
    let key = keys.name_to_key(name);
    dict.set(key, DictValue::UnicodeString(units.to_vec()));
}

// ---------------------------------------------------------------------------
// the validation (SidesList::validateSides and friends)
// ---------------------------------------------------------------------------

/// `SidesList::findSideInfo` — the first side whose `playerName` is
/// `name` (the caller interns nothing; the key is pre-resolved).
fn find_side_index(keys: &mut NameKeys, sides: &[SidesInfoData], name: &[u8]) -> Option<usize> {
    for (i, side) in sides.iter().enumerate() {
        if dict_ascii_string(keys, &side.dict, PLAYER_NAME_KEY) == name {
            return Some(i);
        }
    }
    None
}

/// `TeamsInfoRec::findTeamInfo` — the first team whose `teamName` is
/// `name`.
fn find_team_index(keys: &mut NameKeys, teams: &[Dict], name: &[u8]) -> Option<usize> {
    for (i, team) in teams.iter().enumerate() {
        if dict_ascii_string(keys, team, TEAM_NAME_KEY) == name {
            return Some(i);
        }
    }
    None
}

/// `AsciiString::nextToken` — the default separators `" \n\r\t"`: skip
/// the separators, take the non-separator run, advance the remainder.
/// The empty remainder ends the walk (None).
fn next_token<'a>(s: &mut &'a [u8]) -> Option<&'a [u8]> {
    const SEPS: &[u8] = b" \n\r\t";
    let start = s.iter().position(|b| !SEPS.contains(b))?;
    let end = s[start..]
        .iter()
        .position(|b| SEPS.contains(b))
        .map(|n| start + n)
        .unwrap_or(s.len());
    let token = &s[start..end];
    *s = &s[end..];
    Some(token)
}

/// `SidesList::validateAllyEnemyList` — owners/allies/enemies must be
/// players: the self-reference and the unknown names drop, the
/// survivors rejoin with single spaces. Returns whether anything
/// changed.
fn validate_ally_enemy_list(
    keys: &mut NameKeys,
    sides: &[SidesInfoData],
    player_name: &[u8],
    list: &mut Vec<u8>,
) -> bool {
    let mut modified = false;

    let mut rest: &[u8] = list;
    let mut rebuilt: Vec<u8> = Vec::new();
    while let Some(token) = next_token(&mut rest) {
        if token == player_name {
            modified = true; // no allies/enemies with self
            continue;
        }
        if find_side_index(keys, sides, token).is_none() {
            modified = true; // player not found
            continue;
        }
        if !rebuilt.is_empty() {
            rebuilt.push(b' ');
        }
        rebuilt.extend_from_slice(token);
    }
    *list = rebuilt;
    modified
}

/// `SidesList::addPlayerByTemplate("")` — the neutral player: the
/// empty name (the reserved Neutral marker), the `L"Neutral"` display
/// name, human false, the empty faction and relation lists, plus its
/// default `team` team. The side add is bounded ([`MAX_PLAYER_COUNT`]);
/// the team add is not.
fn add_neutral_player(keys: &mut NameKeys, data: &mut SidesListData) {
    // playerName = "" — magic code for "neutral"; displayName =
    // L"Neutral" (the UnicodeString pair); isHuman = false
    let mut dict = Dict::new();
    set_ascii_string(keys, &mut dict, PLAYER_NAME_KEY, b"");
    set_bool(keys, &mut dict, PLAYER_IS_HUMAN_KEY, false);
    set_unicode_string(
        keys,
        &mut dict,
        PLAYER_DISPLAY_NAME_KEY,
        &[
            0x004E, 0x0065, 0x0075, 0x0074, 0x0072, 0x0061, 0x006C, // "Neutral"
        ],
    );
    set_ascii_string(keys, &mut dict, PLAYER_FACTION_KEY, b"");
    set_ascii_string(keys, &mut dict, PLAYER_ALLIES_KEY, b"");
    set_ascii_string(keys, &mut dict, PLAYER_ENEMIES_KEY, b"");
    if data.sides.len() < MAX_PLAYER_COUNT {
        data.sides.push(SidesInfoData {
            dict,
            build_list: Vec::new(),
        });
    }

    // the default team: "team" + "" = "team", owned by the neutral
    let mut team = Dict::new();
    set_ascii_string(keys, &mut team, TEAM_NAME_KEY, b"team");
    set_ascii_string(keys, &mut team, TEAM_OWNER_KEY, b"");
    set_bool(keys, &mut team, TEAM_IS_SINGLETON_KEY, true);
    data.teams.push(team);
}

/// `TeamsInfoRec::removeTeam` — refuses the last team (and the
/// out-of-range index, which the caller never passes); the shift tail
/// is the vector's remove.
fn remove_team(teams: &mut Vec<Dict>, i: usize) -> bool {
    if teams.len() <= 1 {
        return false;
    }
    teams.remove(i);
    true
}

/// `SidesList::validateSides` — the post-load cleanup (see the module
/// docs). The single-team collision is the unreachable refusal (the
/// default-team ensure pads the list first — the deepest default never
/// collides so it survives every removal).
fn validate_sides(data: &mut SidesListData, keys: &mut NameKeys) -> Result<(), SidesError> {
    // ensure we have at least one player, and at least one neutral
    // player.
    let mut neutral = None;
    for (i, side) in data.sides.iter().enumerate() {
        if dict_ascii_string(keys, &side.dict, PLAYER_NAME_KEY).is_empty() {
            neutral = Some(i);
            break;
        }
    }
    if neutral.is_none() {
        add_neutral_player(keys, data);
        data.modified = true;
    }

    // now ensure that every player has a proper "default team"
    for i in 0..data.sides.len() {
        let player_name = dict_ascii_string(keys, &data.sides[i].dict, PLAYER_NAME_KEY);
        let mut team_name = b"team".to_vec();
        team_name.extend_from_slice(&player_name);

        match find_team_index(keys, &data.teams, &team_name) {
            Some(ti) => {
                // make sure the team owner points back to the player
                if dict_ascii_string(keys, &data.teams[ti], TEAM_OWNER_KEY) != player_name {
                    set_ascii_string(keys, &mut data.teams[ti], TEAM_OWNER_KEY, &player_name);
                    data.modified = true;
                }
                // default teams are always singletons
                if !dict_bool(keys, &data.teams[ti], TEAM_IS_SINGLETON_KEY) {
                    set_bool(keys, &mut data.teams[ti], TEAM_IS_SINGLETON_KEY, true);
                    data.modified = true;
                }
            }
            None => {
                let mut team = Dict::new();
                set_ascii_string(keys, &mut team, TEAM_NAME_KEY, &team_name);
                set_ascii_string(keys, &mut team, TEAM_OWNER_KEY, &player_name);
                set_bool(keys, &mut team, TEAM_IS_SINGLETON_KEY, true);
                data.teams.push(team);
                data.modified = true;
            }
        }

        // ensure all teams have valid allies & enemies (the tokens must
        // name players)
        let mut allies = dict_ascii_string(keys, &data.sides[i].dict, PLAYER_ALLIES_KEY);
        let mut enemies = dict_ascii_string(keys, &data.sides[i].dict, PLAYER_ENEMIES_KEY);

        if validate_ally_enemy_list(keys, &data.sides, &player_name, &mut allies) {
            set_ascii_string(keys, &mut data.sides[i].dict, PLAYER_ALLIES_KEY, &allies);
            data.modified = true;
        }
        if validate_ally_enemy_list(keys, &data.sides, &player_name, &mut enemies) {
            set_ascii_string(keys, &mut data.sides[i].dict, PLAYER_ENEMIES_KEY, &enemies);
            data.modified = true;
        }
    }

    // ensure there's no overlap between team names and player names
    // (if there is, the player wins and the team is whacked).
    let mut i = 0;
    while i < data.teams.len() {
        let team_name = dict_ascii_string(keys, &data.teams[i], TEAM_NAME_KEY);
        if find_side_index(keys, &data.sides, &team_name).is_some() {
            if !remove_team(&mut data.teams, i) {
                // removeTeam's last-team refusal: validateSides'
                // restart spins forever — unreachable through this
                // load path (see the module docs)
                return Err(SidesError::TeamRemovalLoop);
            }
            data.modified = true;
            i = 0; // goto validate_team_names
            continue;
        }
        i += 1;
    }

    for team in &mut data.teams {
        let team_name = dict_ascii_string(keys, team, TEAM_NAME_KEY);
        let owner = dict_ascii_string(keys, team, TEAM_OWNER_KEY);
        if find_side_index(keys, &data.sides, &owner).is_none() || owner == team_name {
            set_ascii_string(keys, team, TEAM_OWNER_KEY, b"");
            data.modified = true;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// the walk (the constructor's logical-load shape, sides only)
// ---------------------------------------------------------------------------

impl SidesListData {
    /// The `WorldHeightMap` constructor's **logical-data-only** walk,
    /// restricted to this payload: `SidesList` registered at the global
    /// scope, each chunk clearing and replacing the whole payload (the
    /// last chunk owns the final state, `validateSides` included).
    ///
    /// `Ok(None)` is the map with no `SidesList` chunk at all — the
    /// original's singleton keeps whatever it held (a fresh process's
    /// empty lists).
    ///
    /// The other payloads the logical load registers arrive with their
    /// own slices — here their chunks are skipped whole, the walk's
    /// behaviour for an unmatched label.
    pub fn from_map(bytes: &[u8]) -> Result<Option<Self>, SidesError> {
        let mut input = DataChunkInput::new(bytes);
        let mut table: ParserTable<'_, SidesWalk> = ParserTable::new();
        table.register(SIDES_LIST_LABEL, b"", sides_chunk_parser);
        let mut user = SidesWalk {
            result: None,
            keys: NameKeys::new(),
            error: None,
        };
        // the nine well-known keys seeded the way the engine's static
        // initialization does (WellKnownKeys.h's declaration order)
        for key in [
            TEAM_NAME_KEY,
            TEAM_OWNER_KEY,
            TEAM_IS_SINGLETON_KEY,
            PLAYER_NAME_KEY,
            PLAYER_IS_HUMAN_KEY,
            PLAYER_DISPLAY_NAME_KEY,
            PLAYER_FACTION_KEY,
            PLAYER_ENEMIES_KEY,
            PLAYER_ALLIES_KEY,
        ] {
            user.keys.name_to_key(key);
        }
        if !input.parse(&mut table, &mut user) {
            return Err(user.error.unwrap_or(SidesError::CorruptFileFormat));
        }
        Ok(user.result)
    }
}

/// The walk's user state: the payload the last chunk left, the
/// name-key table and the error capture.
struct SidesWalk {
    result: Option<SidesListData>,
    keys: NameKeys,
    error: Option<SidesError>,
}

/// The registered `SidesList` parser: dispatch to the reader (which
/// ends with `validateSides`), then the nested scripts walk — the
/// original's `file.parse(nullptr)` after registering the script
/// parser in this scope. Nothing is registered for it here (the
/// script-engine slice's payload), so the walk skips those chunks
/// whole and never fails; the call keeps the original's shape for the
/// slice that extends it.
fn sides_chunk_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, SidesWalk>,
    user: &mut SidesWalk,
) -> bool {
    match parse_sides_data_chunk(file, info, &mut user.keys) {
        Ok(data) => {
            // (with no parser registered in this scope, the verdict is
            // always true — the same dead throw the original keeps)
            let _ = file.parse(table, user);
            user.result = Some(data);
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
    use crate::map_chunk::{DataChunkOutput, dict_type};

    /// A test file's table of contents.
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
    fn put_f32(b: &mut Vec<u8>, v: f32) {
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

    fn pair_ascii(toc: &Toc, key: &[u8], v: &[u8]) -> Vec<u8> {
        let mut p = pair(toc, dict_type::DICT_ASCIISTRING, key, &[]);
        p.extend_from_slice(&(v.len() as u16).to_le_bytes());
        p.extend_from_slice(v);
        p
    }
    fn pair_bool(toc: &Toc, key: &[u8], v: bool) -> Vec<u8> {
        let mut p = pair(toc, dict_type::DICT_BOOL, key, &[]);
        p.push(u8::from(v));
        p
    }

    /// A dict payload (the pair count and pairs).
    fn dict_payload(pairs: &[Vec<u8>]) -> Vec<u8> {
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
        assert_eq!(K_SIDES_DATA_VERSION_1, 1);
        assert_eq!(K_SIDES_DATA_VERSION_2, 2);
        assert_eq!(K_SIDES_DATA_VERSION_3, 3);
        assert_eq!(SIDES_LIST_LABEL, b"SidesList");
        assert_eq!(PLAYER_SCRIPTS_LIST_LABEL, b"PlayerScriptsList");
        assert_eq!(MAX_PLAYER_COUNT, 16);
        assert_eq!(TEAM_NAME_KEY, b"teamName");
        assert_eq!(PLAYER_NAME_KEY, b"playerName");
    }

    #[test]
    fn a_v3_map_loads_with_its_defaults_repaired() {
        let toc = Toc::new(&[
            b"SidesList",
            b"playerName",
            b"playerIsHuman",
            b"playerFaction",
            b"playerEnemies",
            b"playerAllies",
            b"teamName",
            b"teamOwner",
            b"teamIsSingleton",
        ]);
        let mut payload = Vec::new();
        put_i32(&mut payload, 1);
        // a named player with no default team and a self-referencing
        // allies list
        payload.extend_from_slice(&dict_payload(&[
            pair_ascii(&toc, b"playerName", b"Plyr1"),
            pair_bool(&toc, b"playerIsHuman", true),
            pair_ascii(&toc, b"playerAllies", b"Plyr1 Nobody"),
        ]));
        put_i32(&mut payload, 0); // no builds
        put_i32(&mut payload, 0); // no teams
        let mut chunk = Vec::new();
        put_chunk(
            &mut chunk,
            toc.id(b"SidesList"),
            K_SIDES_DATA_VERSION_3,
            &payload,
        );
        let bytes = build_map(&toc, &[chunk]);

        let data = SidesListData::from_map(&bytes).unwrap().expect("fired");
        // validateSides: the neutral added, the default team added, the
        // allies cleaned (both tokens dropped — self and unknown)
        assert_eq!(data.sides.len(), 2);
        assert_eq!(data.teams.len(), 2);
        assert!(data.modified);
        let allies =
            dict_ascii_string(&mut NameKeys::new(), &data.sides[1].dict, PLAYER_ALLIES_KEY);
        assert!(allies.is_empty());
        let team_name = {
            let mut keys = NameKeys::new();
            for key in [TEAM_NAME_KEY, TEAM_OWNER_KEY, TEAM_IS_SINGLETON_KEY] {
                keys.name_to_key(key);
            }
            dict_ascii_string(&mut keys, &data.teams[1], TEAM_NAME_KEY)
        };
        assert_eq!(team_name, b"teamPlyr1");
    }

    #[test]
    fn the_build_tail_keeps_the_defaults_below_v3() {
        let toc = Toc::new(&[b"SidesList", b"playerName"]);
        let mut payload = Vec::new();
        put_i32(&mut payload, 1);
        payload.extend_from_slice(&dict_payload(&[pair_ascii(&toc, b"playerName", b"")]));
        // one build entry, the v2 shape (no tail)
        put_i32(&mut payload, 1);
        payload.extend_from_slice(&(4u16).to_le_bytes());
        payload.extend_from_slice(b"Name");
        payload.extend_from_slice(&(4u16).to_le_bytes());
        payload.extend_from_slice(b"Temp");
        put_f32(&mut payload, 10.0);
        put_f32(&mut payload, 20.0);
        put_f32(&mut payload, 500.0); // the z (forced to 0)
        put_f32(&mut payload, 1.5); // the angle
        payload.push(1); // initially built
        put_i32(&mut payload, -1); // the rebuilds (wraps to 0xFFFFFFFF)
        put_i32(&mut payload, 0); // no teams
        let mut chunk = Vec::new();
        put_chunk(
            &mut chunk,
            toc.id(b"SidesList"),
            K_SIDES_DATA_VERSION_2,
            &payload,
        );
        let bytes = build_map(&toc, &[chunk]);

        let data = SidesListData::from_map(&bytes).unwrap().expect("fired");
        let entry = &data.sides[0].build_list[0];
        assert_eq!(entry.location.z, 0.0); // forced to ground
        assert_eq!(entry.location.x, 10.0);
        assert_eq!(entry.angle, 1.5);
        assert!(entry.initially_built);
        assert_eq!(entry.num_rebuilds, 0xFFFF_FFFF); // the wrap
        assert!(entry.script.is_empty()); // the v2 defaults
        assert_eq!(entry.health, 100);
        assert!(entry.whiner);
        assert!(!entry.unsellable);
        assert!(entry.repairable);
    }

    #[test]
    fn the_side_loop_breaks_at_sixteen() {
        let toc = Toc::new(&[b"SidesList", b"playerName"]);
        let mut payload = Vec::new();
        put_i32(&mut payload, 17);
        for _ in 0..16 {
            payload.extend_from_slice(&dict_payload(&[pair_ascii(&toc, b"playerName", b"P")]));
            put_i32(&mut payload, 0); // no builds
        }
        // the 17th side's dict: never parsed by the side loop — its
        // words shape the garbage team count (the id-0 BOOL pair makes
        // the word 1: one empty team read from the keyAndType tail)
        let mut pair = pair(&toc, dict_type::DICT_BOOL, b"", &[]);
        pair.push(1);
        payload.extend_from_slice(&dict_payload(&[pair]));
        put_i32(&mut payload, 0);
        let mut chunk = Vec::new();
        put_chunk(
            &mut chunk,
            toc.id(b"SidesList"),
            K_SIDES_DATA_VERSION_3,
            &payload,
        );
        let bytes = build_map(&toc, &[chunk]);

        let data = SidesListData::from_map(&bytes).unwrap().expect("fired");
        // sixteen sides (the added neutral is dropped at the bound —
        // the seventeenth never parsed), the neutral's default team
        // still appended, the 16 defaults, and the garbage "team" read
        // from the 17th side's dict words
        assert_eq!(data.sides.len(), 16);
    }

    #[test]
    fn an_absent_chunk_is_none_and_a_nested_one_never_fires() {
        let toc = Toc::new(&[b"HeightMapData"]);
        let mut chunk = Vec::new();
        put_chunk(&mut chunk, 1, 1, &[]);
        let bytes = build_map(&toc, &[chunk]);
        assert_eq!(SidesListData::from_map(&bytes).unwrap(), None);

        let toc = Toc::new(&[b"Container", b"SidesList"]);
        let mut inner = Vec::new();
        put_i32(&mut inner, 0);
        let mut nested = Vec::new();
        put_chunk(&mut nested, 2, K_SIDES_DATA_VERSION_3, &inner);
        let mut chunk = Vec::new();
        put_chunk(&mut chunk, 1, 1, &nested);
        let bytes = build_map(&toc, &[chunk]);
        assert_eq!(SidesListData::from_map(&bytes).unwrap(), None);
    }

    #[test]
    fn the_walk_refuses_non_chunky_bytes() {
        assert_eq!(
            SidesListData::from_map(b"not a map at all"),
            Err(SidesError::CorruptFileFormat)
        );
    }

    #[test]
    fn the_tokenizer_splits_on_the_default_separators() {
        let mut s: &[u8] = b"  Alpha\tBeta\nNobody\r ";
        assert_eq!(next_token(&mut s), Some(&b"Alpha"[..]));
        assert_eq!(next_token(&mut s), Some(&b"Beta"[..]));
        assert_eq!(next_token(&mut s), Some(&b"Nobody"[..]));
        assert_eq!(next_token(&mut s), None);
    }

    /// The example in the module docs (kept here so a doctest failure
    /// names the unit).
    #[test]
    fn the_doctest_shape_loads() {
        let mut out = DataChunkOutput::new();
        out.open_data_chunk(SIDES_LIST_LABEL, K_SIDES_DATA_VERSION_3);
        out.write_int(1);
        out.write_int(0);
        out.write_int(0);
        out.write_int(0);
        out.close_data_chunk();
        let bytes = out.into_bytes().expect("writes");
        let sides = SidesListData::from_map(&bytes).unwrap().expect("fired");
        assert_eq!(sides.sides.len(), 1);
        assert_eq!(sides.teams.len(), 1);
        assert!(sides.modified);
    }
}
