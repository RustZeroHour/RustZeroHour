// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! The `Locomotor` block type: complete port of the locomotor templates.
//!
//! Ported from `GeneralsMD/Code/GameEngine/Source/GameLogic/Object/
//! Locomotor.cpp` (constructor defaults, `TheFieldParse`, `validate`,
//! `parseLocomotorTemplateDefinition`, `newOverride`) and
//! `.../Include/GameLogic/Locomotor.h` / `LocomotorSet.h` (the name lists)
//! @ 571a7c5, Zero Hour configuration:
//!
//! * the `RTS_GENERALS` `DecelerationPitchLimit` defaulting is **not**
//!   ported (base game only);
//! * the `PRESERVE_RETAIL_PHYSICS_FORWARD_SPEED_AVERAGE` machinery is
//!   **disabled in the baseline default build** (`USE_RETAIL_PHYSICS_
//!   FORWARD_SPEED_DISCREPANCY()` is true there, which turns the average
//!   off), so `m_speedScale`/`scaleSpeed` — identity in this
//!   configuration — are not ported.
//!
//! Store semantics preserved from the original:
//!
//! * a **new** name creates a template with constructor defaults (marked
//!   as an override under [`LoadType::CreateOverrides`]);
//! * an **existing** name under [`LoadType::CreateOverrides`] chains a
//!   copy of the *final* override off it (the copy keeps every
//!   unspecified field, lookups resolve through the chain);
//! * an **existing** name under any other load type parses **over the
//!   existing template without clearing it** — a partial redefinition
//!   keeps unmentioned fields (unlike `Armor`, which resets).
//!
//! # Parity level
//!
//! **P0 exact** via the golden vectors (`tests/golden/ini.txt`).

use std::collections::BTreeMap;

use rzh_util::NameKeyGenerator;

use crate::reader::{Ini, IniFieldTarget, LoadType, SECONDS_PER_LOGICFRAME};
use crate::{ErrorKind, IniError};

/// `TheLocomotorSurfaceTypeNames` (LocomotorSet.h).
pub const SURFACE_TYPE_NAMES: [&str; 5] = ["GROUND", "WATER", "CLIFF", "AIR", "RUBBLE"];

/// `TheLocomotorBehaviorZNames` (Locomotor.h).
pub const BEHAVIOR_Z_NAMES: [&str; 8] = [
    "NO_Z_MOTIVE_FORCE",
    "SEA_LEVEL",
    "SURFACE_RELATIVE_HEIGHT",
    "ABSOLUTE_HEIGHT",
    "FIXED_SURFACE_RELATIVE_HEIGHT",
    "FIXED_ABSOLUTE_HEIGHT",
    "FIXED_RELATIVE_TO_GROUND_AND_BUILDINGS",
    "RELATIVE_TO_HIGHEST_LAYER",
];

/// `TheLocomotorAppearanceNames` (Locomotor.h).
pub const APPEARANCE_NAMES: [&str; 9] = [
    "TWO_LEGS",
    "FOUR_WHEELS",
    "TREADS",
    "HOVER",
    "THRUST",
    "WINGS",
    "CLIMBER",
    "OTHER",
    "MOTORCYCLE",
];

/// `TheLocomotorPriorityNames` (Locomotor.cpp).
pub const PRIORITY_NAMES: [&str; 3] = ["MOVES_BACK", "MOVES_MIDDLE", "MOVES_FRONT"];

/// `BIGNUM` (Locomotor.cpp).
const BIGNUM: f32 = 99999.0;

/// `LocomotorTemplate` — every field of the original parse table, in
/// table order, with the constructor's defaults.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct LocomotorTemplate {
    /// `m_nextOverride` chain (final-override lookups).
    pub next_override: Option<Box<LocomotorTemplate>>,
    /// `m_isOverride`.
    pub is_override: bool,
    /// The template name (`friend_setName`).
    pub name: Vec<u8>,

    pub surfaces: u32,
    pub max_speed: f32,
    pub max_speed_damaged: f32,
    pub max_turn_rate: f32,
    pub max_turn_rate_damaged: f32,
    pub acceleration: f32,
    pub acceleration_damaged: f32,
    pub lift: f32,
    pub lift_damaged: f32,
    pub braking: f32,
    pub min_speed: f32,
    pub min_turn_speed: f32,
    pub preferred_height: f32,
    pub preferred_height_damping: f32,
    pub circling_radius: f32,
    pub extra_2d_friction: f32,
    pub speed_limit_z: f32,
    pub max_thrust_angle: f32,
    pub behavior_z: i32,
    pub appearance: i32,
    pub move_priority: i32,
    pub accel_pitch_limit: f32,
    pub decel_pitch_limit: f32,
    pub bounce_kick: f32,
    pub pitch_stiffness: f32,
    pub roll_stiffness: f32,
    pub pitch_damping: f32,
    pub roll_damping: f32,
    pub thrust_roll: f32,
    pub wobble_rate: f32,
    pub min_wobble: f32,
    pub max_wobble: f32,
    pub pitch_by_z_vel_coef: f32,
    pub forward_vel_coef: f32,
    pub lateral_vel_coef: f32,
    pub forward_accel_coef: f32,
    pub lateral_accel_coef: f32,
    pub uniform_axial_damping: f32,
    pub turn_pivot_offset: f32,
    pub apply_2d_friction_when_airborne: bool,
    pub downhill_only: bool,
    pub allow_motive_force_while_airborne: bool,
    pub locomotor_works_when_dead: bool,
    pub airborne_targeting_height: i32,
    pub stick_to_ground: bool,
    pub can_move_backward: bool,
    pub has_suspension: bool,
    pub wheel_turn_angle: f32,
    pub maximum_wheel_extension: f32,
    pub maximum_wheel_compression: f32,
    pub close_enough_dist: f32,
    pub is_close_enough_dist_3d: bool,
    pub ultra_accurate_slide_into_place_factor: f32,
    pub wander_width_factor: f32,
    pub wander_length_factor: f32,
    pub wander_about_point_radius: f32,
    pub rudder_correction_degree: f32,
    pub rudder_correction_rate: f32,
    pub elevator_correction_degree: f32,
    pub elevator_correction_rate: f32,
}

impl Default for LocomotorTemplate {
    fn default() -> Self {
        Self::new()
    }
}

impl LocomotorTemplate {
    /// The constructor: the "same as undamaged if not specified"
    /// sentinels are negative; braking and minTurnSpeed default to
    /// `BIGNUM`.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn new() -> Self {
        Self {
            next_override: None,
            is_override: false,
            name: Vec::new(),

            surfaces: 0,
            max_speed: 0.0,
            max_speed_damaged: -1.0,
            max_turn_rate: 0.0,
            max_turn_rate_damaged: -1.0,
            acceleration: 0.0,
            acceleration_damaged: -1.0,
            lift: 0.0,
            lift_damaged: -1.0,
            braking: BIGNUM,
            min_speed: 0.0,
            min_turn_speed: BIGNUM,
            preferred_height: 0.0,
            preferred_height_damping: 1.0,
            circling_radius: 0.0,
            extra_2d_friction: 0.0,
            speed_limit_z: 999_999.0,
            max_thrust_angle: 0.0,
            behavior_z: 0,    // Z_NO_Z_MOTIVE_FORCE
            appearance: 7,    // LOCO_OTHER
            move_priority: 1, // LOCO_MOVES_MIDDLE
            accel_pitch_limit: 0.0,
            decel_pitch_limit: 0.0,
            bounce_kick: 0.0,
            pitch_stiffness: 0.1,
            roll_stiffness: 0.1,
            pitch_damping: 0.9,
            roll_damping: 0.9,
            thrust_roll: 0.0,
            wobble_rate: 0.0,
            min_wobble: 0.0,
            max_wobble: 0.0,
            pitch_by_z_vel_coef: 0.0,
            forward_vel_coef: 0.0,
            lateral_vel_coef: 0.0,
            forward_accel_coef: 0.0,
            lateral_accel_coef: 0.0,
            uniform_axial_damping: 1.0,
            turn_pivot_offset: 0.0,
            apply_2d_friction_when_airborne: false,
            downhill_only: false,
            allow_motive_force_while_airborne: false,
            locomotor_works_when_dead: false,
            airborne_targeting_height: i32::MAX,
            stick_to_ground: false,
            can_move_backward: false,
            has_suspension: false,
            wheel_turn_angle: 0.0,
            maximum_wheel_extension: 0.0,
            maximum_wheel_compression: 0.0,
            close_enough_dist: 1.0,
            is_close_enough_dist_3d: false,
            ultra_accurate_slide_into_place_factor: 0.0,
            wander_width_factor: 0.0,
            wander_length_factor: 1.0,
            wander_about_point_radius: 0.0,
            rudder_correction_degree: 0.0,
            rudder_correction_rate: 0.0,
            elevator_correction_degree: 0.0,
            elevator_correction_rate: 0.0,
        }
    }

    /// `friend_getFinalOverride`: walks the override chain.
    #[must_use]
    pub fn final_override(&self) -> &Self {
        match &self.next_override {
            Some(next) => next.final_override(),
            None => self,
        }
    }

    /// `validate()`: fills the damaged variants from the undamaged ones
    /// when their sentinels survive, heals WINGS minimum speeds, and
    /// rejects THRUST locomotors that carry Z behaviour or lift.
    ///
    /// The WINGS healing (`m_minSpeed = 0.01f`) is a release-build
    /// silent heal — the DEBUG_CRASH around it is a no-op outside debug
    /// builds. Validation failures carry no position context here; the
    /// store attaches the engine's current file/line when it propagates
    /// them.
    pub fn validate(&mut self) -> Result<(), ErrorKind> {
        if self.max_speed_damaged < 0.0 {
            self.max_speed_damaged = self.max_speed;
        }
        if self.max_turn_rate_damaged < 0.0 {
            self.max_turn_rate_damaged = self.max_turn_rate;
        }
        if self.acceleration_damaged < 0.0 {
            self.acceleration_damaged = self.acceleration;
        }
        if self.lift_damaged < 0.0 {
            self.lift_damaged = self.lift;
        }

        if self.appearance == 5 {
            // LOCO_WINGS
            if self.min_speed <= 0.0 {
                self.min_speed = 0.01;
            }
            if self.min_turn_speed <= 0.0 {
                self.min_turn_speed = 0.01;
            }
        }

        if self.appearance == 4 {
            // LOCO_THRUST
            if self.behavior_z != 0 || self.lift != 0.0 || self.lift_damaged != 0.0 {
                // "THRUST locos may not use ZAxisBehavior or lift!"
                return Err(ErrorKind::InvalidData);
            }
            if self.max_speed <= 0.0 {
                self.max_speed = 0.01;
            }
            if self.max_speed_damaged <= 0.0 {
                self.max_speed_damaged = 0.01;
            }
            if self.min_speed <= 0.0 {
                self.min_speed = 0.01;
            }
        }
        Ok(())
    }
}

/// The `parseFrictionPerSec` helper (Locomotor.cpp): per-second friction
/// converted to per-frame.
fn next_friction_per_sec(ini: &mut Ini) -> Result<f32, IniError> {
    let fric_per_sec = ini.scan_next_real()?;
    Ok(fric_per_sec * SECONDS_PER_LOGICFRAME)
}

impl IniFieldTarget for LocomotorTemplate {
    /// `LocomotorTemplate::getFieldParse` — every field of the original
    /// table, in table order.
    #[allow(clippy::too_many_lines)]
    fn parse_field(&mut self, ini: &mut Ini, field: &[u8]) -> Result<bool, IniError> {
        match field {
            b"Surfaces" => ini.next_bit_string32(&SURFACE_TYPE_NAMES, &mut self.surfaces)?,
            b"Speed" => self.max_speed = ini.next_velocity_real()?,
            b"SpeedDamaged" => self.max_speed_damaged = ini.next_velocity_real()?,
            b"TurnRate" => self.max_turn_rate = ini.next_angular_velocity_real()?,
            b"TurnRateDamaged" => self.max_turn_rate_damaged = ini.next_angular_velocity_real()?,
            b"Acceleration" => self.acceleration = ini.next_acceleration_real()?,
            b"AccelerationDamaged" => self.acceleration_damaged = ini.next_acceleration_real()?,
            b"Lift" => self.lift = ini.next_acceleration_real()?,
            b"LiftDamaged" => self.lift_damaged = ini.next_acceleration_real()?,
            b"Braking" => self.braking = ini.next_acceleration_real()?,
            b"MinSpeed" => self.min_speed = ini.next_velocity_real()?,
            b"MinTurnSpeed" => self.min_turn_speed = ini.next_velocity_real()?,
            b"PreferredHeight" => self.preferred_height = ini.scan_next_real()?,
            b"PreferredHeightDamping" => self.preferred_height_damping = ini.scan_next_real()?,
            b"CirclingRadius" => self.circling_radius = ini.scan_next_real()?,
            b"Extra2DFriction" => self.extra_2d_friction = next_friction_per_sec(ini)?,
            b"SpeedLimitZ" => self.speed_limit_z = ini.next_velocity_real()?,
            // yes, angle, not angular-vel (original comment preserved)
            b"MaxThrustAngle" => self.max_thrust_angle = ini.next_angle_real()?,
            b"ZAxisBehavior" => self.behavior_z = ini.scan_next_index_list(&BEHAVIOR_Z_NAMES)?,
            b"Appearance" => self.appearance = ini.scan_next_index_list(&APPEARANCE_NAMES)?,
            b"GroupMovementPriority" => {
                self.move_priority = ini.scan_next_index_list(&PRIORITY_NAMES)?;
            }
            b"AccelerationPitchLimit" => self.accel_pitch_limit = ini.next_angle_real()?,
            // Added in Zero Hour
            b"DecelerationPitchLimit" => self.decel_pitch_limit = ini.next_angle_real()?,
            b"BounceAmount" => self.bounce_kick = ini.next_angular_velocity_real()?,
            b"PitchStiffness" => self.pitch_stiffness = ini.scan_next_real()?,
            b"RollStiffness" => self.roll_stiffness = ini.scan_next_real()?,
            b"PitchDamping" => self.pitch_damping = ini.scan_next_real()?,
            b"RollDamping" => self.roll_damping = ini.scan_next_real()?,
            b"ThrustRoll" => self.thrust_roll = ini.scan_next_real()?,
            b"ThrustWobbleRate" => self.wobble_rate = ini.scan_next_real()?,
            b"ThrustMinWobble" => self.min_wobble = ini.scan_next_real()?,
            b"ThrustMaxWobble" => self.max_wobble = ini.scan_next_real()?,
            b"PitchInDirectionOfZVelFactor" => self.pitch_by_z_vel_coef = ini.scan_next_real()?,
            b"ForwardVelocityPitchFactor" => self.forward_vel_coef = ini.scan_next_real()?,
            b"LateralVelocityRollFactor" => self.lateral_vel_coef = ini.scan_next_real()?,
            b"ForwardAccelerationPitchFactor" => self.forward_accel_coef = ini.scan_next_real()?,
            b"LateralAccelerationRollFactor" => self.lateral_accel_coef = ini.scan_next_real()?,
            b"UniformAxialDamping" => self.uniform_axial_damping = ini.scan_next_real()?,
            b"TurnPivotOffset" => self.turn_pivot_offset = ini.scan_next_real()?,
            b"Apply2DFrictionWhenAirborne" => {
                self.apply_2d_friction_when_airborne = ini.scan_next_bool()?;
            }
            b"DownhillOnly" => self.downhill_only = ini.scan_next_bool()?,
            b"AllowAirborneMotiveForce" => {
                self.allow_motive_force_while_airborne = ini.scan_next_bool()?;
            }
            b"LocomotorWorksWhenDead" => {
                self.locomotor_works_when_dead = ini.scan_next_bool()?;
            }
            b"AirborneTargetingHeight" => self.airborne_targeting_height = ini.scan_next_int()?,
            b"StickToGround" => self.stick_to_ground = ini.scan_next_bool()?,
            b"CanMoveBackwards" => self.can_move_backward = ini.scan_next_bool()?,
            b"HasSuspension" => self.has_suspension = ini.scan_next_bool()?,
            b"FrontWheelTurnAngle" => self.wheel_turn_angle = ini.next_angle_real()?,
            b"MaximumWheelExtension" => self.maximum_wheel_extension = ini.scan_next_real()?,
            b"MaximumWheelCompression" => self.maximum_wheel_compression = ini.scan_next_real()?,
            b"CloseEnoughDist" => self.close_enough_dist = ini.scan_next_real()?,
            b"CloseEnoughDist3D" => self.is_close_enough_dist_3d = ini.scan_next_bool()?,
            b"SlideIntoPlaceTime" => {
                self.ultra_accurate_slide_into_place_factor = ini.next_duration_real()?;
            }
            b"WanderWidthFactor" => self.wander_width_factor = ini.scan_next_real()?,
            b"WanderLengthFactor" => self.wander_length_factor = ini.scan_next_real()?,
            b"WanderAboutPointRadius" => self.wander_about_point_radius = ini.scan_next_real()?,
            b"RudderCorrectionDegree" => self.rudder_correction_degree = ini.scan_next_real()?,
            b"RudderCorrectionRate" => self.rudder_correction_rate = ini.scan_next_real()?,
            b"ElevatorCorrectionDegree" => {
                self.elevator_correction_degree = ini.scan_next_real()?;
            }
            b"ElevatorCorrectionRate" => self.elevator_correction_rate = ini.scan_next_real()?,
            _ => return Ok(false),
        }
        Ok(true)
    }
}

/// `LocomotorStore`: base templates by name key; overrides chain off
/// them and stay reachable through [`LocomotorTemplate::final_override`].
#[derive(Debug, Default)]
pub struct LocomotorStore {
    templates: BTreeMap<u32, LocomotorTemplate>,
}

impl LocomotorStore {
    /// An empty store.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            templates: BTreeMap::new(),
        }
    }

    /// `LocomotorStore::newOverride`: copies the final override of
    /// `base`, chains the copy off it and marks it.
    fn new_override(base: &mut LocomotorTemplate) -> &mut LocomotorTemplate {
        let final_node = base.final_override_mut();
        let mut copy = final_node.clone();
        copy.is_override = true;
        final_node.next_override = Some(Box::new(copy));
        final_node
            .next_override
            .as_deref_mut()
            .expect("just assigned")
    }

    /// `parseLocomotorTemplateDefinition`.
    pub fn parse_definition(
        &mut self,
        ini: &mut Ini,
        names: &mut NameKeyGenerator,
    ) -> Result<(), IniError> {
        let token = ini.get_next_token()?;
        let namekey = names.name_to_key(&token);
        let load_type = ini.load_type();

        if let Some(existing) = self.templates.get_mut(&namekey.ordinal()) {
            if load_type == LoadType::CreateOverrides {
                // Parse into a fresh copy of the final override.
                let node = Self::new_override(existing);
                node.name = token;
                ini.init_from_ini(node)?;
                node.validate().map_err(|kind| ini.error(kind))?;
            } else {
                // Parse over the existing template WITHOUT clearing:
                // unmentioned fields keep their values.
                existing.name = token;
                ini.init_from_ini(existing)?;
                existing.validate().map_err(|kind| ini.error(kind))?;
            }
        } else {
            let mut template = LocomotorTemplate::new();
            if load_type == LoadType::CreateOverrides {
                template.is_override = true;
            }
            template.name = token;
            ini.init_from_ini(&mut template)?;
            template.validate().map_err(|kind| ini.error(kind))?;
            self.templates.insert(namekey.ordinal(), template);
        }
        Ok(())
    }

    /// `findLocomotorTemplate(namekey)`, resolved through the override
    /// chain (`friend_getFinalOverride`).
    #[must_use]
    pub fn find_final(&self, namekey: u32) -> Option<&LocomotorTemplate> {
        self.templates
            .get(&namekey)
            .map(LocomotorTemplate::final_override)
    }

    /// The base template for a name key (no override resolution).
    #[must_use]
    pub fn get(&self, namekey: u32) -> Option<&LocomotorTemplate> {
        self.templates.get(&namekey)
    }

    /// The number of base templates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    /// Iterates the base templates in name-key order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &LocomotorTemplate)> {
        self.templates.iter().map(|(k, v)| (*k, v))
    }
}

impl LocomotorTemplate {
    /// The mutable `friend_getFinalOverride` walk.
    fn final_override_mut(&mut self) -> &mut Self {
        if self.next_override.is_some() {
            Self::final_override_mut(
                self.next_override
                    .as_deref_mut()
                    .expect("is_some was checked"),
            )
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::BlockParser;

    struct LocoBlocks {
        names: NameKeyGenerator,
        store: LocomotorStore,
    }

    impl BlockParser for LocoBlocks {
        fn parse_block(&mut self, ini: &mut Ini, block: &[u8]) -> Result<bool, IniError> {
            if block == b"Locomotor" {
                self.store.parse_definition(ini, &mut self.names)?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }

    #[test]
    fn defaults_and_full_field_parse() {
        let mut game = LocoBlocks {
            names: NameKeyGenerator::new(),
            store: LocomotorStore::new(),
        };
        let mut ini = Ini::new();
        ini.load(
            "Locomotor.ini",
            b"Locomotor TestLoco\n\
              Surfaces = GROUND\n\
              Speed = 30\n\
              TurnRate = 60\n\
              Acceleration = 100\n\
              MaxThrustAngle = 15\n\
              Appearance = TREADS\n\
              ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\n\
              GroupMovementPriority = MOVES_FRONT\n\
              AirborneTargetingHeight = 55\n\
              CloseEnoughDist = 1.5\n\
              StickToGround = Yes\n\
              UnknownField = 42\n\
              End\n"
                .to_vec(),
            LoadType::Overwrite,
            &mut game,
        )
        .unwrap();

        let key = game.names.name_to_key("TestLoco").ordinal();
        let template = game.store.find_final(key).unwrap();
        assert_eq!(template.surfaces, 0b00001);
        // Speed = 30 dist/sec -> 1 dist/frame.
        assert_eq!(template.max_speed, 30.0f32 * (1.0f32 / 30.0f32));
        assert_eq!(template.appearance, 2);
        assert_eq!(template.behavior_z, 2);
        assert_eq!(template.move_priority, 2);
        assert_eq!(template.airborne_targeting_height, 55);
        assert_eq!(template.close_enough_dist, 1.5);
        assert!(template.stick_to_ground);
        // Unmentioned defaults survive (BIGNUM braking, 1.0 damping...).
        assert_eq!(template.braking, 99999.0);
        assert_eq!(template.preferred_height_damping, 1.0);
        assert_eq!(template.uniform_axial_damping, 1.0);
        assert_eq!(template.wander_length_factor, 1.0);
        // validate() filled the damaged variants.
        assert_eq!(template.max_speed_damaged, template.max_speed);
    }

    #[test]
    fn override_keeps_unmentioned_fields() {
        let mut game = LocoBlocks {
            names: NameKeyGenerator::new(),
            store: LocomotorStore::new(),
        };
        let mut ini = Ini::new();
        ini.load(
            "base.ini",
            b"Locomotor L\nSpeed = 20\nAppearance = FOUR_WHEELS\nEnd\n".to_vec(),
            LoadType::Overwrite,
            &mut game,
        )
        .unwrap();
        ini.load(
            "map.ini",
            b"Locomotor L\nSpeed = 30\nEnd\n".to_vec(),
            LoadType::CreateOverrides,
            &mut game,
        )
        .unwrap();

        let key = game.names.name_to_key("L").ordinal();
        let final_template = game.store.find_final(key).unwrap();
        assert_eq!(final_template.max_speed, 30.0f32 * (1.0f32 / 30.0f32));
        // The override copy kept FOUR_WHEELS.
        assert_eq!(final_template.appearance, 1);
        // The base is untouched underneath the chain.
        assert_eq!(
            game.store.get(key).unwrap().max_speed,
            20.0f32 * (1.0f32 / 30.0f32)
        );
        assert!(game.store.get(key).unwrap().next_override.is_some());
        assert!(final_template.is_override);
    }

    #[test]
    fn overwrite_redefinition_parses_over_without_clearing() {
        let mut game = LocoBlocks {
            names: NameKeyGenerator::new(),
            store: LocomotorStore::new(),
        };
        let mut ini = Ini::new();
        ini.load(
            "a.ini",
            b"Locomotor L\nSpeed = 20\nAppearance = HOVER\nEnd\n".to_vec(),
            LoadType::Overwrite,
            &mut game,
        )
        .unwrap();
        ini.load(
            "b.ini",
            b"Locomotor L\nSpeed = 30\nEnd\n".to_vec(),
            LoadType::Overwrite,
            &mut game,
        )
        .unwrap();

        let key = game.names.name_to_key("L").ordinal();
        let template = game.store.find_final(key).unwrap();
        assert_eq!(template.max_speed, 30.0f32 * (1.0f32 / 30.0f32));
        // No clear(): HOVER survives the redefinition.
        assert_eq!(template.appearance, 3);
    }

    #[test]
    fn thrust_validation_rejects_lift() {
        let mut game = LocoBlocks {
            names: NameKeyGenerator::new(),
            store: LocomotorStore::new(),
        };
        let mut ini = Ini::new();
        let result = ini.load(
            "bad.ini",
            b"Locomotor T\nAppearance = THRUST\nLift = 5\nEnd\n".to_vec(),
            LoadType::Overwrite,
            &mut game,
        );
        assert!(result.is_err());
    }

    #[test]
    fn wings_validation_heals_minimum_speeds() {
        let mut template = LocomotorTemplate::new();
        template.appearance = 5; // WINGS
        template.validate().unwrap();
        // Only non-positive minimums heal; the BIGNUM default for
        // minTurnSpeed is positive and survives.
        assert_eq!(template.min_speed, 0.01);
        assert_eq!(template.min_turn_speed, 99999.0);
    }
}
