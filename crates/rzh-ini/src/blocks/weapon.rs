// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! The `Weapon` block type: a **representative subset** of the weapon
//! template fields, chosen to exercise every generic parse procedure the
//! engine port offers.
//!
//! Ported from `GeneralsMD/Code/GameEngine/Source/GameLogic/Object/
//! Weapon.cpp` (`TheWeaponTemplateFieldParseTable` entries listed below,
//! the constructor defaults for those fields, `parseShotDelay`,
//! `parseWeaponTemplateDefinition`, `newWeaponTemplate`, `newOverride`)
//! and `.../Include/GameLogic/Weapon.h` @ 571a7c5. The remaining fields
//! (FX/OCL/particle/audio/veterancy arrays, weapon bonuses) land with
//! `rzh-data` in Phase 2 of the migration plan, together with the other
//! 190-odd field tables.
//!
//! Ported fields: `PrimaryDamage`, `PrimaryDamageRadius`,
//! `SecondaryDamage`, `SecondaryDamageRadius`, `AttackRange`,
//! `MinimumAttackRange`, `AcceptableAimDelta`, `DamageType`, `DeathType`,
//! `WeaponSpeed`, `MinWeaponSpeed`, `ScaleWeaponSpeed`, `WeaponRecoil`,
//! `ProjectileObject`, `ClipSize`, `ClipReloadTime`, `DelayBetweenShots`,
//! `ShotsPerBarrel`, `DamageDealtAtSelfPosition`, `RadiusDamageAffects`,
//! `AntiAirborneVehicle`, `AntiGround`, `AntiBallisticMissile`,
//! `AutoReloadsClip`, `PreAttackDelay`, `PreAttackType`,
//! `HistoricBonusRadius`, `LeechRangeWeapon`, `ShowsAmmoPips`,
//! `LaserName`.
//!
//! # Preserved upstream quirks (both pinned by the golden vectors)
//!
//! * `newOverride` chains the **new** template's next pointer *back to
//!   the old one* — the opposite direction of the Locomotor/ThingTemplate
//!   override chains — and never registers the override in the store, so
//!   the parsed override is unreachable and lookups keep returning the
//!   base template. (Every other override site in the tree chains
//!   forward from the parent.) Transcribed as-is.
//! * Redefining an existing weapon under a non-override load type
//!   returns before the block is consumed: the release build then fails
//!   the file with `INI_UNKNOWN_TOKEN` on the block's first field line
//!   (the debug build crashes with "Weapon '%s' already exists, but
//!   OVERRIDE not specified" instead).
//!
//! # Parity level
//!
//! **P0 exact** via the golden vectors (`tests/golden/ini.txt`).

use std::collections::BTreeMap;

use rzh_util::NameKeyGenerator;

use crate::IniError;
use crate::blocks::armor::DAMAGE_TYPE_NAMES;
use crate::reader::{Ini, IniFieldTarget, LOGICFRAMES_PER_MSEC, LoadType, SEPS_COLON};

/// `TheDeathNames` (Damage.h).
pub const DEATH_TYPE_NAMES: [&str; 21] = [
    "NORMAL",
    "NONE",
    "CRUSHED",
    "BURNED",
    "EXPLODED",
    "POISONED",
    "TOPPLED",
    "FLOODED",
    "SUICIDED",
    "LASERED",
    "DETONATED",
    "SPLATTED",
    "POISONED_BETA",
    "EXTRA_2",
    "EXTRA_3",
    "EXTRA_4",
    "EXTRA_5",
    "EXTRA_6",
    "EXTRA_7",
    "EXTRA_8",
    "POISONED_GAMMA",
];

/// `TheWeaponReloadNames` (Weapon.h).
pub const WEAPON_RELOAD_NAMES: [&str; 3] = ["YES", "NO", "RETURN_TO_BASE"];

/// `TheWeaponPrefireNames` (Weapon.h).
pub const WEAPON_PREFIRE_NAMES: [&str; 3] = ["PER_SHOT", "PER_ATTACK", "PER_CLIP"];

/// `TheWeaponAffectsMaskNames` (Weapon.h).
pub const WEAPON_AFFECTS_MASK_NAMES: [&str; 7] = [
    "SELF",
    "ALLIES",
    "ENEMIES",
    "NEUTRALS",
    "SUICIDE",
    "NOT_SIMILAR",
    "NOT_AIRBORNE",
];

/// `WeaponAntiMaskType` bits (Weapon.h).
pub const WEAPON_ANTI_AIRBORNE_VEHICLE: u32 = 0x01;
/// `WEAPON_ANTI_GROUND`.
pub const WEAPON_ANTI_GROUND: u32 = 0x02;
/// `WEAPON_ANTI_PROJECTILE`.
pub const WEAPON_ANTI_PROJECTILE: u32 = 0x04;
/// `WEAPON_ANTI_SMALL_MISSILE`.
pub const WEAPON_ANTI_SMALL_MISSILE: u32 = 0x08;
/// `WEAPON_ANTI_MINE`.
pub const WEAPON_ANTI_MINE: u32 = 0x10;
/// `WEAPON_ANTI_AIRBORNE_INFANTRY`.
pub const WEAPON_ANTI_AIRBORNE_INFANTRY: u32 = 0x20;
/// `WEAPON_ANTI_BALLISTIC_MISSILE`.
pub const WEAPON_ANTI_BALLISTIC_MISSILE: u32 = 0x40;
/// `WEAPON_ANTI_PARACHUTE`.
pub const WEAPON_ANTI_PARACHUTE: u32 = 0x80;

/// `WeaponTemplate` — the ported field subset, in table order.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct WeaponTemplate {
    /// The next pointer of the (unreachable) override chain; see the
    /// module docs on the preserved upstream quirk.
    pub next_template: Option<Box<WeaponTemplate>>,
    /// `m_name`.
    pub name: Vec<u8>,
    /// `m_nameKey`.
    pub name_key: u32,

    pub primary_damage: f32,
    pub primary_damage_radius: f32,
    pub secondary_damage: f32,
    pub secondary_damage_radius: f32,
    pub attack_range: f32,
    pub minimum_attack_range: f32,
    pub aim_delta: f32,
    /// `m_damageType`: the damage-type *index* (the original stores the
    /// single-bit scan result, which is the index).
    pub damage_type: i32,
    /// `m_deathType`.
    pub death_type: i32,
    pub weapon_speed: f32,
    pub min_weapon_speed: f32,
    pub is_scale_weapon_speed: bool,
    pub weapon_recoil: f32,
    /// `m_projectileName`: the raw name; `Weapon None` clears it.
    pub projectile_name: Vec<u8>,
    pub clip_size: i32,
    pub clip_reload_time: u32,
    pub min_delay_between_shots: i32,
    pub max_delay_between_shots: i32,
    pub shots_per_barrel: i32,
    pub damage_dealt_at_self_position: bool,
    pub affects_mask: u32,
    pub anti_mask: u32,
    /// `m_reloadType`.
    pub reload_type: i32,
    pub pre_attack_delay: u32,
    /// `m_prefireType`.
    pub prefire_type: i32,
    pub historic_bonus_radius: f32,
    pub leech_range_weapon: bool,
    pub is_shows_ammo_pips: bool,
    pub laser_name: Vec<u8>,
}

impl Default for WeaponTemplate {
    fn default() -> Self {
        Self::new()
    }
}

impl WeaponTemplate {
    /// The constructor defaults for the ported fields.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn new() -> Self {
        let affects_allies_enemies_neutrals = 0x02 | 0x04 | 0x08; // WEAPON_AFFECTS_ALLIES | _ENEMIES | _NEUTRALS
        Self {
            next_template: None,
            name: b"NoNameWeapon".to_vec(),
            name_key: 0, // NAMEKEY_INVALID

            primary_damage: 0.0,
            primary_damage_radius: 0.0,
            secondary_damage: 0.0,
            secondary_damage_radius: 0.0,
            attack_range: 0.0,
            minimum_attack_range: 0.0,
            aim_delta: 0.0,
            damage_type: 0,              // DAMAGE_EXPLOSION
            death_type: 0,               // DEATH_NORMAL
            weapon_speed: 999_999.0,     // effectively instant
            min_weapon_speed: 999_999.0, // effectively instant
            is_scale_weapon_speed: false,
            weapon_recoil: 0.0,
            projectile_name: Vec::new(),
            clip_size: 0,
            clip_reload_time: 0,
            min_delay_between_shots: 0,
            max_delay_between_shots: 0,
            shots_per_barrel: 1,
            damage_dealt_at_self_position: false,
            affects_mask: affects_allies_enemies_neutrals,
            anti_mask: WEAPON_ANTI_GROUND,
            reload_type: 0, // AUTO_RELOAD
            pre_attack_delay: 0,
            prefire_type: 0, // PREFIRE_PER_SHOT
            historic_bonus_radius: 0.0,
            leech_range_weapon: false,
            is_shows_ammo_pips: false,
            laser_name: Vec::new(),
        }
    }

    /// `WeaponTemplate::parseShotDelay`: a single millisecond number, or
    /// a `Min: n Max: n` pair, converted (rounding up) to frames.
    fn parse_shot_delay(&mut self, ini: &mut Ini) -> Result<(), IniError> {
        // This smart parser allows both a single number for traditional
        // delay, and a labeled pair of numbers for a delay range.
        let mut token = ini.get_next_token_with(SEPS_COLON)?;
        if token.eq_ignore_ascii_case(b"Min") {
            // Two entry min/max
            self.min_delay_between_shots = ini
                .get_next_token_with(SEPS_COLON)
                .and_then(|t| crate::scan::scan_int(&t).map_err(|e| ini.error(e.into())))?;
            token = ini.get_next_token_with(SEPS_COLON)?;
            if !token.eq_ignore_ascii_case(b"Max") {
                // Messed up double entry
                self.max_delay_between_shots = self.min_delay_between_shots;
            } else {
                self.max_delay_between_shots = ini
                    .get_next_token_with(SEPS_COLON)
                    .and_then(|t| crate::scan::scan_int(&t).map_err(|e| ini.error(e.into())))?;
            }
        } else {
            // single entry, as in no label so the first token is just a
            // number
            self.min_delay_between_shots =
                crate::scan::scan_int(&token).map_err(|e| ini.error(e.into()))?;
            self.max_delay_between_shots = self.min_delay_between_shots;
        }

        // No matter what we have now, we want to convert it to frames
        // from msec. ShotDelay used to use parseDurationUnsignedInt, and
        // we are expanding on that.
        self.min_delay_between_shots =
            frames_ceil((self.min_delay_between_shots as f32) * LOGICFRAMES_PER_MSEC);
        self.max_delay_between_shots =
            frames_ceil((self.max_delay_between_shots as f32) * LOGICFRAMES_PER_MSEC);
        Ok(())
    }
}

/// `WWMath::Ceilf` followed by the float→`Int` conversion.
fn frames_ceil(frames: f32) -> i32 {
    frames.ceil() as i32
}

impl IniFieldTarget for WeaponTemplate {
    /// The ported subset of `TheWeaponTemplateFieldParseTable`, in table
    /// order.
    fn parse_field(&mut self, ini: &mut Ini, field: &[u8]) -> Result<bool, IniError> {
        match field {
            b"PrimaryDamage" => self.primary_damage = ini.scan_next_real()?,
            b"PrimaryDamageRadius" => self.primary_damage_radius = ini.scan_next_real()?,
            b"SecondaryDamage" => self.secondary_damage = ini.scan_next_real()?,
            b"SecondaryDamageRadius" => self.secondary_damage_radius = ini.scan_next_real()?,
            b"AttackRange" => self.attack_range = ini.scan_next_real()?,
            b"MinimumAttackRange" => self.minimum_attack_range = ini.scan_next_real()?,
            b"AcceptableAimDelta" => self.aim_delta = ini.next_angle_real()?,
            // DamageTypeFlags::parseSingleBitFromINI: the stored value is
            // the bit INDEX, not the bit.
            b"DamageType" => self.damage_type = ini.scan_next_index_list(&DAMAGE_TYPE_NAMES)?,
            b"DeathType" => self.death_type = ini.scan_next_index_list(&DEATH_TYPE_NAMES)?,
            b"WeaponSpeed" => self.weapon_speed = ini.next_velocity_real()?,
            b"MinWeaponSpeed" => self.min_weapon_speed = ini.next_velocity_real()?,
            b"ScaleWeaponSpeed" => self.is_scale_weapon_speed = ini.scan_next_bool()?,
            b"WeaponRecoil" => self.weapon_recoil = ini.next_angle_real()?,
            b"ProjectileObject" => self.projectile_name = ini.get_next_ascii_string(),
            b"ClipSize" => self.clip_size = ini.scan_next_int()?,
            b"ClipReloadTime" => self.clip_reload_time = ini.next_duration_unsigned_int()?,
            b"DelayBetweenShots" => self.parse_shot_delay(ini)?,
            b"ShotsPerBarrel" => self.shots_per_barrel = ini.scan_next_int()?,
            b"DamageDealtAtSelfPosition" => {
                self.damage_dealt_at_self_position = ini.scan_next_bool()?;
            }
            b"RadiusDamageAffects" => {
                ini.next_bit_string32(&WEAPON_AFFECTS_MASK_NAMES, &mut self.affects_mask)?;
            }
            b"AntiAirborneVehicle" => {
                ini.next_bit_in_int32(WEAPON_ANTI_AIRBORNE_VEHICLE, &mut self.anti_mask)?;
            }
            b"AntiGround" => {
                ini.next_bit_in_int32(WEAPON_ANTI_GROUND, &mut self.anti_mask)?;
            }
            b"AntiBallisticMissile" => {
                ini.next_bit_in_int32(WEAPON_ANTI_BALLISTIC_MISSILE, &mut self.anti_mask)?;
            }
            b"AutoReloadsClip" => {
                self.reload_type = ini.scan_next_index_list(&WEAPON_RELOAD_NAMES)?;
            }
            b"PreAttackDelay" => self.pre_attack_delay = ini.next_duration_unsigned_int()?,
            b"PreAttackType" => {
                self.prefire_type = ini.scan_next_index_list(&WEAPON_PREFIRE_NAMES)?;
            }
            b"HistoricBonusRadius" => self.historic_bonus_radius = ini.scan_next_real()?,
            b"LeechRangeWeapon" => self.leech_range_weapon = ini.scan_next_bool()?,
            b"ShowsAmmoPips" => self.is_shows_ammo_pips = ini.scan_next_bool()?,
            b"LaserName" => self.laser_name = ini.get_next_ascii_string(),
            _ => return Ok(false),
        }
        Ok(true)
    }
}

/// `WeaponStore`: templates by name key. The C++ held a vector plus a
/// hash map; the port keeps the ordered map (deterministic iteration)
/// and the override-chain quirk documented above.
#[derive(Debug, Default)]
pub struct WeaponStore {
    templates: BTreeMap<u32, WeaponTemplate>,
}

impl WeaponStore {
    /// An empty store.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            templates: BTreeMap::new(),
        }
    }

    /// `WeaponStore::newWeaponTemplate`.
    fn new_weapon_template(&mut self, name: &[u8], names: &mut NameKeyGenerator) -> u32 {
        let key = names.name_to_key(name);
        let mut template = WeaponTemplate::new();
        template.name = name.to_vec();
        template.name_key = key.ordinal();
        self.templates.insert(key.ordinal(), template);
        key.ordinal()
    }

    /// `WeaponStore::newOverride`: copies the template and chains the
    /// copy's next pointer back to it — **without registering it**, so
    /// the override stays unreachable (see the module docs).
    fn new_override(base: &WeaponTemplate) -> WeaponTemplate {
        let mut copy = base.clone();
        copy.next_template = Some(Box::new(base.clone()));
        copy
    }

    /// `WeaponStore::parseWeaponTemplateDefinition`.
    pub fn parse_definition(
        &mut self,
        ini: &mut Ini,
        names: &mut NameKeyGenerator,
    ) -> Result<(), IniError> {
        // read the weapon name
        let name = ini.get_next_token()?;

        // find existing item if present
        let key = names.name_to_key(&name).ordinal();
        if let Some(base) = self.templates.get(&key).cloned() {
            if ini.load_type() == LoadType::CreateOverrides {
                // Parse into the (unreachable) override copy; the store
                // keeps serving the base template. Preserved upstream
                // quirk — see the module docs.
                let mut weapon = Self::new_override(&base);
                ini.init_from_ini(&mut weapon)?;
                if weapon.projectile_name.eq_ignore_ascii_case(b"None") {
                    weapon.projectile_name.clear();
                }
            } else {
                // DEBUG_CRASH is a no-op in release builds; the original
                // returns *without consuming the block*. The load loop
                // then reads the block's field lines as top-level lines
                // and fails with INI_UNKNOWN_TOKEN (or succeeds if the
                // input ends right here).
                return Ok(());
            }
        } else {
            // no item is present, create a new one
            let key = self.new_weapon_template(&name, names);
            let weapon = self
                .templates
                .get_mut(&key)
                .expect("template was just inserted");
            ini.init_from_ini(weapon)?;
            if weapon.projectile_name.eq_ignore_ascii_case(b"None") {
                weapon.projectile_name.clear();
            }
        }
        Ok(())
    }

    /// `findWeaponTemplate(name)`.
    #[must_use]
    pub fn find(&self, name: &str, names: &mut NameKeyGenerator) -> Option<&WeaponTemplate> {
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
    pub fn iter(&self) -> impl Iterator<Item = (u32, &WeaponTemplate)> {
        self.templates.iter().map(|(k, v)| (*k, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorKind;
    use crate::reader::BlockParser;

    struct WeaponBlocks {
        names: NameKeyGenerator,
        store: WeaponStore,
    }

    impl BlockParser for WeaponBlocks {
        fn parse_block(&mut self, ini: &mut Ini, block: &[u8]) -> Result<bool, IniError> {
            if block == b"Weapon" {
                self.store.parse_definition(ini, &mut self.names)?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }

    #[test]
    fn parses_the_field_subset() {
        let mut game = WeaponBlocks {
            names: NameKeyGenerator::new(),
            store: WeaponStore::new(),
        };
        let mut ini = Ini::new();
        ini.load(
            "Weapon.ini",
            b"Weapon TestWeapon\n\
              PrimaryDamage = 50.0\n\
              PrimaryDamageRadius = 5.0\n\
              AttackRange = 150.0\n\
              AcceptableAimDelta = 5\n\
              DamageType = FLAME\n\
              DeathType = BURNED\n\
              WeaponSpeed = 600\n\
              ClipSize = 10\n\
              ClipReloadTime = 2000\n\
              DelayBetweenShots = Min: 500 Max: 700\n\
              RadiusDamageAffects = ENEMIES NOT_SIMILAR\n\
              AntiGround = No\n\
              AntiBallisticMissile = Yes\n\
              AutoReloadsClip = Yes\n\
              PreAttackType = PER_ATTACK\n\
              ProjectileObject = None\n\
              End\n"
                .to_vec(),
            LoadType::Overwrite,
            &mut game,
        )
        .unwrap();

        let template = game.store.find("TestWeapon", &mut game.names).unwrap();
        assert_eq!(template.primary_damage, 50.0);
        assert_eq!(template.attack_range, 150.0);
        assert_eq!(template.damage_type, 6); // FLAME index
        assert_eq!(template.death_type, 3); // BURNED index
        // 600 dist/sec -> 600 * (1/30) dist/frame (f32 operation order).
        assert_eq!(template.weapon_speed, 600.0f32 * (1.0f32 / 30.0f32));
        assert_eq!(template.clip_reload_time, 60); // 2000 msec -> 60 frames
        assert_eq!(template.min_delay_between_shots, 15); // 500 msec -> 15
        assert_eq!(template.max_delay_between_shots, 21); // 700 msec -> 21
        assert_eq!(template.affects_mask, 0x04 | 0x20);
        assert_eq!(template.anti_mask, 0x40);
        assert_eq!(template.reload_type, 0);
        assert_eq!(template.prefire_type, 1);
        // `Weapon None` clears the projectile name.
        assert!(template.projectile_name.is_empty());
    }

    #[test]
    fn shot_delay_single_entry_doubles() {
        let mut game = WeaponBlocks {
            names: NameKeyGenerator::new(),
            store: WeaponStore::new(),
        };
        let mut ini = Ini::new();
        ini.load(
            "Weapon.ini",
            b"Weapon W\nDelayBetweenShots = 333\nEnd\n".to_vec(),
            LoadType::Overwrite,
            &mut game,
        )
        .unwrap();
        let template = game.store.find("W", &mut game.names).unwrap();
        // 333 msec -> 9.99 frames -> ceil 10.
        assert_eq!(template.min_delay_between_shots, 10);
        assert_eq!(template.max_delay_between_shots, 10);
    }

    #[test]
    fn redefinition_without_override_fails_like_the_release_build() {
        let mut game = WeaponBlocks {
            names: NameKeyGenerator::new(),
            store: WeaponStore::new(),
        };
        let mut ini = Ini::new();
        ini.load(
            "a.ini",
            b"Weapon W\nPrimaryDamage = 5\nEnd\n".to_vec(),
            LoadType::Overwrite,
            &mut game,
        )
        .unwrap();
        let result = ini.load(
            "b.ini",
            b"Weapon W\nPrimaryDamage = 9\nEnd\n".to_vec(),
            LoadType::Overwrite,
            &mut game,
        );
        // The block was not consumed: the next line fails as an unknown
        // block, mirroring the release build.
        let error = result.unwrap_err();
        assert_eq!(error.kind, ErrorKind::UnknownToken);
        assert_eq!(error.line, 2);
        // The store keeps the original definition.
        assert_eq!(
            game.store
                .find("W", &mut game.names)
                .unwrap()
                .primary_damage,
            5.0
        );
    }

    #[test]
    fn overrides_parse_but_never_apply() {
        let mut game = WeaponBlocks {
            names: NameKeyGenerator::new(),
            store: WeaponStore::new(),
        };
        let mut ini = Ini::new();
        ini.load(
            "a.ini",
            b"Weapon W\nPrimaryDamage = 5\nEnd\n".to_vec(),
            LoadType::Overwrite,
            &mut game,
        )
        .unwrap();
        ini.load(
            "map.ini",
            b"Weapon W\nPrimaryDamage = 99\nEnd\n".to_vec(),
            LoadType::CreateOverrides,
            &mut game,
        )
        .unwrap();
        // Preserved upstream quirk: the override was parsed into an
        // unreachable copy; lookups still see the base.
        assert_eq!(
            game.store
                .find("W", &mut game.names)
                .unwrap()
                .primary_damage,
            5.0
        );
    }
}
