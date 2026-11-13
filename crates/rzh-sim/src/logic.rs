//! The game-logic kernel: the frame loop, the object registry and the
//! driver that ties the scheduler, the store and the host together.
//!
//! Origin: `GameLogic.cpp` at baseline 571a7c5 — `GameLogic::update`
//! (the ordered subsystem walk: the startNewGame kick, the client frame
//! hand-off, the script engine, the time-freeze gate, the terrain, the
//! recorder/CRC block, the command list, the sleepy updates, the AI, the
//! build assistant, the partition manager, the destroy list, the
//! command-list reset, the weapon/locomotor/victory stores, the
//! disabled-status sweep and the frame increment), `preUpdate` (the
//! pause-frame transition), `allocateObjectID` (the counter scheme),
//! `registerObject` (the list prepend and the initial sleepy wake —
//! `getFrame() == 0` becomes 1), `destroyObject` (the destroyed
//! re-refusal, the DESTROYED status, the destroy-list append),
//! `processDestroyList` (the two-pass sleepy erase with its MAX_SUO =
//! 256 cap, the destroy-during-destroy same-pass append, the list
//! removal and the lookup erase) and `loadPostProcess` (the ID
//! compaction — `max(id) + 1` walked from INVALID_ID — and the flat
//! heap rebuild with the bottom-up heapify); `Object.cpp`'s kernel state
//! (`setDisabledUntil`'s `frame > now` mask write, `checkDisabledStatus`'s
//! `now >= tillFrame` re-enable, `reactToTransformChange`'s 0.01
//! position/angle thresholds, the geometry-change
//! unregister/re-register); and `ThingFactory.cpp`'s `newObject` shape
//! (the ID allocation, the module construction, the registrations).
//!
//! The subsystem bodies the walk orders are later slices and plug in
//! through [`KernelHost`]: their call ORDER is pinned by the golden
//! corpora; their state is the host's. The wall-clock pacing around the
//! loop (the FramePacer and the render interleaving) is the client
//! platform layer's (Phase 4); the kernel sees only the time-freeze
//! gate.
//!
//! Stated contracts (not replayed — see the transcription's header):
//! the 257th-and-beyond sleepy entries of an object destroyed with more
//! than 256 update modules dangle in the original (freed-memory reads);
//! the port refuses that domain outright (the arena's dead-slot lookup
//! panics). Real objects never carry a quarter-thousand update modules.
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use std::collections::VecDeque;

use crate::partition::{PartitionData, PartitionManager, Region2};
use crate::scheduler::{ModuleArena, ModuleHandle, SleepyScheduler, SleepyState};
use crate::types::{
    Coord3, DISABLED_COUNT, DISABLED_TYPES, DisabledMask, DisabledType, FOREVER, GeometryInfo,
    INVALID_ID, KINDOF_IMMOBILE_BIT, KINDOF_NO_COLLIDE_BIT, NEVER, OBJECT_STATUS_DESTROYED_BIT,
    OBJECT_STATUS_NO_COLLISIONS_BIT, ObjectID, SleepyUpdatePhase, UPDATE_SLEEP_NONE,
    UpdateSleepTime,
};

/// The outcome of one update-module body: the sleep length plus the
/// side effects the corpus model programs (applied by the kernel in the
/// transcription's order — destroy first, then wake). The WP2 module
/// framework replaces this with real module bodies.
#[derive(Clone, Copy, Debug)]
pub struct UpdateOutcome {
    /// The `update()` return code: the frame count to sleep.
    pub sleep: UpdateSleepTime,
    /// A `GameLogic::destroyObject` request issued from inside the body.
    pub destroy: Option<ObjectID>,
    /// A `setWakeFrame` on another module, as an absolute frame.
    pub wake: Option<(ObjectID, u32, u32)>,
}

impl Default for UpdateOutcome {
    fn default() -> Self {
        Self {
            sleep: UPDATE_SLEEP_NONE,
            destroy: None,
            wake: None,
        }
    }
}

/// The outcome of one collide-module body: the side effects the corpus
/// model programs (applied by the dispatch in this order — absorb, then
/// destroy, then unregister). The WP2 module framework replaces this
/// with real module bodies.
#[derive(Clone, Copy, Debug, Default)]
pub struct CollideOutcome {
    /// A `setObjectStatus(OBJECT_STATUS_NO_COLLISIONS)` request — the
    /// mid-walk break of `Object::onCollide` and the next pair's
    /// pre-gate.
    pub absorb: bool,
    /// A `GameLogic::destroyObject` request issued from inside the body.
    pub destroy: Option<ObjectID>,
    /// A `PartitionManager::unRegisterObject` request issued from inside
    /// the body (the mid-processing `TheContactList` cleanup path).
    pub unregister: Option<ObjectID>,
}

/// The subsystem surface of the frame loop. The kernel calls these in
/// the original's order; the default bodies are no-ops (the counters
/// the transcription dumps live on the test hosts).
#[allow(unused_variables)]
pub trait KernelHost {
    /// `TheGameEngine->isTimeFrozen()` — read after the script-engine
    /// step (scripts may freeze the world).
    fn is_time_frozen(&mut self) -> bool {
        false
    }

    /// The `startNewGame(FALSE)` kick (the loading gate).
    fn on_start_new_game(&mut self) {}

    /// `TheGameClient->setFrame(now)`.
    fn on_client_frame(&mut self, frame: u32) {}

    /// `TheScriptEngine->UPDATE()`.
    fn on_scripts_update(&mut self) {}

    /// TheFramePacer's freeze gate, after the script step.
    fn on_frozen_gate(&mut self, frozen: bool) {}

    /// `TheTerrainLogic->UPDATE()`.
    fn on_terrain_update(&mut self) {}

    /// The recorder/CRC block (the generation gates need
    /// TheRecorder/TheGameInfo — later slices; the position is pinned).
    fn on_recorder_update(&mut self) {}

    /// `processCommandList(TheCommandList)` (WP1's GameMessage
    /// processing — its own slice).
    fn on_commands_update(&mut self) {}

    /// One update module dispatching (before the body runs).
    fn on_update_module(&mut self, id: ObjectID, module: u32) {}

    /// `TheAI->UPDATE()`.
    fn on_ai_update(&mut self) {}

    /// `TheBuildAssistant->UPDATE()`.
    fn on_build_assistant_update(&mut self) {}

    /// `ThePartitionManager->UPDATE()`.
    fn on_partition_update(&mut self) {}

    /// The destroy-list pass, carrying the processed count.
    fn on_destroy_list(&mut self, count: usize) {}

    /// `TheCommandList->reset()`.
    fn on_commands_reset(&mut self) {}

    /// `TheWeaponStore->UPDATE()`.
    fn on_weapon_store_update(&mut self) {}

    /// `TheLocomotorStore->UPDATE()`.
    fn on_locomotor_store_update(&mut self) {}

    /// `TheVictoryConditions->UPDATE()`.
    fn on_victory_conditions_update(&mut self) {}

    /// The disabled-status sweep (after the stores).
    fn on_disabled_sweep(&mut self) {}

    /// The frame increment (the loop's last step).
    fn on_frame_increment(&mut self) {}

    /// `sendObjectCreated` (the creation message).
    fn on_object_created(&mut self, id: ObjectID) {}

    /// `sendObjectDestroyed` (the destruction message).
    fn on_object_destroyed(&mut self, id: ObjectID) {}

    /// `Object::onPartitionCellChange`.
    fn on_partition_cell_change(&mut self, id: ObjectID, cell: i32) {}

    /// One collide-module dispatch inside `Object::onCollide` (before the
    /// body runs) — the pair, the module, and the collision location and
    /// normal the collide test produced.
    fn on_collide_module(
        &mut self,
        id: ObjectID,
        module: u32,
        other: ObjectID,
        loc: &Coord3,
        normal: &Coord3,
    ) {
    }

    /// The collide-module body. Returns the outcome the dispatch applies
    /// (absorb, then destroy, then unregister).
    fn run_collide(
        &mut self,
        id: ObjectID,
        module: u32,
        other: ObjectID,
        loc: &Coord3,
        normal: &Coord3,
    ) -> CollideOutcome {
        CollideOutcome::default()
    }

    /// The update-module body. Returns the outcome the kernel applies.
    fn run_update(&mut self, id: ObjectID, module: u32) -> UpdateOutcome {
        UpdateOutcome::default()
    }

    /// `Player::update`'s body position (the AI update, the team
    /// generic-script walk, the energy sabotage check, the academy
    /// stats, the retaliation-mode posting, the tunnel heal and the
    /// money income bucket are the AI, script-engine, economy and
    /// client slices' — the 16-slot walk's position is the pinned
    /// observable).
    fn on_player_update(&mut self, player: usize) {}

    /// `Player::becomingLocalPlayer`'s transition (the client body — the
    /// team color, the garrison recalc, the disguiser colors — is the
    /// client phase's).
    fn on_becoming_local_player(&mut self, player: usize, yes: bool) {}

    /// `Player::becomingTeamMember`'s transition (the energy adjust, the
    /// capture bonus, the battle plans and the idle-worker UI are the
    /// economy and client slices').
    fn on_becoming_team_member(&mut self, obj: ObjectID, player: usize, yes: bool) {}

    /// `Object::onCapture`'s call (the module notifications — contain,
    /// body, experience — are the module framework's; the player pair
    /// can name the SAME player, the original fires on any team
    /// change).
    fn on_capture(&mut self, obj: ObjectID, old_player: usize, new_player: usize) {}

    /// One of `Team::updateState`'s script runs (the run itself is the
    /// script engine's — WP7).
    fn on_team_script(&mut self, team: crate::types::TeamID, kind: crate::players::TeamScriptKind) {
    }

    /// `createInactiveTeam`'s production-condition `executeActions` (the
    /// script engine's).
    fn on_production_script(&mut self, proto: crate::types::TeamProtoID) {}

    /// `~Team`'s `notifyOfTeamDestruction` (the script engine's).
    fn on_team_destroyed(&mut self, team: crate::types::TeamID) {}

    /// Money's `triggerAudioEvent` (Money.cpp — the withdraw/deposit
    /// sounds; the volume math — `m_preferredMoneyTransactionVolume`
    /// times the event's volume, the `<= 0` early return — is the
    /// audio phase's, and so is the event queue itself; the hook pins
    /// the call, its player and the direction).
    fn on_money_audio(&mut self, player: usize, withdraw: bool) {
        let _ = (player, withdraw);
    }

    /// `AcademyStats::recordIncome` (Money::deposit's academy hook —
    /// the academy-stats slice's body, fired even for the untracked,
    /// silent deposits).
    fn on_academy_income(&mut self, player: usize) {
        let _ = player;
    }

    /// `AcademyStats::recordGeneralsPointsSpent`
    /// (Player::attemptToPurchaseScience's academy hook).
    fn on_academy_points_spent(&mut self, player: usize, points: i32) {
        let _ = (player, points);
    }

    /// `TheScriptEngine->notifyOfAcquiredScience` (Player::addScience
    /// and Player::resetSciences's notify — the script engine's body,
    /// WP7; the resetSciences tail loop fires it AGAIN for every
    /// rank-granted science, the double-notify quirk the corpora pin).
    fn on_acquired_science(&mut self, player: usize, science: crate::sciences::ScienceType) {
        let _ = (player, science);
    }
}

/// The kernel's object: the registry state, the scheduler's modules and
/// the spatial-store attachment in one id-addressed record
/// (`Object`'s kernel half — the behaviors/bodies are the module
/// framework's).
#[derive(Debug)]
pub struct Object {
    /// `m_id`.
    pub id: ObjectID,
    /// `m_geometryInfo`.
    pub geometry: GeometryInfo,
    /// The position.
    pub position: crate::types::Coord3,
    /// The orientation (the facing angle, radians).
    pub orientation: f32,
    /// `m_kindof` (the bitset — `KindOfMaskType`, a 122-entry
    /// `BitFlags` in the original; the port's `u128` carries the real
    /// ordinals: the IMMOBILE, NO_COLLIDE and POWERED bits are what
    /// the store, the collide family and the brownout sweep consult —
    /// the full table is the data layer's).
    pub kindof: u128,
    /// `m_status` (the ObjectStatusMaskType).
    pub status: u32,
    /// `m_privateStatus` (the ObjectPrivateStatusBits: EFFECTIVELY_DEAD,
    /// OFF_MAP — the query family's alive / on-map / same-map-status
    /// filters read them; the setters are the body-module and
    /// ghost-object slices').
    pub private_status: u32,
    /// The template's `friend_getBuildCost()` (the cost-sorted
    /// iterators read it through the template; the template itself is
    /// the rzh-data object model's — the corpus programs the value).
    pub build_cost: f32,
    /// `m_disabledMask`.
    pub disabled_mask: DisabledMask,
    /// `m_disabledTillFrame[DISABLED_COUNT]` (born NEVER).
    pub disabled_till_frame: [u32; DISABLED_COUNT],
    /// The update modules (the scheduler state each carries).
    pub updates: Vec<SleepyState>,
    /// The behavior list's collide modules — `Object::onCollide`'s walk
    /// length (the bodies are the host's [`KernelHost::run_collide`];
    /// the non-collide behaviors contribute nothing observable).
    pub colliders: usize,
    /// The partition data (None until the store registers the object).
    pub partition: Option<PartitionData>,
    /// `onPartitionCellChange()` invocations.
    pub cell_change_count: i32,
    /// `m_team` (Object.cpp — the `Team*` as its id; `TEAM_ID_INVALID` =
    /// no team; the `setTeam` family in [`crate::players`] maintains
    /// it).
    pub team: crate::types::TeamID,
    /// The template's `getEnergyProduction()` (ThingTemplate.h — the
    /// energy block's influence read; negative = consumption; the
    /// corpus programs the value, the template itself is the rzh-data
    /// object model's).
    pub energy_production: i32,
    /// The template's `getEnergyBonus()` (the power-plant upgrade
    /// bonus; ditto).
    pub energy_bonus: i32,
    /// `getVisionRange()` — the template's vision (the rzh-data object
    /// model's; the corpora program the value — the enemy-sighted walk
    /// reads it).
    pub vision_range: f32,
    /// `getAIUpdateInterface() != null` (the AI update's existence — the
    /// onIdle walk's gate; the module itself is the module framework's).
    pub has_ai: bool,
    /// `AIUpdateInterface::isIdle()`.
    pub ai_idle: bool,
}

impl Object {
    /// `isKindOf(KINDOF_IMMOBILE)`.
    #[must_use]
    pub fn is_kind_of_immobile(&self) -> bool {
        (self.kindof & (1u128 << KINDOF_IMMOBILE_BIT)) != 0
    }

    /// `isKindOf(KINDOF_NO_COLLIDE)`.
    #[must_use]
    pub fn is_kind_of_no_collide(&self) -> bool {
        (self.kindof & (1u128 << KINDOF_NO_COLLIDE_BIT)) != 0
    }

    /// `isDestroyed()` (the DESTROYED status bit).
    #[must_use]
    pub fn is_destroyed(&self) -> bool {
        (self.status & (1 << OBJECT_STATUS_DESTROYED_BIT)) != 0
    }

    /// `getStatusBits().test(OBJECT_STATUS_NO_COLLISIONS)`.
    #[must_use]
    pub fn is_no_collisions(&self) -> bool {
        (self.status & (1 << OBJECT_STATUS_NO_COLLISIONS_BIT)) != 0
    }

    /// `isEffectivelyDead()` (Object.h: the EFFECTIVELY_DEAD private bit).
    #[must_use]
    pub fn is_effectively_dead(&self) -> bool {
        (self.private_status & (1 << crate::types::OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT)) != 0
    }

    /// `isOffMap()` (Object.h: the OFF_MAP private bit).
    #[must_use]
    pub fn is_off_map(&self) -> bool {
        (self.private_status & (1 << crate::types::OBJECT_PRIVATE_OFF_MAP_BIT)) != 0
    }

    /// `getIsUndetectedDefector()` (Object.h: the UNDETECTED_DEFECTOR
    /// private bit — the relationship chain's defector gates read it;
    /// the setters are the defect/capture paths', a later slice).
    #[must_use]
    pub fn is_undetected_defector(&self) -> bool {
        (self.private_status & (1 << crate::types::OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT)) != 0
    }

    /// `isKindOfMulti(mustBeSet, mustBeClear)` (Thing.cpp → the
    /// template's kindof bits; the port keeps the bits on the object):
    /// every must-be-set bit set and every must-be-clear bit clear.
    #[must_use]
    pub fn is_kind_of_multi(&self, must_be_set: u128, must_be_clear: u128) -> bool {
        (self.kindof & must_be_set) == must_be_set && (self.kindof & must_be_clear) == 0
    }

    /// `Thing::getUnitDirectionVector2D` (Thing.cpp): the facing angle's
    /// unit vector in the xy plane (the original caches it behind the
    /// VALID_DIRVECTOR flag; the on-demand form computes the identical
    /// value).
    #[must_use]
    pub fn unit_direction_vector_2d(&self) -> crate::types::Coord3 {
        crate::types::Coord3 {
            x: rzh_math::gmath::cosf(self.orientation),
            y: rzh_math::gmath::sinf(self.orientation),
            z: 0.0,
        }
    }
}

/// The object arena: id-indexed slots, ids never reused. This is the
/// registry's lookup table (the original's `m_objHash` — probed by key,
/// never iterated) and the object store in one.
#[derive(Debug, Default)]
pub struct Objects {
    /// Slot per id (slot 0 — INVALID_ID — is always empty).
    slots: Vec<Option<Object>>,
}

impl Objects {
    /// The object for an id (panics on dead or unknown ids — an
    /// internal-invariant violation, never a data condition).
    #[must_use]
    pub fn get(&self, id: ObjectID) -> &Object {
        self.slots
            .get(id as usize)
            .and_then(|s| s.as_ref())
            .unwrap_or_else(|| panic!("object {id} is not live"))
    }

    /// The mutable accessor.
    #[must_use]
    pub fn get_mut(&mut self, id: ObjectID) -> &mut Object {
        self.slots
            .get_mut(id as usize)
            .and_then(|s| s.as_mut())
            .unwrap_or_else(|| panic!("object {id} is not live"))
    }

    /// The object for an id, if live.
    #[must_use]
    pub fn try_get(&self, id: ObjectID) -> Option<&Object> {
        self.slots.get(id as usize).and_then(|s| s.as_ref())
    }

    /// The live ids in slot order (the arena walk — the corpus surface;
    /// ids never collide so the order is the id order).
    #[must_use]
    pub fn slot_ids(&self) -> Vec<ObjectID> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|_| i as ObjectID))
            .collect()
    }

    /// The object's partition data (panics when unregistered — the
    /// callers' contract).
    #[must_use]
    pub(crate) fn partition(&self, id: ObjectID) -> &PartitionData {
        self.get(id).partition.as_ref().expect("partition data")
    }

    /// The partition data, mutable.
    pub(crate) fn partition_mut(&mut self, id: ObjectID) -> &mut PartitionData {
        self.get_mut(id).partition.as_mut().expect("partition data")
    }

    /// Insert an object into its id's slot.
    pub(crate) fn put(&mut self, obj: Object) {
        let idx = obj.id as usize;
        if self.slots.len() <= idx {
            self.slots.resize_with(idx + 1, || None);
        }
        self.slots[idx] = Some(obj);
    }

    /// The deletion: the slot goes empty (the id is never reused).
    pub(crate) fn slot_clear(&mut self, id: ObjectID) {
        if let Some(slot) = self.slots.get_mut(id as usize) {
            *slot = None;
        }
    }

    /// `friend_getBehaviorModules()` order: the module list walk.
    pub(crate) fn module_count(&self, id: ObjectID) -> usize {
        self.get(id).updates.len()
    }
}

impl ModuleArena for Objects {
    fn module(&self, handle: ModuleHandle) -> &SleepyState {
        &self.get(handle.obj).updates[handle.module as usize]
    }

    fn module_mut(&mut self, handle: ModuleHandle) -> &mut SleepyState {
        &mut self.get_mut(handle.obj).updates[handle.module as usize]
    }
}

/// `GameLogic`'s registry and frame-loop state.
#[derive(Debug, Default)]
pub struct GameLogic {
    /// `m_frame`.
    pub frame: u32,
    /// `m_nextObjID`.
    pub next_obj_id: ObjectID,
    /// `m_objList` (prepend order; the front is `getFirstObject`).
    pub obj_list: VecDeque<ObjectID>,
    /// `m_sleepyUpdates` + `m_curUpdateModule`.
    pub scheduler: SleepyScheduler,
    /// `m_objectsToDestroy`.
    pub objects_to_destroy: Vec<ObjectID>,
    /// `m_startNewGame`.
    pub start_new_game: bool,
    /// `m_hasUpdated`.
    pub has_updated: bool,
    /// `m_isInUpdate`.
    pub is_in_update: bool,
    /// `m_pauseFrame`.
    pub pause_frame: u32,
    /// `m_gamePaused` (setGamePaused's kernel half; the FramePacer
    /// machinery is the client layer's).
    pub game_paused: bool,
    /// `m_rankLevelLimit` — `GameLogic::init`'s 1000 (and
    /// `startNewGame`'s "this is reset every game" re-set; the reset
    /// position is the game-setup slice's, stated). The rank ladder's
    /// clamp; `setRankLevelLimit` refuses limits below 1.
    pub rank_level_limit: i32,
}

impl GameLogic {
    /// `getRankLevelLimit()`.
    #[must_use]
    pub fn rank_level_limit(&self) -> i32 {
        self.rank_level_limit
    }

    /// `setRankLevelLimit(Int)`: the `< 1` refusal.
    pub fn set_rank_level_limit(&mut self, mut limit: i32) {
        if limit < 1 {
            limit = 1;
        }
        self.rank_level_limit = limit;
    }

    /// `allocateObjectID`: return the counter, then advance (no reuse;
    /// the `@todo Find unused value` is still in the original).
    pub fn allocate_object_id(&mut self) -> ObjectID {
        let ret = self.next_obj_id;
        self.next_obj_id = ret.wrapping_add(1);
        ret
    }

    /// `setObjectIDCounter` (the save/load hook).
    pub fn set_object_id_counter(&mut self, next_obj_id: ObjectID) {
        self.next_obj_id = next_obj_id;
    }

    /// `getFrame()`.
    #[must_use]
    pub fn frame(&self) -> u32 {
        self.frame
    }

    /// `registerObject`: the list prepend and the initial sleepy wake.
    pub(crate) fn register_object(&mut self, objects: &mut Objects, id: ObjectID) {
        // add the object to the global list (prepend)
        self.obj_list.push_front(id);

        // add object to lookup table (the arena is the table)

        let mut now = self.frame;
        if now == 0 {
            now = 1;
        }
        let count = objects.module_count(id);
        for m in 0..count {
            let handle = ModuleHandle {
                obj: id,
                module: m as u32,
            };
            // note that 'when' can be zero here for any update module
            // that didn't bother to call setWakeFrame() in its ctor.
            // this is legal.
            if objects.module(handle).next_call_frame() == 0 {
                objects.module_mut(handle).set_next_call_frame(now);
            }
            self.scheduler.push(objects, handle);
        }
    }

    /// `destroyObject`: the destroyed re-refusal, the status, the
    /// destroy-list append.
    pub(crate) fn destroy_object(
        &mut self,
        objects: &mut Objects,
        host: &mut dyn KernelHost,
        id: ObjectID,
    ) {
        // if already flagged for destruction, ignore
        let Some(obj) = objects.try_get(id) else {
            return;
        };
        if obj.is_destroyed() {
            return;
        }

        // mark object as destroyed
        objects.get_mut(id).status |= 1 << OBJECT_STATUS_DESTROYED_BIT;

        // add to end of destruction list, in case something is being destroyed and trying to destroy subobjects
        self.objects_to_destroy.push(id);

        // (the DestroyModule onDestroy() bodies and obj->onDestroy() are
        // the module framework's; the scripted effects cover the
        // destroy-during-destroy reentrancy)

        host.on_object_destroyed(id);
    }

    /// `processDestroyList`: the two-pass sleepy erase, the list
    /// removal, the lookup erase and the deletion.
    pub fn process_destroy_list(
        &mut self,
        objects: &mut Objects,
        pm: &mut PartitionManager,
        host: &mut dyn KernelHost,
    ) {
        // The destroy list exists to ensure that all objects have a chance to
        // see each other at each simulation frame - the object list is the
        // same at the start of the update as it is at the end of the update.

        if !self.objects_to_destroy.is_empty() {
            host.on_destroy_list(self.objects_to_destroy.len());
        }

        let mut it = 0usize;
        while it < self.objects_to_destroy.len() {
            // index walk: appended entries (the destroy-during-destroy
            // requests) process in the same pass
            let current = self.objects_to_destroy[it];

            // this looks odd, but is necessary; since erasing a single entry can shuffle others in the
            // list (in order to maintain its heap-ness), we must do two passes: one to find the updates
            // for this object, another to actually erase 'em.
            const MAX_SUO: usize = 256;
            let mut sleepy_for_object: Vec<ModuleHandle> = Vec::new();
            for &h in &self.scheduler.heap {
                if h.obj == current && sleepy_for_object.len() < MAX_SUO {
                    sleepy_for_object.push(h);
                }
            }

            for i in (0..sleepy_for_object.len()).rev() {
                // have to re-get idx each time since each call to erase might change others.
                let handle = sleepy_for_object[i];
                let idx = objects.module(handle).index_in_logic;
                self.scheduler.erase(objects, idx as usize);
            }

            // remove from the object list
            if let Some(pos) = self.obj_list.iter().position(|&o| o == current) {
                self.obj_list.remove(pos);
            }

            // remove object from lookup table; the delete instance
            // (Object::~Object: the partition unregister, the id reset)
            if objects.get(current).partition.is_some() {
                pm.unregister_object(objects, current);
            }
            objects.slot_clear(current);
            it += 1;
        }

        self.objects_to_destroy.clear(); // list full of bad pointers now, clear it.
    }

    /// `friend_awakenUpdateModule`: `setWakeFrame`'s path, with the
    /// refusal family.
    pub(crate) fn awaken(
        &mut self,
        objects: &mut Objects,
        obj: ObjectID,
        module: u32,
        when_to_wake_up: u32,
    ) {
        let handle = ModuleHandle { obj, module };
        let now = self.frame;

        if self.scheduler.cur_update_module == Some(handle) {
            // You should not call setWakeFrame() from inside your update(), because
            // it will be ignored, in favor of the return code from update.
            return;
        }

        if when_to_wake_up == objects.module(handle).next_call_frame() {
            return; // my, that was easy
        }

        let next_call = objects.module(handle).next_call_frame();
        if now > 0 && next_call == now && when_to_wake_up == now.wrapping_add(1) {
            // subtle but important case: if we already awake, and someone calls
            // setWakeFrame(self, UPDATE_SLEEP_NONE), we don't want to reset our wake frame,
            // since that would prevent us from getting called THIS frame. since UPDATE_SLEEP_NONE
            // really means "wake up as soon as possible", we don't want to change our status
            // if we are already awake. (srj)
            return;
        }

        let idx = objects.module(handle).index_in_logic;
        let in_list = self.obj_list.contains(&obj);
        if in_list {
            assert!(
                idx >= 0 && (idx as usize) < self.scheduler.heap.len(),
                "sleepy update module illegal index"
            );
            assert_eq!(
                self.scheduler.heap[idx as usize], handle,
                "sleepy update module index mismatch"
            );

            // update the value.
            objects
                .module_mut(handle)
                .set_next_call_frame(when_to_wake_up);

            // rebalance.
            self.scheduler.rebalance(objects, idx as usize);
        } else {
            assert_eq!(idx, -1, "sleepy update module index mismatch");

            // this can happen if stuff happens during object initialization. fortunately,
            // it's easy to deal with:
            objects
                .module_mut(handle)
                .set_next_call_frame(when_to_wake_up);
        }
    }

    /// `UpdateModule::frameToSleepTime`: the min-of-four against now.
    #[must_use]
    pub fn frame_to_sleep_time(
        &self,
        frame1: u32,
        frame2: u32,
        frame3: u32,
        frame4: u32,
    ) -> UpdateSleepTime {
        let mut frame1 = frame1;
        if frame1 > frame2 {
            frame1 = frame2;
        }
        if frame1 > frame3 {
            frame1 = frame3;
        }
        if frame1 > frame4 {
            frame1 = frame4;
        }
        let now = self.frame;
        if frame1 > now {
            (frame1 - now) as UpdateSleepTime // UPDATE_SLEEP(numFrames)
        } else if frame1 == now {
            UPDATE_SLEEP_NONE
        } else {
            // frameToSleepTime: frame is in the past. oops. (DEBUG_CRASH
            // in the original; release returns NONE)
            UPDATE_SLEEP_NONE
        }
    }

    /// `preUpdate`: the pause-frame transition.
    pub(crate) fn pre_update(&mut self) {
        self.has_updated = false;

        if self.pause_frame == self.frame && self.pause_frame != 0 {
            self.pause_frame = 0;
            self.game_paused = true;
        }
    }

    /// `loadPostProcess`: the ID compaction and the heap rebuild.
    pub fn load_post_process(&mut self, objects: &mut Objects) {
        self.next_obj_id = INVALID_ID;
        for &id in &self.obj_list {
            if id >= self.next_obj_id {
                self.next_obj_id = id.wrapping_add(1);
            }
        }

        // blow away the sleepy update and normal update module lists
        for &h in &self.scheduler.heap {
            objects.module_mut(h).index_in_logic = -1;
        }
        self.scheduler.heap.clear();

        let mut now = self.frame;
        if now == 0 {
            now = 1;
        }

        // go through all objects, examine each update module and put it on the appropriate update list
        for &id in &self.obj_list {
            let count = objects.module_count(id);
            for m in 0..count {
                let handle = ModuleHandle {
                    obj: id,
                    module: m as u32,
                };
                // note that 'when' will only be zero for legacy save files.
                if objects.module(handle).next_call_frame() == 0 {
                    objects.module_mut(handle).set_next_call_frame(now);
                }
                self.scheduler.push_back_raw(objects, handle);
            }
        }

        // re-sort the priority queue all at once now that all modules are on it
        self.scheduler.remake(objects);
    }
}

/// The creation spec (the `ThingFactory::newObject` input: the template
/// geometry and the module set, distilled to what the kernel consumes).
#[derive(Clone, Debug, Default)]
pub struct ObjectSpec {
    /// The object's geometry.
    pub geometry: GeometryInfo,
    /// The IMMOBILE kindof bit.
    pub immobile: bool,
    /// The POWERED kindof bit (the brownout sweep's gate).
    pub powered: bool,
    /// One entry per update module: its phase, its
    /// disabled-types-to-process mask and its constructor wake (an
    /// absolute frame, clamped to now).
    pub modules: Vec<UpdateModuleSpec>,
    /// The behavior list's collide modules — `Object::onCollide`'s walk
    /// length.
    pub colliders: usize,
    /// The birth position.
    pub position: crate::types::Coord3,
    /// The birth orientation.
    pub orientation: f32,
    /// The template's `getEnergyProduction()` (the energy block's
    /// influence read; negative = consumption).
    pub energy_production: i32,
    /// The template's `getEnergyBonus()` (the power-plant upgrade
    /// bonus).
    pub energy_bonus: i32,
}

/// One update module's birth configuration.
#[derive(Clone, Debug, Default)]
pub struct UpdateModuleSpec {
    /// `getUpdatePhase()`.
    pub phase: SleepyUpdatePhase,
    /// `getDisabledTypesToProcess()`.
    pub disabled_types_to_process: DisabledMask,
    /// The constructor's `setWakeFrame` (an absolute frame; `None` =
    /// never woken — registerObject clamps those to now).
    pub ctor_wake: Option<u32>,
}

/// `Object::setDisabledUntil`'s kernel half (the `m_disabledTillFrame`
/// write with its `!= frame` gate and the `frame > now` mask write) —
/// shared by [`Kernel::set_disabled_until`] and the money/energy block's
/// brownout sweep (the `doPowerDisable` walk in [`crate::players`]).
///
/// (The original's audio events, `pauseAllSpecialPowers`, the drawable
/// tint, the contain-rider recursion and the spawn-slave orders are
/// their own slices'; the invalid-type refusal rides along.)
pub(crate) fn set_disabled_until_core(
    objects: &mut Objects,
    now: u32,
    obj: ObjectID,
    ty: DisabledType,
    frame: u32,
) {
    let idx = ty.index();
    if idx >= DISABLED_COUNT {
        return; // the invalid-type refusal
    }
    let o = objects.get_mut(obj);
    if o.disabled_till_frame[idx] != frame {
        o.disabled_till_frame[idx] = frame;
        o.disabled_mask.set(ty, frame > now);
        // (the rider/spawn/carbomb side effects are their own slices')
    }
}

/// The kernel: the logic, the object arena, the partition manager and
/// the players/teams block in one explicit context (plan §3.3 rule 5 —
/// no global state).
#[derive(Debug, Default)]
pub struct Kernel {
    /// The game-logic state.
    pub logic: GameLogic,
    /// The object arena.
    pub objects: Objects,
    /// The partition manager.
    pub pm: PartitionManager,
    /// The players/teams/relationships block (`ThePlayerList` +
    /// `TheTeamFactory` + the name-key interning). Pre-`init` until
    /// [`Kernel::with_grid`] runs — construct through it, not `Default`.
    pub players: crate::players::Players,
}

impl Kernel {
    /// A kernel with the partition grid built and the players inited
    /// (`GameLogic::init`'s `m_nextObjID = 1` reset included — the
    /// ctor's INVALID_ID start would hand out id 0, which no real
    /// object ever carries; `PlayerList::init`'s birth transitions ride
    /// the host).
    pub fn with_grid(
        partition_cell_size: f32,
        world_extents: Region2,
        host: &mut dyn KernelHost,
    ) -> Self {
        let mut kernel = Self::default();
        kernel.logic.next_obj_id = 1;
        // GameLogic::init's rank-limit reset ("this is reset every game"
        // re-runs it in startNewGame — the game-setup slice's, stated)
        kernel.logic.rank_level_limit = 1000;
        kernel.pm.init(partition_cell_size, world_extents);
        kernel.players.init(host);
        kernel
    }

    /// `ThingFactory::newObject`: allocate the id, build the object and
    /// its modules (with the constructor wakes), register with the
    /// logic, then with the partition manager.
    pub fn create_object(&mut self, spec: &ObjectSpec, host: &mut dyn KernelHost) -> ObjectID {
        let id = self.logic.allocate_object_id();
        let obj = Object {
            id,
            geometry: spec.geometry,
            position: spec.position,
            orientation: spec.orientation,
            kindof: u128::from(if spec.immobile {
                1u32 << KINDOF_IMMOBILE_BIT
            } else {
                0u32
            }) | (if spec.powered {
                1u128 << crate::types::KINDOF_POWERED_BIT
            } else {
                0u128
            }),
            status: 0,
            private_status: 0,
            build_cost: 0.0,
            disabled_mask: DisabledMask::default(),
            disabled_till_frame: [NEVER; DISABLED_COUNT],
            updates: spec
                .modules
                .iter()
                .map(|m| SleepyState::new(m.phase, m.disabled_types_to_process))
                .collect(),
            colliders: spec.colliders,
            partition: None,
            cell_change_count: 0,
            team: crate::types::TEAM_ID_INVALID,
            energy_production: spec.energy_production,
            energy_bonus: spec.energy_bonus,
            vision_range: 0.0,
            has_ai: false,
            ai_idle: false,
        };
        self.objects.put(obj);

        // module constructors may setWakeFrame (the not-in-list path)
        let now = self.logic.frame;
        for (m, module_spec) in spec.modules.iter().enumerate() {
            if let Some(wake) = module_spec.ctor_wake {
                let when = if wake > now { wake } else { now };
                self.logic.awaken(&mut self.objects, id, m as u32, when);
            }
        }

        // register the object with the GameLogic
        self.logic.register_object(&mut self.objects, id);

        // all objects are part of the partition manager system, add it to that
        // system now
        self.pm.register_object(&mut self.objects, id);

        // (obj->initObject() — the create modules' onCreate() — is the
        // module framework's)

        host.on_object_created(id);
        id
    }

    /// `UpdateModule::setWakeFrame(obj, wakeDelay)` — the delay form.
    pub fn set_wake_frame(&mut self, obj: ObjectID, module: u32, wake_delay: u32) {
        let now = self.logic.frame;
        self.logic
            .awaken(&mut self.objects, obj, module, now.wrapping_add(wake_delay));
    }

    /// `friend_awakenUpdateModule` — the absolute-frame form (the
    /// corpus-driving surface).
    pub fn awaken_module(&mut self, obj: ObjectID, module: u32, when: u32) {
        self.logic.awaken(&mut self.objects, obj, module, when);
    }

    /// `GameLogic::destroyObject` — the only interface to destroy
    /// objects (they die at the end of the update).
    pub fn destroy_object(&mut self, obj: ObjectID, host: &mut impl KernelHost) {
        self.logic.destroy_object(&mut self.objects, host, obj);
    }

    /// `Object::setDisabledUntil` (the kernel half: the mask write is
    /// gated on `frame > now`).
    pub fn set_disabled_until(&mut self, obj: ObjectID, ty: DisabledType, frame: u32) {
        set_disabled_until_core(&mut self.objects, self.logic.frame, obj, ty, frame);
    }

    /// `Object::setDisabled(type)`.
    pub fn set_disabled(&mut self, obj: ObjectID, ty: DisabledType) {
        self.set_disabled_until(obj, ty, FOREVER);
    }

    /// `Object::checkDisabledStatus`: the sweep body (clears expired
    /// disables; `now >= tillFrame`).
    pub(crate) fn check_disabled_status(&mut self, obj: ObjectID) {
        let now = self.logic.frame;
        let object = self.objects.get_mut(obj);
        for ty in DISABLED_TYPES {
            if object.disabled_mask.test(ty) && now >= object.disabled_till_frame[ty.index()] {
                // clearDisabled(type) — the mask clear is the kernel half
                object.disabled_mask.set(ty, false);
            }
        }
    }

    /// `Object::reactToTransformChange`'s dirty gate: the 0.01 position
    /// and angle thresholds.
    pub fn react_to_transform_change(
        &mut self,
        obj: ObjectID,
        old_pos: crate::types::Coord3,
        old_angle: f32,
    ) {
        // this is necessary because PhysicsBehavior may generate tiny changes even when
        // "standing still", due to roundoff errors.  It's important that we only invalidate
        // the PartitionManager stuff when the pos/orientation really changes (for efficiency purposes)
        // so we must put in some cleverness...
        const THRESH: f32 = 0.01;

        let object = self.objects.get(obj);
        let pos_diff = (old_pos.x - object.position.x).abs() > THRESH
            || (old_pos.y - object.position.y).abs() > THRESH
            || (old_pos.z - object.position.z).abs() > THRESH;
        // in radians, this is approx 1/2 degree.
        let ang_diff = (old_angle - object.orientation).abs() > THRESH;
        let registered = object.partition.is_some();

        if (pos_diff || ang_diff) && registered {
            self.pm.make_dirty(&mut self.objects, obj, true);
            // (the contain/trigger-area reactions are their own slices)
        }
    }

    /// `GameLogic::preUpdate`.
    pub fn pre_update(&mut self) {
        self.logic.pre_update();
    }

    /// `GameLogic::loadPostProcess`: the ID compaction and heap rebuild.
    pub fn load_post_process(&mut self) {
        self.logic.load_post_process(&mut self.objects);
    }

    /// `GameLogic::update`: the ordered walk. See the module docs for
    /// the order and the host surface.
    pub fn update(&mut self, host: &mut impl KernelHost) {
        // (LatchRestore<Bool> inUpdateLatch(m_isInUpdate, TRUE))
        let prev_in_update = self.logic.is_in_update;
        self.logic.is_in_update = true;

        // the startNewGame gate
        if self.logic.start_new_game {
            host.on_start_new_game();
            self.logic.start_new_game = false;
        }

        // send the current time to the GameClient
        let now = self.logic.frame;
        host.on_client_frame(now);

        // update (execute) scripts
        host.on_scripts_update();

        // Updates the frozen time status because it may have changed after the script engine update.
        let time_frozen = host.is_time_frozen();
        host.on_frozen_gate(time_frozen);
        if time_frozen {
            self.logic.is_in_update = prev_in_update;
            return;
        }

        // Note - TerrainLogic update needs to happen after ScriptEngine update, but before object updates.  jba.
        // This way changes in bridges are noted in the script engine before being cleared in TerrainLogic->update
        host.on_terrain_update();

        // force CRC calculation, so we can keep a cache of the last N CRCs...
        // (the recorder block: stated, not modelled — the hook pins the position)
        host.on_recorder_update();

        // process client commands
        host.on_commands_update();

        // (ALLOW_NONSLEEPY_UPDATES is not defined in this build — no
        // normal updates list exists)

        // the sleepy dispatch
        while let Some(handle) = self.logic.scheduler.peek() {
            // we're done, everyone else is sleeping.
            // break from the loop BEFORE we pop this item off.
            if self.objects.module(handle).next_call_frame() > now {
                break;
            }

            let mut sleep_len = UPDATE_SLEEP_NONE; // default, if it is disabled.

            let dis = self.objects.get(handle.obj).disabled_mask;
            let process_mask = self.objects.module(handle).disabled_types_to_process;
            // TheSuperHackers @bugfix Stubbjax 15/03/2026 The disabled-types-to-process mask is now exclusive.
            // Previously, if the disabled mask had any bits in common with the disabled-types-to-process mask,
            // the update would be processed. Now, if any *other* bits are set in the disabled mask, the update
            // is no longer processed.
            if process_mask.test_for_all(dis) {
                host.on_update_module(handle.obj, handle.module);
                self.logic.scheduler.cur_update_module = Some(handle);

                let outcome = host.run_update(handle.obj, handle.module);
                // the corpus's side-effect order: destroy first, then wake
                if let Some(victim) = outcome.destroy {
                    self.logic.destroy_object(&mut self.objects, host, victim);
                }
                if let Some((other_obj, other_module, when)) = outcome.wake {
                    self.logic
                        .awaken(&mut self.objects, other_obj, other_module, when);
                }
                sleep_len = outcome.sleep;

                self.logic.scheduler.cur_update_module = None;
            }

            // else defer it till next frame and re-push it
            if sleep_len < 1 {
                sleep_len = UPDATE_SLEEP_NONE;
            }
            self.objects
                .module_mut(handle)
                .set_next_call_frame(now.wrapping_add(sleep_len as u32));
            self.logic.scheduler.rebalance(&mut self.objects, 0);
        }

        // update the Artificial Intelligence system
        host.on_ai_update();

        // production updates
        host.on_build_assistant_update();

        // update partition info
        host.on_partition_update();
        self.pm.update(&mut self.objects, &mut self.logic, host);

        //
        // End of frame clean-up
        //

        // destroy all pending objects
        self.logic
            .process_destroy_list(&mut self.objects, &mut self.pm, host);

        // reset the command list, destroying all messages
        host.on_commands_reset();

        host.on_weapon_store_update();
        host.on_locomotor_store_update();
        host.on_victory_conditions_update();

        {
            // Handle disabled statii (and re-enable objects once frame matches)
            let sweep: Vec<ObjectID> = self.logic.obj_list.iter().copied().collect();
            for obj in sweep {
                if self.objects.get(obj).disabled_mask.any() {
                    self.check_disabled_status(obj);
                }
            }
        }
        host.on_disabled_sweep();

        // increment world time
        if !self.logic.start_new_game {
            self.logic.frame = self.logic.frame.wrapping_add(1);
            self.logic.has_updated = true;
            host.on_frame_increment();
        }

        self.logic.is_in_update = prev_in_update;
    }

    /// `GameLogic::processDestroyList` — the direct pass (the corpora's
    /// out-of-update driver; the frame loop calls it at its tail).
    pub fn process_destroy_list(&mut self, host: &mut impl KernelHost) {
        self.logic
            .process_destroy_list(&mut self.objects, &mut self.pm, host);
    }

    /// `setGamePausedInFrame` (the preUpdate pause trigger).
    pub fn set_pause_frame(&mut self, frame: u32) {
        self.logic.pause_frame = frame;
    }

    /// The pause flag (the dumped `m_gamePaused`).
    #[must_use]
    pub fn game_paused(&self) -> bool {
        self.logic.game_paused
    }

    /// `setStartNewGame` (the loading gate's input).
    pub fn request_new_game(&mut self) {
        self.logic.start_new_game = true;
    }

    /// `getFrame()`.
    #[must_use]
    pub fn frame(&self) -> u32 {
        self.logic.frame
    }

    /// The heap-validation debug helper (the `validateSleepyUpdate`
    /// contract, exposed for tests).
    #[cfg(debug_assertions)]
    #[must_use]
    pub fn scheduler_valid(&self) -> bool {
        self.logic.scheduler.validate(&self.objects)
    }
}
