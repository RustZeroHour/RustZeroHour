// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   GeneralsMD/Code/GameEngine/Source/GameLogic/ScriptEngine/Scripts.cpp
//     (ScriptList::ParseScriptsDataChunk with the TScriptListReadInfo
//     statics and their leftover clear, ParseScriptListDataChunk with the
//     MAX_PLAYER_COUNT bound refusal, ScriptGroup::ParseGroupDataChunk
//     with the exact version-2 subroutine gate, Script::ParseScript and
//     the FromList/FromGroup wrappers with the AT_END appends,
//     OrCondition::ParseOrConditionDataChunk's tail-append,
//     Condition::ParseConditionDataChunk with the version-4 name-key
//     rematch, the version-1 ParameterChangesVer2 append heal, the
//     SKIRMISH_SPECIAL_POWER_READY heal and the template count check,
//     ScriptAction::ParseAction with the version-2 name-key rematch,
//     the whole heal switch and the template count check, and
//     Parameter::ReadParameter with the COORD3D shape, the OBJECT_TYPE
//     Fundamentalist->GLA rename, the UPGRADE rename trio, the
//     OBJECT_STATUS name-to-mask heal and the KIND_OF_PARAM heal)
//   GeneralsMD/Code/GameEngine/Source/GameLogic/ScriptEngine/ScriptEngine.cpp
//     (ScriptEngine::init's template registry — every condition's and
//     action's internal name and declared parameter count — with the
//     closing name-key loops, and getConditionTemplate/getActionTemplate
//     with the out-of-range clamp to index 0)
//   GeneralsMD/Code/GameEngine/Include/GameLogic/Scripts.h
//     (the ParameterType, ConditionType and ScriptActionType enums,
//     MAX_PARMS = 12, THIS_PLAYER)
//   GeneralsMD/Code/GameEngine/Include/GameLogic/ScriptEngine.h
//     (TScriptListReadInfo)
//   Core/GameEngine/Include/Common/GameCommon.h (MAX_PLAYER_COUNT = 16)
//   Core/GameEngine/Source/Common/System/KindOf.cpp
//     (KindOfMaskType::s_bitNameList, the Zero Hour build)
//   Core/GameEngine/Source/Common/System/ObjectStatusTypes.cpp
//     (ObjectStatusMaskType::s_bitNameList)
//   Core/GameEngine/Source/GameLogic/Map/SidesList.cpp and
//     GeneralsMD/Code/GameEngine/Source/GameLogic/System/GameLogic.cpp
//     (the two registrations that drive the reader: the SidesList tail
//     inside the map and the root scope of the standalone script files)
//
// See the module docs for the format layout and the documented
// differences from the C++ (the verdict family; the node stack; the
// vectors).

//! The **script-engine load path** — the `PlayerScriptsList` chunk of a
//! `.map` file (nested in its [`SidesList`](crate::map_sideslist)
//! chunk) and of the standalone `.scb` script files, on top of
//! [`crate::map_chunk`].
//!
//! A player's scripts are a flat list plus one level of grouping: each
//! [`ScriptData`] carries four comment strings, six flag bytes and a
//! delay word, an or-chain of and-chains of
//! [`ConditionData`](ConditionData)s (each a type word, a name key and
//! up to [`MAX_PARMS`] parameters) and two action chains (the THEN and
//! the ELSE lists, each [`ScriptActionData`]s of the same shape). The
//! reader is the first Phase 3 slice's territory the map format
//! reaches: it consults the script engine's **template registry** —
//! the internal names and parameter counts `ScriptEngine::init()`
//! gives every condition and action — to repair id/name mismatches by
//! name and to reject parm-count mismatches, and it heals old files
//! (version-1 area conditions, one-parm special-power checks, the
//! camera action shapes of every generation).
//!
//! # The chunks in one page
//!
//! ```text
//! PlayerScriptsList chunk (version never read; the writer writes 5)
//!   ScriptList chunk × n          one per player (at most 16)
//!     Script chunk*                 the flat scripts
//!     ScriptGroup chunk*            the groups
//!       str16                        the group name
//!       u8                           active
//!       [v == 2] u8                  subroutine (an exact equality:
//!                                     a version 3 group reads like a 1)
//!       Script chunk*                the group's scripts
//! Script chunk (version gates the delay word)
//!   str16 × 4                       name, comment, cond/act comments
//!   u8 × 6                          active, one-shot, easy, normal,
//!                                   hard, subroutine
//!   [v >= 2] i32                    the evaluation delay (seconds)
//!   OrCondition chunk*              the or-clauses
//!     Condition chunk*                the and-chain
//!       i32                            the condition type
//!       [v >= 4] i32                  the name key (the id/name check)
//!       i32                            the parameter count (≤ 12)
//!       parameter × n                 see below
//!   ScriptAction chunk*             the THEN list
//!   ScriptActionFalse chunk*        the ELSE list
//!     i32                            the action type
//!     [v >= 2] i32                   the name key
//!     i32                            the parameter count (≤ 12)
//!     parameter × n
//! parameter
//!   i32                            the parameter type
//!   [COORD3D] f32 × 3               the coordinate shape
//!   [else] i32, f32, str16          the int/real/string shape
//! ```
//!
//! # The load-path quirks (transcribed)
//!
//! - **The name-key rematch**: a version-4 condition (or version-2
//!   action) whose type word disagrees with its name key is *repaired
//!   by name* — the registry is rescanned for the key, and the found
//!   index wins. An unknown name leaves the type `CONDITION_FALSE`
//!   (or `NO_OP`) with zero parms. An out-of-range type word reads
//!   template 0 (the `getConditionTemplate` clamp), so an out-of-range
//!   **condition** type with zero parms or an out-of-range **action**
//!   type with exactly one parm survives the count check as the raw
//!   type — the goldens pin both survivors.
//! - **The empty name key**: a name key whose table-of-contents id is
//!   unknown interns the empty string — which is also the internal
//!   name the registry's *defunct* entries carry (the six conditions
//!   and one action `init()` never assigns). Such a key therefore
//!   rematches to the first defunct condition
//!   ([`CT_OBSOLETE_SCRIPT_1`]) or the defunct action
//!   ([`AT_NAMED_RECEIVE_UPGRADE`]). A negative key word shifts
//!   arithmetically to the same unknown id.
//! - **The heal family**: version-1 area conditions (the seven
//!   [`PARAMETER_CHANGES_VER2`] types) get a `SURFACES_ALLOWED`
//!   parameter **appended at the loaded count** and the count forced
//!   to 3 — the append lands at slot `numParms`, so a count above 2
//!   hides the appended parameter (a 5-parm condition keeps its loaded
//!   first three); a one-parm `SKIRMISH_SPECIAL_POWER_READY` gets the
//!   SIDE `"<This Player>"` prepended; and the action switch pads or
//!   re-types a dozen legacy shapes (the SIDE prepends, the BOOLEAN
//!   appends, the FRONT/FLANK flank switch, the attitude re-type to
//!   `AI_MOOD`, the camera `PERCENT`/`REAL` pads).
//! - **The count check**: a condition whose parm count disagrees with
//!   its (possibly rematched) template's becomes `CONDITION_FALSE`
//!   with zero parms; an action's becomes `NO_OP` — the loaded
//!   parameters stay in the slots but become unobservable (the port
//!   drops them with the count).
//! - **Parameter rewrites**: an `OBJECT_TYPE` whose name starts with
//!   `Fundamentalist` becomes `GLA` + the suffix (truncated at 255
//!   bytes, the original's `strlcat` into `char[256]`); three obsolete
//!   capture-building upgrade names become
//!   `Upgrade_InfantryCaptureBuilding`; an `OBJECT_STATUS` name
//!   resolves case-insensitively to its bit (`"NONE"` is bit 0!) into
//!   the 45-bit mask; a `KIND_OF_PARAM` name resolves
//!   case-insensitively to its table index, the legacy `CRUSHER`,
//!   `CRUSHABLE` and `OVERLAPPABLE` names map to index 0 (OBSTACLE,
//!   "switched to OBSTACLE"), and `MISSILE` rewrites to
//!   `SMALL_MISSILE` with the real `SMALL_MISSILE` index — the
//!   original's re-compare tests the already-rewritten string against
//!   the literal, so it matches at index 0 first and the outer loop's
//!   continuation overwrites it with the table index.
//! - **The 17th script list**: the reader's bound refusal returns
//!   false and walks the whole file out — sixteen lists is the
//!   maximum, and the refusal is the only false path in the parser
//!   tree (the goldens pin it as the `TooManyLists` error).
//! - **The statics' leftover clear**: a second `PlayerScriptsList`
//!   chunk in the same file deletes the first walk's lists before
//!   parsing — last wins.
//! - **The scoping**: the chunk labels only fire under their
//!   registered parents — a `Script` directly under
//!   `PlayerScriptsList`, a `ScriptList` under `ScriptList`, an
//!   `OrCondition` at the root and a `ScriptGroup` under
//!   `ScriptGroup` all skip whole.
//! - The reader never checks it consumed a chunk (the
//!   `atEndOfChunk` asserts are debug-only); leftovers go to the
//!   walk's close, and reads past a short chunk or file zero-fill and
//!   drive `dataLeft` negative — the container's pinned stance.
//!
//! # Differences from the C++ (documented)
//!
//! - The verdict family: the original's out-of-bounds stores — a 13th
//!   parameter past `m_parms[12]`, the version-1 heal's append at a
//!   count of 12 or negative, and the `KIND_OF_PARAM` empty-string
//!   normalization reading `kindofNames[]` out of range (the 117 slot
//!   is the null terminator, an `AsciiString`-from-null death) — are
//!   [`ScriptsError`] variants instead, the walk refusing where the
//!   original leaves defined behaviour; the parameter's bytes are
//!   consumed first, matching the original's read-then-store order.
//! - The unknown-kindof name is the original's `throw ERROR_BUG` —
//!   the port's [`ScriptsError::KindOfUnknown`] keeps the abort.
//! - The linked lists (the scripts, groups, or-clauses, and-chains and
//!   action chains) are vectors in file order — the appends are all
//!   `AT_END`. The fixed `m_parms[12]` arrays are vectors of
//!   `Option<ParameterData>`: the `None`s are the null slots the
//!   version-1 heal leaves (a zero- or one-parm area condition heals
//!   to three parms with null tails).
//! - The parse-time parameter slots the count check abandons (the
//!   `CONDITION_FALSE`/`NO_OP` fallbacks) are dropped with the count
//!   — the original leaks them unobservably.
//! - The `userData` threading through the nested `parse` calls (the
//!   original passes each node's pointer) is a node stack inside
//!   [`ScriptsWalk`]: each parser pushes its node, parses, pops and
//!   attaches — the observable walk is identical.
//! - `ParseScript`'s failure path (a null script reaching `addScript`,
//!   a null dereference) is unreachable through this walk — the only
//!   false-returning parser in the tree is the list bound, which
//!   aborts the walk before the add — and is not modelled.
//! - The name-key interning starts from the template registry's
//!   closing loops (all 447 internal names, conditions first): the
//!   game's earlier startup interning is not observable through this
//!   walk (the rematch compares interned keys, where equality is
//!   string equality however the ids fell).
//!
//! # Examples
//!
//! ```no_run
//! # use rzh_formats::map_scripts::PlayerScriptsData;
//! # let bytes: &[u8] = &[];
//! // the standalone .scb / GameLogic root registration
//! let scripts = PlayerScriptsData::from_map(bytes).unwrap();
//! // the map's SidesList tail registration
//! let scripts_nested = PlayerScriptsData::from_map_in_sides(bytes).unwrap();
//! ```
//!
//! The manual-walk surface (what the module's own walks wrap):
//!
//! ```
//! use rzh_formats::map_chunk::{DataChunkInput, ParserTable};
//! use rzh_formats::map_scripts::{ScriptsWalk, scripts_chunk_parser};
//!
//! # fn main() {
//! # let bytes: &[u8] = include_bytes!(
//! #     "../tests/golden/mapscripts.txt" // (placeholder: any bytes)
//! # );
//! # let bytes: &[u8] = &bytes[..0];
//! let mut input = DataChunkInput::new(bytes);
//! let mut table: ParserTable<'_, ScriptsWalk> = ParserTable::new();
//! table.register(b"PlayerScriptsList", b"", scripts_chunk_parser);
//! let mut user = ScriptsWalk::new();
//! if input.parse(&mut table, &mut user) {
//!     let lists = user.take_read_scripts();
//!     # let _ = lists;
//! }
//! # }
//! ```

use std::fmt;

use crate::map_chunk::{DataChunkInfo, DataChunkInput, NameKey, NameKeys, ParserTable};
use crate::map_lighting::Coord3d;
use crate::map_sideslist::{MAX_PLAYER_COUNT, PLAYER_SCRIPTS_LIST_LABEL, SIDES_LIST_LABEL};

// ---------------------------------------------------------------------------
// constants (Scripts.cpp, Scripts.h, GameCommon.h)
// ---------------------------------------------------------------------------

/// The maximum parameter count a condition or action carries
/// (`MAX_PARMS`): the `m_parms` array the load stores into — a larger
/// loaded count is the [`ScriptsError::ParameterStoreOob`] verdict.
pub const MAX_PARMS: i32 = 12;

/// The parameter the special-power heals prepend (`THIS_PLAYER`).
pub const THIS_PLAYER: &[u8] = b"<This Player>";

/// The version words of the script chunk family (the anonymous enum in
/// `Scripts.cpp`).
pub mod script_version {
    /// `K_SCRIPTS_DATA_VERSION_1` — the version the writer puts on the
    /// `PlayerScriptsList` chunk (the enumeration continues past the
    /// condition versions: it is 5). No reader of that chunk consults
    /// it.
    pub const K_SCRIPTS_DATA_VERSION_1: u16 = 5;
    /// `K_SCRIPT_LIST_DATA_VERSION_1` — never read either.
    pub const K_SCRIPT_LIST_DATA_VERSION_1: u16 = 1;
    /// `K_SCRIPT_GROUP_DATA_VERSION_1` — the group version without the
    /// subroutine byte.
    pub const K_SCRIPT_GROUP_DATA_VERSION_1: u16 = 1;
    /// `K_SCRIPT_GROUP_DATA_VERSION_2` — the only group version that
    /// carries the subroutine byte (an exact equality: version 3 and up
    /// read like version 1).
    pub const K_SCRIPT_GROUP_DATA_VERSION_2: u16 = 2;
    /// `K_SCRIPT_DATA_VERSION_1` — the script version without the
    /// delay word.
    pub const K_SCRIPT_DATA_VERSION_1: u16 = 1;
    /// `K_SCRIPT_DATA_VERSION_2` — the script version with the delay
    /// word (and any later version reads the same).
    pub const K_SCRIPT_DATA_VERSION_2: u16 = 2;
    /// `K_SCRIPT_OR_CONDITION_DATA_VERSION_1`.
    pub const K_SCRIPT_OR_CONDITION_DATA_VERSION_1: u16 = 1;
    /// `K_SCRIPT_ACTION_VERSION_1` — the action version without the
    /// name key.
    pub const K_SCRIPT_ACTION_VERSION_1: u16 = 1;
    /// `K_SCRIPT_ACTION_VERSION_2` — the action version with the name
    /// key (and any later version reads the same).
    pub const K_SCRIPT_ACTION_VERSION_2: u16 = 2;
    /// `K_SCRIPT_CONDITION_VERSION_1` — no name key, and the
    /// version-1 area-condition heal fires.
    pub const K_SCRIPT_CONDITION_VERSION_1: u16 = 1;
    /// `K_SCRIPT_CONDITION_VERSION_2` — no name key, no heal.
    pub const K_SCRIPT_CONDITION_VERSION_2: u16 = 2;
    /// `K_SCRIPT_CONDITION_VERSION_3`.
    pub const K_SCRIPT_CONDITION_VERSION_3: u16 = 3;
    /// `K_SCRIPT_CONDITION_VERSION_4` — the name key (and any later
    /// version reads the same).
    pub const K_SCRIPT_CONDITION_VERSION_4: u16 = 4;
}

pub use script_version::{
    K_SCRIPT_ACTION_VERSION_1, K_SCRIPT_ACTION_VERSION_2, K_SCRIPT_CONDITION_VERSION_1,
    K_SCRIPT_CONDITION_VERSION_2, K_SCRIPT_CONDITION_VERSION_3, K_SCRIPT_CONDITION_VERSION_4,
    K_SCRIPT_DATA_VERSION_1, K_SCRIPT_DATA_VERSION_2, K_SCRIPT_GROUP_DATA_VERSION_1,
    K_SCRIPT_GROUP_DATA_VERSION_2, K_SCRIPT_LIST_DATA_VERSION_1,
    K_SCRIPT_OR_CONDITION_DATA_VERSION_1, K_SCRIPTS_DATA_VERSION_1,
};

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Errors the script-engine load path can produce.
///
/// The original has one escape — `throw ERROR_BUG` for the unknown
/// kindof name — plus the walk refusals (`ERROR_CORRUPT_FILE_FORMAT`
/// when the container never opened, the 17th-list bound returning
/// false) and the process deaths its unchecked stores reach. The port
/// states each site as its own variant; every one of them stops the
/// walk the way the escaping exception does (the chunks stay open, the
/// statics keep nothing — the read-back never ran).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptsError {
    /// The walk itself failed — the bytes never opened as a `CkMp`
    /// table of contents (the standalone caller's "unable to read"
    /// log, the map caller's `ERROR_CORRUPT_FILE_FORMAT`).
    CorruptFileFormat,
    /// `Parameter::ReadParameter`'s unknown kindof name — the
    /// original's `throw ERROR_BUG`.
    KindOfUnknown,
    /// A condition or action whose loaded parameter count exceeds
    /// [`MAX_PARMS`]: the original's `m_parms[12]`-and-beyond store,
    /// past the array into the members and the heap. The parameter's
    /// bytes are consumed before the refusal (the original's
    /// read-then-store order).
    ParameterStoreOob,
    /// The version-1 area-condition heal's append at a loaded count of
    /// [`MAX_PARMS`] or negative: the original's
    /// `m_parms[m_numParms]` store outside the array.
    HealStoreOob,
    /// The `KIND_OF_PARAM` empty-string normalization whose int word
    /// points outside `kindofNames[]` — index 117 is the null
    /// terminator (an `AsciiString`-from-null death), beyond or below
    /// is past the array.
    KindOfIndexOob,
    /// The 17th `ScriptList` chunk: the `MAX_PLAYER_COUNT` bound
    /// refusal that walks the whole file out — the only false path in
    /// the parser tree.
    TooManyLists,
}

impl fmt::Display for ScriptsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptsError::CorruptFileFormat => {
                write!(f, "script stream is not a chunky data-chunk file")
            }
            ScriptsError::KindOfUnknown => {
                write!(f, "unknown kindof name (the original's ERROR_BUG throw)")
            }
            ScriptsError::ParameterStoreOob => {
                write!(f, "parameter count past m_parms[{}]", MAX_PARMS)
            }
            ScriptsError::HealStoreOob => {
                write!(f, "version-1 heal append past m_parms[{}]", MAX_PARMS)
            }
            ScriptsError::KindOfIndexOob => {
                write!(f, "kindof index out of range (the normalization read)")
            }
            ScriptsError::TooManyLists => {
                write!(f, "more than {} script lists", MAX_PLAYER_COUNT)
            }
        }
    }
}

impl std::error::Error for ScriptsError {}

// ---------------------------------------------------------------------------
// the payload (the scripts as the load leaves them)
// ---------------------------------------------------------------------------

/// One parameter of a condition or action — `Parameter`'s saved members
/// as the load leaves them.
///
/// The type word is the raw `i32` from the file (the
/// [`parameter_type`](self) constants name the values the original
/// defines; an out-of-range word stores raw — nothing in the load
/// rejects it). A `COORD3D` parameter leaves the int/real/string words
/// untouched (they are never read); every other type leaves the
/// coordinate zero. The `object_status` mask is the 45-bit
/// `ObjectStatusMaskType` the `OBJECT_STATUS` name-to-bit heal fills.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterData {
    /// The type word (`m_paramType`).
    pub param_type: i32,
    /// The int slot (`m_int` — for `KIND_OF_PARAM` the resolved table
    /// index, for `BOOLEAN`/`COMPARISON`/… the value).
    pub int_value: i32,
    /// The real slot (`m_real`).
    pub real_value: f32,
    /// The string slot (`m_string` — NUL-truncated by the read).
    pub string_value: Vec<u8>,
    /// The coordinate (`m_coord` — only a `COORD3D` parameter reads
    /// it).
    pub coord: Coord3d,
    /// The object-status mask (`m_objectStatus`, a `BitFlags<45>`): the
    /// `OBJECT_STATUS` heal's resolved bit (bit 0 is `"NONE"`), zero
    /// when the name matched nothing.
    pub object_status: u64,
}

impl ParameterData {
    /// `Parameter(type, val)` — the constructor the heals construct
    /// their padded parameters with.
    fn new(param_type: i32, val: i32) -> Self {
        ParameterData {
            param_type,
            int_value: val,
            real_value: 0.0,
            string_value: Vec::new(),
            coord: Coord3d {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            object_status: 0,
        }
    }
}

/// One and-condition — `Condition`'s saved members as the load leaves
/// them.
///
/// The parameter vector's length is the observable count (zero when
/// the count check rejected the condition); its `None`s are the null
/// slots the version-1 heal leaves behind (an area condition that
/// loaded fewer than three parms heals to three with null tails).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConditionData {
    /// The condition type (`m_conditionType` — the raw `i32`; the
    /// rematch may have replaced it, the count check may have forced
    /// `CONDITION_FALSE`).
    pub condition_type: i32,
    /// The parameters (`m_parms[0..m_numParms]`).
    pub params: Vec<Option<ParameterData>>,
}

/// One or-clause: its and-chain, in file order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrConditionData {
    /// The and-conditions (`m_firstAnd`'s chain).
    pub and_conditions: Vec<ConditionData>,
}

/// One action — `ScriptAction`'s saved members as the load leaves them
/// (the THEN or the ELSE arm of a script).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptActionData {
    /// The action type (`m_actionType` — the raw `i32`; the rematch
    /// may have replaced it, the count check may have forced `NO_OP`).
    pub action_type: i32,
    /// The parameters (`m_parms[0..m_numParms]`).
    pub params: Vec<Option<ParameterData>>,
}

/// One script — `Script`'s saved members as the load leaves them.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptData {
    /// The short name (`m_scriptName`).
    pub name: Vec<u8>,
    /// The long comment (`m_comment`).
    pub comment: Vec<u8>,
    /// The condition comment (`m_conditionComment`).
    pub condition_comment: Vec<u8>,
    /// The action comment (`m_actionComment`).
    pub action_comment: Vec<u8>,
    /// Whether the script is enabled (`m_isActive`).
    pub is_active: bool,
    /// Whether the script deactivates after firing once
    /// (`m_isOneShot`).
    pub is_one_shot: bool,
    /// Whether the easy campaign runs it (`m_easy`).
    pub easy: bool,
    /// Whether the normal campaign runs it (`m_normal`).
    pub normal: bool,
    /// Whether the hard campaign runs it (`m_hard`).
    pub hard: bool,
    /// Whether the script is a subroutine (`m_isSubroutine`).
    pub is_subroutine: bool,
    /// The evaluation delay in seconds (`m_delayEvaluationSeconds` —
    /// zero below script version 2).
    pub delay_evaluation_seconds: i32,
    /// The or-clauses (`m_condition`'s chain).
    pub or_conditions: Vec<OrConditionData>,
    /// The THEN actions (`m_action`'s chain).
    pub actions: Vec<ScriptActionData>,
    /// The ELSE actions (`m_actionFalse`'s chain).
    pub false_actions: Vec<ScriptActionData>,
}

impl Default for ScriptData {
    fn default() -> Self {
        // Script::Script()'s constructor defaults.
        ScriptData {
            name: Vec::new(),
            comment: Vec::new(),
            condition_comment: Vec::new(),
            action_comment: Vec::new(),
            is_active: true,
            is_one_shot: true,
            easy: true,
            normal: true,
            hard: true,
            is_subroutine: false,
            delay_evaluation_seconds: 0,
            or_conditions: Vec::new(),
            actions: Vec::new(),
            false_actions: Vec::new(),
        }
    }
}

/// One script group — `ScriptGroup`'s saved members (a version-1 or
/// version-3-plus group leaves `is_subroutine` at the constructor's
/// `false`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptGroupData {
    /// The group name (`m_groupName`).
    pub name: Vec<u8>,
    /// Whether the group is enabled (`m_isGroupActive` — the
    /// constructor's `true` survives when the version gate skips the
    /// byte).
    pub is_active: bool,
    /// Whether the group is a subroutine (`m_isGroupSubroutine` — only
    /// the exact version 2 reads it).
    pub is_subroutine: bool,
    /// The group's scripts (`m_firstScript`'s chain).
    pub scripts: Vec<ScriptData>,
}

impl Default for ScriptGroupData {
    fn default() -> Self {
        // ScriptGroup::ScriptGroup()'s constructor defaults (minus the
        // formatted default name — every loaded group's name comes
        // from the file).
        ScriptGroupData {
            name: Vec::new(),
            is_active: true,
            is_subroutine: false,
            scripts: Vec::new(),
        }
    }
}

/// One player's script list — the flat scripts and the groups, in file
/// order (`ScriptList`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptListData {
    /// The flat scripts (`m_firstScript`'s chain).
    pub scripts: Vec<ScriptData>,
    /// The groups (`m_firstGroup`'s chain).
    pub groups: Vec<ScriptGroupData>,
}

/// The `PlayerScriptsList` payload: the lists `getReadScripts` hands
/// out (at most [`MAX_PLAYER_COUNT`], in file order).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerScriptsData {
    /// The handed-out script lists.
    pub lists: Vec<ScriptListData>,
}

// ---------------------------------------------------------------------------
// the template registry (ScriptEngine::init)
// ---------------------------------------------------------------------------

include!("map_scripts_tables.rs");

/// The number of kindof names (`KINDOF_NAMES` minus its terminator
/// slot).
const KINDOF_NAME_COUNT: i32 = KINDOF_NAMES.len() as i32 - 1;

/// The seven conditions whose parameter list changed for condition
/// version 2 (`ParameterChangesVer2`, in the original's order): a
/// version-1 chunk gets the `SURFACES_ALLOWED` third parameter
/// appended and the count forced to 3.
pub const PARAMETER_CHANGES_VER2: [i32; 7] = [
    CT_TEAM_INSIDE_AREA_PARTIALLY,
    CT_TEAM_INSIDE_AREA_ENTIRELY,
    CT_TEAM_OUTSIDE_AREA_ENTIRELY,
    CT_TEAM_ENTERED_AREA_PARTIALLY,
    CT_TEAM_ENTERED_AREA_ENTIRELY,
    CT_TEAM_EXITED_AREA_ENTIRELY,
    CT_TEAM_EXITED_AREA_PARTIALLY,
];

/// The script engine's template registry and name-key interning as the
/// load path meets it — `ScriptEngine::init()`'s closing state.
///
/// `init()` assigns every condition's and action's internal name and
/// parameter count, then interns all the internal names
/// (`m_internalNameKey = NAMEKEY(m_internalName)`), conditions first,
/// in enum order — the defunct entries (the six conditions and one
/// action with no assignment) all intern the empty string once. The
/// rematch compares these keys; the count checks compare these counts.
///
/// The interning starts from the registry (the game's earlier startup
/// interning is not observable through this walk — see the module
/// docs).
#[derive(Debug, Clone)]
pub struct ScriptEngineTemplates {
    keys: NameKeys,
    cond_keys: Vec<NameKey>,
    act_keys: Vec<NameKey>,
}

impl ScriptEngineTemplates {
    /// The registry after `init()`'s closing loops.
    pub fn new() -> Self {
        let mut keys = NameKeys::new();
        let mut cond_keys = Vec::with_capacity(CONDITION_TEMPLATE_NAMES.len());
        for name in CONDITION_TEMPLATE_NAMES {
            cond_keys.push(keys.name_to_key(name.as_bytes()));
        }
        let mut act_keys = Vec::with_capacity(ACTION_TEMPLATE_NAMES.len());
        for name in ACTION_TEMPLATE_NAMES {
            act_keys.push(keys.name_to_key(name.as_bytes()));
        }
        ScriptEngineTemplates {
            keys,
            cond_keys,
            act_keys,
        }
    }

    /// The name-key table (`TheNameKeyGenerator`) — the file's name
    /// keys intern into it as the walk reads them.
    pub fn keys(&self) -> &NameKeys {
        &self.keys
    }

    /// The mutable view `readNameKey` drives.
    pub(crate) fn keys_mut(&mut self) -> &mut NameKeys {
        &mut self.keys
    }

    /// `getConditionTemplate`'s out-of-range clamp: an index outside
    /// the enum reads template 0.
    fn cond_index(ndx: i32) -> usize {
        if !(0..CONDITION_TEMPLATE_NAMES.len() as i32).contains(&ndx) {
            return 0;
        }
        ndx as usize
    }

    /// `getActionTemplate`'s clamp.
    fn act_index(ndx: i32) -> usize {
        if !(0..ACTION_TEMPLATE_NAMES.len() as i32).contains(&ndx) {
            return 0;
        }
        ndx as usize
    }

    /// A condition template's interned internal name
    /// (`m_internalNameKey`, the clamped index).
    pub fn condition_template_key(&self, ndx: i32) -> NameKey {
        self.cond_keys[Self::cond_index(ndx)]
    }

    /// A condition template's declared parameter count.
    pub fn condition_template_param_count(&self, ndx: i32) -> i32 {
        CONDITION_TEMPLATE_PARAMS[Self::cond_index(ndx)] as i32
    }

    /// An action template's interned internal name.
    pub fn action_template_key(&self, ndx: i32) -> NameKey {
        self.act_keys[Self::act_index(ndx)]
    }

    /// An action template's declared parameter count.
    pub fn action_template_param_count(&self, ndx: i32) -> i32 {
        ACTION_TEMPLATE_PARAMS[Self::act_index(ndx)] as i32
    }
}

impl Default for ScriptEngineTemplates {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// helpers (the C++ string layer)
// ---------------------------------------------------------------------------

/// `AsciiString::compareNoCase` — an ASCII case-insensitive compare
/// (`_stricmp`).
fn eq_no_case(a: &[u8], b: &[u8]) -> bool {
    fn lower(c: u8) -> u8 {
        if c.is_ascii_uppercase() {
            c + (b'a' - b'A')
        } else {
            c
        }
    }
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).all(|(x, y)| lower(*x) == lower(*y))
}

/// The kindof names slice without the terminator slot.
fn kindof_names() -> &'static [&'static str] {
    &KINDOF_NAMES[..KINDOF_NAMES.len() - 1]
}

// ---------------------------------------------------------------------------
// Parameter::ReadParameter
// ---------------------------------------------------------------------------

/// `Parameter::ReadParameter` — the type word, the COORD3D or
/// int/real/string shape, and the four rewrites.
fn read_parameter(file: &mut DataChunkInput<'_>) -> Result<ParameterData, ScriptsError> {
    let mut p = ParameterData::new(file.read_int(), 0);

    if p.param_type == PT_COORD3D {
        let x = file.read_real();
        let y = file.read_real();
        let z = file.read_real();
        // setCoord3D only assigns when the type matches (it does here).
        p.coord = Coord3d { x, y, z };
    } else {
        p.int_value = file.read_int();
        p.real_value = file.read_real();
        p.string_value = file.read_ascii_string();
    }

    if p.param_type == PT_OBJECT_TYPE {
        // quick hack to make loading models with "Fundamentalist"
        // switch to "GLA"
        if p.string_value.starts_with(b"Fundamentalist") {
            // strcpy(newName, "GLA"); strlcat(newName, suffix, 256) —
            // the strlcat truncates at 255 bytes total.
            let mut new_name = b"GLA".to_vec();
            new_name.extend_from_slice(&p.string_value[b"Fundamentalist".len()..]);
            new_name.truncate(255);
            p.string_value = new_name;
        }
    }

    if p.param_type == PT_UPGRADE {
        // quick hack to make obsolete capture building upgrades switch
        // to the new one. jba.
        if p.string_value == b"Upgrade_AmericaRangerCaptureBuilding"
            || p.string_value == b"Upgrade_ChinaRedguardCaptureBuilding"
            || p.string_value == b"Upgrade_GLARebelCaptureBuilding"
        {
            p.string_value = b"Upgrade_InfantryCaptureBuilding".to_vec();
        }
    }

    if p.param_type == PT_OBJECT_STATUS {
        // Need to change the string to an ObjectStatusMaskType
        for (i, name) in OBJECT_STATUS_NAMES.iter().enumerate() {
            if eq_no_case(&p.string_value, name.as_bytes()) {
                p.object_status |= 1u64 << i; // MAKE_OBJECT_STATUS_MASK(i), set() ORs
                break;
            }
        }
    }

    if p.param_type == PT_KIND_OF_PARAM {
        // Need to change the string to an integer
        if !p.string_value.is_empty() {
            let names = kindof_names();
            let mut found = false;
            let mut i: usize = 0;
            while i < names.len() {
                if eq_no_case(&p.string_value, names[i].as_bytes()) {
                    p.int_value = i as i32;
                    found = true;
                    break;
                }
                if eq_no_case(&p.string_value, b"CRUSHER")
                    || eq_no_case(&p.string_value, b"CRUSHABLE")
                    || eq_no_case(&p.string_value, b"OVERLAPPABLE")
                {
                    // the legacy remaps ("switched to OBSTACLE" — the
                    // kinds no longer exist): the CURRENT loop index,
                    // which is 0 (the first else-if pass), OBSTACLE.
                    p.int_value = i as i32;
                    found = true;
                    break;
                } else if eq_no_case(&p.string_value, b"MISSILE") {
                    // MISSILE was split into SMALL_MISSILE and
                    // BALLISTIC_MISSILE. The re-compare below tests the
                    // (already rewritten) string against the literal,
                    // so it is always true at index 0; the outer loop
                    // then walks on (the branch has no break) and the
                    // real SMALL_MISSILE entry overwrites the int with
                    // its index. The inner loop reuses the outer i.
                    p.string_value = b"SMALL_MISSILE".to_vec();
                    i = 0;
                    while i < names.len() {
                        if eq_no_case(&p.string_value, b"SMALL_MISSILE") {
                            p.int_value = i as i32;
                            found = true;
                            break;
                        }
                        i += 1;
                    }
                    // (DEBUG_CRASH("Unable to find Kindof
                    // SMALL_MISSILE'") — debug-only; the release walk
                    // continues)
                }
                i += 1;
            }
            if !found {
                // throw ERROR_BUG — the walk aborts.
                return Err(ScriptsError::KindOfUnknown);
            }
        } else {
            // "so WB will load them into the proper format":
            // m_string = kindofNames[m_int] — the out-of-range index
            // reads past the array (the 117 slot is the null
            // terminator, an AsciiString-from-null crash; beyond and
            // below are raw OOB).
            if p.int_value < 0 || p.int_value >= KINDOF_NAME_COUNT {
                return Err(ScriptsError::KindOfIndexOob);
            }
            p.string_value = kindof_names()[p.int_value as usize].as_bytes().to_vec();
        }
    }

    Ok(p)
}

// ---------------------------------------------------------------------------
// Condition::ParseConditionDataChunk
// ---------------------------------------------------------------------------

/// The condition reader: the type word, the version-4 name-key
/// rematch, the parameter loop, the two heals and the count check.
fn parse_condition_into(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    engine: &mut ScriptEngineTemplates,
) -> Result<ConditionData, ScriptsError> {
    let mut condition = ConditionData {
        condition_type: file.read_int(),
        params: Vec::new(),
    };

    // getConditionTemplate — the out-of-range clamp to template 0. The
    // rematch below may re-point these at the found template; a failed
    // rematch leaves them at the LAST template the rescan iterated
    // (NUM_ITEMS-1) with the type already CONDITION_FALSE — the count
    // check's reassignment is idempotent for every count, so modelling
    // the last template directly is exact.
    let mut ct_params = engine.condition_template_param_count(condition.condition_type);
    let mut ct_key = engine.condition_template_key(condition.condition_type);

    if info.version >= K_SCRIPT_CONDITION_VERSION_4 {
        let key = file.read_name_key(engine.keys_mut());
        let mut matched = ct_key == key; // All good. jba.
        if !matched {
            // name and id don't match. Find the name [3/20/2003]
            for i in 0..CONDITION_TEMPLATE_NAMES.len() as i32 {
                let k = engine.condition_template_key(i); // in range here
                if key == k {
                    condition.condition_type = i;
                    ct_key = k;
                    ct_params = engine.condition_template_param_count(i);
                    matched = true;
                    break;
                }
            }
        }
        if !matched {
            // Invalid script [3/20/2003]
            condition.condition_type = CT_CONDITION_FALSE;
            // (m_numParms = 0 — the read below overwrites it anyway)
            let last = CONDITION_TEMPLATE_NAMES.len() as i32 - 1;
            ct_key = engine.condition_template_key(last);
            ct_params = engine.condition_template_param_count(last);
        }
    }

    let mut num_parms = file.read_int();
    for i in 0..num_parms {
        // read-then-store: the parameter's bytes are consumed before
        // the store refuses.
        let p = read_parameter(file)?;
        if i >= MAX_PARMS {
            return Err(ScriptsError::ParameterStoreOob);
        }
        condition.params.push(Some(p));
    }

    // The version-1 heal: the seven area conditions get the
    // SURFACES_ALLOWED parameter appended AT THE LOADED COUNT and the
    // count forced to 3 (the append is invisible above a count of 2 —
    // the forced count hides it; the original writes it at
    // m_parms[numParms] regardless).
    if file.get_chunk_version() < K_SCRIPT_CONDITION_VERSION_2 {
        for changed in PARAMETER_CHANGES_VER2 {
            if condition.condition_type == changed {
                let slot = num_parms;
                if !(0..MAX_PARMS).contains(&slot) {
                    return Err(ScriptsError::HealStoreOob);
                }
                if slot as usize == condition.params.len() {
                    condition
                        .params
                        .push(Some(ParameterData::new(PT_SURFACES_ALLOWED, 3)));
                } else {
                    // slot < params.len() cannot happen (the loop
                    // filled exactly num_parms slots); the arm exists
                    // for the parity of the shape.
                    condition
                        .params
                        .push(Some(ParameterData::new(PT_SURFACES_ALLOWED, 3)));
                }
                num_parms = 3;
                condition.params.resize(3, None);
            }
        }
    }

    // heal old files: the one-parm special-power check gains the SIDE
    // "<This Player>" in front.
    if condition.condition_type == CT_SKIRMISH_SPECIAL_POWER_READY && num_parms == 1 {
        num_parms = 2;
        let old = condition.params[0].take();
        condition.params.push(old); // m_parms[1] = m_parms[0]
        let mut side = ParameterData::new(PT_SIDE, 0);
        side.string_value = THIS_PLAYER.to_vec(); // friend_setString
        condition.params[0] = Some(side);
    }

    // the template count check
    if ct_params != num_parms {
        // Invalid script [3/20/2003]
        condition.condition_type = CT_CONDITION_FALSE;
        condition.params.clear();
    }
    let _ = ct_key; // (the rematch's observable is the type/count pair)
    Ok(condition)
}

// ---------------------------------------------------------------------------
// ScriptAction::ParseAction
// ---------------------------------------------------------------------------

/// The action reader: the type word, the version-2 name-key rematch,
/// the parameter loop, the heal switch and the count check.
fn parse_action_into(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    engine: &mut ScriptEngineTemplates,
) -> Result<ScriptActionData, ScriptsError> {
    let mut action = ScriptActionData {
        action_type: file.read_int(),
        params: Vec::new(),
    };

    let mut at_params = engine.action_template_param_count(action.action_type);
    let mut at_key = engine.action_template_key(action.action_type);

    if info.version >= K_SCRIPT_ACTION_VERSION_2 {
        let key = file.read_name_key(engine.keys_mut());
        let mut matched = at_key == key; // All good. jba.
        if !matched {
            for i in 0..ACTION_TEMPLATE_NAMES.len() as i32 {
                let k = engine.action_template_key(i);
                if key == k {
                    action.action_type = i;
                    at_key = k;
                    at_params = engine.action_template_param_count(i);
                    matched = true;
                    break;
                }
            }
            if !matched {
                // Invalid script [3/20/2003]
                action.action_type = AT_NO_OP;
                // (m_numParms = 0 — the read below overwrites it)
                let last = ACTION_TEMPLATE_NAMES.len() as i32 - 1;
                at_key = engine.action_template_key(last);
                at_params = engine.action_template_param_count(last);
            }
        }
    }

    let mut num_parms = file.read_int();
    for i in 0..num_parms {
        let p = read_parameter(file)?;
        if i >= MAX_PARMS {
            return Err(ScriptsError::ParameterStoreOob);
        }
        action.params.push(Some(p));
    }

    // heal old files.
    match action.action_type {
        AT_SKIRMISH_FIRE_SPECIAL_POWER_AT_MOST_COST => {
            if num_parms == 1 {
                num_parms = 2;
                let old = action.params[0].take();
                action.params.push(old);
                let mut side = ParameterData::new(PT_SIDE, 0);
                side.string_value = THIS_PLAYER.to_vec();
                action.params[0] = Some(side);
            }
        }
        AT_TEAM_FOLLOW_WAYPOINTS => {
            if num_parms == 2 {
                num_parms = 3;
                action.params.push(Some(ParameterData::new(PT_BOOLEAN, 1)));
            }
        }
        AT_SKIRMISH_BUILD_BASE_DEFENSE_FRONT => {
            if num_parms == 1 {
                let flank = action.params[0].as_ref().is_some_and(|p| p.int_value != 0);
                action.params.clear(); // deleteInstance(parms[0])
                num_parms = 0;
                if flank {
                    action.action_type = AT_SKIRMISH_BUILD_BASE_DEFENSE_FLANK;
                }
            }
        }
        AT_NAMED_SET_ATTITUDE | AT_TEAM_SET_ATTITUDE => {
            if num_parms >= 2
                && action.params[1]
                    .as_ref()
                    .is_some_and(|p| p.param_type == PT_INT)
            {
                let mood = action.params[1].as_ref().expect("checked").int_value;
                action.params[1] = Some(ParameterData::new(PT_AI_MOOD, mood));
            }
        }
        AT_MAP_REVEAL_AT_WAYPOINT | AT_MAP_SHROUD_AT_WAYPOINT => {
            if num_parms == 2 {
                num_parms = 3;
                action.params.push(Some(ParameterData::new(PT_SIDE, 0)));
            }
        }
        AT_MAP_REVEAL_ALL
        | AT_MAP_REVEAL_ALL_PERM
        | AT_MAP_REVEAL_ALL_UNDO_PERM
        | AT_MAP_SHROUD_ALL => {
            if num_parms == 0 {
                num_parms = 1;
                action.params.push(Some(ParameterData::new(PT_SIDE, 0)));
            }
        }
        AT_SPEECH_PLAY => {
            if num_parms == 1 {
                num_parms = 2;
                // Default it to TRUE, as per conversation with JohnL
                action.params.push(Some(ParameterData::new(PT_BOOLEAN, 1)));
            }
        }
        AT_CAMERA_MOD_SET_FINAL_ZOOM | AT_CAMERA_MOD_SET_FINAL_PITCH => {
            if num_parms == 1 {
                num_parms = 3;
                action.params.push(Some(ParameterData::new(PT_PERCENT, 0)));
                action.params.push(Some(ParameterData::new(PT_PERCENT, 0)));
            }
        }
        AT_MOVE_CAMERA_TO | AT_MOVE_CAMERA_ALONG_WAYPOINT_PATH | AT_CAMERA_LOOK_TOWARD_OBJECT => {
            if num_parms == 3 {
                num_parms = 5;
                action.params.push(Some(ParameterData::new(PT_REAL, 0)));
                action.params.push(Some(ParameterData::new(PT_REAL, 0)));
            }
        }
        AT_RESET_CAMERA | AT_ZOOM_CAMERA | AT_PITCH_CAMERA | AT_ROTATE_CAMERA => {
            if num_parms == 2 {
                num_parms = 4;
                action.params.push(Some(ParameterData::new(PT_REAL, 0)));
                action.params.push(Some(ParameterData::new(PT_REAL, 0)));
            }
        }
        AT_CAMERA_LOOK_TOWARD_WAYPOINT => {
            if num_parms == 2 {
                num_parms = 5;
                action.params.push(Some(ParameterData::new(PT_REAL, 0)));
                action.params.push(Some(ParameterData::new(PT_REAL, 0)));
                action.params.push(Some(ParameterData::new(PT_BOOLEAN, 0)));
            } else if num_parms == 4 {
                num_parms = 5;
                action.params.push(Some(ParameterData::new(PT_BOOLEAN, 0)));
            }
        }
        _ => {}
    }

    // the template count check
    if at_params != num_parms {
        // Invalid script [3/20/2003]
        action.action_type = AT_NO_OP;
        action.params.clear();
    }
    let _ = at_key;
    Ok(action)
}

// ---------------------------------------------------------------------------
// the walk (the parser tree over the container)
// ---------------------------------------------------------------------------

/// The node the nested `parse` calls thread — the original passes each
/// node's pointer as the parse `userData`; the port keeps them on a
/// stack inside [`ScriptsWalk`], each parser pushing, parsing, popping
/// and attaching.
#[derive(Debug, Clone)]
enum WalkNode {
    /// `TScriptListReadInfo` under a `PlayerScriptsList` walk.
    ReadLists(Vec<ScriptListData>),
    /// A `ScriptList` under construction.
    List(ScriptListData),
    /// A `ScriptGroup` under construction.
    Group(ScriptGroupData),
    /// A `Script` under construction.
    Script(ScriptData),
    /// An `OrCondition`'s and-chain under construction.
    Or(Vec<ConditionData>),
}

/// The script walk's state: the engine's template registry with the
/// name-key interning, the `ScriptList` statics the reader fills, the
/// fired flag and the error capture.
///
/// The manual-walk surface (the container's parser table drives it):
/// register [`scripts_chunk_parser`] under
/// [`PLAYER_SCRIPTS_LIST_LABEL`] — [`PlayerScriptsData::from_map`]
/// and [`PlayerScriptsData::from_map_in_sides`] wrap exactly this.
#[derive(Debug, Clone)]
pub struct ScriptsWalk {
    engine: ScriptEngineTemplates,
    fired: bool,
    error: Option<ScriptsError>,
    nodes: Vec<WalkNode>,
    // ScriptList's statics: s_readLists / s_numInReadList
    read_lists: Vec<ScriptListData>,
}

impl ScriptsWalk {
    /// The walk state over a freshly-initialised template registry.
    pub fn new() -> Self {
        ScriptsWalk {
            engine: ScriptEngineTemplates::new(),
            fired: false,
            error: None,
            nodes: Vec::new(),
            read_lists: Vec::new(),
        }
    }

    /// Whether a `PlayerScriptsList` chunk fired at all.
    pub fn fired(&self) -> bool {
        self.fired
    }

    /// The engine's template registry.
    pub fn engine(&self) -> &ScriptEngineTemplates {
        &self.engine
    }

    /// The final name-key table (the registry's interning plus the
    /// file's name keys).
    pub fn keys(&self) -> &NameKeys {
        self.engine.keys()
    }

    /// The captured error, when the walk refused.
    pub fn error(&self) -> Option<ScriptsError> {
        self.error
    }

    /// `ScriptList::getReadScripts` — the hand-out that empties the
    /// statics.
    pub fn take_read_scripts(&mut self) -> Vec<ScriptListData> {
        std::mem::take(&mut self.read_lists)
    }
}

impl Default for ScriptsWalk {
    fn default() -> Self {
        Self::new()
    }
}

/// `Script::ParseScript` — the head words and the nested
/// OrCondition/ScriptAction/ScriptActionFalse walk (registered under
/// the literal `"Script"` parent, exactly as the original).
fn parse_script_body(
    file: &mut DataChunkInput<'_>,
    version: u16,
    table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    user.nodes.push(WalkNode::Script(ScriptData::default()));

    if let Some(WalkNode::Script(script)) = user.nodes.last_mut() {
        script.name = file.read_ascii_string();
        script.comment = file.read_ascii_string();
        script.condition_comment = file.read_ascii_string();
        script.action_comment = file.read_ascii_string();
        script.is_active = file.read_byte() != 0;
        script.is_one_shot = file.read_byte() != 0;
        script.easy = file.read_byte() != 0;
        script.normal = file.read_byte() != 0;
        script.hard = file.read_byte() != 0;
        script.is_subroutine = file.read_byte() != 0;
        if version >= K_SCRIPT_DATA_VERSION_2 {
            script.delay_evaluation_seconds = file.read_int();
        }
    }

    table.register(b"OrCondition", b"Script", or_condition_parser);
    table.register(b"ScriptAction", b"Script", action_parser);
    table.register(b"ScriptActionFalse", b"Script", action_false_parser);

    // (file.parse(pScript) — no parser in this sub-tree returns false;
    // the null return's addScript deref is the documented-unreachable
    // crash contract, see the module docs)
    let ok = file.parse(table, user);

    if let Some(WalkNode::Script(script)) = user.nodes.pop() {
        // addScript(pScript, AT_END) — the tail append into whichever
        // container is under construction.
        match user.nodes.last_mut() {
            Some(WalkNode::List(list)) => list.scripts.push(script),
            Some(WalkNode::Group(group)) => group.scripts.push(script),
            _ => {} // (unreachable: the registration scopes the chunk)
        }
    }
    ok
}

/// `Script::ParseScriptFromListDataChunk` — the wrapper always returns
/// true (the original's null-script crash path is unreachable; see the
/// module docs). An error in flight is the original's escaping
/// exception, which unwinds *past* this wrapper — the false propagates
/// instead of the true.
fn script_from_list_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    parse_script_body(file, info.version, table, user);
    user.error.is_none()
}

/// `Script::ParseScriptFromGroupDataChunk`.
fn script_from_group_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    parse_script_body(file, info.version, table, user);
    user.error.is_none()
}

/// `ScriptGroup::ParseGroupDataChunk`.
fn group_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    let mut group = ScriptGroupData {
        name: file.read_ascii_string(),
        is_active: file.read_byte() != 0,
        is_subroutine: false,
        scripts: Vec::new(),
    };
    if info.version == K_SCRIPT_GROUP_DATA_VERSION_2 {
        // the exact equality: a version 3 group reads like a version 1
        group.is_subroutine = file.read_byte() != 0;
    }
    user.nodes.push(WalkNode::Group(group));

    // addGroup(pGroup, AT_END)
    // (the group attaches to its parent list when the walk below
    // returns — the node pops into it)
    table.register(b"Script", info.label.as_slice(), script_from_group_parser);
    let ok = file.parse(table, user);

    if let (Some(WalkNode::Group(group)), Some(WalkNode::List(list))) =
        (user.nodes.pop(), user.nodes.last_mut())
    {
        list.groups.push(group);
    }
    ok
}

/// `OrCondition::ParseOrConditionDataChunk`.
fn or_condition_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    // the tail-append into the script's or-chain
    if let Some(WalkNode::Script(script)) = user.nodes.last_mut() {
        script.or_conditions.push(OrConditionData::default());
    }
    user.nodes.push(WalkNode::Or(Vec::new()));
    table.register(b"Condition", info.label.as_slice(), condition_parser);
    let ok = file.parse(table, user);
    if let (Some(WalkNode::Or(ands)), Some(WalkNode::Script(script))) =
        (user.nodes.pop(), user.nodes.last_mut())
    {
        script
            .or_conditions
            .last_mut()
            .expect("pushed above")
            .and_conditions = ands;
    }
    ok
}

/// `Condition::ParseConditionDataChunk` — always true (the reader's
/// failures abort the walk as errors instead).
fn condition_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    match parse_condition_into(file, info, &mut user.engine) {
        Ok(condition) => {
            if let Some(WalkNode::Or(ands)) = user.nodes.last_mut() {
                ands.push(condition); // the tail-append into the and-chain
            }
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

/// `ScriptAction::ParseActionDataChunk` — the THEN list's wrapper.
fn action_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    match parse_action_into(file, info, &mut user.engine) {
        Ok(action) => {
            if let Some(WalkNode::Script(script)) = user.nodes.last_mut() {
                script.actions.push(action);
            }
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

/// `ScriptAction::ParseActionFalseDataChunk` — the ELSE list's.
fn action_false_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    _table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    match parse_action_into(file, info, &mut user.engine) {
        Ok(action) => {
            if let Some(WalkNode::Script(script)) = user.nodes.last_mut() {
                script.false_actions.push(action);
            }
            true
        }
        Err(e) => {
            user.error = Some(e);
            false
        }
    }
}

/// `ScriptList::ParseScriptListDataChunk`.
fn script_list_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    // the bound refusal — the only false path in the parser tree
    let read_lists_len = match user.nodes.last() {
        Some(WalkNode::ReadLists(lists)) => lists.len(),
        _ => return false, // (unreachable: the registration scopes it)
    };
    if read_lists_len >= MAX_PLAYER_COUNT {
        return false;
    }

    user.nodes.push(WalkNode::List(ScriptListData::default()));
    table.register(b"Script", info.label.as_slice(), script_from_list_parser);
    table.register(b"ScriptGroup", info.label.as_slice(), group_parser);
    let ok = file.parse(table, user);

    if let (Some(WalkNode::List(list)), Some(WalkNode::ReadLists(lists))) =
        (user.nodes.pop(), user.nodes.last_mut())
    {
        lists.push(list);
    }
    ok
}

/// `ScriptList::ParseScriptsDataChunk` — the `PlayerScriptsList`
/// reader as a container parser: the statics' leftover clear (a second
/// chunk in the same file deletes the first walk's lists), the
/// `ScriptList` registration under this chunk's label, and the
/// statics' fill on success.
///
/// The registration scope: under the map's `SidesList` chunk (the
/// nested walk, [`PlayerScriptsData::from_map_in_sides`]) or at the
/// file root (the standalone script files and the GameLogic
/// registration, [`PlayerScriptsData::from_map`]).
pub fn scripts_chunk_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    user.fired = true;
    // the statics' clear ("Leftover scripts floating around")
    user.read_lists.clear();

    user.nodes.push(WalkNode::ReadLists(Vec::new()));
    table.register(b"ScriptList", info.label.as_slice(), script_list_parser);
    let ok = file.parse(table, user);

    if let (Some(WalkNode::ReadLists(lists)), true) = (user.nodes.pop(), ok) {
        // (the DEBUG_ASSERTCRASH(numLists<MAX_PLAYER_COUNT) fires at
        // exactly 16 — debug-only; the release copy is in range by the
        // bound above)
        user.read_lists = lists;
    }
    ok
}

/// The map-context stub: `SidesList::ParseSidesDataChunk`'s
/// registration tail — the sides payload before it is the
/// [`sides-list slice`](crate::map_sideslist)'s (transcribed there),
/// and the scripts read-back into the sides (the `deleteInstance` /
/// `setScriptList` pair) is dropped with it:
/// [`ScriptsWalk::take_read_scripts`] is the same hand-out the
/// read-back consumes.
fn sides_tail_parser(
    file: &mut DataChunkInput<'_>,
    info: &DataChunkInfo,
    table: &mut ParserTable<'_, ScriptsWalk>,
    user: &mut ScriptsWalk,
) -> bool {
    table.register(
        PLAYER_SCRIPTS_LIST_LABEL,
        info.label.as_slice(),
        scripts_chunk_parser,
    );
    file.parse(table, user)
}

// ---------------------------------------------------------------------------
// the module's own walks
// ---------------------------------------------------------------------------

impl PlayerScriptsData {
    /// The standalone walk — `GameLogic`'s and the WorldBuilder's
    /// registration of the reader at the file root (the `.scb` script
    /// files' shape).
    ///
    /// `Ok(None)` is the file with no `PlayerScriptsList` chunk at the
    /// root (a nested chunk is out of this walk's scope — see
    /// [`from_map_in_sides`](Self::from_map_in_sides)).
    ///
    /// The walk refusals: [`ScriptsError::CorruptFileFormat`] when the
    /// bytes never opened as a chunk stream, [`ScriptsError::TooManyLists`]
    /// for the 17th script list, and the verdict family the parameter
    /// loads reach.
    pub fn from_map(bytes: &[u8]) -> Result<Option<Self>, ScriptsError> {
        Self::walk(bytes, false)
    }

    /// The map's walk — the `SidesList` tail registration (the
    /// `PlayerScriptsList` chunk nested inside the map's sides chunk;
    /// the sides payload around it is the sides-list slice's).
    pub fn from_map_in_sides(bytes: &[u8]) -> Result<Option<Self>, ScriptsError> {
        Self::walk(bytes, true)
    }

    fn walk(bytes: &[u8], nested: bool) -> Result<Option<Self>, ScriptsError> {
        let mut input = DataChunkInput::new(bytes);
        if !input.is_valid_file_type() {
            return Err(ScriptsError::CorruptFileFormat);
        }
        let mut table: ParserTable<'_, ScriptsWalk> = ParserTable::new();
        if nested {
            table.register(SIDES_LIST_LABEL, b"", sides_tail_parser);
        } else {
            table.register(PLAYER_SCRIPTS_LIST_LABEL, b"", scripts_chunk_parser);
        }
        let mut user = ScriptsWalk::new();
        if !input.parse(&mut table, &mut user) {
            return Err(user.error.unwrap_or(ScriptsError::TooManyLists));
        }
        if !user.fired() {
            return Ok(None);
        }
        Ok(Some(PlayerScriptsData {
            lists: user.take_read_scripts(),
        }))
    }
}

// ---------------------------------------------------------------------------
// unit tests (the port's own contracts; the golden vectors pin the walk)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry's shape: every defunct entry is the empty name,
    /// every named entry has a stable count, and the interning
    /// sequence is conditions-then-actions.
    #[test]
    fn the_registry_transcribes() {
        let engine = ScriptEngineTemplates::new();
        // the defunct conditions (no init assignment)
        for i in [
            CT_OBSOLETE_SCRIPT_1,
            CT_OBSOLETE_SCRIPT_2,
            CT_DEFUNCT_PLAYER_SELECTED_GENERAL,
            CT_DEFUNCT_PLAYER_SELECTED_GENERAL_FROM_NAMED,
            CT_UNIT_COMPLETED_SEQUENTIAL_EXECUTION,
            CT_TEAM_COMPLETED_SEQUENTIAL_EXECUTION,
        ] {
            assert_eq!(CONDITION_TEMPLATE_NAMES[i as usize], "");
            assert_eq!(engine.condition_template_param_count(i), 0);
        }
        assert_eq!(CONDITION_TEMPLATE_NAMES.len(), CT_NUM_ITEMS as usize);
        assert_eq!(ACTION_TEMPLATE_NAMES.len(), AT_NUM_ITEMS as usize);
        // the defunct action
        assert_eq!(ACTION_TEMPLATE_NAMES[AT_NAMED_RECEIVE_UPGRADE as usize], "");
        // the clamps
        assert_eq!(
            engine.condition_template_key(-5),
            engine.condition_template_key(0)
        );
        assert_eq!(
            engine.condition_template_key(9999),
            engine.condition_template_key(0)
        );
        assert_eq!(
            engine.action_template_key(-1),
            engine.action_template_key(0)
        );
        // template 0's counts (the raw-type survivors' partner)
        assert_eq!(engine.condition_template_param_count(0), 0); // CONDITION_FALSE
        assert_eq!(engine.action_template_param_count(0), 1); // DEBUG_MESSAGE_BOX
        // the interning: 447 names, the empty string once
        let names: Vec<_> = engine.keys().names().collect();
        assert_eq!(
            names.len(),
            CONDITION_TEMPLATE_NAMES.len() + ACTION_TEMPLATE_NAMES.len() - 7 + 1
        );
        assert_eq!(names.iter().filter(|(_, n)| n.is_empty()).count(), 1);
    }

    /// The kindof tables: the terminator slot, the legacy remaps and
    /// the case-insensitive compare.
    #[test]
    fn the_kindof_table_transcribes() {
        assert_eq!(kindof_names().len(), 117);
        assert_eq!(KINDOF_NAMES[117], ""); // the terminator slot
        assert_eq!(kindof_names()[0], "OBSTACLE");
        assert_eq!(
            kindof_names().iter().position(|n| *n == "SMALL_MISSILE"),
            Some(47)
        );
        assert!(eq_no_case(b"OBSTACLE", b"obstacle"));
        assert!(eq_no_case(b"Fs_Power", b"FS_POWER"));
        assert!(!eq_no_case(b"MISSILE", b"SMALL_MISSILE"));
        // the object-status table: 45 names, NONE first, DEPLOYED last
        assert_eq!(OBJECT_STATUS_NAMES.len(), 45);
        assert_eq!(OBJECT_STATUS_NAMES[0], "NONE");
        assert_eq!(OBJECT_STATUS_NAMES[44], "DEPLOYED");
    }

    /// The error display strings are stable (the golden's perr tokens
    /// map from the variants, not the text).
    #[test]
    fn error_display_is_stable() {
        assert_eq!(
            ScriptsError::KindOfUnknown.to_string(),
            "unknown kindof name (the original's ERROR_BUG throw)"
        );
        assert_eq!(
            ScriptsError::ParameterStoreOob.to_string(),
            "parameter count past m_parms[12]"
        );
    }

    /// The walk's data defaults match the constructors.
    #[test]
    fn the_constructor_defaults_transcribe() {
        let script = ScriptData::default();
        assert!(
            script.is_active && script.is_one_shot && script.easy && script.normal && script.hard
        );
        assert!(!script.is_subroutine);
        assert_eq!(script.delay_evaluation_seconds, 0);

        let group = ScriptGroupData::default();
        assert!(group.is_active);
        assert!(!group.is_subroutine);
    }
}
