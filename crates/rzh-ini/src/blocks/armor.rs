// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! The `Armor` block type: complete port of the armor templates.
//!
//! Ported from `GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Armor.cpp`
//! and `.../Include/GameLogic/Armor.h` @ 571a7c5, plus the damage-type name
//! list from `Core/GameEngine/Source/GameLogic/System/Damage.cpp`
//! (`DamageTypeFlags::s_bitNameList`, Zero Hour configuration: no
//! `FLESHY_SNIPER`).
//!
//! An armor template is one damage coefficient per damage type:
//!
//! ```text
//! Armor TemplateA
//!   Armor = Default 100%
//!   Armor = FLAME 50%
//!   Armor = SUBDUAL_MISSILE 0%
//! End
//! ```
//!
//! `Default` sets every coefficient; a named type sets one (the name is
//! looked up in the damage-type list, case-insensitively). Redefining a
//! name always resets first (`clear()`), regardless of load type — the
//! original's `operator[]` + `clear()` path.
//!
//! # Parity level
//!
//! **P0 exact** via the golden vectors (`tests/golden/ini.txt`).

use std::collections::BTreeMap;

use rzh_util::NameKeyGenerator;

use crate::reader::{Ini, IniFieldTarget};
use crate::{IniError, scan};

/// `DamageType` (Damage.h, Zero Hour configuration). The discriminants
/// are the positions in [`DAMAGE_TYPE_NAMES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum DamageType {
    Explosion = 0,
    Crush,
    ArmorPiercing,
    SmallArms,
    Gattling,
    Radiation,
    Flame,
    Laser,
    Sniper,
    Poison,
    Healing,
    Unresistable,
    Water,
    Deploy,
    Surrender,
    Hack,
    KillPilot,
    Penalty,
    Falling,
    Melee,
    Disarm,
    HazardCleanup,
    ParticleBeam,
    Toppling,
    InfantryMissile,
    AuroraBomb,
    LandMine,
    JetMissiles,
    StealthjetMissiles,
    MolotovCocktail,
    ComancheVulcan,
    SubdualMissile,
    SubdualVehicle,
    SubdualBuilding,
    SubdualUnresistable,
    Microwave,
    KillGarrisoned,
    Status,
}

/// `DAMAGE_NUM_TYPES`.
pub const DAMAGE_NUM_TYPES: usize = 38;

/// `DamageTypeFlags::s_bitNameList` (Damage.cpp), Zero Hour
/// configuration. The names are matched case-insensitively.
pub const DAMAGE_TYPE_NAMES: [&str; DAMAGE_NUM_TYPES] = [
    "EXPLOSION",
    "CRUSH",
    "ARMOR_PIERCING",
    "SMALL_ARMS",
    "GATTLING",
    "RADIATION",
    "FLAME",
    "LASER",
    "SNIPER",
    "POISON",
    "HEALING",
    "UNRESISTABLE",
    "WATER",
    "DEPLOY",
    "SURRENDER",
    "HACK",
    "KILL_PILOT",
    "PENALTY",
    "FALLING",
    "MELEE",
    "DISARM",
    "HAZARD_CLEANUP",
    "PARTICLE_BEAM",
    "TOPPLING",
    "INFANTRY_MISSILE",
    "AURORA_BOMB",
    "LAND_MINE",
    "JET_MISSILES",
    "STEALTHJET_MISSILES",
    "MOLOTOV_COCKTAIL",
    "COMANCHE_VULCAN",
    "SUBDUAL_MISSILE",
    "SUBDUAL_VEHICLE",
    "SUBDUAL_BUILDING",
    "SUBDUAL_UNRESISTABLE",
    "MICROWAVE",
    "KILL_GARRISONED",
    "STATUS",
];

impl DamageType {
    /// The type with the given list position, if it is in range.
    #[must_use]
    pub const fn from_index(index: i32) -> Option<Self> {
        if index >= 0 && (index as usize) < DAMAGE_NUM_TYPES {
            Some(match index {
                0 => Self::Explosion,
                1 => Self::Crush,
                2 => Self::ArmorPiercing,
                3 => Self::SmallArms,
                4 => Self::Gattling,
                5 => Self::Radiation,
                6 => Self::Flame,
                7 => Self::Laser,
                8 => Self::Sniper,
                9 => Self::Poison,
                10 => Self::Healing,
                11 => Self::Unresistable,
                12 => Self::Water,
                13 => Self::Deploy,
                14 => Self::Surrender,
                15 => Self::Hack,
                16 => Self::KillPilot,
                17 => Self::Penalty,
                18 => Self::Falling,
                19 => Self::Melee,
                20 => Self::Disarm,
                21 => Self::HazardCleanup,
                22 => Self::ParticleBeam,
                23 => Self::Toppling,
                24 => Self::InfantryMissile,
                25 => Self::AuroraBomb,
                26 => Self::LandMine,
                27 => Self::JetMissiles,
                28 => Self::StealthjetMissiles,
                29 => Self::MolotovCocktail,
                30 => Self::ComancheVulcan,
                31 => Self::SubdualMissile,
                32 => Self::SubdualVehicle,
                33 => Self::SubdualBuilding,
                34 => Self::SubdualUnresistable,
                35 => Self::Microwave,
                36 => Self::KillGarrisoned,
                _ => Self::Status,
            })
        } else {
            None
        }
    }

    /// The list position of this type.
    #[must_use]
    pub const fn index(self) -> i32 {
        self as i32
    }
}

/// `ArmorTemplate`: one damage coefficient per damage type.
#[derive(Debug, Clone)]
pub struct ArmorTemplate {
    /// `m_damageCoefficient`: the multiplier per [`DamageType`].
    pub damage_coefficient: [f32; DAMAGE_NUM_TYPES],
}

impl Default for ArmorTemplate {
    fn default() -> Self {
        Self::new()
    }
}

impl ArmorTemplate {
    /// The constructor (via `clear()`): every coefficient `1.0`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            damage_coefficient: [1.0; DAMAGE_NUM_TYPES],
        }
    }

    /// `ArmorTemplate::clear`.
    pub fn clear(&mut self) {
        self.damage_coefficient = [1.0; DAMAGE_NUM_TYPES];
    }

    /// `ArmorTemplate::adjustDamage`: unresistable damage passes through
    /// unchanged; everything else is scaled by its coefficient and
    /// clamped at zero.
    #[must_use]
    pub const fn adjust_damage(&self, damage_type: DamageType, damage: f32) -> f32 {
        match damage_type {
            DamageType::Unresistable | DamageType::SubdualUnresistable => damage,
            _ => {
                let scaled = damage * self.damage_coefficient[damage_type.index() as usize];
                if scaled < 0.0 { 0.0 } else { scaled }
            }
        }
    }
}

impl IniFieldTarget for ArmorTemplate {
    /// The `myFieldParse` table: the single `Armor` field, parsed by
    /// `ArmorTemplate::parseArmorCoefficients`.
    fn parse_field(&mut self, ini: &mut Ini, field: &[u8]) -> Result<bool, IniError> {
        if field != b"Armor" {
            return Ok(false);
        }
        // parseArmorCoefficients: the field token is case-sensitive, the
        // damage name is not.
        let damage_name = ini.get_next_token()?;
        let pct = ini.scan_next_percent_to_real()?;
        if damage_name.eq_ignore_ascii_case(b"Default") {
            self.damage_coefficient = [pct; DAMAGE_NUM_TYPES];
        } else {
            let dt = scan::scan_index_list(&damage_name, &DAMAGE_TYPE_NAMES)
                .map_err(|e| ini.error(e.into()))?;
            self.damage_coefficient[dt as usize] = pct;
        }
        Ok(true)
    }
}

/// `ArmorStore`: the templates by name key. The C++ held a `hash_map`;
/// the port uses an ordered map (iteration is deterministic, plan §3.3)
/// and the golden dump iterates it in key order.
#[derive(Debug, Default)]
pub struct ArmorStore {
    templates: BTreeMap<u32, ArmorTemplate>,
}

impl ArmorStore {
    /// An empty store.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            templates: BTreeMap::new(),
        }
    }

    /// `ArmorStore::parseArmorDefinition`: reads the block name, resets
    /// the template (redefinition always clears) and parses the fields.
    pub fn parse_definition(
        &mut self,
        ini: &mut Ini,
        names: &mut NameKeyGenerator,
    ) -> Result<(), IniError> {
        let name = ini.get_next_token()?;
        let key = names.name_to_key(&name);
        let template = self.templates.entry(key.ordinal()).or_default();
        template.clear();
        ini.init_from_ini(template)
    }

    /// `findArmorTemplate(name)`. Like the original this interns the
    /// name (an unknown name allocates a key, then misses).
    pub fn find(&self, name: &str, names: &mut NameKeyGenerator) -> Option<&ArmorTemplate> {
        let key = names.name_to_key(name);
        self.templates.get(&key.ordinal())
    }

    /// The number of templates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    /// Iterates the templates in name-key order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &ArmorTemplate)> {
        self.templates.iter().map(|(k, v)| (*k, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::LoadType;

    struct ArmorBlocks {
        names: NameKeyGenerator,
        store: ArmorStore,
    }

    impl crate::reader::BlockParser for ArmorBlocks {
        fn parse_block(&mut self, ini: &mut Ini, block: &[u8]) -> Result<bool, IniError> {
            if block == b"Armor" {
                self.store.parse_definition(ini, &mut self.names)?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }

    #[test]
    fn parses_default_and_named_coefficients() {
        let mut game = ArmorBlocks {
            names: NameKeyGenerator::new(),
            store: ArmorStore::new(),
        };
        let mut ini = Ini::new();
        ini.load(
            "Armor.ini",
            b"Armor TestArmor\n\
              Armor = Default 100%\n\
              Armor = FLAME 50%\n\
              Armor = subdual_missile 0%\n\
              End\n"
                .to_vec(),
            LoadType::Overwrite,
            &mut game,
        )
        .unwrap();

        let template = game.store.find("TestArmor", &mut game.names).unwrap();
        assert_eq!(
            template.damage_coefficient[DamageType::Flame.index() as usize],
            0.5
        );
        assert_eq!(
            template.damage_coefficient[DamageType::SubdualMissile.index() as usize],
            0.0
        );
        assert_eq!(
            template.damage_coefficient[DamageType::Explosion.index() as usize],
            1.0
        );
    }

    #[test]
    fn redefinition_resets_all_coefficients() {
        let mut game = ArmorBlocks {
            names: NameKeyGenerator::new(),
            store: ArmorStore::new(),
        };
        let mut ini = Ini::new();
        ini.load(
            "a.ini",
            b"Armor A\nArmor = FLAME 25%\nEnd\n".to_vec(),
            LoadType::Overwrite,
            &mut game,
        )
        .unwrap();
        ini.load(
            "b.ini",
            b"Armor A\nArmor = LASER 75%\nEnd\n".to_vec(),
            LoadType::CreateOverrides,
            &mut game,
        )
        .unwrap();

        let template = game.store.find("A", &mut game.names).unwrap();
        // clear() ran before the override parse, so FLAME is back to 1.0.
        assert_eq!(
            template.damage_coefficient[DamageType::Flame.index() as usize],
            1.0
        );
        assert_eq!(
            template.damage_coefficient[DamageType::Laser.index() as usize],
            0.75
        );
    }

    #[test]
    fn adjust_damage_passes_unresistable_through_and_clamps() {
        let mut template = ArmorTemplate::new();
        template.damage_coefficient[DamageType::Flame.index() as usize] = -1.0;
        assert_eq!(template.adjust_damage(DamageType::Unresistable, 10.0), 10.0);
        assert_eq!(
            template.adjust_damage(DamageType::SubdualUnresistable, 10.0),
            10.0
        );
        assert_eq!(template.adjust_damage(DamageType::Flame, 10.0), 0.0);
        assert_eq!(template.adjust_damage(DamageType::Crush, 10.0), 10.0);
    }
}
