//! The sciences and ranks block — `Science.cpp` and `RankInfo.h`, the
//! fifth Phase 3 slice's progression half.
//!
//! Origin: `GeneralsMD/Code/GameEngine/Source/Common/RTS/Science.cpp` +
//! `Include/Common/Science.h` (the science catalog: the linear
//! `findScienceInfo` scan, the recursive `addRootSciences` root
//! computation, the prereq/root-purchase queries and the purchasable
//! walk) and `Core/GameEngine/Include/GameLogic/RankInfo.h` +
//! `Source/GameLogic/System/RankInfo.cpp` (the rank ladder: the 1-based
//! `getRankInfo`, the per-level skill-point thresholds, the
//! science-purchase-point grants and the sciences each rank grants) at
//! baseline 571a7c5.
//!
//! A science's identity is its interned name key — `ScienceType` is
//! `(ScienceType)NameKeyType` in the original, so [`ScienceType`] is
//! the exact [`rzh_util::namekey::NameKey`] port (the original's
//! `SCIENCE_INVALID = -1` sentinel is only ever the "no science"
//! null; the port's `NameKey::INVALID` (0) — a key no live name ever
//! interns — carries the same role; stated, no observable depends on
//! the bit pattern).
//!
//! The pinned quirks:
//!
//! - the catalog is a plain vector in load order; every lookup is the
//!   linear `findScienceInfo` walk (no map) — the purchasable walk and
//!   the prereq queries observe the load order;
//! - the root sciences are computed AT LOAD (`friend_parseScience
//!   Definition`'s `info->addRootSciences(info->m_rootSciences)`): a
//!   prereq defined LATER in the catalog is not yet in the store when
//!   the roots resolve, so it contributes nothing (the parse-order
//!   dependency the corpora pin);
//! - `addRootSciences` recurses through the prereq chain (cycles are
//!   the INI author's crash, out of contract like the original's
//!   stack overflow);
//! - a purchase cost of **0** means "cannot be purchased" — not "free";
//! - an unknown science has purchase cost 0, is not grantable, has no
//!   prereqs satisfied and no name — every query's null branch.
//!
//! The INI parse/override machinery (`friend_parseScienceDefinition`'s
//! `INI_LOAD_CREATE_OVERRIDES` chain, `friend_lookupScience`'s throw)
//! is the rzh-data loader's; the distilled [`ScienceSpec`] / [`RankSpec`]
//! catalogs arrive resolved. The `SubsystemInterface` init/reset
//! lifecycle is the data layer's. `RankInfo`'s `m_rankName` display
//! string is the client's (stated).
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use rzh_util::namekey::{NameKey, NameKeyGenerator};

/// `ScienceType` (Science.h): the science's interned name key.
pub type ScienceType = NameKey;

/// `SCIENCE_INVALID` (Science.h): the "no science" sentinel — the key
/// no live name ever interns.
pub const SCIENCE_INVALID: ScienceType = NameKey::INVALID;

/// One science definition of the distilled catalog (the `Science.ini`
/// entry's kernel half; `DisplayName`/`Description` are the client's).
#[derive(Clone, Debug, Default)]
pub struct ScienceSpec {
    /// The science's internal name (the block header).
    pub name: String,
    /// `PrerequisiteSciences`' token list.
    pub prereq_sciences: Vec<String>,
    /// `SciencePurchasePointCost` (0 means "cannot be purchased").
    pub science_purchase_point_cost: i32,
    /// `IsGrantable` (the default is true).
    pub grantable: bool,
}

/// One rank definition of the distilled catalog (the `RankInfo.ini`
/// entry's kernel half; `RankName` is the client's).
#[derive(Clone, Debug, Default)]
pub struct RankSpec {
    /// `m_skillPointsNeeded` — the cumulative skill points the level
    /// demands.
    pub skill_points_needed: i32,
    /// `m_sciencePurchasePointsGranted`.
    pub science_purchase_points_granted: i32,
    /// `m_sciencesGranted`' token list.
    pub sciences_granted: Vec<String>,
}

/// `ScienceInfo` (Science.h): one catalog entry.
#[derive(Clone, Debug)]
pub struct ScienceInfo {
    /// `m_science` — the interned name key (the identity).
    pub science: ScienceType,
    /// `m_prereqSciences` — the raw prereq keys (an unknown key stays
    /// unknown at query time: `findScienceInfo` misses it).
    pub prereq_sciences: Vec<ScienceType>,
    /// `m_rootSciences` — "this is calced at runtime, NOT read from
    /// INI": the roots of the prereq chain, computed at load against
    /// the catalog entries loaded SO FAR (the parse-order quirk).
    pub root_sciences: Vec<ScienceType>,
    /// `m_sciencePurchasePointCost`.
    pub science_purchase_point_cost: i32,
    /// `m_grantable`.
    pub grantable: bool,
}

/// `ScienceStore` (Science.h): the catalog in load order.
#[derive(Clone, Debug, Default)]
pub struct ScienceStore {
    /// `m_sciences` — the load order (every lookup walks it linearly).
    pub sciences: Vec<ScienceInfo>,
}

/// `RankInfo` (RankInfo.h): one rung of the ladder.
#[derive(Clone, Debug)]
pub struct RankInfo {
    /// `m_skillPointsNeeded`.
    pub skill_points_needed: i32,
    /// `m_sciencePurchasePointsGranted`.
    pub science_purchase_points_granted: i32,
    /// `m_sciencesGranted`.
    pub sciences_granted: Vec<ScienceType>,
}

/// `RankInfoStore` (RankInfo.h): the ladder, 1-based through
/// [`RankStore::rank_info`].
#[derive(Clone, Debug, Default)]
pub struct RankStore {
    /// `m_rankInfos` — index 0 is level 1.
    pub ranks: Vec<RankInfo>,
}

/// `ScienceAvailabilityType` (Player.h): the script-side availability
/// selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ScienceAvailabilityType {
    /// `SCIENCE_AVAILABILITY_INVALID` = -1 (the string-parse miss).
    Invalid = -1,
    /// `SCIENCE_AVAILABLE` = 0.
    Available = 0,
    /// `SCIENCE_DISABLED` = 1.
    Disabled = 1,
    /// `SCIENCE_HIDDEN` = 2.
    Hidden = 2,
}

/// `ScienceAvailabilityNames` (Player.h), in enum order (the nullptr
/// terminator omitted).
pub const SCIENCE_AVAILABILITY_NAMES: [&str; 3] = ["Available", "Disabled", "Hidden"];

impl ScienceAvailabilityType {
    /// `Player::getScienceAvailabilityTypeFromString` — the
    /// case-insensitive walk over the names table.
    #[must_use]
    pub fn from_string(name: &str) -> Self {
        for (i, n) in SCIENCE_AVAILABILITY_NAMES.iter().enumerate() {
            if name.eq_ignore_ascii_case(n) {
                return match i {
                    0 => Self::Available,
                    1 => Self::Disabled,
                    _ => Self::Hidden,
                };
            }
        }
        Self::Invalid
    }
}

impl ScienceStore {
    /// `friend_parseScienceDefinition`, distilled: append in load
    /// order, resolve the prereq names to their raw keys, then compute
    /// the roots against the catalog SO FAR (a forward reference is
    /// not yet present — the parse-order quirk).
    ///
    /// (The duplicate-name refusal of the non-override load —
    /// `DEBUG_CRASH(("duplicate science %s!"))` + the throw — is the
    /// INI loader's contract, stated; the distilled catalog is
    /// trusted.)
    pub fn load(&mut self, keys: &mut NameKeyGenerator, specs: &[ScienceSpec]) {
        for spec in specs {
            let science = keys.name_to_key(&spec.name);
            let mut info = ScienceInfo {
                science,
                prereq_sciences: spec
                    .prereq_sciences
                    .iter()
                    .map(|n| keys.name_to_key(n))
                    .collect(),
                root_sciences: Vec::new(),
                science_purchase_point_cost: spec.science_purchase_point_cost,
                grantable: spec.grantable,
            };
            let roots = std::mem::take(&mut info.root_sciences);
            let mut roots = roots;
            self.add_root_sciences(&info, &mut roots);
            info.root_sciences = roots;
            self.sciences.push(info);
        }
    }

    /// `ScienceInfo::addRootSciences` (Science.cpp): "we're a root" for
    /// a science with no prereqs (added once, first-discovery order),
    /// otherwise the roots of every known prereq (unknown prereqs
    /// contribute nothing).
    fn add_root_sciences(&self, info: &ScienceInfo, v: &mut Vec<ScienceType>) {
        if info.prereq_sciences.is_empty() {
            // we're a root. add ourselves.
            if !v.contains(&info.science) {
                v.push(info.science);
            }
        } else {
            // we're not a root. add the roots of all our prereqs.
            for &it in &info.prereq_sciences {
                if let Some(si) = self.find_science_info(it) {
                    self.add_root_sciences(si, v);
                }
            }
        }
    }

    /// `ScienceStore::findScienceInfo` — the linear scan (the load
    /// order).
    #[must_use]
    pub fn find_science_info(&self, st: ScienceType) -> Option<&ScienceInfo> {
        self.sciences.iter().find(|si| si.science == st)
    }

    /// `ScienceStore::isValidScience`.
    #[must_use]
    pub fn is_valid_science(&self, st: ScienceType) -> bool {
        self.find_science_info(st).is_some()
    }

    /// `ScienceStore::isScienceGrantable` — an unknown science is not
    /// grantable.
    #[must_use]
    pub fn is_science_grantable(&self, st: ScienceType) -> bool {
        self.find_science_info(st).is_some_and(|si| si.grantable)
    }

    /// `ScienceStore::getSciencePurchaseCost` — an unknown science
    /// costs 0.
    #[must_use]
    pub fn get_science_purchase_cost(&self, st: ScienceType) -> i32 {
        self.find_science_info(st)
            .map_or(0, |si| si.science_purchase_point_cost)
    }

    /// `ScienceStore::playerHasPrereqsForScience` — every prereq in
    /// the player's science list (an unknown science has no info, so
    /// no prereqs are ever satisfied).
    #[must_use]
    pub fn player_has_prereqs_for_science(
        &self,
        player_sciences: &[ScienceType],
        st: ScienceType,
    ) -> bool {
        match self.find_science_info(st) {
            Some(si) => si
                .prereq_sciences
                .iter()
                .all(|&it2| player_sciences.contains(&it2)),
            None => false,
        }
    }

    /// `ScienceStore::playerHasRootPrereqsForScience` — "this is a
    /// subtle call, and should ALMOST NEVER be called by external
    /// code... this is used to determine if you have the 'root'
    /// requirements for a given science, and thus could *potentially*
    /// obtain it if you got extra prereqs."
    #[must_use]
    pub fn player_has_root_prereqs_for_science(
        &self,
        player_sciences: &[ScienceType],
        st: ScienceType,
    ) -> bool {
        match self.find_science_info(st) {
            Some(si) => si
                .root_sciences
                .iter()
                .all(|&it2| player_sciences.contains(&it2)),
            None => false,
        }
    }

    /// `ScienceStore::getPurchasableSciences` — "return a list of the
    /// sciences the given player can purchase now, and a list he might
    /// be able to purchase in the future, but currently lacks prereqs
    /// or points for. (either might be an empty list)". The walk is
    /// the load order; the cost-0 ("cannot be purchased") and the
    /// already-owned entries are skipped.
    #[must_use]
    pub fn get_purchasable_sciences(
        &self,
        player_sciences: &[ScienceType],
    ) -> (Vec<ScienceType>, Vec<ScienceType>) {
        let mut purchasable = Vec::new();
        let mut potentially_purchasable = Vec::new();
        for si in &self.sciences {
            if si.science_purchase_point_cost == 0 {
                // 0 means "cannot be purchased"
                continue;
            }

            if player_sciences.contains(&si.science) {
                continue;
            }

            if self.player_has_prereqs_for_science(player_sciences, si.science) {
                purchasable.push(si.science);
            } else if self.player_has_root_prereqs_for_science(player_sciences, si.science) {
                potentially_purchasable.push(si.science);
            }
        }
        (purchasable, potentially_purchasable)
    }
}

impl RankStore {
    /// `friend_parseRankDefinition`, distilled: append in level order
    /// (level 1 first), the granted sciences interned to their raw
    /// keys. (The INI loader validates them against the science
    /// catalog — `parseScienceVector`'s `friend_lookupScience` throw —
    /// the rzh-data loader's contract, stated; the distilled catalog
    /// is trusted.)
    pub fn load(&mut self, keys: &mut NameKeyGenerator, specs: &[RankSpec]) {
        for spec in specs {
            self.ranks.push(RankInfo {
                skill_points_needed: spec.skill_points_needed,
                science_purchase_points_granted: spec.science_purchase_points_granted,
                sciences_granted: spec
                    .sciences_granted
                    .iter()
                    .map(|n| keys.name_to_key(n))
                    .collect(),
            });
        }
    }

    /// `RankInfoStore::getRankLevelCount`.
    #[must_use]
    pub fn rank_level_count(&self) -> i32 {
        self.ranks.len() as i32
    }

    /// `RankInfoStore::getRankInfo` — "note that level is 1...n, NOT
    /// 0...n-1"; out of range is the null.
    #[must_use]
    pub fn rank_info(&self, level: i32) -> Option<&RankInfo> {
        if (1..=self.rank_level_count()).contains(&level) {
            self.ranks.get(level as usize - 1)
        } else {
            None
        }
    }
}
