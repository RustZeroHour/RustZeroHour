//! The players/teams/relationships block — the fourth Phase 3 slice (the
//! plan's WP1 remainder, next after the query family) — carrying the
//! money/energy/ranks/sciences state of the fifth slice (the block that
//! hangs its wallets, power pools, rank ladders and science catalogs on
//! these player objects).
//!
//! Origin: `PlayerList.cpp` (the 16-slot player registry with its
//! `init`/`reset`/`newGame` lifecycle, the local-player selection, the
//! mask queries and the slot bookkeeping), `Player.cpp`'s kernel half
//! (the relation maps with their get/set/remove family and the
//! two-level [`player_get_relationship`](Players::player_get_relationship)
//! chain, the team-prototype list, `setDefaultTeam`, `preTeamDestroy`,
//! the `becomingLocalPlayer`/`becomingTeamMember` transitions),
//! `Team.cpp` (`TeamFactory` with its `std::map`-ordered prototype
//! registry and the `++id` allocation scheme, `TeamPrototype` with the
//! instance list and the empty-team removal sweep, `Team` with the
//! override relation maps, the member-list choreography and
//! `updateState`'s state half), `Object.cpp`'s `setTeam` family (the
//! `isPlayerActive` refusal, the member-list swap, the capture hook)
//! and `Object::getRelationship` (the defector gates), and
//! `PartitionManager.cpp`'s player-state filters
//! (`PartitionFilterSamePlayer`, `PartitionFilterRelationship`,
//! `PartitionFilterAcceptOnTeam`) — at baseline 571a7c5.
//!
//! The fifth slice adds, from the same baseline: `Player.cpp`'s
//! progression half (the init paths with the zero-wallet starting-cash
//! deposit, [`Players::reset_rank`]/[`Players::reset_sciences`] with
//! the double script-notify, [`Players::add_skill_points`] with the
//! Hoffesommer ceil arithmetic, [`Players::set_rank_level`]'s upgrade
//! and the lose-everything downgrade, the purchase family's gate chain
//! — [`Players::attempt_to_purchase_science`] /
//! [`Players::grant_science`] /
//! [`Players::is_capable_of_purchasing_science`] — the script
//! availability choreography, the radar refcounts and
//! [`Player::update`]'s sabotage expiry), `Money.cpp`'s wallet (see
//! [`crate::money`]), `Energy.cpp`'s power pool with the brownout
//! choreography (see [`crate::energy`] and the `energy_*` methods
//! below), `Science.cpp`'s catalog and `RankInfo.cpp`'s ladder (see
//! [`crate::sciences`]), and `Player.cpp`'s `onPowerBrownOutChange`
//! (the radar restriction plus the `DISABLED_UNDERPOWERED` sweep over
//! every `KINDOF_POWERED` member).
//!
//! The name-key interning (`NameKeyGenerator::nameToKey`) comes from
//! `rzh-util`: the prototype registry's iteration order (the
//! `findTeamByID` / `teamAboutToBeDeleted` / `clear()` walks) is keyed
//! on the interning ordinals.
//!
//! The map data drives this block through the distilled [`SidesSpec`]
//! (the real dict parse is `rzh-formats`' sides-list slice). The bodies
//! this slice does not own are counted hooks whose call positions the
//! corpora pin: the AI/economy/script-engine interiors of
//! `Player::update` and `updateState`'s script runs, the client halves
//! of `becomingLocalPlayer`/`becomingTeamMember`/`onCapture`, and the
//! script-engine surface (`deleteTeam`, `killTeam`, the
//! didAllEnter/polygon-trigger family).
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use std::collections::BTreeMap;

use rzh_util::namekey::{NameKey, NameKeyGenerator};

use crate::energy::Energy;
use crate::logic::{KernelHost, Objects, set_disabled_until_core};
use crate::money::Money;
use crate::partition::PartitionManager;
use crate::sciences::{
    RankSpec, SCIENCE_INVALID, ScienceAvailabilityType, ScienceStore, ScienceType,
};
use crate::types::*;

// --- the distilled inputs (the map's SidesList dicts). -----------------------

/// One side of the map's sides list (the `Dict` the original
/// `PlayerList::newGame` reads, distilled to the fields the players
/// block consumes — the factions, build lists and skirmish script
/// duplication are the data/script-engine slices').
#[derive(Clone, Debug, Default)]
pub struct SideSpec {
    /// `TheKey_playerName` — empty means the neutral side ("it's neutral,
    /// which we've already done, so skip it").
    pub player_name: String,
    /// `TheKey_playerIsHuman`.
    pub is_human: bool,
    /// `TheKey_multiplayerIsLocal` — the multiplayer lobby's local flag,
    /// which beats the human heuristic.
    pub multiplayer_is_local: bool,
    /// the `PlayerTemplate`'s `isObserver()` bit (distilled; observers
    /// are dead).
    pub observer: bool,
    /// `TheKey_playerEnemies`' token list.
    pub enemies: Vec<String>,
    /// `TheKey_playerAllies`' token list.
    pub allies: Vec<String>,
    /// the `PlayerTemplate`'s money (`pt->getMoney()->countMoney()` —
    /// the INI's `StartingCash`); a ZERO wallet takes the game's
    /// starting-cash deposit (the money block's init path).
    pub template_cash: u32,
    /// the `PlayerTemplate`'s `IntrinsicSciences` token list (resolved
    /// through the name-key interning at `init`).
    pub intrinsic_sciences: Vec<String>,
    /// the `PlayerTemplate`'s `IntrinsicSciencePurchasePoints`.
    pub intrinsic_science_purchase_points: i32,
}

/// One team of the map's team list (`TeamTemplateInfo`'s
/// `updateState`-relevant half).
#[derive(Clone, Debug, Default)]
pub struct TeamSpec {
    /// `TheKey_teamName`.
    pub name: String,
    /// `TheKey_teamOwner`.
    pub owner: String,
    /// `TheKey_teamIsSingleton`.
    pub singleton: bool,
    /// `TheKey_teamOnCreateScript` (empty = none).
    pub script_on_create: String,
    /// `TheKey_teamOnDestroyedScript`.
    pub script_on_destroyed: String,
    /// `TheKey_teamOnIdleScript`.
    pub script_on_idle: String,
    /// `TheKey_teamEnemySightedScript`.
    pub script_on_enemy_sighted: String,
    /// `TheKey_teamAllClearScript`.
    pub script_on_all_clear: String,
    /// `TheKey_teamOnUnitDestroyedScript`.
    pub script_on_unit_destroyed: String,
    /// `TheKey_teamDestroyedThreshold` (the onDestroyed fraction).
    pub destroyed_threshold: f32,
    /// `TheKey_teamExecutesActionsOnCreate`.
    pub execute_actions: bool,
    /// `TheKey_teamProductionCondition`.
    pub production_condition: String,
    /// `findScriptByName`'s verdict for the condition (the script catalog
    /// is the script-engine slice's).
    pub production_script_exists: bool,
}

/// The distilled `GameInfo` (the occupied slots `assignSlotIndices`
/// walks the `player%d` names against).
#[derive(Clone, Debug, Default)]
pub struct GameInfoSpec {
    /// The occupied slot indices (`GameSlot::isOccupied()`).
    pub occupied_slots: Vec<i32>,
    /// `TheGameInfo->getStartingCash().countMoney()` — the game's
    /// starting cash the zero-wallet init deposit reads.
    pub starting_cash: u32,
}

/// The distilled `SidesList` + `GameInfo` pair `newGame` consumes.
#[derive(Clone, Debug, Default)]
pub struct SidesSpec {
    /// The sides (the neutral side first, as the maps carry it).
    pub sides: Vec<SideSpec>,
    /// The team list.
    pub teams: Vec<TeamSpec>,
    /// `TheGameInfo != nullptr` — whether to run `assignSlotIndices`.
    pub has_game_info: bool,
    /// The occupied slots.
    pub game_info: GameInfoSpec,
    /// `TheGlobalData->m_defaultStartingCash.countMoney()` — the
    /// no-`GameInfo` fallback of the zero-wallet init deposit.
    pub default_starting_cash: u32,
}

// --- the script kinds Team::updateState / notifyTeamOfObjectDeath run. -------

/// The team scripts `updateState` gates on (the runs themselves are the
/// script engine's — WP7; these name the hooks' positions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum TeamScriptKind {
    /// `m_scriptOnCreate` — the created latch's first update.
    OnCreate = 0,
    /// `m_scriptOnDestroyed` — the threshold trigger.
    OnDestroyed = 1,
    /// `m_scriptOnIdle` — the wasIdle latch.
    OnIdle = 2,
    /// `m_scriptOnEnemySighted`.
    OnEnemySighted = 3,
    /// `m_scriptOnAllClear`.
    OnAllClear = 4,
    /// `m_scriptOnUnitDestroyed` (`notifyTeamOfObjectDeath`).
    OnUnitDestroyed = 5,
}

// --- Player (Player.cpp's kernel half). --------------------------------------

/// One player slot: the identity, the relation maps, the team list and
/// the money/energy/ranks/sciences state (the fifth Phase 3 slice's
/// block).
///
/// The relation maps are `std::hash_map` in the original but are only
/// ever PROBED by key here (find / insert / erase — never iterated), so
/// the port's `BTreeMap` changes no observable (the dumps read them in
/// key order).
#[derive(Debug)]
pub struct Player {
    /// `m_playerIndex` — the slot (the array index; the mask bit).
    index: usize,
    /// `m_playerName`.
    pub name: String,
    /// `m_playerNameKey`.
    pub name_key: NameKey,
    /// `m_playerType`.
    pub player_type: PlayerType,
    /// `m_observer`.
    pub observer: bool,
    /// `m_isPlayerDead`.
    pub is_player_dead: bool,
    /// `m_playerRelations` (`PlayerRelationMapType`: player index →
    /// relationship).
    pub player_relations: BTreeMap<i32, Relationship>,
    /// `m_teamRelations` (`TeamRelationMapType`: team id → relationship).
    pub team_relations: BTreeMap<TeamID, Relationship>,
    /// `m_defaultTeam` (the `Team*` as its id; `TEAM_ID_INVALID` = none).
    pub default_team: TeamID,
    /// `m_playerTeamPrototypes` — the factory slots, push order (the
    /// `TeamProtoT*` pointers; slots are never reused, so the
    /// duplicate-name orphan the release refusal leaks stays distinct
    /// from any later prototype, exactly as the original's leaked
    /// object).
    pub team_prototypes: Vec<usize>,
    /// `m_money` — Player's current wealth (the money block).
    pub money: Money,
    /// `m_energy` — current energy production & consumption.
    pub energy: Energy,
    /// `m_rankLevel` — "our RankLevel, 1...n" (0 only between the ctor
    /// and the first `init`).
    pub rank_level: i32,
    /// `m_skillPoints` — our cumulative SkillPoint total.
    pub skill_points: i32,
    /// `m_sciencePurchasePoints` — our unspent SciencePurchasePoint
    /// total.
    pub science_purchase_points: i32,
    /// `m_levelUp` — skill points to go up a level (runtime only;
    /// `INT_MAX` past the ladder's end).
    pub level_up: i32,
    /// `m_levelDown` — skill points to go down a level (runtime only).
    pub level_down: i32,
    /// `m_sciences` — sciences that we know (either intrinsically or
    /// via later purchases).
    pub sciences: Vec<ScienceType>,
    /// `m_sciencesDisabled` — sciences that we are not permitted to
    /// purchase "yet". Controlled by mission scripts.
    pub sciences_disabled: Vec<ScienceType>,
    /// `m_sciencesHidden` — sciences that aren't shown. Controlled by
    /// mission scripts.
    pub sciences_hidden: Vec<ScienceType>,
    /// the template's `IntrinsicSciences`, resolved at `init` (the
    /// `m_playerTemplate` reads of `resetSciences`/`resetRank`
    /// distilled — the template itself is immutable data, so the
    /// snapshot is exact).
    pub intrinsic_sciences: Vec<ScienceType>,
    /// the template's `IntrinsicSciencePurchasePoints` (ditto).
    pub intrinsic_science_purchase_points: i32,
    /// `m_skillPointsModifier` — "Multiplied by skill points before
    /// they are applied".
    pub skill_points_modifier: f32,
    /// `m_radarCount` — # of facilities that have a radar under the
    /// players control.
    pub radar_count: i32,
    /// `m_disableProofRadarCount` — # of disable proof radars. "A
    /// disable proof one will be in both refcounts".
    pub disable_proof_radar_count: i32,
    /// `m_radarDisabled` — "The radar is disabled regardless of the
    /// number of radar objects".
    pub radar_disabled: bool,
}

impl Player {
    fn new(index: usize) -> Self {
        // the Player ctor's defaults (Player.cpp: m_rankLevel = 0,
        // m_sciencePurchasePoints = 0, m_skillPoints = 0,
        // m_skillPointsModifier = 1.0f, the radar refcounts zeroed; the
        // ctor's init(nullptr) — which runs resetRank — is modelled by
        // the PlayerList init walk, where the stores and the host
        // notifies exist)
        Self {
            index,
            name: String::new(),
            name_key: NameKey::INVALID,
            player_type: PlayerType::Computer,
            observer: false,
            is_player_dead: false,
            player_relations: BTreeMap::new(),
            team_relations: BTreeMap::new(),
            default_team: TEAM_ID_INVALID,
            team_prototypes: Vec::new(),
            money: Money::default(),
            energy: Energy::new(),
            rank_level: 0,
            skill_points: 0,
            science_purchase_points: 0,
            level_up: 0,
            level_down: 0,
            sciences: Vec::new(),
            sciences_disabled: Vec::new(),
            sciences_hidden: Vec::new(),
            intrinsic_sciences: Vec::new(),
            intrinsic_science_purchase_points: 0,
            skill_points_modifier: 1.0,
            radar_count: 0,
            disable_proof_radar_count: 0,
            radar_disabled: false,
        }
    }

    /// `getPlayerIndex()`.
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /// `getPlayerMask()` (Player.h): `1 << m_playerIndex`.
    #[must_use]
    pub fn mask(&self) -> PlayerMaskType {
        1u16 << self.index
    }

    /// `isPlayerActive()`: alive and not observer.
    #[must_use]
    pub fn is_player_active(&self) -> bool {
        !self.observer && !self.is_player_dead
    }

    // ---- the science state half (Player.cpp) --------------------------------

    /// `Player::hasScience` — "return t if the player has the given
    /// science, either intrinsically, via specialization, or via
    /// capture."
    #[must_use]
    pub fn has_science(&self, t: ScienceType) -> bool {
        self.sciences.contains(&t)
    }

    /// `Player::isScienceDisabled` — "Can't purchase this science
    /// because of script reasons."
    #[must_use]
    pub fn is_science_disabled(&self, t: ScienceType) -> bool {
        self.sciences_disabled.contains(&t)
    }

    /// `Player::isScienceHidden` — "This science is effectively hidden
    /// due to script reasons."
    #[must_use]
    pub fn is_science_hidden(&self, t: ScienceType) -> bool {
        self.sciences_hidden.contains(&t)
    }

    /// `Player::setScienceAvailability` — "Allows scripts to make
    /// specific sciences available, hidden, or disabled": the remove
    /// from both lists, then the one-list re-add (AVAILABLE adds to
    /// neither — "being available means that the science isn't in one
    /// of the hidden or disabled vecs").
    pub fn set_science_availability(&mut self, science: ScienceType, ty: ScienceAvailabilityType) {
        // First remove it from disabled sciences if it's there.
        let found_disabled = self.sciences_disabled.iter().position(|&s| s == science);
        if let Some(pos) = found_disabled {
            self.sciences_disabled.remove(pos);
        } else {
            // If still not found, remove it from hidden sciences if it's there.
            if let Some(pos) = self.sciences_hidden.iter().position(|&s| s == science) {
                self.sciences_hidden.remove(pos);
            }
        }

        // At this point, this science shouldn't be disabled nor hidden!
        match ty {
            ScienceAvailabilityType::Disabled => {
                // Add science to disabled vec.
                self.sciences_disabled.push(science);
            }
            ScienceAvailabilityType::Hidden => {
                // Add science to hidden vec.
                self.sciences_hidden.push(science);
            }
            ScienceAvailabilityType::Available | ScienceAvailabilityType::Invalid => {}
        }
    }

    /// `Player::addScience` — "give the science. doesn't check prereqs,
    /// nor charge to purchase points... and for a good reason; if
    /// public, this could be used to grant sciences that shouldn't be
    /// directly granted (e.g., SCIENCE_RankX, which should be managed
    /// via setRankLevel instead)". Returns TRUE if sciences were
    /// gained/lost.
    pub fn add_science(&mut self, science: ScienceType, host: &mut dyn KernelHost) -> bool {
        if self.has_science(science) {
            return false;
        }

        self.sciences.push(science);

        // 'wake up' any special powers controlled by, well, stuff — the
        // team/member/behavior walk finds no special-power modules yet
        // (the module framework slice's, WP2; TheControlBar's
        // markUIDirty inside the loop is the client's)

        // notify the script engine
        host.on_acquired_science(self.index, science);

        true
    }

    // ---- the rank/points getters (Player.h) ---------------------------------

    /// `getSkillPoints()`.
    #[must_use]
    pub fn get_skill_points(&self) -> i32 {
        self.skill_points
    }

    /// `getSciencePurchasePoints()`.
    #[must_use]
    pub fn get_science_purchase_points(&self) -> i32 {
        self.science_purchase_points
    }

    /// `getRankLevel()`.
    #[must_use]
    pub fn get_rank_level(&self) -> i32 {
        self.rank_level
    }

    /// `getSkillPointsLevelUp()`.
    #[must_use]
    pub fn get_skill_points_level_up(&self) -> i32 {
        self.level_up
    }

    /// `getSkillPointsLevelDown()`.
    #[must_use]
    pub fn get_skill_points_level_down(&self) -> i32 {
        self.level_down
    }

    /// `Player::setSkillPointsModifier`.
    pub fn set_skill_points_modifier(&mut self, exp_mod: f32) {
        self.skill_points_modifier = exp_mod;
    }

    /// `Player::getSkillPointsModifier`.
    #[must_use]
    pub fn get_skill_points_modifier(&self) -> f32 {
        self.skill_points_modifier
    }

    // ---- the radar half (Player.cpp) -----------------------------------------

    /// `Player::hasRadar` — "A positive number of radar producers, plus
    /// my radar is not disabled."
    #[must_use]
    pub fn has_radar(&self) -> bool {
        if self.radar_disabled && self.disable_proof_radar_count == 0 {
            return false; // Nope, no matter how many you have, if I say no, you don't have it
        }

        // Otherwise, check if I actually do have it.
        self.radar_count > 0
    }

    /// `Player::addRadar` — "The parameter object has just acquired a
    /// radar" (the edge sound is gated by `okToPlayRadarEdgeSound`,
    /// which consults TheVictoryConditions/TheInGameUI — absent from
    /// the kernel half; the sound is the client's, stated).
    pub fn add_radar(&mut self, disable_proof: bool) {
        // increment count
        self.radar_count += 1;

        if disable_proof {
            self.disable_proof_radar_count += 1; // Disable proof is also in the normal refcount
        }
    }

    /// `Player::removeRadar` — "The parameter object has is taking its
    /// radar away from the player" (the DEBUG_ASSERTCRASH on the
    /// underflow is debug-only; the edge sound ditto
    /// [`Self::add_radar`]'s note).
    pub fn remove_radar(&mut self, disable_proof: bool) {
        // decrement count
        self.radar_count -= 1;

        if disable_proof {
            self.disable_proof_radar_count -= 1; // Disable proof is also in the normal refcount
        }
    }

    /// `Player::disableRadar` — "No matter how many radar producers I
    /// have, I do not have radar."
    pub fn disable_radar(&mut self) {
        self.radar_disabled = true;
    }

    /// `Player::enableRadar` — "remove the restriction imposed by
    /// disableRadar."
    pub fn enable_radar(&mut self) {
        self.radar_disabled = false;
    }

    /// `Player::setPlayerType` (the AI instance lifecycle — create/delete
    /// — is the AI slice's, stated).
    pub fn set_player_type(&mut self, t: PlayerType) {
        self.player_type = t;
    }

    /// `Player::setPlayerRelationship`: note that this creates the entry
    /// if it doesn't exist.
    pub fn set_player_relationship(&mut self, that: usize, r: Relationship) {
        self.player_relations.insert(that as i32, r);
    }

    /// `Player::removePlayerRelationship`: the null form (`that == None`)
    /// clears the whole map; the empty map refuses.
    pub fn remove_player_relationship(&mut self, that: Option<usize>) -> bool {
        if !self.player_relations.is_empty() {
            match that {
                None => {
                    self.player_relations.clear();
                    true
                }
                Some(idx) => self.player_relations.remove(&(idx as i32)).is_some(),
            }
        } else {
            false
        }
    }

    /// `Player::setTeamRelationship`.
    pub fn set_team_relationship(&mut self, that: TeamID, r: Relationship) {
        self.team_relations.insert(that, r);
    }

    /// `Player::removeTeamRelationship`.
    pub fn remove_team_relationship(&mut self, that: Option<TeamID>) -> bool {
        if !self.team_relations.is_empty() {
            match that {
                None => {
                    self.team_relations.clear();
                    true
                }
                Some(id) => self.team_relations.remove(&id).is_some(),
            }
        } else {
            false
        }
    }

    /// `Player::addTeamToList`: the dedup scan, the push.
    pub fn add_team_to_list(&mut self, proto_slot: usize) {
        if self.team_prototypes.contains(&proto_slot) {
            return; // already present
        }
        self.team_prototypes.push(proto_slot);
    }

    /// `Player::removeTeamFromList`: the first-match erase.
    pub fn remove_team_from_list(&mut self, proto_slot: usize) {
        if let Some(pos) = self.team_prototypes.iter().position(|&p| p == proto_slot) {
            self.team_prototypes.remove(pos);
        }
    }

    /// `Player::preTeamDestroy`: the AI notification (stated) and the
    /// default-team nulling (TheSuperHackers @bugfix Mauller/Xezon
    /// 03/05/2025).
    fn pre_team_destroy(&mut self, team: TeamID) {
        // (m_ai->aiPreTeamDestroy(team) — the AI slice's)
        if self.default_team == team {
            self.default_team = TEAM_ID_INVALID;
        }
    }
}

// --- TeamPrototype (Team.cpp). -----------------------------------------------

/// The prototype's template half (`TeamTemplateInfo`, distilled to the
/// `updateState`-relevant fields).
#[derive(Clone, Debug, Default)]
pub struct TeamTemplate {
    /// `m_scriptOnCreate` (empty = none — the gates read emptiness).
    pub script_on_create: String,
    /// `m_scriptOnDestroyed`.
    pub script_on_destroyed: String,
    /// `m_scriptOnIdle`.
    pub script_on_idle: String,
    /// `m_scriptOnEnemySighted`.
    pub script_on_enemy_sighted: String,
    /// `m_scriptOnAllClear`.
    pub script_on_all_clear: String,
    /// `m_scriptOnUnitDestroyed`.
    pub script_on_unit_destroyed: String,
    /// `m_destroyedThreshold` (the onDestroyed fraction).
    pub destroyed_threshold: f32,
    /// `m_executeActions` (the production-condition gate).
    pub execute_actions: bool,
    /// `m_productionCondition`.
    pub production_condition: String,
    /// `findScriptByName`'s verdict (the catalog is the script slice's).
    pub production_script_exists: bool,
}

/// One team prototype: the owning player, the singleton flag, the
/// template and the team instances.
#[derive(Debug)]
pub struct TeamProto {
    /// `m_id` (`TeamPrototypeID` — the ++counter, reset by
    /// `TeamFactory::reset`; observable, not the identity).
    pub id: TeamProtoID,
    /// `m_name`.
    pub name: String,
    /// the interned name key (the registry's map key).
    pub name_key: NameKey,
    /// `m_owningPlayer` (the player index; never null once constructed —
    /// the ctor asserts).
    pub owner: usize,
    /// `TEAM_SINGLETON`.
    pub singleton: bool,
    /// `m_teamTemplate` (the distilled TeamTemplateInfo).
    pub template_: TeamTemplate,
    /// the team instances (`m_teamInstanceList` — the DLINK prepend:
    /// pushed to the back, iterated in reverse so the newest comes
    /// first). The entries are factory team slots.
    pub instances: Vec<usize>,
}

impl TeamProto {
    /// `getFirstItemIn_TeamInstanceList` — the head (the newest).
    fn first_instance(&self) -> Option<usize> {
        self.instances.last().copied()
    }
}

// --- Team (Team.cpp). --------------------------------------------------------

/// `Team`'s mutable runtime half — `Copy` so `updateState`'s
/// read-then-write split (the borrow puzzle's resolution: the partition
/// query holds `&Players` while the team state mutates) is exact — every
/// written field is read only after the member walk, so deferring the
/// write changes nothing observable.
#[derive(Clone, Copy, Debug, Default)]
pub struct TeamRuntimeState {
    /// `m_active`.
    pub active: bool,
    /// `m_created` (the first-update latch `setActive` arms).
    pub created: bool,
    /// `m_enteredOrExited`.
    pub entered_or_exited: bool,
    /// `m_seeEnemy`.
    pub see_enemy: bool,
    /// `m_prevSeeEnemy`.
    pub prev_see_enemy: bool,
    /// `m_checkEnemySighted` (derived at birth from the template's
    /// on-all-clear / on-enemy-sighted scripts).
    pub check_enemy_sighted: bool,
    /// `m_curUnits`.
    pub cur_units: i32,
    /// `m_destroyThreshold` (the onDestroyed trigger; `-1` is the
    /// don't-trigger-again latch).
    pub destroy_threshold: i32,
    /// `m_wasIdle`.
    pub was_idle: bool,
}

/// One team instance: the runtime state, the override relation maps and
/// the member list.
#[derive(Debug)]
pub struct Team {
    /// `m_id` (`TeamID` — the ++counter, era-scoped by
    /// `TeamFactory::reset`; the relation maps key on the VALUE, so a
    /// stale entry from a previous era aliases a same-idd new team,
    /// exactly as the original's value-keyed `hash_map` does).
    pub id: TeamID,
    /// the prototype slot (`m_proto`).
    pub proto: usize,
    /// the mutable half.
    pub state: TeamRuntimeState,
    /// `m_teamRelations` (the team-id override map).
    pub team_relations: BTreeMap<TeamID, Relationship>,
    /// `m_playerRelations` (the player-index override map).
    pub player_relations: BTreeMap<i32, Relationship>,
    /// `m_teamMemberList` (the DLINK prepend: pushed to the back,
    /// iterated in reverse).
    pub members: Vec<ObjectID>,
    /// `m_isRecruitablitySet` / `m_isRecruitable` (Team.h's inline pair —
    /// script-side state, carried for the surface).
    pub recruitable_set: bool,
    /// `m_isRecruitable`.
    pub recruitable: bool,
}

impl Team {
    /// `Team::setActive` (Team.h's inline): arms the created latch.
    pub fn set_active(&mut self) {
        if !self.state.active {
            self.state.created = true;
            self.state.active = true;
        }
    }

    /// `Team::setRecruitable` (Team.h's inline).
    pub fn set_recruitable(&mut self, recruitable: bool) {
        self.recruitable_set = true;
        self.recruitable = recruitable;
    }

    /// `Team::setOverrideTeamRelationship`: note that this creates the
    /// entry if it doesn't exist; `TEAM_ID_INVALID` is refused.
    pub fn set_override_team_relationship(&mut self, team_id: TeamID, r: Relationship) {
        if team_id != TEAM_ID_INVALID {
            self.team_relations.insert(team_id, r);
        }
    }

    /// `Team::removeOverrideTeamRelationship`: the `TEAM_ID_INVALID` form
    /// clears the whole map; the empty map refuses.
    pub fn remove_override_team_relationship(&mut self, team_id: TeamID) -> bool {
        if !self.team_relations.is_empty() {
            if team_id == TEAM_ID_INVALID {
                self.team_relations.clear();
                true
            } else {
                self.team_relations.remove(&team_id).is_some()
            }
        } else {
            false
        }
    }

    /// `Team::setOverridePlayerRelationship` (`PLAYER_INDEX_INVALID`
    /// refused).
    pub fn set_override_player_relationship(&mut self, player_index: i32, r: Relationship) {
        if player_index != PLAYER_INDEX_INVALID {
            self.player_relations.insert(player_index, r);
        }
    }

    /// `Team::removeOverridePlayerRelationship`.
    pub fn remove_override_player_relationship(&mut self, player_index: i32) -> bool {
        if !self.player_relations.is_empty() {
            if player_index == PLAYER_INDEX_INVALID {
                self.player_relations.clear();
                true
            } else {
                self.player_relations.remove(&player_index).is_some()
            }
        } else {
            false
        }
    }
}

// --- PlayerList + TeamFactory (the Players context). -------------------------

/// `PlayerList`: the 16 pre-created players, the count, the local player
/// and the slot bookkeeping.
#[derive(Debug)]
pub struct PlayerList {
    /// `m_local` (the slot; `None` only before the first `init`). The
    /// corpus surface (the golden dumps read it).
    pub local: Option<usize>,
    /// `m_playerCount` (the corpus surface).
    pub count: usize,
    /// `m_players[MAX_PLAYER_COUNT]` — the slots ("we only allocate a
    /// few of these, so don't bother pooling 'em"). The corpus surface.
    pub players: Vec<Player>,
    /// `m_slotIndices`.
    slot_indices: [i32; MAX_PLAYER_COUNT],
    /// `m_slotToPlayerIndices`.
    slot_to_player: [i32; MAX_SLOTS],
}

/// `TeamFactory`: the prototype registry (`std::map<NameKeyType, ...>` —
/// the BTreeMap keeps the key order the walks observe), the ++id
/// counters and the team arena.
#[derive(Debug)]
pub struct TeamFactory {
    /// the prototype arena (slots never reused; the duplicate-name
    /// orphan the release refusal leaks stays alive here, as the
    /// original's leaked object does). The corpus surface (the golden
    /// dumps walk it through the map).
    pub protos: Vec<Option<TeamProto>>,
    /// the registry (the std::map: the key order is the interning
    /// order). The corpus surface.
    pub map: BTreeMap<NameKey, usize>,
    /// `m_uniqueTeamPrototypeID` (the corpus surface).
    pub unique_team_proto_id: TeamProtoID,
    /// `m_uniqueTeamID` (the corpus surface).
    pub unique_team_id: TeamID,
    /// the team arena (slots never reused; a dead slot is `None`). The
    /// corpus surface.
    pub teams: Vec<Option<Team>>,
}

impl PlayerList {
    /// `PlayerList::teamAboutToBeDeleted` — every player drops its
    /// team-relation entry for the dying team (the factory's walk calls
    /// this through its list handle).
    pub(crate) fn team_about_to_be_deleted(&mut self, team: TeamID) {
        for p in &mut self.players {
            p.remove_team_relationship(Some(team));
        }
    }
}

/// The players/teams block: the `PlayerList` and the `TeamFactory` in
/// one explicit context (plan §3.3 rule 5 — no `ThePlayerList` /
/// `TheTeamFactory` globals), plus the name-key interning the lookups
/// key on and the money/energy/ranks/sciences block's data stores
/// (`TheScienceStore` / `TheRankInfoStore` — the data catalogs the rank
/// ladder and the purchase family consult; the load-time override
/// machinery is the rzh-data loader's, and a `new_game` reset does NOT
/// clear them, exactly as the original's process-lifetime subsystems).
#[derive(Debug)]
pub struct Players {
    /// `ThePlayerList`.
    pub list: PlayerList,
    /// `TheTeamFactory`.
    pub factory: TeamFactory,
    /// `TheScienceStore`.
    pub science_store: ScienceStore,
    /// `TheRankInfoStore`.
    pub rank_store: crate::sciences::RankStore,
    /// `TheNameKeyGenerator` (the sim's interning context; the real
    /// global's catalog is shared with data loading — the relative order
    /// of this block's own names is the observable).
    pub name_keys: NameKeyGenerator,
    /// `TheNetwork != nullptr` — the `newGame` local-selection gate.
    pub network_present: bool,
}

impl Default for Players {
    fn default() -> Self {
        Self::new()
    }
}

impl Players {
    /// `PlayerList::PlayerList` — the 16 pre-created players, pre-`init`
    /// (the birth events need a host: `init` fires them).
    #[must_use]
    pub fn new() -> Self {
        let mut players = Vec::with_capacity(MAX_PLAYER_COUNT);
        for i in 0..MAX_PLAYER_COUNT {
            players.push(Player::new(i));
        }
        Self {
            list: PlayerList {
                local: None,
                count: 0,
                players,
                slot_indices: [-1; MAX_PLAYER_COUNT],
                slot_to_player: [-1; MAX_SLOTS],
            },
            factory: TeamFactory {
                protos: Vec::new(),
                map: BTreeMap::new(),
                unique_team_proto_id: TEAM_PROTO_ID_INVALID,
                unique_team_id: TEAM_ID_INVALID,
                teams: Vec::new(),
            },
            science_store: ScienceStore::default(),
            rank_store: crate::sciences::RankStore::default(),
            name_keys: NameKeyGenerator::new(),
            network_present: false,
        }
    }

    // ---- the reads ------------------------------------------------------------

    /// `getPlayerCount()`.
    #[must_use]
    pub fn player_count(&self) -> usize {
        self.list.count
    }

    /// `getNthPlayer(i)` — the bounds refusal.
    #[must_use]
    pub fn nth_player(&self, i: i32) -> Option<&Player> {
        if i < 0 || i as usize >= MAX_PLAYER_COUNT {
            return None;
        }
        Some(&self.list.players[i as usize])
    }

    /// `getNeutralPlayer()` (slot 0).
    #[must_use]
    pub fn neutral_player(&self) -> &Player {
        &self.list.players[0]
    }

    /// `getLocalPlayer()`.
    #[must_use]
    pub fn local_player(&self) -> &Player {
        // (DEBUG_ASSERTCRASH(m_local != nullptr) — init guarantees it)
        let idx = self.list.local.unwrap_or(0);
        &self.list.players[idx]
    }

    /// `findPlayerWithNameKey` — the count-bounded walk (the index; the
    /// slot is the identity).
    #[must_use]
    pub fn find_player_with_name_key(&self, key: NameKey) -> Option<usize> {
        self.list
            .players
            .iter()
            .take(self.list.count)
            .find(|p| p.name_key == key)
            .map(|p| p.index)
    }

    /// `NAMEKEY(name)` — the interning entry point.
    pub fn name_key(&mut self, name: &str) -> NameKey {
        self.name_keys.name_to_key(name)
    }

    /// the live team for an id (the arena scan — a dead slot's id is
    /// gone; a stale relation from a previous era resolves to a
    /// same-idd new team, the original value-keyed `hash_map`'s exact
    /// aliasing).
    #[must_use]
    fn team_by_id(&self, id: TeamID) -> Option<&Team> {
        if id == TEAM_ID_INVALID {
            return None;
        }
        self.factory.teams.iter().flatten().find(|t| t.id == id)
    }

    /// the live team's slot for an id.
    #[must_use]
    fn team_slot_by_id(&self, id: TeamID) -> Option<usize> {
        if id == TEAM_ID_INVALID {
            return None;
        }
        self.factory
            .teams
            .iter()
            .position(|t| t.as_ref().is_some_and(|t| t.id == id))
    }

    /// the live team (mutable).
    fn team_mut(&mut self, slot: usize) -> &mut Team {
        self.factory.teams[slot].as_mut().expect("live team")
    }

    /// the team's prototype.
    #[must_use]
    fn team_proto(&self, team_slot: usize) -> &TeamProto {
        let proto_slot = self.factory.teams[team_slot]
            .as_ref()
            .expect("live team")
            .proto;
        self.factory.protos[proto_slot]
            .as_ref()
            .expect("live proto")
    }

    /// `Team::getControllingPlayer` — the prototype's owner.
    #[must_use]
    fn team_owner(&self, team_slot: usize) -> usize {
        self.team_proto(team_slot).owner
    }

    /// `Object::getControllingPlayer` (Object.h):
    /// `m_team->getControllingPlayer()`.
    #[must_use]
    pub fn controlling_player_of(&self, objects: &Objects, obj: ObjectID) -> Option<usize> {
        let team = objects.get(obj).team;
        let slot = self.team_slot_by_id(team)?;
        Some(self.team_owner(slot))
    }

    /// `Player::getRelationship(const Team *that)` — the player's
    /// team-override first, then that team's controlling player's entry,
    /// then `NEUTRAL`.
    #[must_use]
    pub fn player_get_relationship(&self, player: usize, that: TeamID) -> Relationship {
        let p = &self.list.players[player];
        if let Some(team) = self.team_by_id(that) {
            // do we have an override for that particular team? if so, return it.
            if let Some(&r) = p.team_relations.get(&team.id) {
                return r;
            }
            // hummm... well, do we have something for that team's player?
            // (thatPlayer != nullptr — the prototype's owner is never
            // null once constructed; the ctor asserts)
            let owner = self
                .team_proto(self.team_slot_by_id(team.id).expect("live team"))
                .owner;
            if let Some(&r) = p.player_relations.get(&(owner as i32)) {
                return r;
            }
        }
        Relationship::Neutral
    }

    /// `Team::getRelationship(const Team *that)` — the team's overrides,
    /// then its owner's view.
    #[must_use]
    pub fn team_get_relationship(&self, team: TeamID, that: TeamID) -> Relationship {
        let Some(t) = self.team_by_id(team) else {
            return Relationship::Neutral;
        };
        // do we have an override for that particular team? if so, return it.
        if let Some(that_team) = self.team_by_id(that) {
            if let Some(&r) = t.team_relations.get(&that_team.id) {
                return r;
            }
            // hummm... well, do we have an override for that team's player?
            let owner = self
                .team_proto(self.team_slot_by_id(that_team.id).expect("live team"))
                .owner;
            if let Some(&r) = t.player_relations.get(&(owner as i32)) {
                return r;
            }
        }
        // nope -- go with our Player's view on the matter.
        let owner = self
            .team_proto(self.team_slot_by_id(t.id).expect("live team"))
            .owner;
        self.player_get_relationship(owner, that)
    }

    /// `Object::getRelationship` (Object.cpp): the defector gates, then
    /// the team chain.
    #[must_use]
    pub fn object_get_relationship(
        &self,
        objects: &Objects,
        obj: ObjectID,
        that: ObjectID,
    ) -> Relationship {
        let o = objects.get(obj);
        let my_team = o.team;
        if my_team != TEAM_ID_INVALID {
            let that_obj = objects.get(that);
            // (getIsUndetectedDefector: so my AI does not give away my
            // position by auto acquire)
            if o.is_undetected_defector() {
                return Relationship::Neutral;
            }
            // (so I treat undetecteddefectors like they were my very own)
            if that_obj.is_undetected_defector() {
                return Relationship::Allies;
            }
            return self.team_get_relationship(my_team, that_obj.team);
        }
        Relationship::Neutral
    }

    // ---- the lifecycle --------------------------------------------------------

    /// The money/energy/ranks/sciences block's data load: the science
    /// and rank catalogs (the distilled `Science.ini` / `RankInfo.ini`
    /// the rzh-data loader would hand over; the load-time override
    /// machinery and the duplicate/unknown-name throws are its
    /// contract). The stores are NOT cleared by `reset`/`new_game` —
    /// process-lifetime data, exactly as the original's subsystems.
    pub fn load_stores(&mut self, sciences: &[crate::sciences::ScienceSpec], ranks: &[RankSpec]) {
        let mut keys = std::mem::take(&mut self.name_keys);
        self.science_store.load(&mut keys, sciences);
        self.rank_store.load(&mut keys, ranks);
        self.name_keys = keys;
    }

    /// `Player::init` — the full body (the common head: the identity
    /// resets, the radar zeroing, the energy re-init, the skill-point
    /// modifier; then the template branch — the money copy with the
    /// zero-wallet starting-cash deposit, the observer bits — or the
    /// neutral branch; then `resetRank` and the science-availability
    /// clears). The distilled template is the [`SideSpec`] (`None` =
    /// the neutral player); `starting_cash` is the resolved
    /// `TheGameInfo`/`TheGlobalData` figure the zero-wallet deposit
    /// reads (unused on the neutral branch).
    ///
    /// NOTE the pinned quirk (the players slice's): the relation maps
    /// are NOT cleared — they are born in the constructor and die in
    /// the destructor only, so entries survive `reset()`/`init()`
    /// within a process lifetime.
    fn init_player(
        &mut self,
        idx: usize,
        template: Option<&SideSpec>,
        starting_cash: u32,
        host: &mut dyn KernelHost,
    ) {
        {
            let p = &mut self.list.players[idx];
            p.is_player_dead = false;
            p.observer = false;
            p.default_team = TEAM_ID_INVALID;
            // (the squads, selection, tunnels, build lists, upgrades,
            // battle plans, stats — the economy and client slices')
            p.skill_points_modifier = 1.0;
            p.radar_count = 0;
            p.disable_proof_radar_count = 0;
            p.radar_disabled = false;
            p.energy.init(idx);
        }

        match template {
            Some(side) => {
                // the pt branch (distilled): the money copy — the
                // template's wallet wholesale (a fresh-parsed template
                // money is setStartingCash semantics: the amount, the
                // buckets zeroed) — the index fix, then the zero-wallet
                // starting-cash deposit (TheSuperHackers @bugfix "Now
                // correctly deposits the money and fixes its audio and
                // academy issues" — silent and untracked, but the
                // academy hook still fires).
                let intrinsic_sciences: Vec<ScienceType> = side
                    .intrinsic_sciences
                    .iter()
                    .map(|n| self.name_keys.name_to_key(n))
                    .collect();
                {
                    let p = &mut self.list.players[idx];
                    p.money.set_starting_cash(side.template_cash);
                    p.money.set_player_index(idx as i32);
                    if p.money.count_money() == 0 {
                        p.money.deposit(starting_cash, false, false, host);
                    }
                    p.intrinsic_sciences = intrinsic_sciences;
                    p.intrinsic_science_purchase_points = side.intrinsic_science_purchase_points;
                    p.name.clear();
                    p.name_key = NameKey::INVALID;
                    p.player_type = PlayerType::Computer;
                    p.observer = side.observer;
                    p.is_player_dead = side.observer; // observers are dead
                }
            }
            None => {
                // no player template? must be the neutral player!
                let name_key = self.name_keys.name_to_key("");
                let p = &mut self.list.players[idx];
                p.name.clear();
                p.name_key = name_key;
                p.player_type = PlayerType::Computer;
                // (the neutral branch's m_money.init() — setStartingCash(0);
                // NOTE the player index is NOT re-set here: a slot that
                // lived as a template player keeps its Money index across
                // the reset — the stale-index quirk the corpora pin)
                p.money.init();
                p.intrinsic_sciences.clear();
                p.intrinsic_science_purchase_points = 0;

                // neutral is always "allied" with self -- this is the only thing ever allied with neutral!
                p.player_relations
                    .insert(p.index as i32, Relationship::Allies);
            }
        }

        // reset rank info (m_generalName's fetch — the client's)
        self.reset_rank(idx, host);
        self.list.players[idx].sciences_disabled.clear();
        self.list.players[idx].sciences_hidden.clear();
        // (m_specialPowerReadyTimerList.clear() — the special-power slice's)
    }

    /// `PlayerList::init`: count 1, every slot's neutral birth, the slot
    /// arrays, the local player 0 (so `becomingLocalPlayer()` gets
    /// called appropriately — the transitions ride the host).
    pub fn init(&mut self, host: &mut dyn KernelHost) {
        self.list.count = 1;
        for i in 0..MAX_PLAYER_COUNT {
            self.init_player(i, None, 0, host);
        }

        self.list.slot_indices = [-1; MAX_PLAYER_COUNT];
        self.list.slot_to_player = [-1; MAX_SLOTS];

        self.set_local_player(Some(0), host);
    }

    /// `PlayerList::reset`: the AI delete walk (stated — no observable),
    /// the factory clear, the init.
    pub fn reset(&mut self, objects: &mut Objects, host: &mut dyn KernelHost) {
        // for (int i = 0; i < MAX_PLAYER_COUNT; i++)
        //     m_players[i]->deletePlayerAI();   — the AI slice's (no observable)
        self.factory.clear(objects, &mut self.list, host);
        self.init(host);
    }

    /// `PlayerList::setLocalPlayer` — the null refusal (null → neutral)
    /// and the `becomingLocalPlayer` transitions.
    pub fn set_local_player(&mut self, player: Option<usize>, host: &mut dyn KernelHost) {
        // can't set local player to null -- if you try, you get neutral.
        let player = player.unwrap_or(0);

        if self.list.local != Some(player) {
            // m_local can be null the very first time we call this.
            if let Some(old) = self.list.local {
                // Player::becomingLocalPlayer(false) — the client body is
                // the client phase's
                host.on_becoming_local_player(old, false);
            }
            self.list.local = Some(player);
            // Player::becomingLocalPlayer(true) — ditto
            host.on_becoming_local_player(player, true);
        }
    }

    /// `PlayerList::update` — ALL 16 slots (not the count!): the pinned
    /// quirk. `TheAI->UPDATE()` calls this from the AI subsystem's
    /// position (AI.cpp:363). Each player's update runs the real
    /// sabotage-expiry check and the money bucket rollover around the
    /// stated bodies (the AI update, the team generic-script walk, the
    /// academy stats, the retaliation-mode posting and the tunnel heal
    /// — the position hook's).
    pub fn update(&mut self, objects: &mut Objects, frame: u32, host: &mut dyn KernelHost) {
        for i in 0..MAX_PLAYER_COUNT {
            // (m_ai->update(); the team generic-script walk — the AI and
            // script-engine slices' bodies)

            // the power-sabotage expiry check (Player::update's):
            // "Tells us we're no longer sabotaged" — the strict >, then
            // the brownout re-derivation.
            let expired_brownout = {
                let e = &mut self.list.players[i].energy;
                if e.get_power_sabotaged_till_frame() != 0
                    && frame > e.get_power_sabotaged_till_frame()
                {
                    e.set_power_sabotaged_till_frame(0);
                    Some(!e.has_sufficient_power(frame))
                } else {
                    None
                }
            };
            if let Some(brown_out) = expired_brownout {
                self.on_power_brownout_change(objects, i, brown_out, frame);
            }

            // (the academy stats' update, the local player's
            // retaliation-mode posting, the tunnel heal — the AI,
            // script-engine and client slices' bodies)
            host.on_player_update(i);

            // m_money.updateIncomeBucket()
            self.list.players[i].money.update_income_bucket(frame);
        }
    }

    /// `PlayerList::newMap` (the AI's newMap — stated).
    pub fn new_map(&mut self) {
        // (m_ai->newMap() — the AI slice's)
    }

    /// `PlayerList::updateTeamStates` — the script engine's per-frame
    /// position (ScriptEngine.cpp:5596) walks all 16 players' prototype
    /// lists.
    pub fn update_team_states(
        &mut self,
        objects: &mut Objects,
        pm: &mut PartitionManager,
        host: &mut dyn KernelHost,
    ) {
        for i in 0..MAX_PLAYER_COUNT {
            // (the list is cloned — the sweep below mutates the lists)
            let proto_slots: Vec<usize> = self.list.players[i].team_prototypes.clone();
            for slot in proto_slots {
                self.proto_update_state(slot, objects, pm, host);
            }
        }
    }

    /// `PlayerList::validateTeam` — the name (with the non-singleton
    /// auto-create), the neutral default-team fallback.
    pub fn validate_team(&mut self, owner: &str, host: &mut dyn KernelHost) -> TeamID {
        // owner could be a player or team. first, check team names.
        match self.find_team(owner, host) {
            Some(id) => id,
            // DEBUG_CRASH(("no team or player named %s could be found!"))
            None => self.list.players[0].default_team,
        }
    }

    // ---- the mask queries -----------------------------------------------------

    /// `PlayerList::getPlayerFromMask` — the exact-equality scan.
    #[must_use]
    pub fn get_player_from_mask(&self, mask: PlayerMaskType) -> Option<&Player> {
        // DEBUG_CRASH(("Player does not exist for mask")) — the release
        // build returns the null
        self.list.players.iter().find(|p| p.mask() == mask) // mask not found
    }

    /// `PlayerList::getEachPlayerFromMask` — the peeling walk (the mask
    /// erasure; the exhaustion zeroes it).
    pub fn get_each_player_from_mask(&mut self, mask: &mut PlayerMaskType) -> Option<usize> {
        for i in 0..MAX_PLAYER_COUNT {
            let m = self.list.players[i].mask();
            if (*mask & m) != 0 {
                *mask &= !m;
                return Some(i);
            }
        }
        // DEBUG_CRASH(("No players found that contain any matching masks."))
        *mask = 0;
        None // mask not found
    }

    /// `PlayerList::getPlayersWithRelationship` — the relationship-to-mask
    /// query (the src's view of each candidate's DEFAULT TEAM).
    #[must_use]
    pub fn get_players_with_relationship(
        &self,
        src_player_index: i32,
        allowed_relationships: u32,
    ) -> PlayerMaskType {
        let mut ret_val = PLAYERMASK_NONE;

        if allowed_relationships == 0 {
            return ret_val;
        }

        let Some(src) = self.nth_player(src_player_index) else {
            return ret_val;
        };

        if (allowed_relationships & ALLOW_SAME_PLAYER) != 0 {
            ret_val |= src.mask();
        }

        for i in 0..self.list.count {
            let player = &self.list.players[i];
            if player.index == src.index {
                continue;
            }
            match self.player_get_relationship(src.index, player.default_team) {
                Relationship::Enemies => {
                    if (allowed_relationships & ALLOW_ENEMIES) != 0 {
                        ret_val |= player.mask();
                    }
                }
                Relationship::Allies => {
                    if (allowed_relationships & ALLOW_ALLIES) != 0 {
                        ret_val |= player.mask();
                    }
                }
                Relationship::Neutral => {
                    if (allowed_relationships & ALLOW_NEUTRAL) != 0 {
                        ret_val |= player.mask();
                    }
                }
            }
        }

        ret_val
    }

    // ---- newGame ---------------------------------------------------------------

    /// `PlayerList::newGame` — the distilled sides walk: the reset, the
    /// player creation with the local-selection precedence, the team
    /// factory's `initFromSides`, the enemies/allies/self/neutral
    /// assignment, the default teams and the slot indices.
    pub fn new_game(
        &mut self,
        sides: &SidesSpec,
        objects: &mut Objects,
        host: &mut dyn KernelHost,
    ) {
        self.reset(objects, host);

        // the zero-wallet deposit's starting cash: TheGameInfo's figure
        // when a game is in progress, TheGlobalData's default otherwise
        let starting_cash = if sides.has_game_info {
            sides.game_info.starting_cash
        } else {
            sides.default_starting_cash
        };

        // ok, now create the rest of players we need.
        let mut set_local = false;
        for side in &sides.sides {
            if side.player_name.is_empty() {
                continue; // it's neutral, which we've already done, so skip it.
            }

            let count = self.list.count;
            self.list.count += 1;
            let idx = count;

            // initFromDict, distilled: init(pt) with the template (the
            // money/energy/rank init included — the money block's zero
            // -wallet deposit rides the template branch), then the
            // name, then the type (the skirmish script duplication is
            // the script-engine slice's, stated).
            self.init_player(idx, Some(side), starting_cash, host);
            let name_key = self.name_keys.name_to_key(&side.player_name);
            {
                let p = &mut self.list.players[idx];
                p.name = side.player_name.clone();
                p.name_key = name_key;
                p.set_player_type(if side.is_human {
                    PlayerType::Human
                } else {
                    PlayerType::Computer
                });
            }

            // Multiplayer override
            if side.multiplayer_is_local {
                self.set_local_player(Some(idx), host);
                set_local = true;
            }

            if !set_local && !self.network_present && side.is_human {
                self.set_local_player(Some(idx), host);
                set_local = true;
            }

            // (Set the build list — the economy slice's, stated)
        }

        if !set_local {
            for i in 0..sides.sides.len() {
                if let Some(idx) = self
                    .nth_player(i as i32)
                    .filter(|p| p.index != 0)
                    .map(|p| p.index)
                {
                    self.list.players[idx].set_player_type(PlayerType::Human);
                    self.set_local_player(Some(idx), host);
                    break;
                }
            }
        }

        // must reset teams *after* creating players.
        self.factory
            .init_from_sides(sides, objects, &mut self.list, &mut self.name_keys, host);

        for side in &sides.sides {
            let key = self.name_keys.name_to_key(&side.player_name);
            let Some(idx) = self.find_player_with_name_key(key) else {
                continue;
            };

            for tok in &side.enemies {
                let k2 = self.name_keys.name_to_key(tok);
                if let Some(p2) = self.find_player_with_name_key(k2) {
                    self.list.players[idx].set_player_relationship(p2, Relationship::Enemies);
                } else {
                    // DEBUG_LOG(("unknown enemy %s")) — stated
                }
            }

            for tok in &side.allies {
                let k2 = self.name_keys.name_to_key(tok);
                if let Some(p2) = self.find_player_with_name_key(k2) {
                    self.list.players[idx].set_player_relationship(p2, Relationship::Allies);
                } else {
                    // DEBUG_LOG(("unknown ally %s")) — stated
                }
            }

            // finally, make sure self & neutral are correct.
            self.list.players[idx].set_player_relationship(idx, Relationship::Allies);
            if idx != 0 {
                self.list.players[idx].set_player_relationship(0, Relationship::Neutral);
            }

            self.set_default_team(idx, host);
        }

        if sides.has_game_info {
            self.assign_slot_indices(&sides.game_info);
        }
    }

    /// `Player::setDefaultTeam` — the `"team" + playerName` concat, the
    /// `findTeam`, the `setActive`.
    fn set_default_team(&mut self, player: usize, host: &mut dyn KernelHost) {
        let tname = format!("team{}", self.list.players[player].name);
        if let Some(dt) = self.find_team(&tname, host)
            && let Some(slot) = self.team_slot_by_id(dt)
        {
            self.list.players[player].default_team = dt;
            self.team_mut(slot).set_active();
        }
        // (DEBUG_ASSERTCRASH(dt, ("no team")) — the release build
        // continues with the null default team; the corpora pin the
        // missing-prototype path)
    }

    /// `PlayerList::assignSlotIndices` — the `player%d` walk over the
    /// occupied slots.
    fn assign_slot_indices(&mut self, game_info: &GameInfoSpec) {
        for &slot_idx in &game_info.occupied_slots {
            if !(0..MAX_SLOTS as i32).contains(&slot_idx) {
                continue; // (the const-slot bounds the walk guards)
            }
            let player_name = format!("player{slot_idx}");
            let key = self.name_keys.name_to_key(&player_name);
            if let Some(player) = self.find_player_with_name_key(key) {
                self.set_slot_index(player, slot_idx);
            }
        }
    }

    /// `PlayerList::setSlotIndex`.
    pub fn set_slot_index(&mut self, player_index: usize, slot_index: i32) {
        if player_index < MAX_PLAYER_COUNT {
            let old_slot_index = self.list.slot_indices[player_index];
            if (0..MAX_SLOTS as i32).contains(&old_slot_index) {
                self.list.slot_to_player[old_slot_index as usize] = -1;
            }
            self.list.slot_indices[player_index] = slot_index;
            if (0..MAX_SLOTS as i32).contains(&slot_index) {
                self.list.slot_to_player[slot_index as usize] = player_index as i32;
            }
        }
    }

    /// `PlayerList::getSlotIndex`.
    #[must_use]
    pub fn get_slot_index(&self, player_index: usize) -> i32 {
        if player_index < MAX_PLAYER_COUNT {
            self.list.slot_indices[player_index]
        } else {
            -1
        }
    }

    /// `PlayerList::getPlayerIndexFromSlotIndex`.
    #[must_use]
    pub fn get_player_index_from_slot_index(&self, slot_index: i32) -> i32 {
        if (0..MAX_SLOTS as i32).contains(&slot_index) {
            self.list.slot_to_player[slot_index as usize]
        } else {
            -1
        }
    }

    /// `PlayerList::getPlayerFromSlotIndex`.
    #[must_use]
    pub fn get_player_from_slot_index(&self, slot_index: i32) -> Option<&Player> {
        let player_index = self.get_player_index_from_slot_index(slot_index);
        if player_index >= 0 && (player_index as usize) < self.list.count {
            Some(&self.list.players[player_index as usize])
        } else {
            None
        }
    }

    // ---- the member-list choreography (Object.cpp's setTeam family) -----------

    /// `Object::setTeam` — the `isPlayerActive` refusal (the dead or
    /// observer player's objects go to the neutral player's default
    /// team), then the temporary-team path.
    pub fn set_team(
        &mut self,
        objects: &mut Objects,
        obj: ObjectID,
        team: Option<TeamID>,
        host: &mut dyn KernelHost,
    ) {
        // In order to prevent spawning useful units for a player after he dies, we
        // just assign objects to the neutral player if we try to misbehave.
        let team = match team {
            Some(id) => {
                let owner = self.team_slot_by_id(id).map(|s| self.team_owner(s));
                let active = owner.is_some_and(|o| self.list.players[o].is_player_active());
                if !active {
                    Some(self.list.players[0].default_team)
                } else {
                    Some(id)
                }
            }
            None => None,
        };

        // setTemporaryTeam(team) → setOrRestoreTeam(team, restoring=false)
        self.set_or_restore_team(objects, obj, team, false, host);
        // (m_originalTeamName = m_team ? m_team->getName() : "" — display
        // state, not dumped)
    }

    /// `Object::setOrRestoreTeam` — the member-list swap with the
    /// `becomingTeamMember` calls and the `onCapture` hook.
    fn set_or_restore_team(
        &mut self,
        objects: &mut Objects,
        obj: ObjectID,
        team: Option<TeamID>,
        restoring: bool,
        host: &mut dyn KernelHost,
    ) {
        // don't do anything if the team hasn't changed (the pointer
        // compare is the id compare: live ids are unique)
        let team_id = team.unwrap_or(TEAM_ID_INVALID);
        if objects.get(obj).team == team_id {
            return;
        }
        let old_id = objects.get(obj).team;
        let old_slot = self.team_slot_by_id(old_id);
        let new_slot = self.team_slot_by_id(team_id);

        // the event args, pre-copied (the owners) before the mutation
        let old_owner = old_slot.map(|s| self.team_owner(s));
        let new_owner = new_slot.map(|s| self.team_owner(s));

        // Before Switch //////////////////////////
        if let Some(os) = old_slot {
            let members = &mut self.factory.teams[os].as_mut().expect("live team").members;
            if let Some(pos) = members.iter().position(|&m| m == obj) {
                members.remove(pos);
                if let Some(owner) = old_owner {
                    // m_team->getControllingPlayer()->becomingTeamMember(this, false)
                    host.on_becoming_team_member(obj, owner, false);
                }
            }
        }

        // Switch //////////////////////////
        objects.get_mut(obj).team = team_id;

        // After Switch //////////////////////////
        if team_id != TEAM_ID_INVALID {
            let ns = new_slot.expect("live team");
            let members = &mut self.factory.teams[ns].as_mut().expect("live team").members;
            if !members.contains(&obj) {
                members.push(obj);
                if let Some(owner) = new_owner {
                    host.on_becoming_team_member(obj, owner, true);
                }
            }
            // (now, adjust the attitude of the unit to its new team — the
            // AI update's setAttitude / attack priority; the AI slice's.
            // The drawable's changedTeam is the client's.)
        }

        // This can't just go in ::defect, because some things just do setTeam...
        // And it needs to happen after the switch.
        if old_slot.is_some() && new_slot.is_some() && !restoring {
            // obj->onCapture(oldTeam's player, team's player) — the body is
            // the module framework's (contain, body, experience
            // notifications); the hook pins the call and its player pair
            // (the pair can name the SAME player — the original fires on
            // any team change).
            host.on_capture(
                obj,
                old_owner.unwrap_or(usize::MAX),
                new_owner.unwrap_or(usize::MAX),
            );
        }
        // (the radar resort and TheInGameUI's objectChangedTeam are the
        // client's — stated, not modelled)
    }

    // ---- Team::updateState's state half ---------------------------------------

    /// `TeamPrototype::updateState` — the instance walk (newest first),
    /// then the empty-team removal sweep (the runtime team deletion —
    /// `deleteTeam`'s comment: "the team itself will be deleted in
    /// updateState").
    fn proto_update_state(
        &mut self,
        proto_slot: usize,
        objects: &mut Objects,
        pm: &mut PartitionManager,
        host: &mut dyn KernelHost,
    ) {
        // the instance walk (cloned — the sweep mutates the list)
        let instances: Vec<usize> = self
            .factory
            .protos
            .get(proto_slot)
            .and_then(|p| p.as_ref())
            .map_or_else(Vec::new, |p| p.instances.clone());
        for ts in instances.iter().rev() {
            self.update_state_one(*ts, objects, pm, host);
        }

        /* remove empty teams. */
        loop {
            let mut done = true;
            let instances: Vec<usize> = self
                .factory
                .protos
                .get(proto_slot)
                .and_then(|p| p.as_ref())
                .map_or_else(Vec::new, |p| p.instances.clone());
            'walk: for ts in instances.iter().rev() {
                let Some(team) = self.factory.teams[*ts].as_ref() else {
                    continue;
                };
                if !team.members.is_empty() {
                    continue;
                }
                // Team has no members.
                if self.factory.protos[proto_slot]
                    .as_ref()
                    .expect("live proto")
                    .singleton
                {
                    continue; // Don't delete singleton teams, even if they are empty.
                }
                let owner = self.factory.protos[proto_slot]
                    .as_ref()
                    .expect("live proto")
                    .owner;
                if self.list.players[owner].default_team == team.id {
                    // This is the player's default team, so don't remove it.
                    continue;
                }
                // don't delete inactive teams - they are under construction
                if !team.state.active {
                    continue;
                }
                // So remove it
                let id = team.id;
                self.factory.team_about_to_be_deleted(id, &mut self.list);
                self.factory
                    .delete_team_inner(*ts, objects, &mut self.list, host);
                done = false;
                break 'walk; // Not sure what state the iterator is in after deleting a member of the list. jba
            }
            if done {
                break;
            }
        }
    }

    /// `Team::updateState` — the state half.
    fn update_state_one(
        &mut self,
        team_slot: usize,
        objects: &mut Objects,
        pm: &mut PartitionManager,
        host: &mut dyn KernelHost,
    ) {
        // the compute pass: &self (the query context) + &mut objects +
        // &mut pm + &mut host — the write lands after the walk
        let (mut state, proto_slot, members, team_id) = {
            let team = self.factory.teams[team_slot].as_ref().expect("live team");
            (team.state, team.proto, team.members.clone(), team.id)
        };
        let info = {
            let proto = self.factory.protos[proto_slot]
                .as_ref()
                .expect("live proto");
            proto.template_.clone()
        };

        state.entered_or_exited = false;
        if !state.active {
            self.team_mut(team_slot).state = state;
            return;
        }
        if state.created {
            state.created = false;
            // Run the on create script, if any.
            if !info.script_on_create.is_empty() {
                host.on_team_script(team_id, TeamScriptKind::OnCreate);
            }

            // Set up info for the onDestroyed script, if needed.
            if !info.script_on_destroyed.is_empty() {
                state.cur_units = members.len() as i32;
                // Int = Int - (Int * Real): the promoted float product, the
                // truncating assignment back.
                let f = members.len() as f32;
                state.destroy_threshold = (f - f * info.destroyed_threshold) as i32;
                if state.destroy_threshold > state.cur_units - 1 {
                    state.destroy_threshold = state.cur_units - 1;
                }
                if state.destroy_threshold < 0 {
                    state.destroy_threshold = 0;
                }
            }
        }

        // Do enemy sighted/on clear checks.
        if state.check_enemy_sighted {
            state.prev_see_enemy = state.see_enemy;
            state.see_enemy = false;
            let mut any_alive_in_team = false; // If we're all dead, don't do all clear.
            // only consider enemies.
            for &member in members.iter().rev() {
                if objects.get(member).is_effectively_dead() {
                    continue;
                }

                // and only stuff that is not dead
                let filter_team = crate::query::PartitionFilterRelationship::new(
                    member,
                    crate::query::RELATIONSHIP_FILTER_ALLOW_ENEMIES,
                );
                let filter_alive = crate::query::PartitionFilterAlive;
                let filter_map_status = crate::query::PartitionFilterSameMapStatus::new(member);
                let filters: [&dyn crate::query::PartitionFilter; 3] =
                    [&filter_team, &filter_alive, &filter_map_status];

                let vision_range = objects.get(member).vision_range;
                any_alive_in_team = true;
                let (obj, _, _) = pm.get_closest_object_from_object(
                    objects,
                    self,
                    member,
                    vision_range,
                    DistanceCalculationType::FromCenter2d,
                    &filters,
                );
                if obj.is_some() {
                    state.see_enemy = true;
                    break;
                }
            }
            if any_alive_in_team && state.prev_see_enemy != state.see_enemy {
                if state.see_enemy {
                    // fire onEnemySighted
                    host.on_team_script(team_id, TeamScriptKind::OnEnemySighted);
                } else {
                    // fire on all clear.
                    host.on_team_script(team_id, TeamScriptKind::OnAllClear);
                }
            }
        }

        // Do onDestroyed checks.
        if !info.script_on_destroyed.is_empty() {
            let prev_units = state.cur_units;
            state.cur_units = 0;
            for &member in members.iter().rev() {
                if objects.get(member).is_effectively_dead() {
                    continue;
                }
                state.cur_units += 1;
            }
            if state.cur_units != prev_units && state.cur_units <= state.destroy_threshold {
                host.on_team_script(team_id, TeamScriptKind::OnDestroyed);
                state.destroy_threshold = -1; // don't trigger again.
            }
        }

        // Do onIdle checks.
        if !info.script_on_idle.is_empty() {
            let mut is_idle = true;
            let mut any_alive_in_team = false; // If we're all dead, don't do all clear.
            for &member in members.iter().rev() {
                let m = objects.get(member);
                if m.is_effectively_dead() {
                    continue;
                }
                if !m.has_ai {
                    continue;
                }
                any_alive_in_team = true;
                if !m.ai_idle {
                    is_idle = false;
                }
            }
            if any_alive_in_team && is_idle && state.was_idle {
                host.on_team_script(team_id, TeamScriptKind::OnIdle);
            }
            state.was_idle = is_idle;
        }

        self.team_mut(team_slot).state = state;
    }

    /// `Team::notifyTeamOfObjectDeath` — the unit-destroyed script gate.
    pub fn notify_team_of_object_death(&mut self, team: TeamID, host: &mut dyn KernelHost) {
        let Some(slot) = self.team_slot_by_id(team) else {
            return;
        };
        let proto_slot = self.factory.teams[slot].as_ref().expect("live team").proto;
        let info = &self.factory.protos[proto_slot]
            .as_ref()
            .expect("live proto")
            .template_;
        if info.script_on_unit_destroyed.is_empty() {
            return;
        }
        // TheScriptEngine->runScript(pInfo->m_scriptOnUnitDestroyed, this)
        host.on_team_script(team, TeamScriptKind::OnUnitDestroyed);
    }

    // ---- the Players-level factory surface (the split-borrow wrappers) -------

    /// `TeamFactory::createTeam` (the inactive create + `setActive`).
    pub fn create_team(&mut self, name: &str, host: &mut dyn KernelHost) -> Option<TeamID> {
        let proto_slot = self
            .factory
            .find_team_prototype(name, &mut self.name_keys)?;
        let slot = self.factory.create_inactive_team(proto_slot, host);
        let id = self.factory.teams[slot].as_ref().expect("live team").id;
        self.factory.teams[slot]
            .as_mut()
            .expect("live team")
            .set_active();
        Some(id)
    }

    /// `TeamFactory::createInactiveTeam`.
    pub fn create_inactive_team(
        &mut self,
        name: &str,
        host: &mut dyn KernelHost,
    ) -> Option<TeamID> {
        let proto_slot = self
            .factory
            .find_team_prototype(name, &mut self.name_keys)?;
        let slot = self.factory.create_inactive_team(proto_slot, host);
        Some(self.factory.teams[slot].as_ref().expect("live team").id)
    }

    /// `TeamFactory::createTeamOnPrototype` (by name — the corpus
    /// surface).
    pub fn create_team_on_prototype(
        &mut self,
        name: &str,
        host: &mut dyn KernelHost,
    ) -> Option<TeamID> {
        let proto_slot = self
            .factory
            .find_team_prototype(name, &mut self.name_keys)?;
        self.factory.create_team_on_prototype(proto_slot, host)
    }

    /// `TeamFactory::findTeam` — the non-singleton's first instance
    /// auto-create (with the production-condition events the create
    /// fires).
    pub fn find_team(&mut self, name: &str, host: &mut dyn KernelHost) -> Option<TeamID> {
        let proto_slot = self
            .factory
            .find_team_prototype(name, &mut self.name_keys)?;
        if let Some(t) = self.factory.protos[proto_slot]
            .as_ref()
            .expect("live proto")
            .first_instance()
        {
            Some(self.factory.teams[t].as_ref().expect("live team").id)
        } else if !self.factory.protos[proto_slot]
            .as_ref()
            .expect("live proto")
            .singleton
        {
            let slot = self.factory.create_inactive_team(proto_slot, host);
            Some(self.factory.teams[slot].as_ref().expect("live team").id)
        } else {
            None
        }
    }

    /// `TeamFactory::findTeamPrototype(name)` — the prototype's id (the
    /// corpus surface).
    pub fn find_team_prototype_id(&mut self, name: &str) -> Option<TeamProtoID> {
        let proto_slot = self
            .factory
            .find_team_prototype(name, &mut self.name_keys)?;
        Some(
            self.factory.protos[proto_slot]
                .as_ref()
                .expect("live proto")
                .id,
        )
    }

    /// `TeamFactory::findTeamPrototypeByID` — the prototype's id.
    #[must_use]
    pub fn find_team_prototype_by_id(&self, id: TeamProtoID) -> Option<TeamProtoID> {
        self.factory.map.values().find_map(|&slot| {
            self.factory.protos[slot]
                .as_ref()
                .filter(|p| p.id == id)
                .map(|p| p.id)
        }) // not found
    }

    /// `TeamFactory::findTeamByID`.
    #[must_use]
    pub fn find_team_by_id(&self, id: TeamID) -> Option<TeamID> {
        // simple case
        if id == TEAM_ID_INVALID {
            return None;
        }
        // search all prototypes for the matching team ID
        for slot in self.factory.map.values() {
            for ts in self.factory.protos[*slot]
                .as_ref()
                .expect("live proto")
                .instances
                .iter()
                .rev()
            {
                if self.factory.teams[*ts].as_ref().expect("live team").id == id {
                    return Some(id);
                }
            }
        }
        None
    }

    /// `Team::setControllingPlayer` — the prototype's owner move (the
    /// lists) and the member capture walk (TheSuperHackers @fix xezon
    /// 07/12/2025: the non-retail branch notifies every member). `None`
    /// is "not allowed, but is caught by
    /// TeamPrototype::setControllingPlayer()" — the early refusal.
    pub fn set_controlling_player(
        &mut self,
        team: TeamID,
        new_controller: Option<usize>,
        host: &mut dyn KernelHost,
    ) {
        let Some(new_controller) = new_controller else {
            return;
        };
        let Some(slot) = self.team_slot_by_id(team) else {
            return;
        };
        // nullptr is not allowed, but is caught by
        // TeamPrototype::setControllingPlayer()
        let proto_slot = self.factory.teams[slot].as_ref().expect("live team").proto;
        let old_owner = self.factory.protos[proto_slot]
            .as_ref()
            .expect("live proto")
            .owner;

        {
            let owner = self.factory.protos[proto_slot]
                .as_ref()
                .expect("live proto")
                .owner;
            self.list.players[owner].remove_team_from_list(proto_slot);
        }
        self.factory.protos[proto_slot]
            .as_mut()
            .expect("live proto")
            .owner = new_controller;
        // impossible to get here with a nullptr pointer.
        self.list.players[new_controller].add_team_to_list(proto_slot);

        // The units' onCapture(oldOwner, newController) — the module
        // bodies are the module framework's; the hook pins the calls.
        let members: Vec<ObjectID> = self.factory.teams[slot]
            .as_ref()
            .expect("live team")
            .members
            .clone();
        for obj in members.iter().rev() {
            host.on_capture(*obj, old_owner, new_controller);
        }
    }

    // ---- the ranks/sciences block (Player.cpp's progression half) -------------

    /// `Player::hasPrereqsForScience` — the store delegate over the
    /// player's science list.
    #[must_use]
    pub fn has_prereqs_for_science(&self, player: usize, t: ScienceType) -> bool {
        self.science_store
            .player_has_prereqs_for_science(&self.list.players[player].sciences, t)
    }

    /// `ScienceStore::playerHasRootPrereqsForScience` through the
    /// player ("this is a subtle call, and should ALMOST NEVER be
    /// called by external code").
    #[must_use]
    pub fn has_root_prereqs_for_science(&self, player: usize, t: ScienceType) -> bool {
        self.science_store
            .player_has_root_prereqs_for_science(&self.list.players[player].sciences, t)
    }

    /// `ScienceStore::getPurchasableSciences` through the player.
    #[must_use]
    pub fn get_purchasable_sciences(&self, player: usize) -> (Vec<ScienceType>, Vec<ScienceType>) {
        self.science_store
            .get_purchasable_sciences(&self.list.players[player].sciences)
    }

    /// `ScienceStore::getScienceFromInternalName` — the interning (no
    /// validation: an unknown name is a key that queries miss).
    pub fn science_from_name(&mut self, name: &str) -> ScienceType {
        self.name_keys.name_to_key(name)
    }

    /// `ScienceStore::getInternalNameForScience` — the reverse
    /// interning (`SCIENCE_INVALID` and unknown keys read as none).
    #[must_use]
    pub fn internal_name_for_science(&self, science: ScienceType) -> Option<&[u8]> {
        self.name_keys.key_to_name(science)
    }

    /// `Player::addSkillPoints` — "returns TRUE if the player
    /// gained/lost levels as a result": the modifier's ceil
    /// (`REAL_TO_INT_CEIL(m_skillPointsModifier * INT_TO_REAL(delta))`
    /// of the Zero Hour non-retail build), the point cap (the LOWEST
    /// point of the cap level, not the highest), then the level-up
    /// loop over `setRankLevel` (whose `INT_MAX` levelUp sentinel
    /// terminates it).
    ///
    /// The point-cap read (`getRankInfo(levelCap)->m_skillPointsNeeded`)
    /// null-derefs in the original when the ladder is empty or the
    /// limit is below 1 — out of contract there (the corpora load a
    /// ladder first); the port panics with the same meaning.
    pub fn add_skill_points(
        &mut self,
        player: usize,
        delta: i32,
        rank_level_limit: i32,
        host: &mut dyn KernelHost,
    ) -> bool {
        let delta = crate::fastmath::real_to_int_ceil(
            self.list.players[player].skill_points_modifier * (delta as f32),
        );

        if delta == 0 {
            return false;
        }

        let level_cap = rank_level_limit.min(self.rank_store.rank_level_count());
        // Cap at lowest point of cap level, not highest.
        let point_cap = self
            .rank_store
            .rank_info(level_cap)
            .expect("addSkillPoints: the ladder covers the level cap (the original null-derefs)")
            .skill_points_needed;

        let mut level_gained = false;
        {
            let p = &mut self.list.players[player];
            p.skill_points = point_cap.min(p.skill_points + delta);
        }
        loop {
            let (skill_points, level_up, rank_level) = {
                let p = &self.list.players[player];
                (p.skill_points, p.level_up, p.rank_level)
            };
            if skill_points < level_up {
                break;
            }
            // LevelUp gets increased as a side effect of setRankLevel, and
            // this won't infinitely loop, because when there are no more
            // levels to be gained, m_levelUp is set to INT_MAX.
            self.set_rank_level(player, rank_level + 1, rank_level_limit, host);
            level_gained = true;
        }

        level_gained
    }

    /// `Player::resetSciences` — "reset the sciences to just the
    /// intrinsic ones from the playertemplate, if any": the wipe, the
    /// intrinsic assignment, the per-rank grants through `addScience`
    /// (each firing the script-engine notify), then the tail loop
    /// notifying EVERY science again — the rank-granted ones twice
    /// (the double-notify quirk the corpora pin).
    pub fn reset_sciences(&mut self, player: usize, host: &mut dyn KernelHost) {
        let intrinsic = self.list.players[player].intrinsic_sciences.clone();
        let rank_level = self.list.players[player].rank_level;
        {
            let p = &mut self.list.players[player];
            p.sciences.clear();
            p.sciences.extend(intrinsic.iter().copied());
        }

        for i in 1..=rank_level {
            let granted: Vec<ScienceType> = self
                .rank_store
                .rank_info(i)
                .map_or_else(Vec::new, |r| r.sciences_granted.clone());
            for s in granted {
                self.list.players[player].add_science(s, host);
            }
        }

        let sciences: Vec<ScienceType> = self.list.players[player].sciences.clone();
        for s in sciences {
            host.on_acquired_science(player, s);
        }
    }

    /// `Player::resetRank` — "reset rank to 1": the level, the points,
    /// the next-level threshold (`INT_MAX` past the ladder's end), the
    /// science-purchase points (the template's intrinsic grant plus
    /// rank 1's), the science wipe and the [`Self::reset_sciences`]
    /// walk.
    pub fn reset_rank(&mut self, player: usize, host: &mut dyn KernelHost) {
        let intrinsic_spp = self.list.players[player].intrinsic_science_purchase_points;
        let rank1_grant = self
            .rank_store
            .rank_info(1)
            .map_or(0, |r| r.science_purchase_points_granted);
        let next_needed = self
            .rank_store
            .rank_info(2)
            .map_or(i32::MAX, |r| r.skill_points_needed);
        {
            let p = &mut self.list.players[player];
            p.rank_level = 1;
            p.skill_points = 0;
            p.level_up = next_needed;
            p.level_down = 0;
            p.sciences.clear();
            p.science_purchase_points = intrinsic_spp + rank1_grant;
            // (m_generalName = TheGameText->fetch("SCIENCE:GeneralName") —
            // the client's)
        }
        self.reset_sciences(player, host);
    }

    /// `Player::setRankLevel` — "returns TRUE if rank level really
    /// changed": the clamps (the ladder's ends, then the limit), the
    /// no-change refusal, the downgrade wipe ("when downgrading in
    /// rank, you lose everything. oh well..." — `resetRank`, which
    /// drops the level back to 1 so the re-grant walk below re-runs
    /// the whole range), the per-level grants (the purchase points
    /// with the negative clamp, the skill-point floor raises, the
    /// sciences), and the threshold updates.
    pub fn set_rank_level(
        &mut self,
        player: usize,
        mut new_level: i32,
        rank_level_limit: i32,
        host: &mut dyn KernelHost,
    ) -> bool {
        if new_level < 1 {
            new_level = 1;
        } else if new_level > self.rank_store.rank_level_count() {
            new_level = self.rank_store.rank_level_count();
        }

        if new_level > rank_level_limit {
            new_level = rank_level_limit;
        }

        if new_level == self.list.players[player].rank_level {
            return false;
        }

        // (oldSPP — the deferred UI notification's; the client's)
        if new_level < self.list.players[player].rank_level {
            // when downgrading in rank, you lose everything. oh well...
            self.reset_rank(player, host);
        }

        // (m_rankLevel is 1 again after the downgrade's resetRank — the
        // walk re-grants from there, "you lose everything" then rebuild)
        let cur = self.list.players[player].rank_level;
        for i in (cur + 1)..=new_level {
            let info = self.rank_store.rank_info(i).map(|r| {
                (
                    r.science_purchase_points_granted,
                    r.skill_points_needed,
                    r.sciences_granted.clone(),
                )
            });
            // (DEBUG_ASSERTCRASH(rank, ("rank should never be null
            // here")) — the clamps guarantee it)
            if let Some((granted_spp, needed, granted_sciences)) = info {
                let p = &mut self.list.players[player];
                // do this directly, so we can defer the UI notification
                // till the end
                p.science_purchase_points += granted_spp;
                if p.science_purchase_points < 0 {
                    p.science_purchase_points = 0;
                }

                if p.skill_points < needed {
                    p.skill_points = needed;
                }

                for s in granted_sciences {
                    p.add_science(s, host);
                }

                p.level_down = needed;
            }
        }

        let next_needed = self
            .rank_store
            .rank_info(new_level + 1)
            .map_or(i32::MAX, |r| r.skill_points_needed);
        {
            let p = &mut self.list.players[player];
            p.level_up = next_needed;
            p.rank_level = new_level;
        }
        // (DEBUG_ASSERTCRASH(m_skillPoints >= m_levelDown &&
        // m_skillPoints < m_levelUp) — debug-only; the EVA level-up
        // sound, TheControlBar's rank/SPP notifications — the client's)

        true
    }

    /// `Player::addSciencePurchasePoints` — the add with the negative
    /// clamp at zero (the deferred ControlBar notification — the
    /// client's).
    pub fn add_science_purchase_points(&mut self, player: usize, delta: i32) {
        let p = &mut self.list.players[player];
        p.science_purchase_points += delta;
        if p.science_purchase_points < 0 {
            p.science_purchase_points = 0;
        }
    }

    /// `Player::attemptToPurchaseScience` — "attempt to purchase the
    /// science, but use prereqs, and charge points": the capability
    /// gate, the charge, the grant and the academy's points-spent
    /// hook.
    pub fn attempt_to_purchase_science(
        &mut self,
        player: usize,
        science: ScienceType,
        host: &mut dyn KernelHost,
    ) -> bool {
        if !self.is_capable_of_purchasing_science(player, science) {
            // (DEBUG_CRASH(("isCapableOfPurchasingScience: need other
            // prereqs/points to purchase, request is ignored!")) — stated)
            return false;
        }

        let cost = self.science_store.get_science_purchase_cost(science);
        self.add_science_purchase_points(player, -cost);
        self.list.players[player].add_science(science, host);

        // getAcademyStats()->recordGeneralsPointsSpent(cost)
        host.on_academy_points_spent(player, cost);

        // (TheControlBar->markUIDirty() for the local player — the
        // client's)

        true
    }

    /// `Player::grantScience` — "grant the science, ignore prereqs &
    /// charge no points, but still restrict you to purchasable
    /// sciences".
    pub fn grant_science(
        &mut self,
        player: usize,
        science: ScienceType,
        host: &mut dyn KernelHost,
    ) -> bool {
        if !self.science_store.is_science_grantable(science) {
            // (DEBUG_CRASH(("Cannot grant science %s, since it is marked
            // as nonGrantable.")) — "it's not grantable, so tough, can't
            // have it, even via this method."; stated)
            return false;
        }

        self.list.players[player].add_science(science, host)
    }

    /// `Player::isCapableOfPurchasingScience` — "return true
    /// attemptToPurchaseScience() would succeed for this science": the
    /// invalid refusal, the ownership refusal, the script availability
    /// gates, the prereq chain and the cost ("purchase cost of zero
    /// means 'not purchasable!'").
    #[must_use]
    pub fn is_capable_of_purchasing_science(&self, player: usize, science: ScienceType) -> bool {
        if science == SCIENCE_INVALID {
            return false;
        }

        let p = &self.list.players[player];

        if p.has_science(science) {
            return false;
        }

        if p.is_science_disabled(science) || p.is_science_hidden(science) {
            return false;
        }

        if !self
            .science_store
            .player_has_prereqs_for_science(&p.sciences, science)
        {
            return false;
        }

        let cost = self.science_store.get_science_purchase_cost(science);
        if cost == 0 || cost > p.science_purchase_points {
            return false;
        }

        true
    }

    // ---- the energy block (Energy.cpp's mutation choreography) -----------------

    /// `Energy::getProduction` — the sabotage-gated read.
    #[must_use]
    pub fn energy_get_production(&self, player: usize, frame: u32) -> i32 {
        self.list.players[player].energy.get_production(frame)
    }

    /// `Energy::getConsumption`.
    #[must_use]
    pub fn energy_get_consumption(&self, player: usize) -> i32 {
        self.list.players[player].energy.get_consumption()
    }

    /// `Energy::hasSufficientPower`.
    #[must_use]
    pub fn energy_has_sufficient_power(&self, player: usize, frame: u32) -> bool {
        self.list.players[player].energy.has_sufficient_power(frame)
    }

    /// `Energy::getEnergySupplyRatio`.
    #[must_use]
    pub fn energy_supply_ratio(&self, player: usize, frame: u32) -> f32 {
        self.list.players[player]
            .energy
            .get_energy_supply_ratio(frame)
    }

    /// `Energy::setPowerSabotagedTillFrame`.
    pub fn energy_set_power_sabotaged_till_frame(&mut self, player: usize, frame: u32) {
        self.list.players[player]
            .energy
            .set_power_sabotaged_till_frame(frame);
    }

    /// `Energy::getPowerSabotagedTillFrame`.
    #[must_use]
    pub fn energy_power_sabotaged_till_frame(&self, player: usize) -> u32 {
        self.list.players[player]
            .energy
            .get_power_sabotaged_till_frame()
    }

    /// `Energy::adjustPower` — "If adding is false, we're supposed to
    /// be removing this." ("Seems a little odd, however, consumption
    /// is reversed. Negative power is positive consumption.")
    pub fn energy_adjust_power(
        &mut self,
        objects: &mut Objects,
        player: usize,
        power_delta: i32,
        adding: bool,
        frame: u32,
    ) {
        if power_delta == 0 {
            return;
        }

        if power_delta > 0 {
            if adding {
                self.energy_add_production(objects, player, power_delta, frame);
            } else {
                self.energy_add_production(objects, player, -power_delta, frame);
            }
        } else {
            // Seems a little odd, however, consumption is reversed.
            // Negative power is positive consumption.
            if adding {
                self.energy_add_consumption(objects, player, -power_delta, frame);
            } else {
                self.energy_add_consumption(objects, player, power_delta, frame);
            }
        }
    }

    /// `Energy::objectEnteringInfluence` — "new 'obj' will now
    /// add/subtract from this energy construct" (the template's
    /// energy production: negative = consumption).
    pub fn energy_object_entering_influence(
        &mut self,
        objects: &mut Objects,
        player: usize,
        obj: ObjectID,
        frame: u32,
    ) {
        // sanity (obj == nullptr — the id is live by contract)
        let energy = objects.get(obj).energy_production;

        // adjust energy
        if energy < 0 {
            self.energy_add_consumption(objects, player, -energy, frame);
        } else if energy > 0 {
            self.energy_add_production(objects, player, energy, frame);
        }
        // (the DEBUG_ASSERTCRASH sanity — the corpora keep the contract)
    }

    /// `Energy::objectLeavingInfluence` — "'obj' will now no longer
    /// add/subtrack from this energy construct" (the signs flip: a
    /// consumer's NEGATIVE production adds negative consumption, a
    /// producer loses its output).
    pub fn energy_object_leaving_influence(
        &mut self,
        objects: &mut Objects,
        player: usize,
        obj: ObjectID,
        frame: u32,
    ) {
        // sanity
        let energy = objects.get(obj).energy_production;

        // adjust energy
        if energy < 0 {
            self.energy_add_consumption(objects, player, energy, frame);
        } else if energy > 0 {
            self.energy_add_production(objects, player, -energy, frame);
        }
    }

    /// `Energy::addPowerBonus` — "Adds an energy bonus to the player's
    /// pool of energy when the 'Control Rods' upgrade is made to the
    /// American Cold Fusion Plant" (the disabled-power-plant
    /// DEBUG_ASSERTCRASH — stated).
    pub fn energy_add_power_bonus(
        &mut self,
        objects: &mut Objects,
        player: usize,
        obj: ObjectID,
        frame: u32,
    ) {
        // sanity (obj == nullptr — the id is live by contract)
        let bonus = objects.get(obj).energy_bonus;
        self.energy_add_production(objects, player, bonus, frame);
    }

    /// `Energy::removePowerBonus` — "Removed an energy bonus" (the
    /// disabled-power-plant refusal: TheSuperHackers @bugfix Caball009
    /// 14/11/2025 "Don't remove power bonus for disabled power
    /// plants", the non-retail branch).
    pub fn energy_remove_power_bonus(
        &mut self,
        objects: &mut Objects,
        player: usize,
        obj: ObjectID,
        frame: u32,
    ) {
        // sanity
        if objects.get(obj).disabled_mask.any() {
            return;
        }

        let bonus = objects.get(obj).energy_bonus;
        self.energy_add_production(objects, player, -bonus, frame);
    }

    /// `Energy::addProduction` — the write, then the owner's brownout
    /// re-derivation ("A repeated Brownout signal does nothing bad,
    /// and we need to handle more than just edge cases. Like low
    /// power, now even more low power, refresh disable.").
    fn energy_add_production(
        &mut self,
        objects: &mut Objects,
        player: usize,
        amt: i32,
        frame: u32,
    ) {
        self.list.players[player].energy.energy_production += amt;

        // (m_owner == nullptr — the pre-init state; unreachable once the
        // owning Player::init ran, carried faithfully)
        if self.list.players[player].energy.owner.is_none() {
            return;
        }

        let brown_out = !self.list.players[player].energy.has_sufficient_power(frame);
        self.on_power_brownout_change(objects, player, brown_out, frame);
    }

    /// `Energy::addConsumption`.
    fn energy_add_consumption(
        &mut self,
        objects: &mut Objects,
        player: usize,
        amt: i32,
        frame: u32,
    ) {
        self.list.players[player].energy.energy_consumption += amt;

        if self.list.players[player].energy.owner.is_none() {
            return;
        }

        let brown_out = !self.list.players[player].energy.has_sufficient_power(frame);
        self.on_power_brownout_change(objects, player, brown_out, frame);
    }

    /// `Player::onPowerBrownOutChange` — "Everything that changes due
    /// to Player's power supply goes in here": the radar restriction
    /// and the `doPowerDisable` sweep over the player's objects (the
    /// team-prototype → instance → member walk, newest first) setting
    /// or clearing `DISABLED_UNDERPOWERED` on every `KINDOF_POWERED`
    /// member.
    fn on_power_brownout_change(
        &mut self,
        objects: &mut Objects,
        player: usize,
        brown_out: bool,
        frame: u32,
    ) {
        if brown_out {
            self.list.players[player].disable_radar();
        } else {
            // This doesn't give radar necessarily, it just removes the
            // restriction
            self.list.players[player].enable_radar();
        }

        // iterateObjects(doPowerDisable, &brownOut)
        for &proto_slot in &self.list.players[player].team_prototypes {
            let Some(proto) = self.factory.protos.get(proto_slot).and_then(|p| p.as_ref()) else {
                continue;
            };
            for ts in proto.instances.iter().rev() {
                let Some(team) = self.factory.teams.get(*ts).and_then(|t| t.as_ref()) else {
                    continue;
                };
                for &m in team.members.iter().rev() {
                    // doPowerDisable: the KINDOF_POWERED gate, then the
                    // UNDERPOWERED set/clear (setDisabled/clearDisabled
                    // = setDisabledUntil FOREVER/NEVER — the kernel
                    // half's `frame > now` mask write)
                    if (objects.get(m).kindof >> KINDOF_POWERED_BIT) & 1 != 0 {
                        if brown_out {
                            set_disabled_until_core(
                                objects,
                                frame,
                                m,
                                DisabledType::Underpowered,
                                FOREVER,
                            );
                        } else {
                            set_disabled_until_core(
                                objects,
                                frame,
                                m,
                                DisabledType::Underpowered,
                                NEVER,
                            );
                        }
                    }
                }
            }
        }
    }
}

impl TeamFactory {
    /// `TeamFactory::addTeamPrototypeToList` — the duplicate refusal
    /// ("Team %s already exists... skipping") the std::map insert
    /// semantics give.
    fn add_team_prototype_to_list(&mut self, slot: usize) {
        let key = self.protos[slot].as_ref().expect("live proto").name_key;
        if self.map.contains_key(&key) {
            // DEBUG_ASSERTCRASH((*it).second==team, ...) — the release
            // build skips
            return; // already present
        }
        self.map.insert(key, slot);
    }

    /// `TeamFactory::findTeamPrototype(name)`.
    fn find_team_prototype(&mut self, name: &str, keys: &mut NameKeyGenerator) -> Option<usize> {
        let nk = keys.name_to_key(name);
        self.map.get(&nk).copied()
    }

    /// the team birth (`Team::Team`): the state, the instance-list
    /// prepend, the `checkEnemySighted` derivation. No events — the
    /// original's AppendDebugMessage is debug-log only.
    fn team_new(&mut self, proto_slot: usize) -> usize {
        let slot = self.teams.len();
        self.unique_team_id += 1; // ++m_uniqueTeamID
        let id = self.unique_team_id;
        let mut state = TeamRuntimeState::default();
        let info = &self.protos[proto_slot]
            .as_ref()
            .expect("live proto")
            .template_;
        if !info.script_on_all_clear.is_empty() || !info.script_on_enemy_sighted.is_empty() {
            state.check_enemy_sighted = true; // Only keep track of enemy sighted if there is a script that cares.
        }
        self.teams.push(Some(Team {
            id,
            proto: proto_slot,
            state,
            team_relations: BTreeMap::new(),
            player_relations: BTreeMap::new(),
            members: Vec::new(),
            recruitable_set: false,
            recruitable: false,
        }));
        self.protos[proto_slot]
            .as_mut()
            .expect("live proto")
            .instances
            .push(slot);
        slot
    }

    /// `createInactiveTeam`'s `executeActions` block (the
    /// production-condition script run).
    fn run_production_condition(&self, proto_slot: usize, host: &mut dyn KernelHost) {
        let proto = self.protos[proto_slot].as_ref().expect("live proto");
        if proto.template_.execute_actions
            && !proto.template_.production_condition.is_empty()
            && proto.template_.production_script_exists
        {
            // friend_executeAction(script->getAction()) — the proto's id
            // carries the production the instance is born or reused
            // around
            host.on_production_script(proto.id);
        }
    }

    /// `TeamFactory::createInactiveTeam` — the singleton reuse, the
    /// birth.
    fn create_inactive_team(&mut self, proto_slot: usize, host: &mut dyn KernelHost) -> usize {
        if self.protos[proto_slot]
            .as_ref()
            .expect("live proto")
            .singleton
            && let Some(t) = self.protos[proto_slot]
                .as_ref()
                .expect("live proto")
                .first_instance()
        {
            self.run_production_condition(proto_slot, host);
            return t;
        }

        let t = self.team_new(proto_slot);
        self.run_production_condition(proto_slot, host);
        t
    }

    /// `TeamFactory::createTeamOnPrototype`.
    fn create_team_on_prototype(
        &mut self,
        proto_slot: usize,
        host: &mut dyn KernelHost,
    ) -> Option<TeamID> {
        let Some(proto) = self.protos.get(proto_slot).and_then(|p| p.as_ref()) else {
            return None; // (throw ERROR_BAD_ARG)
        };
        if proto.singleton
            && let Some(t) = proto.first_instance()
        {
            return Some(self.teams[t].as_ref().expect("live team").id);
        }
        let t = self.team_new(proto_slot);
        let _ = host; // (the birth fires no events)
        let id = self.teams[t].as_ref().expect("live team").id;
        self.teams[t].as_mut().expect("live team").set_active();
        Some(id)
    }

    /// `TeamPrototype::teamAboutToBeDeleted` — every remaining instance
    /// drops its override for the dying team's id.
    fn proto_team_about_to_be_deleted(&mut self, proto_slot: usize, team: Option<TeamID>) {
        let team_id = team.unwrap_or(TEAM_ID_INVALID);
        let instances: Vec<usize> = self.protos[proto_slot]
            .as_ref()
            .map_or_else(Vec::new, |p| p.instances.clone());
        for ts in instances.iter().rev() {
            if let Some(team) = self.teams[*ts].as_mut() {
                // removeOverrideTeamRelationship(team ? team->getID() : TEAM_ID_INVALID)
                let _ = team.remove_override_team_relationship(team_id);
            }
        }
    }

    /// `TeamFactory::teamAboutToBeDeleted` — the prototype walk (map
    /// order), then the player walk. During `clear()` the map is ALREADY
    /// EMPTY (the take-then-clear dance) — the prototype half is a no-op
    /// there and only the player half runs; the corpora pin the contrast.
    fn team_about_to_be_deleted(&mut self, team: TeamID, list: &mut PlayerList) {
        let slots: Vec<usize> = self.map.values().copied().collect();
        for slot in slots {
            self.proto_team_about_to_be_deleted(slot, Some(team));
        }
        // (the original checks the global here: "if (ThePlayerList)" —
        // always true once the world exists)
        list.team_about_to_be_deleted(team);
    }

    /// `deleteTeamCallback` — the per-team deletion the prototype's
    /// `removeAll` walks.
    fn delete_team_callback(
        &mut self,
        team_slot: usize,
        objects: &mut Objects,
        list: &mut PlayerList,
        host: &mut dyn KernelHost,
    ) {
        let id = self.teams[team_slot].as_ref().expect("live team").id;
        self.team_about_to_be_deleted(id, list);
        self.delete_team_inner(team_slot, objects, list, host);
    }

    /// `~Team` — the `notifyOfTeamDestruction` hook, the players'
    /// `preTeamDestroy` walk, the member detach, the instance-list
    /// removal.
    fn delete_team_inner(
        &mut self,
        team_slot: usize,
        objects: &mut Objects,
        list: &mut PlayerList,
        host: &mut dyn KernelHost,
    ) {
        let (team_id, proto_slot) = {
            let team = self.teams[team_slot].as_ref().expect("live team");
            (team.id, team.proto)
        };

        // TheScriptEngine->notifyOfTeamDestruction(this)
        host.on_team_destroyed(team_id);

        // Tell the players a team is going away.
        for i in 0..list.count {
            list.players[i].pre_team_destroy(team_id);
        }

        // the member detach (each setTeam(nullptr) fires the
        // becomingTeamMember(false) transitions)
        loop {
            let member = self.teams[team_slot]
                .as_ref()
                .expect("live team")
                .members
                .last()
                .copied();
            let Some(member) = member else { break };
            self.detach_member(objects, member, host);
        }

        // the instance-list removal
        if let Some(proto) = self.protos[proto_slot].as_mut()
            && let Some(pos) = proto.instances.iter().position(|&t| t == team_slot)
        {
            proto.instances.remove(pos);
        }

        // (the relation maps and the xfer member-id list are
        // destructors' — no observable)
        self.teams[team_slot] = None;
    }

    /// the ~Team member detach (the object's team clear + the
    /// `becomingTeamMember(false)` transition — the `setTeam(nullptr)`
    /// path without the isPlayerActive dance, since the team is dying).
    fn detach_member(
        &mut self,
        objects: &mut Objects,
        member: ObjectID,
        host: &mut dyn KernelHost,
    ) {
        let old_id = objects.get(member).team;
        let old_slot = self.team_slot_by_id_local(old_id);
        objects.get_mut(member).team = TEAM_ID_INVALID;
        if let Some(os) = old_slot {
            let owner = self.protos[self.proto_of_team(os)]
                .as_ref()
                .expect("live proto")
                .owner;
            if let Some(team) = self.teams[os].as_mut()
                && let Some(pos) = team.members.iter().position(|&m| m == member)
            {
                team.members.remove(pos);
                host.on_becoming_team_member(member, owner, false);
            }
        }
    }

    /// the arena scan, factory-local.
    fn team_slot_by_id_local(&self, id: TeamID) -> Option<usize> {
        if id == TEAM_ID_INVALID {
            return None;
        }
        self.teams
            .iter()
            .position(|t| t.as_ref().is_some_and(|t| t.id == id))
    }

    /// the team's prototype slot (factory-local).
    fn proto_of_team(&self, team_slot: usize) -> usize {
        self.teams[team_slot].as_ref().expect("live team").proto
    }

    /// `TeamFactory::clear` — the map take-then-clear, the ordered
    /// destruction (key order; each prototype's instances newest-first),
    /// the owner's list removal. The duplicate-name orphan (never in the
    /// map) survives — the leak the corpora pin.
    fn clear(&mut self, objects: &mut Objects, list: &mut PlayerList, host: &mut dyn KernelHost) {
        // must remove it from the map before deleting the TeamProto, since
        // the TeamProto will try to remove itself from the list when it goes away
        let tmp = std::mem::take(&mut self.map);
        for (_, slot) in tmp {
            // ~TeamPrototype: removeAll_TeamInstanceList(deleteTeamCallback),
            // the owner's removeTeamFromList, the factory's
            // removeTeamPrototypeFromList (a no-op — the map is already empty)
            loop {
                let next = self.protos[slot]
                    .as_ref()
                    .and_then(|p| p.instances.last().copied());
                let Some(team_slot) = next else { break };
                self.protos[slot]
                    .as_mut()
                    .expect("live proto")
                    .instances
                    .pop();
                self.delete_team_callback(team_slot, objects, list, host);
            }
            if let Some(proto) = self.protos[slot].take() {
                list.players[proto.owner].remove_team_from_list(slot);
            }
        }
    }

    /// `TeamFactory::initFromSides`.
    fn init_from_sides(
        &mut self,
        sides: &SidesSpec,
        objects: &mut Objects,
        list: &mut PlayerList,
        keys: &mut NameKeyGenerator,
        host: &mut dyn KernelHost,
    ) {
        self.clear(objects, list, host);
        // create the teams we need.
        for team in &sides.teams {
            self.init_team(team, list, keys, host);
        }
    }

    /// `TeamFactory::initTeam` — the owner lookup with the neutral
    /// fallback, the prototype creation (the ++id), the singleton
    /// instance.
    fn init_team(
        &mut self,
        spec: &TeamSpec,
        list: &mut PlayerList,
        keys: &mut NameKeyGenerator,
        host: &mut dyn KernelHost,
    ) {
        let owner_key = keys.name_to_key(&spec.owner);
        let owner = list_find_by_key(list, owner_key).unwrap_or(0); // the neutral fallback
        let name_key = keys.name_to_key(&spec.name);

        self.unique_team_proto_id += 1; // ++m_uniqueTeamPrototypeID
        let slot = self.protos.len();
        self.protos.push(Some(TeamProto {
            id: self.unique_team_proto_id,
            name: spec.name.clone(),
            name_key,
            owner,
            singleton: spec.singleton,
            template_: TeamTemplate {
                script_on_create: spec.script_on_create.clone(),
                script_on_destroyed: spec.script_on_destroyed.clone(),
                script_on_idle: spec.script_on_idle.clone(),
                script_on_enemy_sighted: spec.script_on_enemy_sighted.clone(),
                script_on_all_clear: spec.script_on_all_clear.clone(),
                script_on_unit_destroyed: spec.script_on_unit_destroyed.clone(),
                destroyed_threshold: spec.destroyed_threshold,
                execute_actions: spec.execute_actions,
                production_condition: spec.production_condition.clone(),
                production_script_exists: spec.production_script_exists,
            },
            instances: Vec::new(),
        }));
        self.add_team_prototype_to_list(slot);
        if let Some(p) = list.players.get_mut(owner) {
            p.add_team_to_list(slot);
        }
        if spec.singleton {
            // Create the singleton team. (The name-based createInactiveTeam
            // the original calls resolves through the MAP — a duplicate
            // name's prototype reuses the FIRST prototype's instance.)
            let nk = name_key;
            if let Some(first_slot) = self.map.get(&nk).copied() {
                let _ = self.create_inactive_team(first_slot, host);
            }
        }
    }
}

// ---- the small helpers -------------------------------------------------------

/// the count-bounded name-key lookup over a bare list
/// (`PlayerList::findPlayerWithNameKey`).
fn list_find_by_key(list: &PlayerList, key: NameKey) -> Option<usize> {
    list.players
        .iter()
        .take(list.count)
        .position(|p| p.name_key == key)
}
