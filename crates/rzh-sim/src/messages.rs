//! The GameMessage command processing — WP1's last block: the message
//! stream, the command list and the frame's command walk.
//!
//! Origin: `Core/GameEngine/Include/Common/MessageStream.h` +
//! `Core/GameEngine/Source/Common/MessageStream.cpp` — `GameMessage` (the
//! type vocabulary, the argument union with its eleven typed appenders,
//! the issuing-player stamp, the list links),
//! `GameMessageList` (append / insert-after / remove /
//! containsMessageOfType), `MessageStream` (the priority-sorted
//! translator registry of `attachTranslator` with its stable
//! insert-before-the-first-strictly-greater placement, `findTranslator`,
//! `removeTranslator`, `isReadyForMessages`, and `propagateMessages` —
//! the translator-outer/message-inner walk with the mid-walk
//! DESTROY_MESSAGE unlink, the survivor transfer to TheCommandList, the
//! redundant-network-message sweep over the transferred list and the
//! stream clear), `CommandList` (`appendMessageList`, `reset`'s
//! destroyAllMessages, the direct `appendMessage` the recorder's
//! simulation-playback path uses) and `buildRegion`;
//! `GeneralsMD/Code/GameEngine/Source/GameLogic/System/GameLogic.cpp`'s
//! `processCommandList` (the cached-CRC clear, the per-message dispatch
//! walk, and the CRC-validation tail: the connected-player count over
//! MAX_SLOTS, the not-enough-CRCs verdict, the reference-CRC comparison
//! that skips disconnected slots and the setSawCRCMismatch call) plus
//! the `update` wiring (the commands position and the frame-tail
//! `TheCommandList->reset()`);
//! `Core/GameEngine/Source/GameLogic/System/GameLogicDispatch.cpp`'s
//! `logicMessageDispatcher` prologue (the `getNthPlayer` gate) and the
//! two CRC handlers — `onLogicCrc` (the network branch: the human gate,
//! the slot gate with the connected refusal, the local-player
//! `m_shouldValidateCRCs` raise and the per-player cache write; the
//! playback branch: the recorder hand-off) and `onLogicCrcForPlayback`.
//!
//! The message vocabulary is transcribed for the Zero Hour RELEASE
//! configuration (the repo's pinned build: `RTS_DEBUG`,
//! `_ALLOW_DEBUG_CHEATS_IN_RELEASE`, `ALLOW_SURRENDER`,
//! `DUMP_PERF_STATS` and `RTS_GENERALS` all off) — 288 entries, the
//! network block keeping its fixed 1000..=1999 ordinals ("we start this
//! at a fixed value, so that (regardless of ifdefs upstream from us) we
//! have the same"). The `RTS_DEBUG`-only `isInvalidDebugCommand` filter
//! of `propagateMessages` is therefore absent, as in the release build.
//!
//! The dispatch bodies (the giant `logicMessageDispatcher` switch —
//! move/attack/contain/production/...) are the module framework's and
//! later slices'; they plug in through [`crate::logic::KernelHost`]'s
//! `run_command`, exactly as the update-module bodies do through
//! `run_update`. This slice owns the walk, the prologue gate, the two
//! CRC handlers and every list choreography around them.
//!
//! Stated contracts (not replayed — see the transcription's header):
//!
//! - A translator may not attach or detach translators from inside its
//!   own `translateGameMessage` (the original walks `ss = ss->m_next`
//!   after the inner loop and would read freed memory; the port's
//!   registry is fixed for the duration of one propagation — the
//!   corpora never do this, and no real translator does).
//! - The dispatcher may not destroy or reorder command-list messages
//!   mid-walk (the original's `msg = msg->next()` would read the freed
//!   or moved node; the port reads `next` after the body through the
//!   arena and panics on a dead slot — an internal-invariant
//!   violation, never a data condition). Posting NEW commands to the
//!   stream mid-walk is fine (the corpus pins the same-frame
//!   visitation of tail appends).
//! - The message arena never reuses a slot (the memory-pool recycling
//!   of the original is unobservable without the freed-memory reads
//!   the port refuses).
//! - The punned argument reads (reading a stored Real as Integer, say)
//!   re-read the same four bytes the original's union holds, so they
//!   are byte-faithful on one platform (the goldens are generated and
//!   compared on the same IEEE-754 little-endian machine); the TYPED
//!   reads the real code makes are endianness-independent.
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use crate::logic::{GameLogic, KernelHost};
use crate::players::Players;
use crate::types::{Coord3, DrawableID, ICoord2, IRegion2, MAX_SLOTS, ObjectID, PlayerType};

// (PlayerType re-exported below for the on_logic_crc gate)

// BEGIN GENERATED enum block
/// `GameMessage::Type` (MessageStream.h): the message
/// vocabulary of the Zero Hour RELEASE build — 288 entries
/// (the RTS_DEBUG / cheat / ALLOW_SURRENDER / DUMP_PERF_STATS blocks
/// are out; the network block keeps its fixed 1000..=1999 ordinals;
/// the identifiers stay verbatim for cross-referencing the C++).
#[allow(non_camel_case_types)]
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GameMessageType {
    /// (none) this msg should never actually occur
    MSG_INVALID = 0,
    /// (timestamp) once each frame this message is sent thru the stream
    MSG_FRAME_TICK,
    /// (no doc comment in the original.)
    MSG_RAW_MOUSE_BEGIN,
    /// (pixel) the cursor's position
    MSG_RAW_MOUSE_POSITION,
    /// (pixel, modifiers, time)
    MSG_RAW_MOUSE_LEFT_BUTTON_DOWN,
    /// (pixel, modifiers, time)
    MSG_RAW_MOUSE_LEFT_DOUBLE_CLICK,
    /// (pixel, modifiers, time)
    MSG_RAW_MOUSE_LEFT_BUTTON_UP,
    /// (pixel, modifiers, time)
    MSG_RAW_MOUSE_LEFT_CLICK,
    /// drag of the mouse with a button held down
    MSG_RAW_MOUSE_LEFT_DRAG,
    /// (pixel, modifiers, time)
    MSG_RAW_MOUSE_MIDDLE_BUTTON_DOWN,
    /// (pixel, modifiers, time)
    MSG_RAW_MOUSE_MIDDLE_DOUBLE_CLICK,
    /// (pixel, modifiers, time)
    MSG_RAW_MOUSE_MIDDLE_BUTTON_UP,
    /// drag of the mouse with a button held down
    MSG_RAW_MOUSE_MIDDLE_DRAG,
    /// (pixel, modifiers, time)
    MSG_RAW_MOUSE_RIGHT_BUTTON_DOWN,
    /// (pixel, modifiers, time)
    MSG_RAW_MOUSE_RIGHT_DOUBLE_CLICK,
    /// (pixel, modifiers, time)
    MSG_RAW_MOUSE_RIGHT_BUTTON_UP,
    /// drag of the mouse with a button held down
    MSG_RAW_MOUSE_RIGHT_DRAG,
    /// (Real spin, + is away, - is toward user)
    MSG_RAW_MOUSE_WHEEL,
    /// (no doc comment in the original.)
    MSG_RAW_MOUSE_END,
    /// (KeyDefType) the given key was pressed (uses Microsoft VK_ codes)
    MSG_RAW_KEY_DOWN,
    /// (KeyDefType) the given key was released
    MSG_RAW_KEY_UP,
    /// (pixelRegion, 0 sized means its a point), (Int, modifier keys)
    MSG_MOUSE_LEFT_CLICK,
    /// (pixelRegion, 0 sized means its a point), (Int, modifier keys)
    MSG_MOUSE_LEFT_DOUBLE_CLICK,
    /// (pixelRegion, 0 sized means its a point), (Int, modifier keys)
    MSG_MOUSE_MIDDLE_CLICK,
    /// (pixelRegion, 0 sized means its a point), (Int, modifier keys)
    MSG_MOUSE_MIDDLE_DOUBLE_CLICK,
    /// (pixelRegion, 0 sized means its a point), (Int, modifier keys)
    MSG_MOUSE_RIGHT_CLICK,
    /// (pixelRegion, 0 sized means its a point), (Int, modifier keys)
    MSG_MOUSE_RIGHT_DOUBLE_CLICK,
    /// Clear all game data in memory
    MSG_CLEAR_GAME_DATA,
    /// Start a new game
    MSG_NEW_GAME,
    /// TheSuperHackers @tweak CRC from the logic used internally for replay playback.
    MSG_LOGIC_CRC_PLAYBACK,
    /// Marker to delineate "meta" messages
    MSG_BEGIN_META_MESSAGES,
    /// save current view as the given user-defined view
    MSG_META_SAVE_VIEW1,
    /// save current view as the given user-defined view
    MSG_META_SAVE_VIEW2,
    /// save current view as the given user-defined view
    MSG_META_SAVE_VIEW3,
    /// save current view as the given user-defined view
    MSG_META_SAVE_VIEW4,
    /// save current view as the given user-defined view
    MSG_META_SAVE_VIEW5,
    /// save current view as the given user-defined view
    MSG_META_SAVE_VIEW6,
    /// save current view as the given user-defined view
    MSG_META_SAVE_VIEW7,
    /// save current view as the given user-defined view
    MSG_META_SAVE_VIEW8,
    /// center view on the given user-defined view
    MSG_META_VIEW_VIEW1,
    /// center view on the given user-defined view
    MSG_META_VIEW_VIEW2,
    /// center view on the given user-defined view
    MSG_META_VIEW_VIEW3,
    /// center view on the given user-defined view
    MSG_META_VIEW_VIEW4,
    /// center view on the given user-defined view
    MSG_META_VIEW_VIEW5,
    /// center view on the given user-defined view
    MSG_META_VIEW_VIEW6,
    /// center view on the given user-defined view
    MSG_META_VIEW_VIEW7,
    /// center view on the given user-defined view
    MSG_META_VIEW_VIEW8,
    /// create user-defined team from the selected objects
    MSG_META_CREATE_TEAM0,
    /// create user-defined team from the selected objects
    MSG_META_CREATE_TEAM1,
    /// create user-defined team from the selected objects
    MSG_META_CREATE_TEAM2,
    /// create user-defined team from the selected objects
    MSG_META_CREATE_TEAM3,
    /// create user-defined team from the selected objects
    MSG_META_CREATE_TEAM4,
    /// create user-defined team from the selected objects
    MSG_META_CREATE_TEAM5,
    /// create user-defined team from the selected objects
    MSG_META_CREATE_TEAM6,
    /// create user-defined team from the selected objects
    MSG_META_CREATE_TEAM7,
    /// create user-defined team from the selected objects
    MSG_META_CREATE_TEAM8,
    /// create user-defined team from the selected objects
    MSG_META_CREATE_TEAM9,
    /// deselect all, then select all units in the given user-defined team
    MSG_META_SELECT_TEAM0,
    /// deselect all, then select all units in the given user-defined team
    MSG_META_SELECT_TEAM1,
    /// deselect all, then select all units in the given user-defined team
    MSG_META_SELECT_TEAM2,
    /// deselect all, then select all units in the given user-defined team
    MSG_META_SELECT_TEAM3,
    /// deselect all, then select all units in the given user-defined team
    MSG_META_SELECT_TEAM4,
    /// deselect all, then select all units in the given user-defined team
    MSG_META_SELECT_TEAM5,
    /// deselect all, then select all units in the given user-defined team
    MSG_META_SELECT_TEAM6,
    /// deselect all, then select all units in the given user-defined team
    MSG_META_SELECT_TEAM7,
    /// deselect all, then select all units in the given user-defined team
    MSG_META_SELECT_TEAM8,
    /// deselect all, then select all units in the given user-defined team
    MSG_META_SELECT_TEAM9,
    /// add the user-defined team to the current selection
    MSG_META_ADD_TEAM0,
    /// add the user-defined team to the current selection
    MSG_META_ADD_TEAM1,
    /// add the user-defined team to the current selection
    MSG_META_ADD_TEAM2,
    /// add the user-defined team to the current selection
    MSG_META_ADD_TEAM3,
    /// add the user-defined team to the current selection
    MSG_META_ADD_TEAM4,
    /// add the user-defined team to the current selection
    MSG_META_ADD_TEAM5,
    /// add the user-defined team to the current selection
    MSG_META_ADD_TEAM6,
    /// add the user-defined team to the current selection
    MSG_META_ADD_TEAM7,
    /// add the user-defined team to the current selection
    MSG_META_ADD_TEAM8,
    /// add the user-defined team to the current selection
    MSG_META_ADD_TEAM9,
    /// center view on given user-defined team (but do not affect selection)
    MSG_META_VIEW_TEAM0,
    /// center view on given user-defined team (but do not affect selection)
    MSG_META_VIEW_TEAM1,
    /// center view on given user-defined team (but do not affect selection)
    MSG_META_VIEW_TEAM2,
    /// center view on given user-defined team (but do not affect selection)
    MSG_META_VIEW_TEAM3,
    /// center view on given user-defined team (but do not affect selection)
    MSG_META_VIEW_TEAM4,
    /// center view on given user-defined team (but do not affect selection)
    MSG_META_VIEW_TEAM5,
    /// center view on given user-defined team (but do not affect selection)
    MSG_META_VIEW_TEAM6,
    /// center view on given user-defined team (but do not affect selection)
    MSG_META_VIEW_TEAM7,
    /// center view on given user-defined team (but do not affect selection)
    MSG_META_VIEW_TEAM8,
    /// center view on given user-defined team (but do not affect selection)
    MSG_META_VIEW_TEAM9,
    /// selects matching units, used for both on screen and across map
    MSG_META_SELECT_MATCHING_UNITS,
    /// select 'next' unit
    MSG_META_SELECT_NEXT_UNIT,
    /// select 'prev' unit
    MSG_META_SELECT_PREV_UNIT,
    /// select 'next' worker
    MSG_META_SELECT_NEXT_WORKER,
    /// select 'prev' worker
    MSG_META_SELECT_PREV_WORKER,
    /// TheSuperHackers @feature L3-M 03/08/2025 select next idle worker
    MSG_META_SELECT_NEXT_IDLE_WORKER,
    /// center view on command center
    MSG_META_VIEW_COMMAND_CENTER,
    /// center view on last radar event
    MSG_META_VIEW_LAST_RADAR_EVENT,
    /// selects player's hero character, if exists...
    MSG_META_SELECT_HERO,
    /// selects all units across screen
    MSG_META_SELECT_ALL,
    /// selects all air units just like select all, Added in Zero Hour
    MSG_META_SELECT_ALL_AIRCRAFT,
    /// selected units scatter
    MSG_META_SCATTER,
    /// selected units stop
    MSG_META_STOP,
    /// selected units 'deploy'
    MSG_META_DEPLOY,
    /// selected units become a formation
    MSG_META_CREATE_FORMATION,
    /// selected units 'follow'
    MSG_META_FOLLOW,
    /// send chat msg to all players
    MSG_META_CHAT_PLAYERS,
    /// send chat msg to allied players
    MSG_META_CHAT_ALLIES,
    /// send chat msg to everyone (incl. observers)
    MSG_META_CHAT_EVERYONE,
    /// bring up diplomacy screen
    MSG_META_DIPLOMACY,
    /// bring up options screen
    MSG_META_OPTIONS,
    /// TheSuperHackers @feature Increase the max render fps
    MSG_META_INCREASE_MAX_RENDER_FPS,
    /// TheSuperHackers @feature Decrease the max render fps
    MSG_META_DECREASE_MAX_RENDER_FPS,
    /// TheSuperHackers @feature Increase the logic time scale
    MSG_META_INCREASE_LOGIC_TIME_SCALE,
    /// TheSuperHackers @feature Decrease the logic time scale
    MSG_META_DECREASE_LOGIC_TIME_SCALE,
    /// toggles graphics options to crappy mode instantly
    MSG_META_TOGGLE_LOWER_DETAILS,
    /// show/hide controlbar
    MSG_META_TOGGLE_CONTROL_BAR,
    /// TheSuperHackers @feature Toggle the player observer view in game
    MSG_META_TOGGLE_PLAYER_OBSERVER,
    /// enter path-building mode
    MSG_META_BEGIN_PATH_BUILD,
    /// exit path-building mode
    MSG_META_END_PATH_BUILD,
    /// enter force-attack mode
    MSG_META_BEGIN_FORCEATTACK,
    /// exit force-attack mode
    MSG_META_END_FORCEATTACK,
    /// enter force-move mode
    MSG_META_BEGIN_FORCEMOVE,
    /// exit force-move mode
    MSG_META_END_FORCEMOVE,
    /// enter waypoint mode
    MSG_META_BEGIN_WAYPOINTS,
    /// exit waypoint mode
    MSG_META_END_WAYPOINTS,
    /// The Shift key has been depressed alone
    MSG_META_BEGIN_PREFER_SELECTION,
    /// The Shift key has been released.
    MSG_META_END_PREFER_SELECTION,
    /// take JPEG screenshot
    MSG_META_TAKE_SCREENSHOT,
    /// TheSuperHackers @feature Take lossless PNG screenshot
    MSG_META_TAKE_SCREENSHOT_PNG,
    /// Yay! :)
    MSG_META_ALL_CHEER,
    /// enter attack-move mode
    MSG_META_TOGGLE_ATTACKMOVE,
    /// (no doc comment in the original.)
    MSG_META_BEGIN_CAMERA_ROTATE_LEFT,
    /// (no doc comment in the original.)
    MSG_META_END_CAMERA_ROTATE_LEFT,
    /// TheSuperHackers @feature Rotate camera in 45 degree increments
    MSG_META_ALT_CAMERA_ROTATE_LEFT,
    /// (no doc comment in the original.)
    MSG_META_BEGIN_CAMERA_ROTATE_RIGHT,
    /// (no doc comment in the original.)
    MSG_META_END_CAMERA_ROTATE_RIGHT,
    /// TheSuperHackers @feature Rotate camera in 45 degree increments
    MSG_META_ALT_CAMERA_ROTATE_RIGHT,
    /// (no doc comment in the original.)
    MSG_META_BEGIN_CAMERA_ZOOM_IN,
    /// (no doc comment in the original.)
    MSG_META_END_CAMERA_ZOOM_IN,
    /// (no doc comment in the original.)
    MSG_META_BEGIN_CAMERA_ZOOM_OUT,
    /// (no doc comment in the original.)
    MSG_META_END_CAMERA_ZOOM_OUT,
    /// (no doc comment in the original.)
    MSG_META_CAMERA_RESET,
    /// Added in Zero Hour
    MSG_META_TOGGLE_CAMERA_TRACKING_DRAWABLE,
    /// Toggle the fast forward feature, // Added in Zero Hour
    MSG_META_TOGGLE_FAST_FORWARD_REPLAY,
    /// TheSuperHackers @feature Toggle game pause
    MSG_META_TOGGLE_PAUSE,
    /// TheSuperHackers @feature Toggle game pause (alternative mapping)
    MSG_META_TOGGLE_PAUSE_ALT,
    /// TheSuperHackers @feature Step one frame
    MSG_META_STEP_FRAME,
    /// TheSuperHackers @feature Step one frame (alternative mapping)
    MSG_META_STEP_FRAME_ALT,
    /// bail out of game immediately
    MSG_META_DEMO_INSTANT_QUIT,
    /// (no doc comment in the original.)
    MSG_META_PLACE_BEACON,
    /// (no doc comment in the original.)
    MSG_META_REMOVE_BEACON,
    /// Marker to delineate "meta" messages
    MSG_END_META_MESSAGES,
    /// (drawableid) the given drawable is under the mouse, regardless of button states
    MSG_MOUSEOVER_DRAWABLE_HINT,
    /// (location) The cursor is not over a drawable, but is here.
    MSG_MOUSEOVER_LOCATION_HINT,
    /// posted when the gui command is valid if the user clicked to execute it.
    MSG_VALID_GUICOMMAND_HINT,
    /// posted when the gui command is not valid if the user were to click to attempt to execute it.
    MSG_INVALID_GUICOMMAND_HINT,
    /// (pixelRegion) rectangular selection area under construction, not confirmed
    MSG_BEGIN_AREA_SELECTION_HINT,
    /// (pixelRegion) rectangular selection area finish construction
    MSG_END_AREA_SELECTION_HINT,
    /// (victim objectID) If clicked, an attack would be ordered, "Current Selection" is assumed
    MSG_DO_ATTACK_OBJECT_HINT,
    /// we can't do anything, and target is out of range.
    MSG_IMPOSSIBLE_ATTACK_HINT,
    /// (victim objectID) If clicked, an attack would be ordered, "Current Selection" is assumed
    MSG_DO_FORCE_ATTACK_OBJECT_HINT,
    /// (victim objectID) If clicked, an attack would be ordered, "Current Selection" is assumed
    MSG_DO_FORCE_ATTACK_GROUND_HINT,
    /// If clicked, selected unit will go get repaired at clicked object
    MSG_GET_REPAIRED_HINT,
    /// If clicked, selected unit will go get healed at clicked object
    MSG_GET_HEALED_HINT,
    /// if clicked, dozer will go repair the clicked target
    MSG_DO_REPAIR_HINT,
    /// if clicked, dozer will go construct a partially constructed building
    MSG_RESUME_CONSTRUCTION_HINT,
    /// if clicked, selected unit(s) will attempt to enter clicked object
    MSG_ENTER_HINT,
    /// If clicked, selected unit(s) will dock
    MSG_DOCK_HINT,
    /// (location) If clicked, a move would be ordered, "Current Selection" is assumed
    MSG_DO_MOVETO_HINT,
    /// (location) If clicked, a move would be ordered, "Current Selection" is assumed
    MSG_DO_ATTACKMOVETO_HINT,
    /// (location) If clicked, a waypoint will be added for currently selected units.
    MSG_ADD_WAYPOINT_HINT,
    /// if clicked, selected unit(s) will attempt to take over vehicle.
    MSG_HIJACK_HINT,
    /// Added in Zero Hour
    MSG_SABOTAGE_HINT,
    /// throw a molotov cocktail
    MSG_FIREBOMB_HINT,
    /// if clicked, selected unit(s) will attempt to convert clicked object into a carbomb.
    MSG_CONVERT_TO_CARBOMB_HINT,
    /// (no doc comment in the original.)
    MSG_CAPTUREBUILDING_HINT,
    /// (no doc comment in the original.)
    MSG_SNIPE_VEHICLE_HINT,
    /// (no doc comment in the original.)
    MSG_DEFECTOR_HINT,
    /// (location) if clicked, we will place a rally point here.
    MSG_SET_RALLY_POINT_HINT,
    /// (no doc comment in the original.)
    MSG_DO_SPECIAL_POWER_OVERRIDE_DESTINATION_HINT,
    /// (no doc comment in the original.)
    MSG_DO_SALVAGE_HINT,
    /// Display invalid cursor because no real command can be done in this context.
    MSG_DO_INVALID_HINT,
    /// (no doc comment in the original.)
    MSG_DO_ATTACK_OBJECT_AFTER_MOVING_HINT,
    /// (no doc comment in the original.)
    MSG_HACK_HINT,
    /// MARKER TO DELINEATE MESSAGES THAT GO OVER THE NETWORK
    MSG_BEGIN_NETWORK_MESSAGES = 1000,
    /// (Bool createNewGroup, objectID1, objectID2, ... objectIDN)
    MSG_CREATE_SELECTED_GROUP,
    /// (Bool createNewGroup, objectID1, objectID2, ... objectIDN)
    MSG_CREATE_SELECTED_GROUP_NO_SOUND,
    /// deselect currently selected objects (which can be none)
    MSG_DESTROY_SELECTED_GROUP,
    /// (objectID1, objectID2, ... objectIDN)
    MSG_REMOVE_FROM_SELECTED_GROUP,
    /// (teamID) the NEXT COMMAND acts upon all members of this team
    MSG_SELECTED_GROUP_COMMAND,
    /// Creates a hotkey squad from the currently selected units.
    MSG_CREATE_TEAM0,
    /// (no doc comment in the original.)
    MSG_CREATE_TEAM1,
    /// (no doc comment in the original.)
    MSG_CREATE_TEAM2,
    /// (no doc comment in the original.)
    MSG_CREATE_TEAM3,
    /// (no doc comment in the original.)
    MSG_CREATE_TEAM4,
    /// (no doc comment in the original.)
    MSG_CREATE_TEAM5,
    /// (no doc comment in the original.)
    MSG_CREATE_TEAM6,
    /// (no doc comment in the original.)
    MSG_CREATE_TEAM7,
    /// (no doc comment in the original.)
    MSG_CREATE_TEAM8,
    /// (no doc comment in the original.)
    MSG_CREATE_TEAM9,
    /// Set a hotkey squad to be the currently selected units.
    MSG_SELECT_TEAM0,
    /// Set a hotkey squad to be the currently selected units.
    MSG_SELECT_TEAM1,
    /// Set a hotkey squad to be the currently selected units.
    MSG_SELECT_TEAM2,
    /// Set a hotkey squad to be the currently selected units.
    MSG_SELECT_TEAM3,
    /// Set a hotkey squad to be the currently selected units.
    MSG_SELECT_TEAM4,
    /// Set a hotkey squad to be the currently selected units.
    MSG_SELECT_TEAM5,
    /// Set a hotkey squad to be the currently selected units.
    MSG_SELECT_TEAM6,
    /// Set a hotkey squad to be the currently selected units.
    MSG_SELECT_TEAM7,
    /// Set a hotkey squad to be the currently selected units.
    MSG_SELECT_TEAM8,
    /// Set a hotkey squad to be the currently selected units.
    MSG_SELECT_TEAM9,
    /// Add hotkey squad to the currently selected units.
    MSG_ADD_TEAM0,
    /// Add hotkey squad to the currently selected units.
    MSG_ADD_TEAM1,
    /// Add hotkey squad to the currently selected units.
    MSG_ADD_TEAM2,
    /// Add hotkey squad to the currently selected units.
    MSG_ADD_TEAM3,
    /// Add hotkey squad to the currently selected units.
    MSG_ADD_TEAM4,
    /// Add hotkey squad to the currently selected units.
    MSG_ADD_TEAM5,
    /// Add hotkey squad to the currently selected units.
    MSG_ADD_TEAM6,
    /// Add hotkey squad to the currently selected units.
    MSG_ADD_TEAM7,
    /// Add hotkey squad to the currently selected units.
    MSG_ADD_TEAM8,
    /// Add hotkey squad to the currently selected units.
    MSG_ADD_TEAM9,
    /// (numObjects) (numObjects * objectID)
    MSG_DO_ATTACKSQUAD,
    /// fire specific weapon
    MSG_DO_WEAPON,
    /// fire a specific weapon at location
    MSG_DO_WEAPON_AT_LOCATION,
    /// fire a specific weapon at a target object
    MSG_DO_WEAPON_AT_OBJECT,
    /// do special
    MSG_DO_SPECIAL_POWER,
    /// do special with target location
    MSG_DO_SPECIAL_POWER_AT_LOCATION,
    /// do special at with target object
    MSG_DO_SPECIAL_POWER_AT_OBJECT,
    /// (objectID, location)
    MSG_SET_RALLY_POINT,
    /// purchase a science
    MSG_PURCHASE_SCIENCE,
    /// queue the "research" of an upgrade
    MSG_QUEUE_UPGRADE,
    /// cancel the "research" of an upgrade
    MSG_CANCEL_UPGRADE,
    /// clicked on a button to queue the production of a unit
    MSG_QUEUE_UNIT_CREATE,
    /// clicked on UI button to cancel production of a unit
    MSG_CANCEL_UNIT_CREATE,
    /// building things requires clicking on a dozer
    MSG_DOZER_CONSTRUCT,
    /// Like MSG_CONSTRUCT, but for build processes that occur in a line (like walls)
    MSG_DOZER_CONSTRUCT_LINE,
    /// cancel construction of a building
    MSG_DOZER_CANCEL_CONSTRUCT,
    /// sell a structure
    MSG_SELL,
    /// WE want to exit from whatever WE are inside of
    MSG_EXIT,
    /// Dump out all of OUR contained objects
    MSG_EVACUATE,
    /// Execute railed transport sequence
    MSG_EXECUTE_RAILED_TRANSPORT,
    /// dump out all rappellers
    MSG_COMBATDROP_AT_LOCATION,
    /// dump out all rappellers
    MSG_COMBATDROP_AT_OBJECT,
    /// TheSuperHackers @tweak former MSG_AREA_SELECTION is deprecated as network message.
    MSG_AREA_SELECTION_DEPRECATED,
    /// (objectID, victim objectID)
    MSG_DO_ATTACK_OBJECT,
    /// force attack the given object if picked
    MSG_DO_FORCE_ATTACK_OBJECT,
    /// (locationID) bombard the given location if picked
    MSG_DO_FORCE_ATTACK_GROUND,
    /// selected unit will go get repaired at clicked object
    MSG_GET_REPAIRED,
    /// selected unit will go get healed at clicked object
    MSG_GET_HEALED,
    /// dozer will go repair the clicked target
    MSG_DO_REPAIR,
    /// resume construction on a structure
    MSG_RESUME_CONSTRUCTION,
    /// Enter object
    MSG_ENTER,
    /// Dock with this object
    MSG_DOCK,
    /// location
    MSG_DO_MOVETO,
    /// location
    MSG_DO_ATTACKMOVETO,
    /// location
    MSG_DO_FORCEMOVETO,
    /// location
    MSG_ADD_WAYPOINT,
    /// Guard with the currently selected group
    MSG_DO_GUARD_POSITION,
    /// Guard with the currently selected group
    MSG_DO_GUARD_OBJECT,
    /// Stop with the currently selected group
    MSG_DO_STOP,
    /// Scatter the currently selected group
    MSG_DO_SCATTER,
    /// Begin a persistent internet hack (free slow income)
    MSG_INTERNET_HACK,
    /// Orders selected units to play cheer animation (if possible)
    MSG_DO_CHEER,
    /// Toggle overcharge status of a power plant
    MSG_TOGGLE_OVERCHARGE,
    /// Switches which weapon slot to use for an object
    MSG_SWITCH_WEAPONS,
    /// (no doc comment in the original.)
    MSG_CONVERT_TO_CARBOMB,
    /// (no doc comment in the original.)
    MSG_CAPTUREBUILDING,
    /// (no doc comment in the original.)
    MSG_DISABLEVEHICLE_HACK,
    /// (no doc comment in the original.)
    MSG_STEALCASH_HACK,
    /// (no doc comment in the original.)
    MSG_DISABLEBUILDING_HACK,
    /// (no doc comment in the original.)
    MSG_SNIPE_VEHICLE,
    /// (no doc comment in the original.)
    MSG_DO_SPECIAL_POWER_OVERRIDE_DESTINATION,
    /// (no doc comment in the original.)
    MSG_DO_SALVAGE,
    /// If we want a replay to work with the popup messages then we need it to be passed
    MSG_CLEAR_INGAME_POPUP_MESSAGE,
    /// (no doc comment in the original.)
    MSG_PLACE_BEACON,
    /// (no doc comment in the original.)
    MSG_REMOVE_BEACON,
    /// (no doc comment in the original.)
    MSG_SET_BEACON_TEXT,
    /// Track camera pos for replays
    MSG_SET_REPLAY_CAMERA,
    /// Destroys a player's units (for copy protection or to quit to observer)
    MSG_SELF_DESTRUCT,
    /// Creates a formation.
    MSG_CREATE_FORMATION,
    /// CRC from the logic passed around in a network game
    MSG_LOGIC_CRC,
    /// Order selected unit(s) to clear mines
    MSG_SET_MINE_CLEARING_DETAIL,
    /// Turn retaliation mode on or off. Added in Zero Hour
    MSG_ENABLE_RETALIATION_MODE,
    /// network messages that exist only in debug/internal builds. all grouped separately.
    MSG_BEGIN_DEBUG_NETWORK_MESSAGES = 1900,
    /// MARKER TO DELINEATE MESSAGES THAT GO OVER THE NETWORK
    MSG_END_NETWORK_MESSAGES = 1999,
    /// The current frame number
    MSG_TIMESTAMP,
    /// (objectID, Int type) Cause a drawable to be created and bound to this ID
    MSG_OBJECT_CREATED,
    /// (objectID) Free bound drawable
    MSG_OBJECT_DESTROYED,
    /// (objectID, location) New position of object
    MSG_OBJECT_POSITION,
    /// (objectID, angle) New orientation of object
    MSG_OBJECT_ORIENTATION,
    /// (objectID) New team affiliation of object
    MSG_OBJECT_JOINED_TEAM,
}

/// `MSG_COUNT` (MessageStream.h): one past the last type.
pub const MSG_COUNT: i32 = 2006;

impl GameMessageType {
    /// The original's ordinal (the enum's `repr` value).
    #[must_use]
    pub fn ordinal(self) -> i32 {
        self as i32
    }

    /// The ordinal back to the type (`None` outside the vocabulary
    /// — the fixed network markers' gaps included; the original has
    /// no reverse map, this is the port's parse surface for the
    /// replay/net layers).
    #[must_use]
    pub fn from_ordinal(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::MSG_INVALID),
            1 => Some(Self::MSG_FRAME_TICK),
            2 => Some(Self::MSG_RAW_MOUSE_BEGIN),
            3 => Some(Self::MSG_RAW_MOUSE_POSITION),
            4 => Some(Self::MSG_RAW_MOUSE_LEFT_BUTTON_DOWN),
            5 => Some(Self::MSG_RAW_MOUSE_LEFT_DOUBLE_CLICK),
            6 => Some(Self::MSG_RAW_MOUSE_LEFT_BUTTON_UP),
            7 => Some(Self::MSG_RAW_MOUSE_LEFT_CLICK),
            8 => Some(Self::MSG_RAW_MOUSE_LEFT_DRAG),
            9 => Some(Self::MSG_RAW_MOUSE_MIDDLE_BUTTON_DOWN),
            10 => Some(Self::MSG_RAW_MOUSE_MIDDLE_DOUBLE_CLICK),
            11 => Some(Self::MSG_RAW_MOUSE_MIDDLE_BUTTON_UP),
            12 => Some(Self::MSG_RAW_MOUSE_MIDDLE_DRAG),
            13 => Some(Self::MSG_RAW_MOUSE_RIGHT_BUTTON_DOWN),
            14 => Some(Self::MSG_RAW_MOUSE_RIGHT_DOUBLE_CLICK),
            15 => Some(Self::MSG_RAW_MOUSE_RIGHT_BUTTON_UP),
            16 => Some(Self::MSG_RAW_MOUSE_RIGHT_DRAG),
            17 => Some(Self::MSG_RAW_MOUSE_WHEEL),
            18 => Some(Self::MSG_RAW_MOUSE_END),
            19 => Some(Self::MSG_RAW_KEY_DOWN),
            20 => Some(Self::MSG_RAW_KEY_UP),
            21 => Some(Self::MSG_MOUSE_LEFT_CLICK),
            22 => Some(Self::MSG_MOUSE_LEFT_DOUBLE_CLICK),
            23 => Some(Self::MSG_MOUSE_MIDDLE_CLICK),
            24 => Some(Self::MSG_MOUSE_MIDDLE_DOUBLE_CLICK),
            25 => Some(Self::MSG_MOUSE_RIGHT_CLICK),
            26 => Some(Self::MSG_MOUSE_RIGHT_DOUBLE_CLICK),
            27 => Some(Self::MSG_CLEAR_GAME_DATA),
            28 => Some(Self::MSG_NEW_GAME),
            29 => Some(Self::MSG_LOGIC_CRC_PLAYBACK),
            30 => Some(Self::MSG_BEGIN_META_MESSAGES),
            31 => Some(Self::MSG_META_SAVE_VIEW1),
            32 => Some(Self::MSG_META_SAVE_VIEW2),
            33 => Some(Self::MSG_META_SAVE_VIEW3),
            34 => Some(Self::MSG_META_SAVE_VIEW4),
            35 => Some(Self::MSG_META_SAVE_VIEW5),
            36 => Some(Self::MSG_META_SAVE_VIEW6),
            37 => Some(Self::MSG_META_SAVE_VIEW7),
            38 => Some(Self::MSG_META_SAVE_VIEW8),
            39 => Some(Self::MSG_META_VIEW_VIEW1),
            40 => Some(Self::MSG_META_VIEW_VIEW2),
            41 => Some(Self::MSG_META_VIEW_VIEW3),
            42 => Some(Self::MSG_META_VIEW_VIEW4),
            43 => Some(Self::MSG_META_VIEW_VIEW5),
            44 => Some(Self::MSG_META_VIEW_VIEW6),
            45 => Some(Self::MSG_META_VIEW_VIEW7),
            46 => Some(Self::MSG_META_VIEW_VIEW8),
            47 => Some(Self::MSG_META_CREATE_TEAM0),
            48 => Some(Self::MSG_META_CREATE_TEAM1),
            49 => Some(Self::MSG_META_CREATE_TEAM2),
            50 => Some(Self::MSG_META_CREATE_TEAM3),
            51 => Some(Self::MSG_META_CREATE_TEAM4),
            52 => Some(Self::MSG_META_CREATE_TEAM5),
            53 => Some(Self::MSG_META_CREATE_TEAM6),
            54 => Some(Self::MSG_META_CREATE_TEAM7),
            55 => Some(Self::MSG_META_CREATE_TEAM8),
            56 => Some(Self::MSG_META_CREATE_TEAM9),
            57 => Some(Self::MSG_META_SELECT_TEAM0),
            58 => Some(Self::MSG_META_SELECT_TEAM1),
            59 => Some(Self::MSG_META_SELECT_TEAM2),
            60 => Some(Self::MSG_META_SELECT_TEAM3),
            61 => Some(Self::MSG_META_SELECT_TEAM4),
            62 => Some(Self::MSG_META_SELECT_TEAM5),
            63 => Some(Self::MSG_META_SELECT_TEAM6),
            64 => Some(Self::MSG_META_SELECT_TEAM7),
            65 => Some(Self::MSG_META_SELECT_TEAM8),
            66 => Some(Self::MSG_META_SELECT_TEAM9),
            67 => Some(Self::MSG_META_ADD_TEAM0),
            68 => Some(Self::MSG_META_ADD_TEAM1),
            69 => Some(Self::MSG_META_ADD_TEAM2),
            70 => Some(Self::MSG_META_ADD_TEAM3),
            71 => Some(Self::MSG_META_ADD_TEAM4),
            72 => Some(Self::MSG_META_ADD_TEAM5),
            73 => Some(Self::MSG_META_ADD_TEAM6),
            74 => Some(Self::MSG_META_ADD_TEAM7),
            75 => Some(Self::MSG_META_ADD_TEAM8),
            76 => Some(Self::MSG_META_ADD_TEAM9),
            77 => Some(Self::MSG_META_VIEW_TEAM0),
            78 => Some(Self::MSG_META_VIEW_TEAM1),
            79 => Some(Self::MSG_META_VIEW_TEAM2),
            80 => Some(Self::MSG_META_VIEW_TEAM3),
            81 => Some(Self::MSG_META_VIEW_TEAM4),
            82 => Some(Self::MSG_META_VIEW_TEAM5),
            83 => Some(Self::MSG_META_VIEW_TEAM6),
            84 => Some(Self::MSG_META_VIEW_TEAM7),
            85 => Some(Self::MSG_META_VIEW_TEAM8),
            86 => Some(Self::MSG_META_VIEW_TEAM9),
            87 => Some(Self::MSG_META_SELECT_MATCHING_UNITS),
            88 => Some(Self::MSG_META_SELECT_NEXT_UNIT),
            89 => Some(Self::MSG_META_SELECT_PREV_UNIT),
            90 => Some(Self::MSG_META_SELECT_NEXT_WORKER),
            91 => Some(Self::MSG_META_SELECT_PREV_WORKER),
            92 => Some(Self::MSG_META_SELECT_NEXT_IDLE_WORKER),
            93 => Some(Self::MSG_META_VIEW_COMMAND_CENTER),
            94 => Some(Self::MSG_META_VIEW_LAST_RADAR_EVENT),
            95 => Some(Self::MSG_META_SELECT_HERO),
            96 => Some(Self::MSG_META_SELECT_ALL),
            97 => Some(Self::MSG_META_SELECT_ALL_AIRCRAFT),
            98 => Some(Self::MSG_META_SCATTER),
            99 => Some(Self::MSG_META_STOP),
            100 => Some(Self::MSG_META_DEPLOY),
            101 => Some(Self::MSG_META_CREATE_FORMATION),
            102 => Some(Self::MSG_META_FOLLOW),
            103 => Some(Self::MSG_META_CHAT_PLAYERS),
            104 => Some(Self::MSG_META_CHAT_ALLIES),
            105 => Some(Self::MSG_META_CHAT_EVERYONE),
            106 => Some(Self::MSG_META_DIPLOMACY),
            107 => Some(Self::MSG_META_OPTIONS),
            108 => Some(Self::MSG_META_INCREASE_MAX_RENDER_FPS),
            109 => Some(Self::MSG_META_DECREASE_MAX_RENDER_FPS),
            110 => Some(Self::MSG_META_INCREASE_LOGIC_TIME_SCALE),
            111 => Some(Self::MSG_META_DECREASE_LOGIC_TIME_SCALE),
            112 => Some(Self::MSG_META_TOGGLE_LOWER_DETAILS),
            113 => Some(Self::MSG_META_TOGGLE_CONTROL_BAR),
            114 => Some(Self::MSG_META_TOGGLE_PLAYER_OBSERVER),
            115 => Some(Self::MSG_META_BEGIN_PATH_BUILD),
            116 => Some(Self::MSG_META_END_PATH_BUILD),
            117 => Some(Self::MSG_META_BEGIN_FORCEATTACK),
            118 => Some(Self::MSG_META_END_FORCEATTACK),
            119 => Some(Self::MSG_META_BEGIN_FORCEMOVE),
            120 => Some(Self::MSG_META_END_FORCEMOVE),
            121 => Some(Self::MSG_META_BEGIN_WAYPOINTS),
            122 => Some(Self::MSG_META_END_WAYPOINTS),
            123 => Some(Self::MSG_META_BEGIN_PREFER_SELECTION),
            124 => Some(Self::MSG_META_END_PREFER_SELECTION),
            125 => Some(Self::MSG_META_TAKE_SCREENSHOT),
            126 => Some(Self::MSG_META_TAKE_SCREENSHOT_PNG),
            127 => Some(Self::MSG_META_ALL_CHEER),
            128 => Some(Self::MSG_META_TOGGLE_ATTACKMOVE),
            129 => Some(Self::MSG_META_BEGIN_CAMERA_ROTATE_LEFT),
            130 => Some(Self::MSG_META_END_CAMERA_ROTATE_LEFT),
            131 => Some(Self::MSG_META_ALT_CAMERA_ROTATE_LEFT),
            132 => Some(Self::MSG_META_BEGIN_CAMERA_ROTATE_RIGHT),
            133 => Some(Self::MSG_META_END_CAMERA_ROTATE_RIGHT),
            134 => Some(Self::MSG_META_ALT_CAMERA_ROTATE_RIGHT),
            135 => Some(Self::MSG_META_BEGIN_CAMERA_ZOOM_IN),
            136 => Some(Self::MSG_META_END_CAMERA_ZOOM_IN),
            137 => Some(Self::MSG_META_BEGIN_CAMERA_ZOOM_OUT),
            138 => Some(Self::MSG_META_END_CAMERA_ZOOM_OUT),
            139 => Some(Self::MSG_META_CAMERA_RESET),
            140 => Some(Self::MSG_META_TOGGLE_CAMERA_TRACKING_DRAWABLE),
            141 => Some(Self::MSG_META_TOGGLE_FAST_FORWARD_REPLAY),
            142 => Some(Self::MSG_META_TOGGLE_PAUSE),
            143 => Some(Self::MSG_META_TOGGLE_PAUSE_ALT),
            144 => Some(Self::MSG_META_STEP_FRAME),
            145 => Some(Self::MSG_META_STEP_FRAME_ALT),
            146 => Some(Self::MSG_META_DEMO_INSTANT_QUIT),
            147 => Some(Self::MSG_META_PLACE_BEACON),
            148 => Some(Self::MSG_META_REMOVE_BEACON),
            149 => Some(Self::MSG_END_META_MESSAGES),
            150 => Some(Self::MSG_MOUSEOVER_DRAWABLE_HINT),
            151 => Some(Self::MSG_MOUSEOVER_LOCATION_HINT),
            152 => Some(Self::MSG_VALID_GUICOMMAND_HINT),
            153 => Some(Self::MSG_INVALID_GUICOMMAND_HINT),
            154 => Some(Self::MSG_BEGIN_AREA_SELECTION_HINT),
            155 => Some(Self::MSG_END_AREA_SELECTION_HINT),
            156 => Some(Self::MSG_DO_ATTACK_OBJECT_HINT),
            157 => Some(Self::MSG_IMPOSSIBLE_ATTACK_HINT),
            158 => Some(Self::MSG_DO_FORCE_ATTACK_OBJECT_HINT),
            159 => Some(Self::MSG_DO_FORCE_ATTACK_GROUND_HINT),
            160 => Some(Self::MSG_GET_REPAIRED_HINT),
            161 => Some(Self::MSG_GET_HEALED_HINT),
            162 => Some(Self::MSG_DO_REPAIR_HINT),
            163 => Some(Self::MSG_RESUME_CONSTRUCTION_HINT),
            164 => Some(Self::MSG_ENTER_HINT),
            165 => Some(Self::MSG_DOCK_HINT),
            166 => Some(Self::MSG_DO_MOVETO_HINT),
            167 => Some(Self::MSG_DO_ATTACKMOVETO_HINT),
            168 => Some(Self::MSG_ADD_WAYPOINT_HINT),
            169 => Some(Self::MSG_HIJACK_HINT),
            170 => Some(Self::MSG_SABOTAGE_HINT),
            171 => Some(Self::MSG_FIREBOMB_HINT),
            172 => Some(Self::MSG_CONVERT_TO_CARBOMB_HINT),
            173 => Some(Self::MSG_CAPTUREBUILDING_HINT),
            174 => Some(Self::MSG_SNIPE_VEHICLE_HINT),
            175 => Some(Self::MSG_DEFECTOR_HINT),
            176 => Some(Self::MSG_SET_RALLY_POINT_HINT),
            177 => Some(Self::MSG_DO_SPECIAL_POWER_OVERRIDE_DESTINATION_HINT),
            178 => Some(Self::MSG_DO_SALVAGE_HINT),
            179 => Some(Self::MSG_DO_INVALID_HINT),
            180 => Some(Self::MSG_DO_ATTACK_OBJECT_AFTER_MOVING_HINT),
            181 => Some(Self::MSG_HACK_HINT),
            1000 => Some(Self::MSG_BEGIN_NETWORK_MESSAGES),
            1001 => Some(Self::MSG_CREATE_SELECTED_GROUP),
            1002 => Some(Self::MSG_CREATE_SELECTED_GROUP_NO_SOUND),
            1003 => Some(Self::MSG_DESTROY_SELECTED_GROUP),
            1004 => Some(Self::MSG_REMOVE_FROM_SELECTED_GROUP),
            1005 => Some(Self::MSG_SELECTED_GROUP_COMMAND),
            1006 => Some(Self::MSG_CREATE_TEAM0),
            1007 => Some(Self::MSG_CREATE_TEAM1),
            1008 => Some(Self::MSG_CREATE_TEAM2),
            1009 => Some(Self::MSG_CREATE_TEAM3),
            1010 => Some(Self::MSG_CREATE_TEAM4),
            1011 => Some(Self::MSG_CREATE_TEAM5),
            1012 => Some(Self::MSG_CREATE_TEAM6),
            1013 => Some(Self::MSG_CREATE_TEAM7),
            1014 => Some(Self::MSG_CREATE_TEAM8),
            1015 => Some(Self::MSG_CREATE_TEAM9),
            1016 => Some(Self::MSG_SELECT_TEAM0),
            1017 => Some(Self::MSG_SELECT_TEAM1),
            1018 => Some(Self::MSG_SELECT_TEAM2),
            1019 => Some(Self::MSG_SELECT_TEAM3),
            1020 => Some(Self::MSG_SELECT_TEAM4),
            1021 => Some(Self::MSG_SELECT_TEAM5),
            1022 => Some(Self::MSG_SELECT_TEAM6),
            1023 => Some(Self::MSG_SELECT_TEAM7),
            1024 => Some(Self::MSG_SELECT_TEAM8),
            1025 => Some(Self::MSG_SELECT_TEAM9),
            1026 => Some(Self::MSG_ADD_TEAM0),
            1027 => Some(Self::MSG_ADD_TEAM1),
            1028 => Some(Self::MSG_ADD_TEAM2),
            1029 => Some(Self::MSG_ADD_TEAM3),
            1030 => Some(Self::MSG_ADD_TEAM4),
            1031 => Some(Self::MSG_ADD_TEAM5),
            1032 => Some(Self::MSG_ADD_TEAM6),
            1033 => Some(Self::MSG_ADD_TEAM7),
            1034 => Some(Self::MSG_ADD_TEAM8),
            1035 => Some(Self::MSG_ADD_TEAM9),
            1036 => Some(Self::MSG_DO_ATTACKSQUAD),
            1037 => Some(Self::MSG_DO_WEAPON),
            1038 => Some(Self::MSG_DO_WEAPON_AT_LOCATION),
            1039 => Some(Self::MSG_DO_WEAPON_AT_OBJECT),
            1040 => Some(Self::MSG_DO_SPECIAL_POWER),
            1041 => Some(Self::MSG_DO_SPECIAL_POWER_AT_LOCATION),
            1042 => Some(Self::MSG_DO_SPECIAL_POWER_AT_OBJECT),
            1043 => Some(Self::MSG_SET_RALLY_POINT),
            1044 => Some(Self::MSG_PURCHASE_SCIENCE),
            1045 => Some(Self::MSG_QUEUE_UPGRADE),
            1046 => Some(Self::MSG_CANCEL_UPGRADE),
            1047 => Some(Self::MSG_QUEUE_UNIT_CREATE),
            1048 => Some(Self::MSG_CANCEL_UNIT_CREATE),
            1049 => Some(Self::MSG_DOZER_CONSTRUCT),
            1050 => Some(Self::MSG_DOZER_CONSTRUCT_LINE),
            1051 => Some(Self::MSG_DOZER_CANCEL_CONSTRUCT),
            1052 => Some(Self::MSG_SELL),
            1053 => Some(Self::MSG_EXIT),
            1054 => Some(Self::MSG_EVACUATE),
            1055 => Some(Self::MSG_EXECUTE_RAILED_TRANSPORT),
            1056 => Some(Self::MSG_COMBATDROP_AT_LOCATION),
            1057 => Some(Self::MSG_COMBATDROP_AT_OBJECT),
            1058 => Some(Self::MSG_AREA_SELECTION_DEPRECATED),
            1059 => Some(Self::MSG_DO_ATTACK_OBJECT),
            1060 => Some(Self::MSG_DO_FORCE_ATTACK_OBJECT),
            1061 => Some(Self::MSG_DO_FORCE_ATTACK_GROUND),
            1062 => Some(Self::MSG_GET_REPAIRED),
            1063 => Some(Self::MSG_GET_HEALED),
            1064 => Some(Self::MSG_DO_REPAIR),
            1065 => Some(Self::MSG_RESUME_CONSTRUCTION),
            1066 => Some(Self::MSG_ENTER),
            1067 => Some(Self::MSG_DOCK),
            1068 => Some(Self::MSG_DO_MOVETO),
            1069 => Some(Self::MSG_DO_ATTACKMOVETO),
            1070 => Some(Self::MSG_DO_FORCEMOVETO),
            1071 => Some(Self::MSG_ADD_WAYPOINT),
            1072 => Some(Self::MSG_DO_GUARD_POSITION),
            1073 => Some(Self::MSG_DO_GUARD_OBJECT),
            1074 => Some(Self::MSG_DO_STOP),
            1075 => Some(Self::MSG_DO_SCATTER),
            1076 => Some(Self::MSG_INTERNET_HACK),
            1077 => Some(Self::MSG_DO_CHEER),
            1078 => Some(Self::MSG_TOGGLE_OVERCHARGE),
            1079 => Some(Self::MSG_SWITCH_WEAPONS),
            1080 => Some(Self::MSG_CONVERT_TO_CARBOMB),
            1081 => Some(Self::MSG_CAPTUREBUILDING),
            1082 => Some(Self::MSG_DISABLEVEHICLE_HACK),
            1083 => Some(Self::MSG_STEALCASH_HACK),
            1084 => Some(Self::MSG_DISABLEBUILDING_HACK),
            1085 => Some(Self::MSG_SNIPE_VEHICLE),
            1086 => Some(Self::MSG_DO_SPECIAL_POWER_OVERRIDE_DESTINATION),
            1087 => Some(Self::MSG_DO_SALVAGE),
            1088 => Some(Self::MSG_CLEAR_INGAME_POPUP_MESSAGE),
            1089 => Some(Self::MSG_PLACE_BEACON),
            1090 => Some(Self::MSG_REMOVE_BEACON),
            1091 => Some(Self::MSG_SET_BEACON_TEXT),
            1092 => Some(Self::MSG_SET_REPLAY_CAMERA),
            1093 => Some(Self::MSG_SELF_DESTRUCT),
            1094 => Some(Self::MSG_CREATE_FORMATION),
            1095 => Some(Self::MSG_LOGIC_CRC),
            1096 => Some(Self::MSG_SET_MINE_CLEARING_DETAIL),
            1097 => Some(Self::MSG_ENABLE_RETALIATION_MODE),
            1900 => Some(Self::MSG_BEGIN_DEBUG_NETWORK_MESSAGES),
            1999 => Some(Self::MSG_END_NETWORK_MESSAGES),
            2000 => Some(Self::MSG_TIMESTAMP),
            2001 => Some(Self::MSG_OBJECT_CREATED),
            2002 => Some(Self::MSG_OBJECT_DESTROYED),
            2003 => Some(Self::MSG_OBJECT_POSITION),
            2004 => Some(Self::MSG_OBJECT_ORIENTATION),
            2005 => Some(Self::MSG_OBJECT_JOINED_TEAM),
            _ => None,
        }
    }

    /// `GameMessage::getCommandTypeAsString` (MessageStream.cpp):
    /// the debug name (the switch's `#x` labels; the out-of-range
    /// default is `"Invalid command"` — unreachable through the
    /// typed enum — and the in-range unmapped one stays
    /// `"UnknownMessage"` exactly as the original's default arm).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::MSG_INVALID => "MSG_INVALID",
            Self::MSG_FRAME_TICK => "MSG_FRAME_TICK",
            Self::MSG_RAW_MOUSE_BEGIN => "MSG_RAW_MOUSE_BEGIN",
            Self::MSG_RAW_MOUSE_POSITION => "MSG_RAW_MOUSE_POSITION",
            Self::MSG_RAW_MOUSE_LEFT_BUTTON_DOWN => "MSG_RAW_MOUSE_LEFT_BUTTON_DOWN",
            Self::MSG_RAW_MOUSE_LEFT_DOUBLE_CLICK => "MSG_RAW_MOUSE_LEFT_DOUBLE_CLICK",
            Self::MSG_RAW_MOUSE_LEFT_BUTTON_UP => "MSG_RAW_MOUSE_LEFT_BUTTON_UP",
            Self::MSG_RAW_MOUSE_LEFT_CLICK => "MSG_RAW_MOUSE_LEFT_CLICK",
            Self::MSG_RAW_MOUSE_LEFT_DRAG => "MSG_RAW_MOUSE_LEFT_DRAG",
            Self::MSG_RAW_MOUSE_MIDDLE_BUTTON_DOWN => "MSG_RAW_MOUSE_MIDDLE_BUTTON_DOWN",
            Self::MSG_RAW_MOUSE_MIDDLE_DOUBLE_CLICK => "MSG_RAW_MOUSE_MIDDLE_DOUBLE_CLICK",
            Self::MSG_RAW_MOUSE_MIDDLE_BUTTON_UP => "MSG_RAW_MOUSE_MIDDLE_BUTTON_UP",
            Self::MSG_RAW_MOUSE_MIDDLE_DRAG => "MSG_RAW_MOUSE_MIDDLE_DRAG",
            Self::MSG_RAW_MOUSE_RIGHT_BUTTON_DOWN => "MSG_RAW_MOUSE_RIGHT_BUTTON_DOWN",
            Self::MSG_RAW_MOUSE_RIGHT_DOUBLE_CLICK => "MSG_RAW_MOUSE_RIGHT_DOUBLE_CLICK",
            Self::MSG_RAW_MOUSE_RIGHT_BUTTON_UP => "MSG_RAW_MOUSE_RIGHT_BUTTON_UP",
            Self::MSG_RAW_MOUSE_RIGHT_DRAG => "MSG_RAW_MOUSE_RIGHT_DRAG",
            Self::MSG_RAW_MOUSE_WHEEL => "MSG_RAW_MOUSE_WHEEL",
            Self::MSG_RAW_MOUSE_END => "MSG_RAW_MOUSE_END",
            Self::MSG_RAW_KEY_DOWN => "MSG_RAW_KEY_DOWN",
            Self::MSG_RAW_KEY_UP => "MSG_RAW_KEY_UP",
            Self::MSG_MOUSE_LEFT_CLICK => "MSG_MOUSE_LEFT_CLICK",
            Self::MSG_MOUSE_LEFT_DOUBLE_CLICK => "MSG_MOUSE_LEFT_DOUBLE_CLICK",
            Self::MSG_MOUSE_MIDDLE_CLICK => "MSG_MOUSE_MIDDLE_CLICK",
            Self::MSG_MOUSE_MIDDLE_DOUBLE_CLICK => "MSG_MOUSE_MIDDLE_DOUBLE_CLICK",
            Self::MSG_MOUSE_RIGHT_CLICK => "MSG_MOUSE_RIGHT_CLICK",
            Self::MSG_MOUSE_RIGHT_DOUBLE_CLICK => "MSG_MOUSE_RIGHT_DOUBLE_CLICK",
            Self::MSG_CLEAR_GAME_DATA => "MSG_CLEAR_GAME_DATA",
            Self::MSG_NEW_GAME => "MSG_NEW_GAME",
            Self::MSG_LOGIC_CRC_PLAYBACK => "MSG_LOGIC_CRC_PLAYBACK",
            Self::MSG_BEGIN_META_MESSAGES => "MSG_BEGIN_META_MESSAGES",
            Self::MSG_META_SAVE_VIEW1 => "MSG_META_SAVE_VIEW1",
            Self::MSG_META_SAVE_VIEW2 => "MSG_META_SAVE_VIEW2",
            Self::MSG_META_SAVE_VIEW3 => "MSG_META_SAVE_VIEW3",
            Self::MSG_META_SAVE_VIEW4 => "MSG_META_SAVE_VIEW4",
            Self::MSG_META_SAVE_VIEW5 => "MSG_META_SAVE_VIEW5",
            Self::MSG_META_SAVE_VIEW6 => "MSG_META_SAVE_VIEW6",
            Self::MSG_META_SAVE_VIEW7 => "MSG_META_SAVE_VIEW7",
            Self::MSG_META_SAVE_VIEW8 => "MSG_META_SAVE_VIEW8",
            Self::MSG_META_VIEW_VIEW1 => "MSG_META_VIEW_VIEW1",
            Self::MSG_META_VIEW_VIEW2 => "MSG_META_VIEW_VIEW2",
            Self::MSG_META_VIEW_VIEW3 => "MSG_META_VIEW_VIEW3",
            Self::MSG_META_VIEW_VIEW4 => "MSG_META_VIEW_VIEW4",
            Self::MSG_META_VIEW_VIEW5 => "MSG_META_VIEW_VIEW5",
            Self::MSG_META_VIEW_VIEW6 => "MSG_META_VIEW_VIEW6",
            Self::MSG_META_VIEW_VIEW7 => "MSG_META_VIEW_VIEW7",
            Self::MSG_META_VIEW_VIEW8 => "MSG_META_VIEW_VIEW8",
            Self::MSG_META_CREATE_TEAM0 => "MSG_META_CREATE_TEAM0",
            Self::MSG_META_CREATE_TEAM1 => "MSG_META_CREATE_TEAM1",
            Self::MSG_META_CREATE_TEAM2 => "MSG_META_CREATE_TEAM2",
            Self::MSG_META_CREATE_TEAM3 => "MSG_META_CREATE_TEAM3",
            Self::MSG_META_CREATE_TEAM4 => "MSG_META_CREATE_TEAM4",
            Self::MSG_META_CREATE_TEAM5 => "MSG_META_CREATE_TEAM5",
            Self::MSG_META_CREATE_TEAM6 => "MSG_META_CREATE_TEAM6",
            Self::MSG_META_CREATE_TEAM7 => "MSG_META_CREATE_TEAM7",
            Self::MSG_META_CREATE_TEAM8 => "MSG_META_CREATE_TEAM8",
            Self::MSG_META_CREATE_TEAM9 => "MSG_META_CREATE_TEAM9",
            Self::MSG_META_SELECT_TEAM0 => "MSG_META_SELECT_TEAM0",
            Self::MSG_META_SELECT_TEAM1 => "MSG_META_SELECT_TEAM1",
            Self::MSG_META_SELECT_TEAM2 => "MSG_META_SELECT_TEAM2",
            Self::MSG_META_SELECT_TEAM3 => "MSG_META_SELECT_TEAM3",
            Self::MSG_META_SELECT_TEAM4 => "MSG_META_SELECT_TEAM4",
            Self::MSG_META_SELECT_TEAM5 => "MSG_META_SELECT_TEAM5",
            Self::MSG_META_SELECT_TEAM6 => "MSG_META_SELECT_TEAM6",
            Self::MSG_META_SELECT_TEAM7 => "MSG_META_SELECT_TEAM7",
            Self::MSG_META_SELECT_TEAM8 => "MSG_META_SELECT_TEAM8",
            Self::MSG_META_SELECT_TEAM9 => "MSG_META_SELECT_TEAM9",
            Self::MSG_META_ADD_TEAM0 => "MSG_META_ADD_TEAM0",
            Self::MSG_META_ADD_TEAM1 => "MSG_META_ADD_TEAM1",
            Self::MSG_META_ADD_TEAM2 => "MSG_META_ADD_TEAM2",
            Self::MSG_META_ADD_TEAM3 => "MSG_META_ADD_TEAM3",
            Self::MSG_META_ADD_TEAM4 => "MSG_META_ADD_TEAM4",
            Self::MSG_META_ADD_TEAM5 => "MSG_META_ADD_TEAM5",
            Self::MSG_META_ADD_TEAM6 => "MSG_META_ADD_TEAM6",
            Self::MSG_META_ADD_TEAM7 => "MSG_META_ADD_TEAM7",
            Self::MSG_META_ADD_TEAM8 => "MSG_META_ADD_TEAM8",
            Self::MSG_META_ADD_TEAM9 => "MSG_META_ADD_TEAM9",
            Self::MSG_META_VIEW_TEAM0 => "MSG_META_VIEW_TEAM0",
            Self::MSG_META_VIEW_TEAM1 => "MSG_META_VIEW_TEAM1",
            Self::MSG_META_VIEW_TEAM2 => "MSG_META_VIEW_TEAM2",
            Self::MSG_META_VIEW_TEAM3 => "MSG_META_VIEW_TEAM3",
            Self::MSG_META_VIEW_TEAM4 => "MSG_META_VIEW_TEAM4",
            Self::MSG_META_VIEW_TEAM5 => "MSG_META_VIEW_TEAM5",
            Self::MSG_META_VIEW_TEAM6 => "MSG_META_VIEW_TEAM6",
            Self::MSG_META_VIEW_TEAM7 => "MSG_META_VIEW_TEAM7",
            Self::MSG_META_VIEW_TEAM8 => "MSG_META_VIEW_TEAM8",
            Self::MSG_META_VIEW_TEAM9 => "MSG_META_VIEW_TEAM9",
            Self::MSG_META_SELECT_MATCHING_UNITS => "MSG_META_SELECT_MATCHING_UNITS",
            Self::MSG_META_SELECT_NEXT_UNIT => "MSG_META_SELECT_NEXT_UNIT",
            Self::MSG_META_SELECT_PREV_UNIT => "MSG_META_SELECT_PREV_UNIT",
            Self::MSG_META_SELECT_NEXT_WORKER => "MSG_META_SELECT_NEXT_WORKER",
            Self::MSG_META_SELECT_PREV_WORKER => "MSG_META_SELECT_PREV_WORKER",
            Self::MSG_META_SELECT_NEXT_IDLE_WORKER => "MSG_META_SELECT_NEXT_IDLE_WORKER",
            Self::MSG_META_VIEW_COMMAND_CENTER => "MSG_META_VIEW_COMMAND_CENTER",
            Self::MSG_META_VIEW_LAST_RADAR_EVENT => "MSG_META_VIEW_LAST_RADAR_EVENT",
            Self::MSG_META_SELECT_HERO => "MSG_META_SELECT_HERO",
            Self::MSG_META_SELECT_ALL => "MSG_META_SELECT_ALL",
            Self::MSG_META_SELECT_ALL_AIRCRAFT => "MSG_META_SELECT_ALL_AIRCRAFT",
            Self::MSG_META_SCATTER => "MSG_META_SCATTER",
            Self::MSG_META_STOP => "MSG_META_STOP",
            Self::MSG_META_DEPLOY => "MSG_META_DEPLOY",
            Self::MSG_META_CREATE_FORMATION => "MSG_META_CREATE_FORMATION",
            Self::MSG_META_FOLLOW => "MSG_META_FOLLOW",
            Self::MSG_META_CHAT_PLAYERS => "MSG_META_CHAT_PLAYERS",
            Self::MSG_META_CHAT_ALLIES => "MSG_META_CHAT_ALLIES",
            Self::MSG_META_CHAT_EVERYONE => "MSG_META_CHAT_EVERYONE",
            Self::MSG_META_DIPLOMACY => "MSG_META_DIPLOMACY",
            Self::MSG_META_OPTIONS => "MSG_META_OPTIONS",
            Self::MSG_META_INCREASE_MAX_RENDER_FPS => "MSG_META_INCREASE_MAX_RENDER_FPS",
            Self::MSG_META_DECREASE_MAX_RENDER_FPS => "MSG_META_DECREASE_MAX_RENDER_FPS",
            Self::MSG_META_INCREASE_LOGIC_TIME_SCALE => "MSG_META_INCREASE_LOGIC_TIME_SCALE",
            Self::MSG_META_DECREASE_LOGIC_TIME_SCALE => "MSG_META_DECREASE_LOGIC_TIME_SCALE",
            Self::MSG_META_TOGGLE_LOWER_DETAILS => "MSG_META_TOGGLE_LOWER_DETAILS",
            Self::MSG_META_TOGGLE_CONTROL_BAR => "MSG_META_TOGGLE_CONTROL_BAR",
            Self::MSG_META_TOGGLE_PLAYER_OBSERVER => "MSG_META_TOGGLE_PLAYER_OBSERVER",
            Self::MSG_META_BEGIN_PATH_BUILD => "MSG_META_BEGIN_PATH_BUILD",
            Self::MSG_META_END_PATH_BUILD => "MSG_META_END_PATH_BUILD",
            Self::MSG_META_BEGIN_FORCEATTACK => "MSG_META_BEGIN_FORCEATTACK",
            Self::MSG_META_END_FORCEATTACK => "MSG_META_END_FORCEATTACK",
            Self::MSG_META_BEGIN_FORCEMOVE => "MSG_META_BEGIN_FORCEMOVE",
            Self::MSG_META_END_FORCEMOVE => "MSG_META_END_FORCEMOVE",
            Self::MSG_META_BEGIN_WAYPOINTS => "MSG_META_BEGIN_WAYPOINTS",
            Self::MSG_META_END_WAYPOINTS => "MSG_META_END_WAYPOINTS",
            Self::MSG_META_BEGIN_PREFER_SELECTION => "MSG_META_BEGIN_PREFER_SELECTION",
            Self::MSG_META_END_PREFER_SELECTION => "MSG_META_END_PREFER_SELECTION",
            Self::MSG_META_TAKE_SCREENSHOT => "MSG_META_TAKE_SCREENSHOT",
            Self::MSG_META_TAKE_SCREENSHOT_PNG => "MSG_META_TAKE_SCREENSHOT_PNG",
            Self::MSG_META_ALL_CHEER => "MSG_META_ALL_CHEER",
            Self::MSG_META_TOGGLE_ATTACKMOVE => "MSG_META_TOGGLE_ATTACKMOVE",
            Self::MSG_META_BEGIN_CAMERA_ROTATE_LEFT => "MSG_META_BEGIN_CAMERA_ROTATE_LEFT",
            Self::MSG_META_END_CAMERA_ROTATE_LEFT => "MSG_META_END_CAMERA_ROTATE_LEFT",
            Self::MSG_META_ALT_CAMERA_ROTATE_LEFT => "MSG_META_ALT_CAMERA_ROTATE_LEFT",
            Self::MSG_META_BEGIN_CAMERA_ROTATE_RIGHT => "MSG_META_BEGIN_CAMERA_ROTATE_RIGHT",
            Self::MSG_META_END_CAMERA_ROTATE_RIGHT => "MSG_META_END_CAMERA_ROTATE_RIGHT",
            Self::MSG_META_ALT_CAMERA_ROTATE_RIGHT => "MSG_META_ALT_CAMERA_ROTATE_RIGHT",
            Self::MSG_META_BEGIN_CAMERA_ZOOM_IN => "MSG_META_BEGIN_CAMERA_ZOOM_IN",
            Self::MSG_META_END_CAMERA_ZOOM_IN => "MSG_META_END_CAMERA_ZOOM_IN",
            Self::MSG_META_BEGIN_CAMERA_ZOOM_OUT => "MSG_META_BEGIN_CAMERA_ZOOM_OUT",
            Self::MSG_META_END_CAMERA_ZOOM_OUT => "MSG_META_END_CAMERA_ZOOM_OUT",
            Self::MSG_META_CAMERA_RESET => "MSG_META_CAMERA_RESET",
            Self::MSG_META_TOGGLE_CAMERA_TRACKING_DRAWABLE => {
                "MSG_META_TOGGLE_CAMERA_TRACKING_DRAWABLE"
            }
            Self::MSG_META_TOGGLE_FAST_FORWARD_REPLAY => "MSG_META_TOGGLE_FAST_FORWARD_REPLAY",
            Self::MSG_META_TOGGLE_PAUSE => "MSG_META_TOGGLE_PAUSE",
            Self::MSG_META_TOGGLE_PAUSE_ALT => "MSG_META_TOGGLE_PAUSE_ALT",
            Self::MSG_META_STEP_FRAME => "MSG_META_STEP_FRAME",
            Self::MSG_META_STEP_FRAME_ALT => "MSG_META_STEP_FRAME_ALT",
            Self::MSG_META_DEMO_INSTANT_QUIT => "MSG_META_DEMO_INSTANT_QUIT",
            Self::MSG_META_PLACE_BEACON => "MSG_META_PLACE_BEACON",
            Self::MSG_META_REMOVE_BEACON => "MSG_META_REMOVE_BEACON",
            Self::MSG_END_META_MESSAGES => "MSG_END_META_MESSAGES",
            Self::MSG_MOUSEOVER_DRAWABLE_HINT => "MSG_MOUSEOVER_DRAWABLE_HINT",
            Self::MSG_MOUSEOVER_LOCATION_HINT => "MSG_MOUSEOVER_LOCATION_HINT",
            Self::MSG_VALID_GUICOMMAND_HINT => "MSG_VALID_GUICOMMAND_HINT",
            Self::MSG_INVALID_GUICOMMAND_HINT => "MSG_INVALID_GUICOMMAND_HINT",
            Self::MSG_BEGIN_AREA_SELECTION_HINT => "MSG_BEGIN_AREA_SELECTION_HINT",
            Self::MSG_END_AREA_SELECTION_HINT => "MSG_END_AREA_SELECTION_HINT",
            Self::MSG_DO_ATTACK_OBJECT_HINT => "MSG_DO_ATTACK_OBJECT_HINT",
            Self::MSG_IMPOSSIBLE_ATTACK_HINT => "MSG_IMPOSSIBLE_ATTACK_HINT",
            Self::MSG_DO_FORCE_ATTACK_OBJECT_HINT => "MSG_DO_FORCE_ATTACK_OBJECT_HINT",
            Self::MSG_DO_FORCE_ATTACK_GROUND_HINT => "MSG_DO_FORCE_ATTACK_GROUND_HINT",
            Self::MSG_GET_REPAIRED_HINT => "MSG_GET_REPAIRED_HINT",
            Self::MSG_GET_HEALED_HINT => "MSG_GET_HEALED_HINT",
            Self::MSG_DO_REPAIR_HINT => "MSG_DO_REPAIR_HINT",
            Self::MSG_RESUME_CONSTRUCTION_HINT => "MSG_RESUME_CONSTRUCTION_HINT",
            Self::MSG_ENTER_HINT => "MSG_ENTER_HINT",
            Self::MSG_DOCK_HINT => "MSG_DOCK_HINT",
            Self::MSG_DO_MOVETO_HINT => "MSG_DO_MOVETO_HINT",
            Self::MSG_DO_ATTACKMOVETO_HINT => "MSG_DO_ATTACKMOVETO_HINT",
            Self::MSG_ADD_WAYPOINT_HINT => "MSG_ADD_WAYPOINT_HINT",
            Self::MSG_HIJACK_HINT => "MSG_HIJACK_HINT",
            Self::MSG_SABOTAGE_HINT => "MSG_SABOTAGE_HINT",
            Self::MSG_FIREBOMB_HINT => "MSG_FIREBOMB_HINT",
            Self::MSG_CONVERT_TO_CARBOMB_HINT => "MSG_CONVERT_TO_CARBOMB_HINT",
            Self::MSG_CAPTUREBUILDING_HINT => "MSG_CAPTUREBUILDING_HINT",
            Self::MSG_SNIPE_VEHICLE_HINT => "MSG_SNIPE_VEHICLE_HINT",
            Self::MSG_DEFECTOR_HINT => "MSG_DEFECTOR_HINT",
            Self::MSG_SET_RALLY_POINT_HINT => "MSG_SET_RALLY_POINT_HINT",
            Self::MSG_DO_SPECIAL_POWER_OVERRIDE_DESTINATION_HINT => {
                "MSG_DO_SPECIAL_POWER_OVERRIDE_DESTINATION_HINT"
            }
            Self::MSG_DO_SALVAGE_HINT => "MSG_DO_SALVAGE_HINT",
            Self::MSG_DO_INVALID_HINT => "MSG_DO_INVALID_HINT",
            Self::MSG_DO_ATTACK_OBJECT_AFTER_MOVING_HINT => {
                "MSG_DO_ATTACK_OBJECT_AFTER_MOVING_HINT"
            }
            Self::MSG_HACK_HINT => "MSG_HACK_HINT",
            Self::MSG_BEGIN_NETWORK_MESSAGES => "MSG_BEGIN_NETWORK_MESSAGES",
            Self::MSG_CREATE_SELECTED_GROUP => "MSG_CREATE_SELECTED_GROUP",
            Self::MSG_CREATE_SELECTED_GROUP_NO_SOUND => "MSG_CREATE_SELECTED_GROUP_NO_SOUND",
            Self::MSG_DESTROY_SELECTED_GROUP => "MSG_DESTROY_SELECTED_GROUP",
            Self::MSG_REMOVE_FROM_SELECTED_GROUP => "MSG_REMOVE_FROM_SELECTED_GROUP",
            Self::MSG_SELECTED_GROUP_COMMAND => "MSG_SELECTED_GROUP_COMMAND",
            Self::MSG_CREATE_TEAM0 => "MSG_CREATE_TEAM0",
            Self::MSG_CREATE_TEAM1 => "MSG_CREATE_TEAM1",
            Self::MSG_CREATE_TEAM2 => "MSG_CREATE_TEAM2",
            Self::MSG_CREATE_TEAM3 => "MSG_CREATE_TEAM3",
            Self::MSG_CREATE_TEAM4 => "MSG_CREATE_TEAM4",
            Self::MSG_CREATE_TEAM5 => "MSG_CREATE_TEAM5",
            Self::MSG_CREATE_TEAM6 => "MSG_CREATE_TEAM6",
            Self::MSG_CREATE_TEAM7 => "MSG_CREATE_TEAM7",
            Self::MSG_CREATE_TEAM8 => "MSG_CREATE_TEAM8",
            Self::MSG_CREATE_TEAM9 => "MSG_CREATE_TEAM9",
            Self::MSG_SELECT_TEAM0 => "MSG_SELECT_TEAM0",
            Self::MSG_SELECT_TEAM1 => "MSG_SELECT_TEAM1",
            Self::MSG_SELECT_TEAM2 => "MSG_SELECT_TEAM2",
            Self::MSG_SELECT_TEAM3 => "MSG_SELECT_TEAM3",
            Self::MSG_SELECT_TEAM4 => "MSG_SELECT_TEAM4",
            Self::MSG_SELECT_TEAM5 => "MSG_SELECT_TEAM5",
            Self::MSG_SELECT_TEAM6 => "MSG_SELECT_TEAM6",
            Self::MSG_SELECT_TEAM7 => "MSG_SELECT_TEAM7",
            Self::MSG_SELECT_TEAM8 => "MSG_SELECT_TEAM8",
            Self::MSG_SELECT_TEAM9 => "MSG_SELECT_TEAM9",
            Self::MSG_ADD_TEAM0 => "MSG_ADD_TEAM0",
            Self::MSG_ADD_TEAM1 => "MSG_ADD_TEAM1",
            Self::MSG_ADD_TEAM2 => "MSG_ADD_TEAM2",
            Self::MSG_ADD_TEAM3 => "MSG_ADD_TEAM3",
            Self::MSG_ADD_TEAM4 => "MSG_ADD_TEAM4",
            Self::MSG_ADD_TEAM5 => "MSG_ADD_TEAM5",
            Self::MSG_ADD_TEAM6 => "MSG_ADD_TEAM6",
            Self::MSG_ADD_TEAM7 => "MSG_ADD_TEAM7",
            Self::MSG_ADD_TEAM8 => "MSG_ADD_TEAM8",
            Self::MSG_ADD_TEAM9 => "MSG_ADD_TEAM9",
            Self::MSG_DO_ATTACKSQUAD => "MSG_DO_ATTACKSQUAD",
            Self::MSG_DO_WEAPON => "MSG_DO_WEAPON",
            Self::MSG_DO_WEAPON_AT_LOCATION => "MSG_DO_WEAPON_AT_LOCATION",
            Self::MSG_DO_WEAPON_AT_OBJECT => "MSG_DO_WEAPON_AT_OBJECT",
            Self::MSG_DO_SPECIAL_POWER => "MSG_DO_SPECIAL_POWER",
            Self::MSG_DO_SPECIAL_POWER_AT_LOCATION => "MSG_DO_SPECIAL_POWER_AT_LOCATION",
            Self::MSG_DO_SPECIAL_POWER_AT_OBJECT => "MSG_DO_SPECIAL_POWER_AT_OBJECT",
            Self::MSG_SET_RALLY_POINT => "MSG_SET_RALLY_POINT",
            Self::MSG_PURCHASE_SCIENCE => "MSG_PURCHASE_SCIENCE",
            Self::MSG_QUEUE_UPGRADE => "MSG_QUEUE_UPGRADE",
            Self::MSG_CANCEL_UPGRADE => "MSG_CANCEL_UPGRADE",
            Self::MSG_QUEUE_UNIT_CREATE => "MSG_QUEUE_UNIT_CREATE",
            Self::MSG_CANCEL_UNIT_CREATE => "MSG_CANCEL_UNIT_CREATE",
            Self::MSG_DOZER_CONSTRUCT => "MSG_DOZER_CONSTRUCT",
            Self::MSG_DOZER_CONSTRUCT_LINE => "MSG_DOZER_CONSTRUCT_LINE",
            Self::MSG_DOZER_CANCEL_CONSTRUCT => "MSG_DOZER_CANCEL_CONSTRUCT",
            Self::MSG_SELL => "MSG_SELL",
            Self::MSG_EXIT => "MSG_EXIT",
            Self::MSG_EVACUATE => "MSG_EVACUATE",
            Self::MSG_EXECUTE_RAILED_TRANSPORT => "MSG_EXECUTE_RAILED_TRANSPORT",
            Self::MSG_COMBATDROP_AT_LOCATION => "MSG_COMBATDROP_AT_LOCATION",
            Self::MSG_COMBATDROP_AT_OBJECT => "MSG_COMBATDROP_AT_OBJECT",
            Self::MSG_AREA_SELECTION_DEPRECATED => "MSG_AREA_SELECTION_DEPRECATED",
            Self::MSG_DO_ATTACK_OBJECT => "MSG_DO_ATTACK_OBJECT",
            Self::MSG_DO_FORCE_ATTACK_OBJECT => "MSG_DO_FORCE_ATTACK_OBJECT",
            Self::MSG_DO_FORCE_ATTACK_GROUND => "MSG_DO_FORCE_ATTACK_GROUND",
            Self::MSG_GET_REPAIRED => "MSG_GET_REPAIRED",
            Self::MSG_GET_HEALED => "MSG_GET_HEALED",
            Self::MSG_DO_REPAIR => "MSG_DO_REPAIR",
            Self::MSG_RESUME_CONSTRUCTION => "MSG_RESUME_CONSTRUCTION",
            Self::MSG_ENTER => "MSG_ENTER",
            Self::MSG_DOCK => "MSG_DOCK",
            Self::MSG_DO_MOVETO => "MSG_DO_MOVETO",
            Self::MSG_DO_ATTACKMOVETO => "MSG_DO_ATTACKMOVETO",
            Self::MSG_DO_FORCEMOVETO => "MSG_DO_FORCEMOVETO",
            Self::MSG_ADD_WAYPOINT => "MSG_ADD_WAYPOINT",
            Self::MSG_DO_GUARD_POSITION => "MSG_DO_GUARD_POSITION",
            Self::MSG_DO_GUARD_OBJECT => "MSG_DO_GUARD_OBJECT",
            Self::MSG_DO_STOP => "MSG_DO_STOP",
            Self::MSG_DO_SCATTER => "MSG_DO_SCATTER",
            Self::MSG_INTERNET_HACK => "MSG_INTERNET_HACK",
            Self::MSG_DO_CHEER => "MSG_DO_CHEER",
            Self::MSG_TOGGLE_OVERCHARGE => "MSG_TOGGLE_OVERCHARGE",
            Self::MSG_SWITCH_WEAPONS => "MSG_SWITCH_WEAPONS",
            Self::MSG_CONVERT_TO_CARBOMB => "MSG_CONVERT_TO_CARBOMB",
            Self::MSG_CAPTUREBUILDING => "MSG_CAPTUREBUILDING",
            Self::MSG_DISABLEVEHICLE_HACK => "MSG_DISABLEVEHICLE_HACK",
            Self::MSG_STEALCASH_HACK => "MSG_STEALCASH_HACK",
            Self::MSG_DISABLEBUILDING_HACK => "MSG_DISABLEBUILDING_HACK",
            Self::MSG_SNIPE_VEHICLE => "MSG_SNIPE_VEHICLE",
            Self::MSG_DO_SPECIAL_POWER_OVERRIDE_DESTINATION => {
                "MSG_DO_SPECIAL_POWER_OVERRIDE_DESTINATION"
            }
            Self::MSG_DO_SALVAGE => "MSG_DO_SALVAGE",
            Self::MSG_CLEAR_INGAME_POPUP_MESSAGE => "MSG_CLEAR_INGAME_POPUP_MESSAGE",
            Self::MSG_PLACE_BEACON => "MSG_PLACE_BEACON",
            Self::MSG_REMOVE_BEACON => "MSG_REMOVE_BEACON",
            Self::MSG_SET_BEACON_TEXT => "MSG_SET_BEACON_TEXT",
            Self::MSG_SET_REPLAY_CAMERA => "MSG_SET_REPLAY_CAMERA",
            Self::MSG_SELF_DESTRUCT => "MSG_SELF_DESTRUCT",
            Self::MSG_CREATE_FORMATION => "MSG_CREATE_FORMATION",
            Self::MSG_LOGIC_CRC => "MSG_LOGIC_CRC",
            Self::MSG_SET_MINE_CLEARING_DETAIL => "MSG_SET_MINE_CLEARING_DETAIL",
            Self::MSG_ENABLE_RETALIATION_MODE => "MSG_ENABLE_RETALIATION_MODE",
            Self::MSG_BEGIN_DEBUG_NETWORK_MESSAGES => "MSG_BEGIN_DEBUG_NETWORK_MESSAGES",
            Self::MSG_END_NETWORK_MESSAGES => "MSG_END_NETWORK_MESSAGES",
            Self::MSG_TIMESTAMP => "MSG_TIMESTAMP",
            Self::MSG_OBJECT_CREATED => "MSG_OBJECT_CREATED",
            Self::MSG_OBJECT_DESTROYED => "MSG_OBJECT_DESTROYED",
            Self::MSG_OBJECT_POSITION => "MSG_OBJECT_POSITION",
            Self::MSG_OBJECT_ORIENTATION => "MSG_OBJECT_ORIENTATION",
            Self::MSG_OBJECT_JOINED_TEAM => "MSG_OBJECT_JOINED_TEAM",
        }
    }
}

// END GENERATED enum block

/// `GameMessageArgumentDataType` (MessageStream.h): the argument's type
/// tag. The ordinals are the C++ enum's.
#[allow(non_camel_case_types)]
#[repr(i32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArgumentDataType {
    /// `ARGUMENTDATATYPE_INTEGER`.
    ARGUMENTDATATYPE_INTEGER = 0,
    /// `ARGUMENTDATATYPE_REAL`.
    ARGUMENTDATATYPE_REAL = 1,
    /// `ARGUMENTDATATYPE_BOOLEAN`.
    ARGUMENTDATATYPE_BOOLEAN = 2,
    /// `ARGUMENTDATATYPE_OBJECTID`.
    ARGUMENTDATATYPE_OBJECTID = 3,
    /// `ARGUMENTDATATYPE_DRAWABLEID`.
    ARGUMENTDATATYPE_DRAWABLEID = 4,
    /// `ARGUMENTDATATYPE_TEAMID`.
    ARGUMENTDATATYPE_TEAMID = 5,
    /// `ARGUMENTDATATYPE_LOCATION`.
    ARGUMENTDATATYPE_LOCATION = 6,
    /// `ARGUMENTDATATYPE_PIXEL`.
    ARGUMENTDATATYPE_PIXEL = 7,
    /// `ARGUMENTDATATYPE_PIXELREGION`.
    ARGUMENTDATATYPE_PIXELREGION = 8,
    /// `ARGUMENTDATATYPE_TIMESTAMP`.
    ARGUMENTDATATYPE_TIMESTAMP = 9,
    /// `ARGUMENTDATATYPE_WIDECHAR`.
    ARGUMENTDATATYPE_WIDECHAR = 10,
    /// `ARGUMENTDATATYPE_UNKNOWN` — the out-of-range answer of
    /// `getArgumentDataType`.
    #[default]
    ARGUMENTDATATYPE_UNKNOWN = 11,
}

impl ArgumentDataType {
    /// The original's ordinal (the C++ enum's value).
    #[must_use]
    pub fn ordinal(self) -> i32 {
        self as i32
    }
}

/// `GameMessageArgumentType` (MessageStream.h): the argument union,
/// modelled as its four 32-bit words so every member read re-reads the
/// same bytes the original's union holds (the typed accessors below;
/// a punned read — a Real stored, an Integer fetched — returns the
/// reinterpreted bits, exactly like the union). The largest member is
/// `IRegion2D` (four `Int`s); `getArgument`'s out-of-range answer is the
/// all-zero union (`static const GameMessageArgumentType zero = { 0 }`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameMessageArgument {
    /// `m_type`.
    pub data_type: ArgumentDataType,
    /// The union's payload, word-interpreted (little-endian layout of
    /// the members matches the goldens' generating platform; the typed
    /// reads are layout-independent).
    words: [u32; 4],
}

impl Default for GameMessageArgument {
    fn default() -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_UNKNOWN,
            words: [0; 4],
        }
    }
}

impl GameMessageArgument {
    /// The all-zero argument (`getArgument`'s out-of-range answer).
    #[must_use]
    pub fn zero() -> Self {
        Self::default()
    }

    /// `ARGUMENTDATATYPE_INTEGER` — `Int integer`.
    #[must_use]
    pub fn new_integer(v: i32) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_INTEGER,
            words: [v as u32, 0, 0, 0],
        }
    }

    /// `ARGUMENTDATATYPE_REAL` — `Real real`.
    #[must_use]
    pub fn new_real(v: f32) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_REAL,
            words: [v.to_bits(), 0, 0, 0],
        }
    }

    /// `ARGUMENTDATATYPE_BOOLEAN` — `Bool boolean`.
    #[must_use]
    pub fn new_boolean(v: bool) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_BOOLEAN,
            words: [u32::from(v), 0, 0, 0],
        }
    }

    /// `ARGUMENTDATATYPE_OBJECTID` — `ObjectID objectID`.
    #[must_use]
    pub fn new_object_id(v: ObjectID) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_OBJECTID,
            words: [v as u32, 0, 0, 0],
        }
    }

    /// `ARGUMENTDATATYPE_DRAWABLEID` — `DrawableID drawableID`.
    #[must_use]
    pub fn new_drawable_id(v: DrawableID) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_DRAWABLEID,
            words: [v as u32, 0, 0, 0],
        }
    }

    /// `ARGUMENTDATATYPE_TEAMID` — `UnsignedInt teamID`.
    #[must_use]
    pub fn new_team_id(v: u32) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_TEAMID,
            words: [v, 0, 0, 0],
        }
    }

    /// `ARGUMENTDATATYPE_LOCATION` — `Coord3D location`.
    #[must_use]
    pub fn new_location(v: Coord3) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_LOCATION,
            words: [v.x.to_bits(), v.y.to_bits(), v.z.to_bits(), 0],
        }
    }

    /// `ARGUMENTDATATYPE_PIXEL` — `ICoord2D pixel`.
    #[must_use]
    pub fn new_pixel(v: ICoord2) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_PIXEL,
            words: [v.x as u32, v.y as u32, 0, 0],
        }
    }

    /// `ARGUMENTDATATYPE_PIXELREGION` — `IRegion2D pixelRegion`.
    #[must_use]
    pub fn new_pixel_region(v: IRegion2) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_PIXELREGION,
            words: [v.lo.x as u32, v.lo.y as u32, v.hi.x as u32, v.hi.y as u32],
        }
    }

    /// `ARGUMENTDATATYPE_TIMESTAMP` — `UnsignedInt timestamp`.
    #[must_use]
    pub fn new_timestamp(v: u32) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_TIMESTAMP,
            words: [v, 0, 0, 0],
        }
    }

    /// `ARGUMENTDATATYPE_WIDECHAR` — `WideChar wChar` (the 16-bit
    /// `unichar`; the upper half of the word stays zero as the
    /// two-byte member leaves it).
    #[must_use]
    pub fn new_wide_char(v: u16) -> Self {
        Self {
            data_type: ArgumentDataType::ARGUMENTDATATYPE_WIDECHAR,
            words: [u32::from(v), 0, 0, 0],
        }
    }

    /// Read `integer` — the union member.
    #[must_use]
    pub fn integer(&self) -> i32 {
        self.words[0] as i32
    }

    /// Read `real` — the union member.
    #[must_use]
    pub fn real(&self) -> f32 {
        f32::from_bits(self.words[0])
    }

    /// Read `boolean` — the union member (`Bool`, read as a word).
    #[must_use]
    pub fn boolean(&self) -> bool {
        self.words[0] != 0
    }

    /// Read `objectID` — the union member.
    #[must_use]
    pub fn object_id(&self) -> ObjectID {
        self.words[0] as i32
    }

    /// Read `drawableID` — the union member.
    #[must_use]
    pub fn drawable_id(&self) -> DrawableID {
        self.words[0] as i32
    }

    /// Read `teamID` — the union member.
    #[must_use]
    pub fn team_id(&self) -> u32 {
        self.words[0]
    }

    /// Read `location` — the union member.
    #[must_use]
    pub fn location(&self) -> Coord3 {
        Coord3 {
            x: f32::from_bits(self.words[0]),
            y: f32::from_bits(self.words[1]),
            z: f32::from_bits(self.words[2]),
        }
    }

    /// Read `pixel` — the union member.
    #[must_use]
    pub fn pixel(&self) -> ICoord2 {
        ICoord2 {
            x: self.words[0] as i32,
            y: self.words[1] as i32,
        }
    }

    /// Read `pixelRegion` — the union member.
    #[must_use]
    pub fn pixel_region(&self) -> IRegion2 {
        IRegion2 {
            lo: ICoord2 {
                x: self.words[0] as i32,
                y: self.words[1] as i32,
            },
            hi: ICoord2 {
                x: self.words[2] as i32,
                y: self.words[3] as i32,
            },
        }
    }

    /// Read `timestamp` — the union member.
    #[must_use]
    pub fn timestamp(&self) -> u32 {
        self.words[0]
    }

    /// Read `wChar` — the union member (the low 16 bits).
    #[must_use]
    pub fn wide_char(&self) -> u16 {
        (self.words[0] & 0xffff) as u16
    }
}

/// A message handle into the [`MessageArena`] (the original's
/// `GameMessage*`; slots are never reused, so the handle is stable).
pub type MessageID = u32;

/// Which list a message is on (`m_list`; `None` = detached).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageListKind {
    /// `TheMessageStream`.
    Stream,
    /// `TheCommandList`.
    CommandList,
}

/// `GameMessage` (MessageStream.h): the type, the issuing player, the
/// arguments and the list links. Constructed through the stream's
/// [`MessageStream::append_message`] (the ctor's local-player stamp) or
/// the arena directly (the corpus surface).
#[derive(Clone, Debug)]
pub struct GameMessage {
    /// `m_type`.
    pub ty: GameMessageType,
    /// `m_playerIndex` (the Player who issued the command — the ctor's
    /// `ThePlayerList->getLocalPlayer()->getPlayerIndex()` stamp).
    pub player_index: i32,
    /// `m_argList`.
    pub args: Vec<GameMessageArgument>,
    /// `m_next`.
    pub(crate) next: Option<MessageID>,
    /// `m_prev`.
    pub(crate) prev: Option<MessageID>,
    /// `m_list`.
    pub(crate) list: Option<MessageListKind>,
}

impl GameMessage {
    /// `next()` — the list link.
    #[must_use]
    pub fn next(&self) -> Option<MessageID> {
        self.next
    }

    /// `prev()` — the list link.
    #[must_use]
    pub fn prev(&self) -> Option<MessageID> {
        self.prev
    }

    /// `getType()`.
    #[must_use]
    pub fn get_type(&self) -> GameMessageType {
        self.ty
    }

    /// `getPlayerIndex()`.
    #[must_use]
    pub fn get_player_index(&self) -> i32 {
        self.player_index
    }

    /// `getArgumentCount()` — the `UnsignedByte` cast of the vector
    /// size (the 255-argument domain contract: the port refuses the
    /// 256th append, where the original's release build would wrap).
    #[must_use]
    pub fn get_argument_count(&self) -> u8 {
        self.args.len() as u8
    }

    /// `getArgument(argIndex)` — the out-of-range answer is the
    /// all-zero argument (the original's `static zero` after the
    /// debug crash).
    #[must_use]
    pub fn get_argument(&self, arg_index: i32) -> GameMessageArgument {
        if let Some(i) = usize::try_from(arg_index)
            .ok()
            .filter(|&i| i < self.args.len())
        {
            self.args[i]
        } else {
            GameMessageArgument::zero()
        }
    }

    /// `getArgumentDataType(argIndex)` — `ARGUMENTDATATYPE_UNKNOWN`
    /// out of range.
    #[must_use]
    pub fn get_argument_data_type(&self, arg_index: i32) -> ArgumentDataType {
        if let Some(i) = usize::try_from(arg_index)
            .ok()
            .filter(|&i| i < self.args.len())
        {
            self.args[i].data_type
        } else {
            ArgumentDataType::ARGUMENTDATATYPE_UNKNOWN
        }
    }

    /// `getCommandAsString()` — the debug name.
    #[must_use]
    pub fn get_command_as_string(&self) -> &'static str {
        self.ty.name()
    }

    // ---- the append family (MessageStream.cpp) ----

    fn alloc_arg(&mut self) -> &mut GameMessageArgument {
        // "If a GameMessage needs more than 255 arguments, it needs to be
        // split up into multiple GameMessage's." (the DEBUG_ASSERTCRASH
        // contract; the port refuses the domain outright)
        assert!(
            self.args.len() < 255,
            "a GameMessage needs at most 255 arguments"
        );
        self.args.push(GameMessageArgument::zero());
        self.args.last_mut().expect("just pushed")
    }

    /// `appendIntegerArgument`.
    pub fn append_integer_argument(&mut self, arg: i32) {
        *self.alloc_arg() = GameMessageArgument::new_integer(arg);
    }

    /// `appendRealArgument`.
    pub fn append_real_argument(&mut self, arg: f32) {
        *self.alloc_arg() = GameMessageArgument::new_real(arg);
    }

    /// `appendBooleanArgument`.
    pub fn append_boolean_argument(&mut self, arg: bool) {
        *self.alloc_arg() = GameMessageArgument::new_boolean(arg);
    }

    /// `appendObjectIDArgument`.
    pub fn append_object_id_argument(&mut self, arg: ObjectID) {
        *self.alloc_arg() = GameMessageArgument::new_object_id(arg);
    }

    /// `appendDrawableIDArgument`.
    pub fn append_drawable_id_argument(&mut self, arg: DrawableID) {
        *self.alloc_arg() = GameMessageArgument::new_drawable_id(arg);
    }

    /// `appendTeamIDArgument`.
    pub fn append_team_id_argument(&mut self, arg: u32) {
        *self.alloc_arg() = GameMessageArgument::new_team_id(arg);
    }

    /// `appendLocationArgument`.
    pub fn append_location_argument(&mut self, arg: Coord3) {
        *self.alloc_arg() = GameMessageArgument::new_location(arg);
    }

    /// `appendPixelArgument`.
    pub fn append_pixel_argument(&mut self, arg: ICoord2) {
        *self.alloc_arg() = GameMessageArgument::new_pixel(arg);
    }

    /// `appendPixelRegionArgument`.
    pub fn append_pixel_region_argument(&mut self, arg: IRegion2) {
        *self.alloc_arg() = GameMessageArgument::new_pixel_region(arg);
    }

    /// `appendTimestampArgument`.
    pub fn append_timestamp_argument(&mut self, arg: u32) {
        *self.alloc_arg() = GameMessageArgument::new_timestamp(arg);
    }

    /// `appendWideCharArgument`.
    pub fn append_wide_char_argument(&mut self, arg: u16) {
        *self.alloc_arg() = GameMessageArgument::new_wide_char(arg);
    }

    /// The data snapshot the host hooks see (the dispatch side of the
    /// borrow boundary).
    #[must_use]
    pub fn data(&self) -> MessageData {
        MessageData {
            ty: self.ty,
            player_index: self.player_index,
            args: self.args.clone(),
        }
    }
}

/// The read-only half of a message the host hooks receive.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageData {
    /// `getType()`.
    pub ty: GameMessageType,
    /// `getPlayerIndex()`.
    pub player_index: i32,
    /// The arguments.
    pub args: Vec<GameMessageArgument>,
}

impl MessageData {
    /// `getArgument(0)` read as the union's integer (the dispatch
    /// handlers' ubiquitous first read).
    #[must_use]
    pub fn arg0_integer(&self) -> i32 {
        self.args.first().map_or(0, |a| a.integer())
    }
}

/// `GameMessageDisposition` (MessageStream.h).
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameMessageDisposition {
    /// `KEEP_MESSAGE` — continue processing this message thru other
    /// translators.
    #[default]
    KEEP_MESSAGE,
    /// `DESTROY_MESSAGE` — destroy this message immediately.
    DESTROY_MESSAGE,
}

/// One message a translator posts mid-propagation
/// (`TheMessageStream->appendMessage` from inside
/// `translateGameMessage` — "the client translators above can also
/// create game messages"; the post lands on the stream's tail with the
/// ctor's local-player stamp).
#[derive(Clone, Debug)]
pub struct MessagePost {
    /// The new message's type.
    pub ty: GameMessageType,
    /// The new message's arguments (appended in order).
    pub args: Vec<GameMessageArgument>,
}

/// The translator body's outcome (applied by the propagation in the
/// original's order: the posts first — they are the body's own
/// appends — then the `next` read, then the destroy).
#[derive(Clone, Debug, Default)]
pub struct TranslateOutcome {
    /// The `translateGameMessage` return.
    pub disposition: GameMessageDisposition,
    /// Messages the body appended to the stream.
    pub posts: Vec<MessagePost>,
}

/// The message arena: slots never reused (the original's memory pool is
/// unobservable without the freed reads the port refuses).
#[derive(Debug, Default)]
pub struct MessageArena {
    /// Slot per id (`None` = freed or never born).
    pub(crate) slots: Vec<Option<GameMessage>>,
}

impl MessageArena {
    /// `newInstance(GameMessage)(type)` — the ctor's player stamp is
    /// the caller's (the stream's append passes the local player).
    pub fn alloc(&mut self, ty: GameMessageType, player_index: i32) -> MessageID {
        let id = self.slots.len() as MessageID;
        self.slots.push(Some(GameMessage {
            ty,
            player_index,
            args: Vec::new(),
            next: None,
            prev: None,
            list: None,
        }));
        id
    }

    /// The message for an id (panics on dead or unknown ids — an
    /// internal-invariant violation, never a data condition).
    #[must_use]
    pub fn get(&self, id: MessageID) -> &GameMessage {
        self.slots
            .get(id as usize)
            .and_then(|s| s.as_ref())
            .unwrap_or_else(|| panic!("message {id} is not live"))
    }

    /// The mutable accessor.
    #[must_use]
    pub fn get_mut(&mut self, id: MessageID) -> &mut GameMessage {
        self.slots
            .get_mut(id as usize)
            .and_then(|s| s.as_mut())
            .unwrap_or_else(|| panic!("message {id} is not live"))
    }

    /// `deleteInstance` — the slot dies (the caller unlinks first; the
    /// original's destructor detach is every deletion site's preamble).
    /// Public: the corpus drives mid-list deletes directly, like the
    /// original's deleteInstance.
    pub fn free(&mut self, id: MessageID) {
        if let Some(slot) = self.slots.get_mut(id as usize) {
            *slot = None;
        }
    }

    /// The live message ids in slot order (the corpus surface).
    #[must_use]
    pub fn live_ids(&self) -> Vec<MessageID> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|_| i as MessageID))
            .collect()
    }
}

/// `GameMessageList` (MessageStream.h): the doubly-linked list over the
/// arena — `TheMessageStream`'s and `TheCommandList`'s shared base.
#[derive(Debug)]
pub struct GameMessageList {
    /// Which list this is (the `m_list` the messages carry).
    pub(crate) kind: MessageListKind,
    /// `m_firstMessage`.
    pub(crate) first: Option<MessageID>,
    /// `m_lastMessage`.
    pub(crate) last: Option<MessageID>,
}

impl GameMessageList {
    /// A list of the given kind, empty.
    pub(crate) fn new(kind: MessageListKind) -> Self {
        Self {
            kind,
            first: None,
            last: None,
        }
    }

    /// `getFirstMessage()`.
    #[must_use]
    pub fn first_message(&self) -> Option<MessageID> {
        self.first
    }

    /// The list walk (first to last — the corpus surface).
    #[must_use]
    pub fn ids(&self, arena: &MessageArena) -> Vec<MessageID> {
        let mut out = Vec::new();
        let mut cur = self.first;
        while let Some(id) = cur {
            out.push(id);
            cur = arena.get(id).next;
        }
        out
    }

    /// `GameMessageList::appendMessage` — the tail append (the links
    /// and the containment note `msg->friend_setList(this)`).
    pub fn append_message(&mut self, arena: &mut MessageArena, id: MessageID) {
        arena.get_mut(id).next = None;
        if let Some(last) = self.last {
            arena.get_mut(last).next = Some(id);
            arena.get_mut(id).prev = Some(last);
            self.last = Some(id);
        } else {
            // first message
            self.first = Some(id);
            self.last = Some(id);
            arena.get_mut(id).prev = None;
        }
        // note containment within message itself
        arena.get_mut(id).list = Some(self.kind);
    }

    /// `GameMessageList::insertMessage` — insert after
    /// `message_to_insert_after`.
    pub fn insert_message_after(
        &mut self,
        arena: &mut MessageArena,
        id: MessageID,
        after: MessageID,
    ) {
        // First, set msg's next to be messageToInsertAfter's next.
        let after_next = arena.get(after).next;
        arena.get_mut(id).next = after_next;
        // Next, set msg's prev to be messageToInsertAfter.
        arena.get_mut(id).prev = Some(after);
        // Now update the next message's prev to be msg; if the friend
        // wasn't there, messageToInsertAfter is the last message.
        if let Some(next) = after_next {
            arena.get_mut(next).prev = Some(id);
        } else {
            self.last = Some(id);
        }
        // Finally, update the messageToInsertAfter's next to be msg.
        arena.get_mut(after).next = Some(id);
        // note containment within message itself
        arena.get_mut(id).list = Some(self.kind);
    }

    /// `GameMessageList::removeMessage`.
    pub fn remove_message(&mut self, arena: &mut MessageArena, id: MessageID) {
        let (next, prev) = {
            let m = arena.get(id);
            (m.next, m.prev)
        };
        if let Some(next) = next {
            arena.get_mut(next).prev = prev;
        } else {
            self.last = prev;
        }
        if let Some(prev) = prev {
            arena.get_mut(prev).next = next;
        } else {
            self.first = next;
        }
        arena.get_mut(id).list = None;
    }

    /// `GameMessageList::containsMessageOfType`.
    #[must_use]
    pub fn contains_message_of_type(&self, arena: &MessageArena, ty: GameMessageType) -> bool {
        let mut cur = self.first;
        while let Some(id) = cur {
            if arena.get(id).ty == ty {
                return true;
            }
            cur = arena.get(id).next;
        }
        false
    }
}

/// `TranslatorID` (MessageStream.h): the unique id `attachTranslator`
/// hands back (the counter starts at 1).
pub type TranslatorID = u32;

/// One registry entry (`TranslatorData` — the body is the host's
/// `run_translate`, keyed by the id).
#[derive(Clone, Copy, Debug)]
pub struct TranslatorData {
    /// `m_id`.
    pub id: TranslatorID,
    /// `m_priority` (lower priorities are executed first; ties keep
    /// attach order).
    pub priority: u32,
}

/// `MessageStream` (MessageStream.h): the stream the raw input lands
/// on and the translators filter into commands.
#[derive(Debug)]
pub struct MessageStream {
    /// The list base.
    pub(crate) list: GameMessageList,
    /// The translator registry (`m_firstTranslator`... in
    /// priority-sorted order; ties keep attach order).
    pub(crate) translators: Vec<TranslatorData>,
    /// `m_nextTranslatorID`.
    pub(crate) next_translator_id: TranslatorID,
    /// `ThePlayerList != nullptr` — `isReadyForMessages`'s gate (the
    /// engine creates the PlayerList after the stream; the kernel
    /// boots with it present).
    pub player_list_present: bool,
}

impl Default for MessageStream {
    fn default() -> Self {
        Self {
            list: GameMessageList::new(MessageListKind::Stream),
            translators: Vec::new(),
            next_translator_id: 1,
            player_list_present: false,
        }
    }
}

impl MessageStream {
    /// `isReadyForMessages()`.
    #[must_use]
    pub fn is_ready_for_messages(&self) -> bool {
        self.player_list_present
    }

    /// The engine-lifecycle pin (the `ThePlayerList` birth/shutdown the
    /// kernel does not model).
    pub fn set_player_list_present(&mut self, present: bool) {
        self.player_list_present = present;
    }

    /// `getFirstMessage()`.
    #[must_use]
    pub fn first_message(&self) -> Option<MessageID> {
        self.list.first_message()
    }

    /// The stream walk (first to last — the corpus surface).
    #[must_use]
    pub fn ids(&self, arena: &MessageArena) -> Vec<MessageID> {
        self.list.ids(arena)
    }

    /// `containsMessageOfType`.
    #[must_use]
    pub fn contains_message_of_type(&self, arena: &MessageArena, ty: GameMessageType) -> bool {
        self.list.contains_message_of_type(arena, ty)
    }

    /// `MessageStream::appendMessage(type)` — create the message (the
    /// ctor's local-player stamp) and append it to the stream.
    pub fn append_message(&mut self, arena: &mut MessageArena, players: &Players) -> MessageID {
        // GameMessage::GameMessage: m_playerIndex =
        // ThePlayerList->getLocalPlayer()->getPlayerIndex() (the null
        // deref of a player-list-less world is the port's panic)
        assert!(
            self.player_list_present,
            "appendMessage before the PlayerList exists"
        );
        let local = players
            .list
            .local
            .expect("the local player is set once the PlayerList is born")
            as i32;
        let id = arena.alloc(GameMessageType::MSG_INVALID, local);
        self.list.append_message(arena, id);
        id
    }

    /// `GameMessageList::insertMessage` over the stream — the raw form
    /// (an already-allocated message; the C++ list op is public, the
    /// corpus surface).
    pub fn insert_message_after_raw(
        &mut self,
        arena: &mut MessageArena,
        id: MessageID,
        after: MessageID,
    ) {
        self.list.insert_message_after(arena, id, after);
    }

    /// `GameMessageList::removeMessage` over the stream (the raw form).
    pub fn remove_message_raw(&mut self, arena: &mut MessageArena, id: MessageID) {
        self.list.remove_message(arena, id);
    }

    /// `MessageStream::insertMessage` — create and insert after
    /// `message_to_insert_after`.
    pub fn insert_message(
        &mut self,
        arena: &mut MessageArena,
        players: &Players,
        after: MessageID,
    ) -> MessageID {
        assert!(
            self.player_list_present,
            "insertMessage before the PlayerList exists"
        );
        let local = players
            .list
            .local
            .expect("the local player is set once the PlayerList is born")
            as i32;
        let id = arena.alloc(GameMessageType::MSG_INVALID, local);
        self.list.insert_message_after(arena, id, after);
        id
    }

    /// The post path (a mid-propagation `appendMessage`): the local
    /// player's stamp, the arguments appended in order.
    pub fn append_message_data(
        &mut self,
        arena: &mut MessageArena,
        players: &Players,
        post: &MessagePost,
    ) -> MessageID {
        let id = self.append_message(arena, players);
        let m = arena.get_mut(id);
        m.ty = post.ty;
        m.args = post.args.clone();
        id
    }

    /// `attachTranslator` — the registry insert: before the first
    /// strictly-greater priority (ties keep attach order); the id
    /// counter starts at 1.
    pub fn attach_translator(&mut self, priority: u32) -> TranslatorID {
        let id = self.next_translator_id;
        self.next_translator_id += 1;
        let entry = TranslatorData { id, priority };
        // search the Translator list for our priority location
        let pos = self.translators.iter().position(|t| t.priority > priority);
        match pos {
            Some(p) => self.translators.insert(p, entry),
            None => self.translators.push(entry),
        }
        id
    }

    /// `findTranslator`.
    #[must_use]
    pub fn find_translator(&self, id: TranslatorID) -> Option<TranslatorData> {
        self.translators.iter().find(|t| t.id == id).copied()
    }

    /// `removeTranslator` — the found-and-remove walk.
    pub fn remove_translator(&mut self, id: TranslatorID) {
        if let Some(pos) = self.translators.iter().position(|t| t.id == id) {
            self.translators.remove(pos);
        }
    }

    /// The registry in execution order (the corpus surface).
    #[must_use]
    pub fn translator_order(&self) -> Vec<TranslatorData> {
        self.translators.clone()
    }

    /// `isRedundantNetworkMessage` (MessageStream.cpp): the
    /// TheSuperHackers network-traffic trim over the transferred
    /// command list.
    fn is_redundant_network_message(&self, arena: &MessageArena, id: MessageID) -> bool {
        let msg = arena.get(id);
        match msg.ty {
            GameMessageType::MSG_DESTROY_SELECTED_GROUP => {
                let Some(next) = msg.next else {
                    return false;
                };
                match arena.get(next).ty {
                    GameMessageType::MSG_CREATE_SELECTED_GROUP
                    | GameMessageType::MSG_CREATE_SELECTED_GROUP_NO_SOUND => {
                        let n = arena.get(next);
                        if n.get_argument_count() >= 1 {
                            let new_group = n.get_argument(0).boolean();
                            if new_group {
                                return true;
                            }
                        }
                        false
                    }
                    GameMessageType::MSG_DESTROY_SELECTED_GROUP => true,
                    GameMessageType::MSG_SELECT_TEAM0
                    | GameMessageType::MSG_SELECT_TEAM1
                    | GameMessageType::MSG_SELECT_TEAM2
                    | GameMessageType::MSG_SELECT_TEAM3
                    | GameMessageType::MSG_SELECT_TEAM4
                    | GameMessageType::MSG_SELECT_TEAM5
                    | GameMessageType::MSG_SELECT_TEAM6
                    | GameMessageType::MSG_SELECT_TEAM7
                    | GameMessageType::MSG_SELECT_TEAM8
                    | GameMessageType::MSG_SELECT_TEAM9 => true,
                    _ => false,
                }
            }
            GameMessageType::MSG_SET_REPLAY_CAMERA => {
                let mut cur = msg.next;
                while let Some(next) = cur {
                    if arena.get(next).ty == GameMessageType::MSG_SET_REPLAY_CAMERA {
                        return true;
                    }
                    cur = arena.get(next).next;
                }
                false
            }
            _ => false,
        }
    }

    /// `MessageStream::propagateMessages` — the two-level walk, the
    /// transfer, the redundant sweep and the clear. The translator
    /// bodies are the host's (`run_translate`); the posts a body
    /// makes land on the stream's tail BEFORE the walk's `next` read,
    /// exactly as the original's mid-body appends do.
    pub fn propagate_messages(
        &mut self,
        arena: &mut MessageArena,
        commands: &mut CommandList,
        players: &Players,
        host: &mut dyn KernelHost,
    ) {
        // process each Translator (the registry is fixed for the
        // duration — the stated contract)
        for t in self.translators.clone() {
            // for each message: the body (its posts first), then the
            // next read, then the destroy
            let mut cur = self.list.first;
            while let Some(id) = cur {
                let outcome = host.run_translate(t.id, id, &arena.get(id).data());
                for post in &outcome.posts {
                    self.append_message_data(arena, players, post);
                }
                if outcome.disposition == GameMessageDisposition::DESTROY_MESSAGE {
                    cur = arena.get(id).next;
                    // ~GameMessage: the detach, then the free
                    self.list.remove_message(arena, id);
                    arena.free(id);
                    continue;
                }
                cur = arena.get(id).next;
            }
        }

        // Transfer all messages that reached the end of the stream to
        // TheCommandList (the client translators above can also create
        // game messages — those are on the stream too).
        commands.append_message_list(arena, self.list.first);

        // Delete network messages that are redundant (the sweep walks
        // the COMMAND list — the post-transfer adjacency).
        let mut cur = commands.list.first;
        while let Some(id) = cur {
            let next = arena.get(id).next;
            if self.is_redundant_network_message(arena, id) {
                commands.list.remove_message(arena, id);
                arena.free(id);
            }
            cur = next;
        }

        // clear the stream
        self.list.first = None;
        self.list.last = None;
    }
}

/// `CommandList` (MessageStream.h): the final set of messages the
/// GameLogic executes on its next iteration.
#[derive(Debug)]
pub struct CommandList {
    /// The list base.
    pub(crate) list: GameMessageList,
}

impl Default for CommandList {
    fn default() -> Self {
        Self {
            list: GameMessageList::new(MessageListKind::CommandList),
        }
    }
}

impl CommandList {
    /// `getFirstMessage()`.
    #[must_use]
    pub fn first_message(&self) -> Option<MessageID> {
        self.list.first_message()
    }

    /// The list walk (first to last — the corpus surface).
    #[must_use]
    pub fn ids(&self, arena: &MessageArena) -> Vec<MessageID> {
        self.list.ids(arena)
    }

    /// `containsMessageOfType`.
    #[must_use]
    pub fn contains_message_of_type(&self, arena: &MessageArena, ty: GameMessageType) -> bool {
        self.list.contains_message_of_type(arena, ty)
    }

    /// The direct append (the recorder's simulation-playback path —
    /// GameLogic.cpp's `messageList = TheCommandList` branch).
    pub fn append_message(&mut self, arena: &mut MessageArena, id: MessageID) {
        self.list.append_message(arena, id);
    }

    /// `CommandList::appendMessageList` — walk the given list head and
    /// append each message (the relink moves it onto this list).
    pub fn append_message_list(&mut self, arena: &mut MessageArena, list: Option<MessageID>) {
        let mut cur = list;
        while let Some(id) = cur {
            let next = arena.get(id).next;
            self.list.append_message(arena, id);
            cur = next;
        }
    }

    /// `CommandList::reset` — destroyAllMessages (the list goes empty;
    /// the arena slots die).
    pub fn reset(&mut self, arena: &mut MessageArena) {
        let mut cur = self.list.first;
        while let Some(id) = cur {
            let next = arena.get(id).next;
            arena.free(id);
            cur = next;
        }
        self.list.first = None;
        self.list.last = None;
    }
}

/// `buildRegion` (MessageStream.cpp): the drag-selection rectangle
/// from an anchor and the current position.
#[must_use]
pub fn build_region(anchor: ICoord2, dest: ICoord2) -> IRegion2 {
    let mut region = IRegion2::default();
    // build rectangular region defined by the drag selection
    if anchor.x < dest.x {
        region.lo.x = anchor.x;
        region.hi.x = dest.x;
    } else {
        region.lo.x = dest.x;
        region.hi.x = anchor.x;
    }
    if anchor.y < dest.y {
        region.lo.y = anchor.y;
        region.hi.y = dest.y;
    } else {
        region.lo.y = dest.y;
        region.hi.y = anchor.y;
    }
    region
}

impl GameLogic {
    /// `GameLogic::onLogicCrc` (GameLogicDispatch.cpp) — the network
    /// branch verbatim (the human gate, the slot gate with the
    /// connected refusal, the local-player `m_shouldValidateCRCs`
    /// raise and the per-player cache write) and the playback branch
    /// (the recorder hand-off hook). Returns the handler's `true`
    /// (kept for the dispatch contract's shape; every path returns
    /// true or refuses early with false).
    pub(crate) fn on_logic_crc(
        &mut self,
        players: &Players,
        host: &mut dyn KernelHost,
        player_index: i32,
        crc: u32,
    ) -> bool {
        let Some(msg_player) = players.nth_player(player_index) else {
            return false; // getMessagePlayer's null (the prologue's gate)
        };
        if players.network_present {
            if msg_player.player_type != PlayerType::Human {
                return false;
            }
            let slot_index = if player_index < 0 {
                -1
            } else {
                players.get_slot_index(player_index as usize)
            };
            if slot_index < 0 || !host.is_player_connected(slot_index) {
                return false;
            }
            if players.list.local == Some(player_index as usize) {
                // (RTS_DEBUG's TheDebugIgnoreSyncErrors gate is not in
                // the release build)
                self.should_validate_crcs = true;
            }
            self.cached_crcs.insert(player_index, crc);
        } else if host.recorder_is_playback() {
            host.on_recorder_crc(crc, player_index, false);
        }
        true
    }

    /// `GameLogic::onLogicCrcForPlayback` — the recorder hand-off
    /// with the `-1` player and the playback flag.
    pub(crate) fn on_logic_crc_for_playback(
        &mut self,
        host: &mut dyn KernelHost,
        crc: u32,
    ) -> bool {
        host.on_recorder_crc(crc, -1, true);
        true
    }

    /// `GameLogic::processCommandList` — the cached-CRC clear, the
    /// per-message dispatch walk (the prologue's `getNthPlayer` gate,
    /// the position hook, the kernel-owned CRC handlers, the host's
    /// `run_command` for everything else) and the CRC-validation tail.
    pub(crate) fn process_command_list(
        &mut self,
        players: &Players,
        arena: &crate::messages::MessageArena,
        commands: &mut CommandList,
        host: &mut dyn KernelHost,
    ) {
        self.cached_crcs.clear();
        self.should_validate_crcs = false;

        // the walk (the loop increment reads `next` AFTER the body —
        // the same-frame visitation of tail appends; the stated
        // contract: the dispatcher may not destroy or reorder)
        let mut cur = commands.list.first;
        while let Some(id) = cur {
            let data = arena.get(id).data();

            // logicMessageDispatcher's prologue: getMessagePlayer
            let msg_player = players.nth_player(data.player_index);
            if msg_player.is_some() {
                // (the network-range AIGroup prologue — the selection
                // cleanup and the stats collection — is the AI slice's;
                // the corpus pins the dispatch order without it)
                host.on_command_dispatch(id, &data);

                match data.ty {
                    GameMessageType::MSG_LOGIC_CRC => {
                        let new_crc = data.arg0_integer() as u32;
                        self.on_logic_crc(players, host, data.player_index, new_crc);
                    }
                    GameMessageType::MSG_LOGIC_CRC_PLAYBACK => {
                        let new_crc = data.arg0_integer() as u32;
                        self.on_logic_crc_for_playback(host, new_crc);
                    }
                    _ => {
                        host.run_command(id, &data);
                    }
                }
            }
            cur = arena.get(id).next;
        }

        // the CRC validation tail (only reachable when the network
        // branch raised the flag — TheNetwork is present by
        // construction)
        if self.should_validate_crcs && !host.saw_crc_mismatch() {
            let mut saw_crc_mismatch = false;
            let mut num_players = 0usize;
            // (TheNetwork is guaranteed: m_shouldValidateCRCs is only
            // set inside its branch)
            for slot in 0..MAX_SLOTS as i32 {
                if host.is_player_connected(slot) {
                    num_players += 1;
                }
            }
            if self.cached_crcs.len() < num_players {
                // "Not enough CRCs!"
                saw_crc_mismatch = true;
            } else {
                let mut has_reference_crc = false;
                let mut reference_crc = 0u32;
                for (&player_index, &crc) in &self.cached_crcs {
                    // the Caball009 14/06/2026 bugfix: check if the
                    // player is still connected, to avoid spurious
                    // mismatches at low CRC intervals
                    let slot_index = if player_index < 0 {
                        -1
                    } else {
                        players.get_slot_index(player_index as usize)
                    };
                    if slot_index >= 0 && !host.is_player_connected(slot_index) {
                        continue;
                    }
                    if !has_reference_crc {
                        has_reference_crc = true;
                        reference_crc = crc;
                        continue;
                    }
                    if reference_crc != crc {
                        saw_crc_mismatch = true;
                    }
                }
            }
            if saw_crc_mismatch {
                host.on_crc_mismatch();
            }
        }
    }
}
