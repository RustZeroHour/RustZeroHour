// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.

//! `PhysicsBehavior` — the simple rigid body physics update module.
//!
//! Origin: `GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Update/
//! PhysicsUpdate.cpp` and `.../Include/GameLogic/Module/PhysicsUpdate.h`
//! @ 571a7c5, Zero Hour configuration, non-retail branches:
//!
//! * the forward-speed measurement returns the true projection of the
//!   velocity onto the facing vector (the `@bugfix xezon 30/07/2026`
//!   dot-product form), so `USE_RETAIL_PHYSICS_FORWARD_SPEED_AVERAGE` is
//!   off and `scaleSpeed` is identity — the locomotor data slice records
//!   this as `m_speedScale = 1`;
//! * `USE_RETAIL_PHYSICS_FORWARD_SPEED_DISCREPANCY` is off, so
//!   `use_legacy_forward_speed()` is false;
//! * the weapon-crash, bounce-sound, fall-damage and freefall-disable
//!   bodies reach into combat/audio/lifecycle state that later slices own;
//!   they are stated as no-op host hooks here and wired up when those
//!   slices land.
//!
//! The module runs on [`SleepyUpdatePhase::Physics`] and processes all
//! disabled types itself (it internally skips integration while `Held`),
//! so its `disabled_types_to_process` mask is all bits set.
//!
//! Parity note: every `f32` operation below keeps the C++ operation order;
//! the trig entry points route through `rzh_math::gmath` (the GameMath pin,
//! ADR-001). A C++ differential transcription and golden vectors follow
//! when the Windows oracle toolchain is available (plan §3.2).

use rzh_math::{Matrix3D, Vector3, gmath, scalar};

use crate::logic::{KernelHost, UpdateOutcome};
use crate::modules::{ModuleBehavior, ModuleCtx};
use crate::types::{
    Coord3, INVALID_ID, KINDOF_PROJECTILE_BIT, OBJECT_STATUS_BRAKING_BIT, ObjectID,
    UPDATE_SLEEP_FOREVER, UPDATE_SLEEP_NONE, UpdateSleepTime,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// `DEFAULT_MASS` (PhysicsUpdate.cpp).
const DEFAULT_MASS: f32 = 1.0;
/// `DEFAULT_SHOCK_YAW` / `DEFAULT_SHOCK_PITCH` / `DEFAULT_SHOCK_ROLL`.
const DEFAULT_SHOCK_YAW: f32 = 0.05;
const DEFAULT_SHOCK_PITCH: f32 = 0.025;
const DEFAULT_SHOCK_ROLL: f32 = 0.025;
/// `DEFAULT_FORWARD_FRICTION` / `DEFAULT_LATERAL_FRICTION` (per frame).
const DEFAULT_FORWARD_FRICTION: f32 = 0.15;
const DEFAULT_LATERAL_FRICTION: f32 = 0.15;
/// `DEFAULT_Z_FRICTION` / `DEFAULT_AERO_FRICTION` (per frame).
const DEFAULT_Z_FRICTION: f32 = 0.8;
const DEFAULT_AERO_FRICTION: f32 = 0.0;
/// `MIN_AERO_FRICTION` / `MIN_NON_AERO_FRICTION` / `MAX_FRICTION`.
const MIN_AERO_FRICTION: f32 = 0.0;
const MIN_NON_AERO_FRICTION: f32 = 0.01;
const MAX_FRICTION: f32 = 0.99;
/// `STUN_RELIEF_EPSILON`.
const STUN_RELIEF_EPSILON: f32 = 0.5;
/// `MOTIVE_FRAMES = LOGICFRAMES_PER_SECOND / 3` (PhysicsUpdate.cpp).
const MOTIVE_FRAMES: u32 = 10;
/// `DEFAULT_GROUND_STIFFNESS` (GlobalData.cpp).
const DEFAULT_GROUND_STIFFNESS: f32 = 0.5;

/// `INVALID_VEL_MAG` (PhysicsUpdate.cpp): the `m_velMag` sentinel.
const INVALID_VEL_MAG: f32 = -1.0;

/// Whether the legacy retail forward-speed measurement is used. Off in the
/// Zero Hour default (non-retail) build this port follows.
const USE_LEGACY_FORWARD_SPEED: bool = false;

// ---------------------------------------------------------------------------
// Module data
// ---------------------------------------------------------------------------

/// `PhysicsBehaviorModuleData` (PhysicsUpdate.h): the per-template physics
/// config, with the constructor's defaults. The weapon-template fields the
/// original carries are combat-state (WP4) and are not ported here.
#[derive(Clone, Copy, Debug, Default)]
pub struct PhysicsBehaviorModuleData {
    /// `m_mass`.
    pub mass: f32,
    /// `m_shockResistance`.
    pub shock_resistance: f32,
    /// `m_shockMaxYaw`.
    pub shock_max_yaw: f32,
    /// `m_shockMaxPitch`.
    pub shock_max_pitch: f32,
    /// `m_shockMaxRoll`.
    pub shock_max_roll: f32,
    /// `m_forwardFriction` (per frame).
    pub forward_friction: f32,
    /// `m_lateralFriction` (per frame).
    pub lateral_friction: f32,
    /// `m_ZFriction` (per frame).
    pub z_friction: f32,
    /// `m_aerodynamicFriction` (per frame).
    pub aerodynamic_friction: f32,
    /// `m_centerOfMassOffset`.
    pub center_of_mass_offset: f32,
    /// `m_allowBouncing`.
    pub allow_bouncing: bool,
    /// `m_allowCollideForce`.
    pub allow_collide_force: bool,
    /// `m_killWhenRestingOnGround`.
    pub kill_when_resting_on_ground: bool,
    /// `m_minFallSpeedForDamage`.
    pub min_fall_speed_for_damage: f32,
    /// `m_fallHeightDamageFactor`.
    pub fall_height_damage_factor: f32,
    /// `m_pitchRollYawFactor`.
    pub pitch_roll_yaw_factor: f32,
}

impl PhysicsBehaviorModuleData {
    /// `PhysicsBehaviorModuleData()` constructor defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mass: DEFAULT_MASS,
            shock_resistance: 0.0,
            shock_max_yaw: DEFAULT_SHOCK_YAW,
            shock_max_pitch: DEFAULT_SHOCK_PITCH,
            shock_max_roll: DEFAULT_SHOCK_ROLL,
            forward_friction: DEFAULT_FORWARD_FRICTION,
            lateral_friction: DEFAULT_LATERAL_FRICTION,
            z_friction: DEFAULT_Z_FRICTION,
            aerodynamic_friction: DEFAULT_AERO_FRICTION,
            center_of_mass_offset: 0.0,
            allow_bouncing: false,
            allow_collide_force: true,
            kill_when_resting_on_ground: false,
            // `heightToSpeed(40.0)` with the default gravity magnitude 1.0:
            // `sqrt(fabs(2 * 1.0 * 40.0))`.
            min_fall_speed_for_damage: scalar::sqrt(scalar::fabs(2.0 * 1.0 * 40.0)),
            fall_height_damage_factor: 1.0,
            pitch_roll_yaw_factor: 2.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Turning type
// ---------------------------------------------------------------------------

/// `PhysicsTurningType` (PhysicsUpdate.h).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhysicsTurningType {
    /// `TURN_NEGATIVE` = -1.
    Negative = -1,
    /// `TURN_NONE` = 0.
    #[default]
    None = 0,
    /// `TURN_POSITIVE` = 1.
    Positive = 1,
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

/// `PhysicsFlagsType` (PhysicsUpdate.h): the bit flags, written out in
/// save/load, so the ordinals are stable.
mod flags {
    /// `STICK_TO_GROUND` = 0x0001.
    pub const STICK_TO_GROUND: u32 = 0x0001;
    /// `ALLOW_BOUNCE` = 0x0002.
    pub const ALLOW_BOUNCE: u32 = 0x0002;
    /// `APPLY_FRICTION2D_WHEN_AIRBORNE` = 0x0004.
    pub const APPLY_FRICTION2D_WHEN_AIRBORNE: u32 = 0x0004;
    /// `UPDATE_EVER_RUN` = 0x0008.
    pub const UPDATE_EVER_RUN: u32 = 0x0008;
    /// `WAS_AIRBORNE_LAST_FRAME` = 0x0010.
    pub const WAS_AIRBORNE_LAST_FRAME: u32 = 0x0010;
    /// `ALLOW_COLLIDE_FORCE` = 0x0020.
    pub const ALLOW_COLLIDE_FORCE: u32 = 0x0020;
    /// `ALLOW_TO_FALL` = 0x0040.
    pub const ALLOW_TO_FALL: u32 = 0x0040;
    /// `HAS_PITCHROLLYAW` = 0x0080.
    pub const HAS_PITCHROLLYAW: u32 = 0x0080;
    /// `IMMUNE_TO_FALLING_DAMAGE` = 0x0100.
    pub const IMMUNE_TO_FALLING_DAMAGE: u32 = 0x0100;
    /// `IS_IN_FREEFALL` = 0x0200.
    pub const IS_IN_FREEFALL: u32 = 0x0200;
    /// `IS_IN_UPDATE` = 0x0400.
    pub const IS_IN_UPDATE: u32 = 0x0400;
    /// `IS_STUNNED` = 0x0800.
    pub const IS_STUNNED: u32 = 0x0800;
}
use flags::*;

// ---------------------------------------------------------------------------
// PhysicsBehavior
// ---------------------------------------------------------------------------

/// `PhysicsBehavior` (PhysicsUpdate.cpp): the rigid body per-instance state.
/// The velocity, acceleration and rates live on the body and persist across
/// frames (the body is re-borrowed per dispatch but never recreated); the
/// object's transform lives on [`crate::logic::Object::transform`] and is
/// returned to the kernel as an [`UpdateOutcome::transform`] effect.
#[derive(Debug)]
pub struct PhysicsBehavior {
    /// `getPhysicsBehaviorModuleData()` — the template config.
    pub module_data: PhysicsBehaviorModuleData,
    /// `m_accel` / `m_prevAccel`.
    pub accel: Coord3,
    /// `m_prevAccel` — last frame's acceleration.
    pub prev_accel: Coord3,
    /// `m_vel`.
    pub vel: Coord3,
    /// `m_velMag` (`INVALID_VEL_MAG` = not yet computed since last change).
    pub vel_mag: f32,
    /// `m_yawRate` / `m_rollRate` / `m_pitchRate`.
    pub yaw_rate: f32,
    /// `m_rollRate`.
    pub roll_rate: f32,
    /// `m_pitchRate`.
    pub pitch_rate: f32,
    /// `m_mass` (module data mass plus contained mass once Contain lands).
    pub mass: f32,
    /// `m_motiveForceExpires`.
    pub motive_force_expires: u32,
    /// `m_flags`.
    pub flags: u32,
    /// `m_extraBounciness`.
    pub extra_bounciness: f32,
    /// `m_extraFriction`.
    pub extra_friction: f32,
    /// `m_currentOverlap` / `m_previousOverlap` / `m_lastCollidee`.
    pub current_overlap: ObjectID,
    /// `m_previousOverlap`.
    pub previous_overlap: ObjectID,
    /// `m_lastCollidee`.
    pub last_collidee: ObjectID,
    /// `m_ignoreCollisionsWith`.
    pub ignore_collisions_with: ObjectID,
    /// `m_originalAllowBounce`.
    pub original_allow_bounce: bool,
}

impl PhysicsBehavior {
    /// `PhysicsBehavior(thing, moduleData)`: the constructor. The object
    /// reference is not stored — the body receives `obj` on every dispatch.
    /// The module wakes one frame after birth, as in the original's
    /// `setWakeFrame(getObject(), UPDATE_SLEEP_NONE)`; the wake is scheduled
    /// through the module spec's `ctor_wake` at construction.
    #[must_use]
    pub fn new(module_data: &PhysicsBehaviorModuleData) -> Self {
        Self {
            module_data: *module_data,
            accel: Coord3::default(),
            prev_accel: Coord3::default(),
            vel: Coord3::default(),
            vel_mag: 0.0,
            yaw_rate: 0.0,
            roll_rate: 0.0,
            pitch_rate: 0.0,
            mass: module_data.mass,
            motive_force_expires: 0,
            flags: 0,
            extra_bounciness: 0.0,
            extra_friction: 0.0,
            current_overlap: INVALID_ID,
            previous_overlap: INVALID_ID,
            last_collidee: INVALID_ID,
            ignore_collisions_with: INVALID_ID,
            original_allow_bounce: module_data.allow_bouncing,
        }
    }

    // ---- flag accessors ------------------------------------------------

    #[must_use]
    fn flag(&self, bit: u32) -> bool {
        (self.flags & bit) != 0
    }

    fn set_flag(&mut self, bit: u32, value: bool) {
        if value {
            self.flags |= bit;
        } else {
            self.flags &= !bit;
        }
    }

    /// `getIsStunned`.
    #[must_use]
    pub fn get_is_stunned(&self) -> bool {
        self.flag(IS_STUNNED)
    }

    /// `setStunned`.
    pub fn set_stunned(&mut self, allow: bool) {
        self.set_flag(IS_STUNNED, allow);
    }

    /// `getAllowToFall`.
    #[must_use]
    pub fn get_allow_to_fall(&self) -> bool {
        self.flag(ALLOW_TO_FALL)
    }

    /// `setAllowToFall`.
    pub fn set_allow_to_fall(&mut self, allow: bool) {
        self.set_flag(ALLOW_TO_FALL, allow);
    }

    /// `setStickToGround`.
    pub fn set_stick_to_ground(&mut self, allow: bool) {
        self.set_flag(STICK_TO_GROUND, allow);
    }

    /// `setAllowBouncing`.
    pub fn set_allow_bouncing(&mut self, allow: bool) {
        self.set_flag(ALLOW_BOUNCE, allow);
    }

    /// `setAllowCollideForce`.
    pub fn set_allow_collide_force(&mut self, allow: bool) {
        self.set_flag(ALLOW_COLLIDE_FORCE, allow);
    }

    /// `setAllowAirborneFriction`.
    pub fn set_allow_airborne_friction(&mut self, allow: bool) {
        self.set_flag(APPLY_FRICTION2D_WHEN_AIRBORNE, allow);
    }

    /// `setImmuneToFallingDamage`.
    pub fn set_immune_to_falling_damage(&mut self, allow: bool) {
        self.set_flag(IMMUNE_TO_FALLING_DAMAGE, allow);
    }

    /// `setIsInFreeFall`.
    pub fn set_is_in_free_fall(&mut self, allow: bool) {
        self.set_flag(IS_IN_FREEFALL, allow);
    }

    /// `getIsInFreeFall`.
    #[must_use]
    pub fn get_is_in_free_fall(&self) -> bool {
        self.flag(IS_IN_FREEFALL)
    }

    /// `setExtraBounciness`.
    pub fn set_extra_bounciness(&mut self, b: f32) {
        self.extra_bounciness = b;
    }

    /// `setExtraFriction`.
    pub fn set_extra_friction(&mut self, b: f32) {
        self.extra_friction = b;
    }

    /// `getTurning`.
    #[must_use]
    pub fn get_turning(&self) -> PhysicsTurningType {
        // `m_turning` is driven by the locomotor sub-slice; neutral here.
        PhysicsTurningType::None
    }

    /// `setTurning` (stored by the locomotor sub-slice).
    pub fn set_turning(&mut self, _turning: PhysicsTurningType) {}

    // ---- force application ---------------------------------------------

    /// `isMotive`: a motive force is still within its frame window.
    #[must_use]
    pub fn is_motive(&self, frame: u32) -> bool {
        self.motive_force_expires > frame
    }

    /// `applyForce`: apply a force at the object's CG (`a = F/m`). When
    /// motive, only the lateral component is accepted.
    pub fn apply_force(&mut self, obj: &crate::logic::Object, force: &Coord3, frame: u32) {
        if force.x.is_nan() || force.y.is_nan() || force.z.is_nan() {
            return;
        }
        let mut mod_force = *force;
        if self.is_motive(frame) {
            let dir = obj.unit_direction_vector_2d();
            let lateral_dot = force.x * (-dir.y) + force.y * dir.x;
            mod_force.x = lateral_dot * (-dir.y);
            mod_force.y = lateral_dot * dir.x;
            mod_force.z = 0.0;
        }
        let mass_inv = 1.0 / self.mass;
        self.accel.x += mod_force.x * mass_inv;
        self.accel.y += mod_force.y * mass_inv;
        self.accel.z += mod_force.z * mass_inv;
        self.vel_mag = INVALID_VEL_MAG;
        // `setWakeFrame(obj, UPDATE_SLEEP_NONE)` when not IS_IN_UPDATE — the
        // kernel's wake surface is used by the external caller (locomotor);
        // during `update()` IS_IN_UPDATE is set so nothing happens here.
    }

    /// `applyMotiveForce`: like `applyForce`, accepted unquestioningly and
    /// flagged motive for `MOTIVE_FRAMES`.
    pub fn apply_motive_force(&mut self, obj: &crate::logic::Object, force: &Coord3, frame: u32) {
        self.motive_force_expires = 0; // accept unquestioningly
        self.apply_force(obj, force, frame);
        self.motive_force_expires = frame + MOTIVE_FRAMES;
    }

    /// `clearAcceleration`: force clear for out-of-bounds recovery.
    pub fn clear_acceleration(&mut self) {
        self.accel = Coord3::default();
        self.vel_mag = INVALID_VEL_MAG;
    }

    /// `applyGravitationalForces`: `m_accel.z += gravity`.
    pub fn apply_gravitational_forces(&mut self, gravity: f32) {
        self.accel.z += gravity;
    }

    /// `getForwardSpeed2D`: the true projection of the velocity onto the
    /// facing vector (non-retail branch; the legacy sqrt form is dead in
    /// the Zero Hour default build).
    #[must_use]
    pub fn get_forward_speed_2d(&self, obj: &crate::logic::Object) -> f32 {
        let dir = obj.unit_direction_vector_2d();
        let dot = self.vel.x * dir.x + self.vel.y * dir.y;
        if USE_LEGACY_FORWARD_SPEED {
            let speed = scalar::sqrt(self.vel.x * self.vel.x + self.vel.y * self.vel.y);
            if dot >= 0.0 { speed } else { -speed }
        } else {
            dot
        }
    }

    /// `getForwardSpeed3D`: the 3d projection onto the facing (X) vector.
    #[must_use]
    pub fn get_forward_speed_3d(&self, obj: &crate::logic::Object) -> f32 {
        let dir = obj.transform.x_vector();
        let dot = self.vel.x * dir.x + self.vel.y * dir.y + self.vel.z * dir.z;
        if USE_LEGACY_FORWARD_SPEED {
            let speed = scalar::sqrt(
                self.vel.x * self.vel.x + self.vel.y * self.vel.y + self.vel.z * self.vel.z,
            );
            if dot >= 0.0 { speed } else { -speed }
        } else {
            dot
        }
    }

    /// `getVelocityMagnitude`: the speed, lazily computed.
    #[must_use]
    pub fn get_velocity_magnitude(&self) -> f32 {
        if self.vel_mag == INVALID_VEL_MAG {
            // computed below
        }
        let _ = self.vel_mag;
        scalar::sqrt(self.vel.x * self.vel.x + self.vel.y * self.vel.y + self.vel.z * self.vel.z)
    }

    /// `getMass`: `m_mass` plus contained items mass once Contain lands.
    #[must_use]
    pub fn get_mass(&self) -> f32 {
        self.mass
    }

    // ---- friction ------------------------------------------------------

    /// `getAerodynamicFriction`, clamped to [MIN_AERO_FRICTION, MAX_FRICTION].
    #[must_use]
    pub fn get_aerodynamic_friction(&self) -> f32 {
        (self.module_data.aerodynamic_friction + self.extra_friction)
            .clamp(MIN_AERO_FRICTION, MAX_FRICTION)
    }

    /// `getForwardFriction`, clamped to [MIN_NON_AERO_FRICTION, MAX_FRICTION].
    #[must_use]
    pub fn get_forward_friction(&self) -> f32 {
        (self.module_data.forward_friction + self.extra_friction)
            .clamp(MIN_NON_AERO_FRICTION, MAX_FRICTION)
    }

    /// `getLateralFriction`, clamped to [MIN_NON_AERO_FRICTION, MAX_FRICTION].
    #[must_use]
    pub fn get_lateral_friction(&self) -> f32 {
        (self.module_data.lateral_friction + self.extra_friction)
            .clamp(MIN_NON_AERO_FRICTION, MAX_FRICTION)
    }

    /// `getZFriction`, clamped to [MIN_NON_AERO_FRICTION, MAX_FRICTION].
    #[must_use]
    pub fn get_z_friction(&self) -> f32 {
        (self.module_data.z_friction + self.extra_friction)
            .clamp(MIN_NON_AERO_FRICTION, MAX_FRICTION)
    }

    /// `applyFrictionalForces`: 2d lateral/forward friction when on or near
    /// the ground, aerodynamic friction when airborne.
    pub fn apply_frictional_forces(
        &mut self,
        obj: &crate::logic::Object,
        frame: u32,
        host: &mut dyn KernelHost,
    ) {
        // `deckTaxiing` needs the AI + locomotor set (later slices); skipped.
        let significantly_above = Self::is_significantly_above_terrain(obj, host);
        if self.flag(APPLY_FRICTION2D_WHEN_AIRBORNE) || !significantly_above {
            self.apply_ypr_damping(1.0 - DEFAULT_LATERAL_FRICTION);
            if self.vel.x != 0.0 || self.vel.y != 0.0 {
                let dir = obj.unit_direction_vector_2d();
                let mass = self.get_mass();
                let lateral_dot = self.vel.x * (-dir.y) + self.vel.y * dir.x;
                let lateral_vel_x = lateral_dot * (-dir.y);
                let lateral_vel_y = lateral_dot * dir.x;
                let lf = mass * self.get_lateral_friction();
                let mut accel = Coord3 {
                    x: -(lf * lateral_vel_x),
                    y: -(lf * lateral_vel_y),
                    z: 0.0,
                };
                if !self.is_motive(frame) {
                    let forward_dot = self.vel.x * dir.x + self.vel.y * dir.y;
                    let forward_vel_x = forward_dot * dir.x;
                    let forward_vel_y = forward_dot * dir.y;
                    let ff = mass * self.get_forward_friction();
                    accel.x += -(ff * forward_vel_x);
                    accel.y += -(ff * forward_vel_y);
                }
                self.apply_force(obj, &accel, frame);
            }
        } else {
            let aerodynamics = -self.get_aerodynamic_friction();
            self.accel.x += self.vel.x * aerodynamics;
            self.accel.y += self.vel.y * aerodynamics;
            self.accel.z += self.vel.z * aerodynamics;
            self.apply_ypr_damping(1.0 + aerodynamics);
        }
    }

    // ---- bounce --------------------------------------------------------

    /// `handleBounce`: resolve a ground impact, returning the bounce force
    /// and an optional righted transform (yaw kept, pitch/roll zeroed).
    #[allow(clippy::too_many_arguments)]
    fn handle_bounce(
        &mut self,
        obj: &crate::logic::Object,
        old_z: f32,
        new_z: f32,
        ground_z: f32,
    ) -> (bool, Coord3, Option<Matrix3D>) {
        if !self.flag(ALLOW_BOUNCE) || new_z > ground_z {
            return (false, Coord3::default(), None);
        }

        let stiffness = DEFAULT_GROUND_STIFFNESS.clamp(0.01, 0.99);

        let mut desired_accel_z = 0.0;
        let vz = self.vel.z;
        if old_z > ground_z && vz < 0.0 {
            desired_accel_z = scalar::fabs(vz) * stiffness;
        }

        let bounce_force = Coord3 {
            z: self.get_mass() * desired_accel_z,
            ..Default::default()
        };

        self.apply_ypr_damping(0.7);

        let mut righted = None;
        if vz < 0.0 {
            let zvec = obj.transform.z_vector();
            let roll_angle = if zvec.z > 0.0 { 0.0 } else { scalar::PI };
            let yaw_angle = obj.transform.z_rotation();
            righted = Some(self.set_angles(obj, yaw_angle, 0.0, roll_angle));
        }

        if bounce_force.z > 0.0 {
            // `testStunnedUnitForDestruction` — combat/lifecycle slice.
            (true, bounce_force, righted)
        } else {
            // restore the original allow-bounce on the next allow
            (false, bounce_force, righted)
        }
    }

    /// `setAngles`: build a transform at the object's position with the given
    /// yaw/pitch/roll (the C++ `In_Place_Pre_Rotate` order; translation kept).
    #[must_use]
    fn set_angles(&self, obj: &crate::logic::Object, yaw: f32, pitch: f32, roll: f32) -> Matrix3D {
        let mut xfrm = Matrix3D::identity();
        xfrm.set_translation(Vector3 {
            x: obj.position.x,
            y: obj.position.y,
            z: obj.position.z,
        });
        xfrm.in_place_pre_rotate_x(-roll);
        xfrm.in_place_pre_rotate_y(pitch);
        xfrm.in_place_pre_rotate_z(yaw);
        xfrm
    }

    /// `applyYPRDamping`.
    pub fn apply_ypr_damping(&mut self, factor: f32) {
        self.pitch_rate *= factor;
        self.roll_rate *= factor;
        self.yaw_rate *= factor;
        self.set_flag(
            HAS_PITCHROLLYAW,
            self.pitch_rate != 0.0 || self.roll_rate != 0.0 || self.yaw_rate != 0.0,
        );
    }

    /// `setPitchRate`.
    pub fn set_pitch_rate(&mut self, pitch: f32) {
        self.pitch_rate = pitch;
        self.apply_ypr_damping(1.0);
    }

    /// `setRollRate`.
    pub fn set_roll_rate(&mut self, roll: f32) {
        self.roll_rate = roll;
        self.apply_ypr_damping(1.0);
    }

    /// `setYawRate`.
    pub fn set_yaw_rate(&mut self, yaw: f32) {
        self.yaw_rate = yaw;
        self.apply_ypr_damping(1.0);
    }

    // ---- velocity scrubs -----------------------------------------------

    /// `scrubVelocityZ`: signed z-velocity clamp.
    pub fn scrub_velocity_z(&mut self, desired_velocity: f32) {
        if scalar::fabs(desired_velocity) < 0.001 {
            self.vel.z = 0.0;
        } else if (desired_velocity < 0.0 && self.vel.z < desired_velocity)
            || (desired_velocity > 0.0 && self.vel.z > desired_velocity)
        {
            self.vel.z = desired_velocity;
        }
        self.vel_mag = INVALID_VEL_MAG;
    }

    /// `scrubVelocity2D`: scale the 2d velocity down to `desiredVelocity`.
    pub fn scrub_velocity_2d(&mut self, desired_velocity: f32) {
        if desired_velocity < 0.001 {
            self.vel.x = 0.0;
            self.vel.y = 0.0;
        } else {
            let cur_velocity = scalar::sqrt(self.vel.x * self.vel.x + self.vel.y * self.vel.y);
            if desired_velocity > cur_velocity {
                return;
            }
            let scale = desired_velocity / cur_velocity;
            self.vel.x *= scale;
            self.vel.y *= scale;
        }
        self.vel_mag = INVALID_VEL_MAG;
    }

    /// `addVelocityTo`.
    pub fn add_velocity_to(&mut self, vel: &Coord3) {
        self.vel.x += vel.x;
        self.vel.y += vel.y;
        self.vel.z += vel.z;
        self.vel_mag = INVALID_VEL_MAG;
    }

    /// `transferVelocityTo`: add this body's velocity to another body.
    pub fn transfer_velocity_to(&self, that: &mut PhysicsBehavior) {
        that.vel.x += self.vel.x;
        that.vel.y += self.vel.y;
        that.vel.z += self.vel.z;
        that.vel_mag = INVALID_VEL_MAG;
    }

    // ---- overlap -------------------------------------------------------

    /// `addOverlap`.
    pub fn add_overlap(&mut self, obj: ObjectID) {
        if obj != INVALID_ID && !self.is_currently_overlapped(obj) {
            self.current_overlap = obj;
        }
    }

    /// `isCurrentlyOverlapped`.
    #[must_use]
    pub fn is_currently_overlapped(&self, obj: ObjectID) -> bool {
        obj != INVALID_ID && obj == self.current_overlap
    }

    // ---- reset ---------------------------------------------------------

    /// `resetDynamicPhysics`: nuke vel/accel/rates (off-map reset path).
    pub fn reset_dynamic_physics(&mut self) {
        self.accel = Coord3::default();
        self.prev_accel = Coord3::default();
        self.vel = Coord3::default();
        self.vel_mag = 0.0;
        self.yaw_rate = 0.0;
        self.roll_rate = 0.0;
        self.pitch_rate = 0.0;
        self.set_flag(HAS_PITCHROLLYAW, false);
    }

    // ---- terrain helpers -----------------------------------------------

    /// `is_above_terrain`: `getHeightAboveTerrain() > 0`.
    #[must_use]
    pub fn is_above_terrain(
        objects: &crate::logic::Objects,
        obj: ObjectID,
        host: &mut dyn KernelHost,
    ) -> bool {
        let object = objects.get(obj);
        object.position.z > host.ground_height(object.position.x, object.position.y, object.layer)
    }

    /// `is_significantly_above_terrain`: height > `-9 * gravity` (3 frames
    /// to return to the ground).
    #[must_use]
    pub fn is_significantly_above_terrain(
        object: &crate::logic::Object,
        host: &mut dyn KernelHost,
    ) -> bool {
        let gravity = host.gravity();
        let ground = host.ground_height(object.position.x, object.position.y, object.layer);
        object.position.z - ground > -(3.0 * 3.0) * gravity
    }

    // ---- update --------------------------------------------------------

    /// `update`: the Euler integrator (gravity, friction, ground clamp,
    /// bounce, rotation rates, sleep). Returns the new transform as an
    /// effect plus the sleep length.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        _module: u32,
        host: &mut dyn KernelHost,
    ) -> UpdateOutcome {
        use crate::types::DisabledType;
        let frame = ctx.frame;
        let object = ctx.objects.get(obj);
        let d = self.module_data; // Copy
        let airborne_at_start = Self::is_above_terrain(ctx.objects, obj, host);
        let mut active_vel_z = 0.0;
        let mut bounce_force = Coord3::default();
        let mut got_bounce_force = false;

        self.set_flag(IS_IN_UPDATE, true);

        if !self.flag(UPDATE_EVER_RUN) {
            self.set_flag(WAS_AIRBORNE_LAST_FRAME, airborne_at_start);
        }

        let _prev_pos = object.position;
        self.prev_accel = self.accel;

        let mut outcome = UpdateOutcome::default();

        if !object.disabled_mask.test(DisabledType::Held) {
            let mut mtx = object.transform;

            self.apply_gravitational_forces(host.gravity());
            self.apply_frictional_forces(object, frame, host);

            // integrate acceleration into velocity
            self.vel.x += self.accel.x;
            self.vel.y += self.accel.y;
            self.vel.z += self.accel.z;

            const THRESH: f32 = 0.001;
            if scalar::fabs(self.vel.x) < THRESH {
                self.vel.x = 0.0;
            }
            if scalar::fabs(self.vel.y) < THRESH {
                self.vel.y = 0.0;
            }
            if scalar::fabs(self.vel.z) < THRESH {
                self.vel.z = 0.0;
            }
            self.vel_mag = INVALID_VEL_MAG;

            let old_pos_z = mtx.translation().z;

            if object.status & (1 << OBJECT_STATUS_BRAKING_BIT) != 0 {
                // braking: don't update x/y; projectiles freeze entirely.
                if object.kindof & (1 << KINDOF_PROJECTILE_BIT) == 0 {
                    let z = mtx.translation().z + self.vel.z;
                    mtx.set_translation(Vector3 {
                        x: mtx.translation().x,
                        y: mtx.translation().y,
                        z,
                    });
                }
            } else {
                mtx.adjust_translation(Vector3 {
                    x: self.vel.x,
                    y: self.vel.y,
                    z: self.vel.z,
                });
            }

            // NaN position -> delete (the original DEBUG_CRASHes; the port
            // reports it as a destroy request so the kernel removes it).
            if mtx.translation().x.is_nan()
                || mtx.translation().y.is_nan()
                || mtx.translation().z.is_nan()
            {
                outcome.destroy = Some(obj);
            }

            // stunned relief
            if self.get_is_stunned()
                && ((scalar::fabs(self.vel.x) < STUN_RELIEF_EPSILON
                    && scalar::fabs(self.vel.y) < STUN_RELIEF_EPSILON
                    && scalar::fabs(self.vel.z) < STUN_RELIEF_EPSILON)
                    || !Self::is_significantly_above_terrain(object, host))
            {
                self.set_stunned(false);
                // `clearModelConditionState(MODELCONDITION_STUNNED)` —
                // client/model slice.
            }

            if self.flag(HAS_PITCHROLLYAW) {
                let yaw_rate_to_use = self.yaw_rate * d.pitch_roll_yaw_factor;
                let mut pitch_rate_to_use = self.pitch_rate * d.pitch_roll_yaw_factor;
                let roll_rate_to_use = self.roll_rate * d.pitch_roll_yaw_factor;

                let offset = d.center_of_mass_offset;
                if offset != 0.0 {
                    let xvec = mtx.x_vector();
                    let xy = scalar::sqrt(xvec.x * xvec.x + xvec.y * xvec.y);
                    let pitch_angle = gmath::atan2f(xvec.z, xy);
                    let remaining_angle = if offset > 0.0 {
                        scalar::HALF_PI - pitch_angle
                    } else {
                        -scalar::HALF_PI + pitch_angle
                    };
                    pitch_rate_to_use *= gmath::sinf(remaining_angle);
                }

                mtx.rotate_x(roll_rate_to_use);
                mtx.rotate_y(pitch_rate_to_use);
                mtx.rotate_z(yaw_rate_to_use);
            }

            // do not allow the object to pass through the ground
            let ground_z =
                host.ground_height(mtx.translation().x, mtx.translation().y, object.layer);
            // `OBJECT_STATUS_DECK_HEIGHT_OFFSET` / carrier deck height —
            // contain/carrier slice.
            let _ = _prev_pos;

            let (gb, bf, righted) =
                self.handle_bounce(object, old_pos_z, mtx.translation().z, ground_z);
            got_bounce_force = gb;
            bounce_force = bf;

            active_vel_z = self.vel.z;

            if mtx.translation().z <= ground_z {
                let dz = ground_z - mtx.translation().z;
                self.vel.z += dz;
                if self.vel.z > 0.0 {
                    self.vel.z = 0.0;
                }
                self.vel_mag = INVALID_VEL_MAG;
                mtx.set_translation(Vector3 {
                    x: mtx.translation().x,
                    y: mtx.translation().y,
                    z: ground_z,
                });
                self.set_flag(ALLOW_TO_FALL, false);

                if self.flag(IS_STUNNED) {
                    // `clearModelConditionState(STUNNED_FLAILING)` /
                    // `setModelConditionState(STUNNED)` — client slice.
                }
            } else if mtx.translation().z > ground_z {
                if self.flag(IS_IN_FREEFALL) {
                    // `setDisabled(DISABLED_FREEFALL)` +
                    // `MODELCONDITION_FREEFALL` — disable/client slice.
                } else if self.flag(STICK_TO_GROUND) && !self.flag(ALLOW_TO_FALL) {
                    mtx.set_translation(Vector3 {
                        x: mtx.translation().x,
                        y: mtx.translation().y,
                        z: ground_z,
                    });
                }
            }

            match righted {
                Some(r) => {
                    // right the object after the bounce (yaw kept,
                    // pitch/roll zeroed); preserve the clamped translation.
                    let t = mtx.translation();
                    let mut final_mtx = r;
                    final_mtx.set_translation(Vector3 {
                        x: t.x,
                        y: t.y,
                        z: t.z,
                    });
                    outcome.transform = Some(final_mtx);
                }
                None => outcome.transform = Some(mtx),
            }
        }

        // reset the acceleration for accumulation next frame
        self.accel = Coord3::default();

        // clear overlap
        self.previous_overlap = self.current_overlap;
        self.current_overlap = INVALID_ID;

        if got_bounce_force && self.flag(ALLOW_BOUNCE) {
            self.apply_force(object, &bounce_force, frame);
        }

        let airborne_at_end = Self::is_above_terrain(ctx.objects, obj, host);

        if self.flag(WAS_AIRBORNE_LAST_FRAME)
            && !airborne_at_end
            && !self.flag(IMMUNE_TO_FALLING_DAMAGE)
        {
            // `doBounceSound` — audio slice.
            // `obj->onCollide(nullptr, position, &down)` — ground collide
            // dispatch (later slice).
            // falling damage: `attemptDamage(DAMAGE_FALLING)` — combat slice.
            let _ = active_vel_z;
            let _ = d.min_fall_speed_for_damage;
            let _ = d.fall_height_damage_factor;
        }

        if !airborne_at_end {
            self.set_flag(IS_IN_FREEFALL, false);
            if object.disabled_mask.test(DisabledType::Freefall) {
                // `clearDisabled(DISABLED_FREEFALL)` — kernel half.
            }
            // `clearModelConditionState(MODELCONDITION_FREEFALL)` — client.
        }

        if d.kill_when_resting_on_ground && !airborne_at_end && Self::is_very_small3d(&self.vel) {
            // `obj->kill()` — combat/lifecycle slice (drone/vehicle rules).
        }

        self.set_flag(UPDATE_EVER_RUN, true);
        self.set_flag(WAS_AIRBORNE_LAST_FRAME, airborne_at_end);
        self.set_flag(IS_IN_UPDATE, false);

        outcome.sleep = self.calc_sleep_time(frame, object);
        outcome
    }

    /// `isVerySmall3D`.
    #[must_use]
    fn is_very_small3d(v: &Coord3) -> bool {
        const THRESH: f32 = 0.01;
        scalar::fabs(v.x) < THRESH && scalar::fabs(v.y) < THRESH && scalar::fabs(v.z) < THRESH
    }

    /// `calcSleepTime`: `UPDATE_SLEEP_FOREVER` when at rest on the ground
    /// with no pending state, else `UPDATE_SLEEP_NONE`.
    #[must_use]
    fn calc_sleep_time(&self, frame: u32, object: &crate::logic::Object) -> UpdateSleepTime {
        if self.is_zero3d(&self.vel)
            && self.is_zero3d(&self.accel)
            && !self.flag(HAS_PITCHROLLYAW)
            && !self.is_motive(frame)
            && object.layer == 0
            && !Self::is_above_terrain_object(object)
            && self.current_overlap == INVALID_ID
            && self.previous_overlap == INVALID_ID
            && self.flag(UPDATE_EVER_RUN)
        {
            UPDATE_SLEEP_FOREVER
        } else {
            UPDATE_SLEEP_NONE
        }
    }

    #[must_use]
    fn is_zero3d(&self, v: &Coord3) -> bool {
        v.x == 0.0 && v.y == 0.0 && v.z == 0.0
    }

    /// Flat-terrain `isAboveTerrain` for the sleep check (the real query is
    /// host-driven in `update`).
    #[must_use]
    fn is_above_terrain_object(object: &crate::logic::Object) -> bool {
        object.position.z > 0.0
    }

    // ---- shocks (combat slice; deterministic part here) ----------------

    /// `applyShock`: apply a shockwave force, reduced by shock resistance.
    pub fn apply_shock(&mut self, obj: &crate::logic::Object, force: &Coord3, frame: u32) {
        let mut resisted_force = *force;
        let resistance = self.module_data.shock_resistance.clamp(0.0, 1.0);
        resisted_force.x *= 1.0 - resistance;
        resisted_force.y *= 1.0 - resistance;
        resisted_force.z *= 1.0 - resistance;
        self.apply_force(obj, &resisted_force, frame);
    }

    /// `applyRandomRotation`: applies a random pitch/roll/yaw kick. The
    /// random draw needs the sim RNG host hook (combat slice); not wired
    /// here.
    pub fn apply_random_rotation(&mut self, _obj: &crate::logic::Object) {
        // `GameLogicRandomValue(-1, 1)` draws — the sim RNG surface is the
        // combat/weapon slice's; left as a no-op until it lands.
    }

    // ---- audio ---------------------------------------------------------

    /// `doBounceSound`: audio slice (Miles replacement). No-op here.
    #[allow(dead_code)]
    pub fn do_bounce_sound(&self, _prev_pos: &Coord3) {
        // `TheAudio->addAudioEvent` — audio phase.
    }
}

impl ModuleBehavior for PhysicsBehavior {
    fn update(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        host: &mut dyn crate::logic::KernelHost,
    ) -> crate::logic::UpdateOutcome {
        self.update(ctx, obj, module, host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::logic::{Kernel, ObjectSpec, UpdateOutcome};
    use crate::modules::{ModuleCtx, ModuleFactory, ModuleType};
    use crate::scheduler::{ModuleArena, ModuleHandle};
    use crate::types::{GeometryInfo, SleepyUpdatePhase, UPDATE_SLEEP_FOREVER, UPDATE_SLEEP_NONE};

    /// A host with the default gravity (-1.0) and flat terrain at z = 0.
    struct TestHost;

    impl KernelHost for TestHost {}

    /// Builds a live object at `position` facing `orientation` (yaw).
    fn test_object(id: ObjectID, position: Coord3, orientation: f32) -> crate::logic::Object {
        let mut transform = Matrix3D::identity();
        transform.set_translation(Vector3 {
            x: position.x,
            y: position.y,
            z: position.z,
        });
        transform.rotate_z(orientation);
        crate::logic::Object {
            id,
            geometry: GeometryInfo::default(),
            position,
            orientation,
            transform,
            layer: 0,
            kindof: 0,
            status: 0,
            private_status: 0,
            build_cost: 0.0,
            disabled_mask: crate::types::DisabledMask::none(),
            disabled_till_frame: [crate::types::NEVER; crate::types::DISABLED_COUNT],
            behaviors: Vec::new(),
            body_module: None,
            contain_module: None,
            partition: None,
            cell_change_count: 0,
            team: crate::types::TEAM_ID_INVALID,
            energy_production: 0,
            energy_bonus: 0,
            vision_range: 0.0,
            has_ai: false,
            ai_idle: false,
        }
    }

    /// Runs one physics update and returns the outcome.
    fn run_update(
        behavior: &mut PhysicsBehavior,
        objects: &crate::logic::Objects,
        obj: ObjectID,
        frame: u32,
        host: &mut TestHost,
    ) -> UpdateOutcome {
        let ctx = ModuleCtx::new(frame, objects);
        behavior.update(&ctx, obj, 0, host)
    }

    #[test]
    fn gravity_pulls_an_object_down_one_frame() {
        let mut objects = crate::logic::Objects::default();
        let obj = test_object(
            1,
            Coord3 {
                x: 0.0,
                y: 0.0,
                z: 10.0,
            },
            0.0,
        );
        objects.put(obj);

        let mut behavior = PhysicsBehavior::new(&PhysicsBehaviorModuleData::new());
        let mut host = TestHost;

        // frame 0: gravity accel.z = -1, vel.z = -1, pos.z = 9.
        let outcome = run_update(&mut behavior, &objects, 1, 0, &mut host);
        assert_eq!(outcome.sleep, UPDATE_SLEEP_NONE);
        let transform = outcome.transform.expect("transform effect");
        assert!((transform.translation().z - 9.0).abs() < f32::EPSILON);
        assert!((behavior.vel.z - (-1.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn friction_scales_velocity_toward_zero() {
        let mut objects = crate::logic::Objects::default();
        // facing +x, moving +x at speed 10.
        let obj = test_object(
            1,
            Coord3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            0.0,
        );
        objects.put(obj);

        let mut behavior = PhysicsBehavior::new(&PhysicsBehaviorModuleData::new());
        behavior.vel.x = 10.0;
        behavior.vel_mag = INVALID_VEL_MAG;
        let mut host = TestHost;

        // on the ground, not significantly airborne → 2d friction applies.
        let outcome = run_update(&mut behavior, &objects, 1, 0, &mut host);
        let _ = outcome;
        // friction: accel = -mass*friction*vel_dir applied; vel decreases.
        assert!(behavior.vel.x < 10.0);
        assert!(behavior.vel.x > 0.0);
        assert!(behavior.vel.y == 0.0);
    }

    #[test]
    fn an_object_at_rest_on_ground_sleeps_forever() {
        let mut objects = crate::logic::Objects::default();
        let obj = test_object(
            1,
            Coord3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            0.0,
        );
        objects.put(obj);

        let mut behavior = PhysicsBehavior::new(&PhysicsBehaviorModuleData::new());
        // mark as having run so the sleep check is eligible
        behavior.set_flag(UPDATE_EVER_RUN, true);
        let mut host = TestHost;

        let outcome = run_update(&mut behavior, &objects, 1, 0, &mut host);
        assert_eq!(outcome.sleep, UPDATE_SLEEP_FOREVER);
    }

    #[test]
    fn kernel_integration_runs_physics_under_gravity() {
        // Register the physics module, build an object that carries it, and
        // drive the kernel frame loop: the object falls one unit per frame
        // under gravity until it lands and goes to sleep.
        let mut kernel = Kernel::default();
        // The kernel's `with_grid` inits players/partition; here we only need
        // the factory + scheduler, so construct the pieces by hand.
        let mut factory = ModuleFactory::default();
        let mut keys = rzh_util::NameKeyGenerator::new();
        factory.register_physics_module(&mut keys);

        // Build the object manually: the factory's `new_module` produces the
        // body; the spec's ctor wake of `None` is clamped to `now` (frame 0)
        // by `register_object`, so physics runs this frame.
        let spec = ObjectSpec {
            geometry: GeometryInfo::default(),
            position: Coord3 {
                x: 0.0,
                y: 0.0,
                z: 10.0,
            },
            orientation: 0.0,
            ..ObjectSpec::default()
        };
        let id = 1;
        let mut obj = test_object(id, spec.position, spec.orientation);
        // instance the physics module through the factory as create_object does
        let data_idx = factory
            .new_module_data_from_ini(
                &mut keys,
                "PhysicsBehaviorModule",
                ModuleType::Behavior,
                "physics",
            )
            .expect("physics module registered");
        let data = factory.module_data[data_idx];
        let spec2 = crate::modules::BehaviorSpec {
            name: "PhysicsBehaviorModule".to_string(),
            tag: "physics".to_string(),
            phase: SleepyUpdatePhase::Physics,
            disabled_types_to_process: crate::types::DisabledMask {
                bits: (1u32 << crate::types::DISABLED_COUNT) - 1,
            },
            ctor_wake: None,
        };
        let instance = factory.new_module(
            &mut keys,
            "PhysicsBehaviorModule",
            ModuleType::Behavior,
            &data,
            &spec2,
        );
        obj.behaviors.push(instance);
        kernel.objects.put(obj);

        // register the module in the scheduler at frame 0
        kernel.logic.next_obj_id = 2;
        let handle = ModuleHandle { obj: id, module: 0 };
        kernel
            .logic
            .scheduler
            .push_back_raw(&mut kernel.objects, handle);
        kernel.logic.frame = 0;

        // fall four frames: pos.z = 10 - 1 - 2 - 3 - 4 = 0 at frame 4,
        // then clamped at rest and sleeping.
        let mut host = TestHost;
        for _ in 0..4 {
            let now = kernel.logic.frame;
            while let Some(h) = kernel.logic.scheduler.peek() {
                if kernel.objects.module(h).next_call_frame() > now {
                    break;
                }
                let mut body = kernel.objects.take_body(h.obj, h.module);
                let outcome = body.update(
                    &ModuleCtx::new(now, &kernel.objects),
                    h.obj,
                    h.module,
                    &mut host,
                );
                kernel.objects.put_body(h.obj, h.module, body);
                if let Some(transform) = outcome.transform {
                    let o = kernel.objects.get_mut(h.obj);
                    o.position = crate::types::Coord3 {
                        x: transform.translation().x,
                        y: transform.translation().y,
                        z: transform.translation().z,
                    };
                    o.orientation = transform.z_rotation();
                    o.transform = transform;
                }
                kernel.logic.scheduler.cur_update_module = None;
                let mut sleep_len = outcome.sleep;
                if sleep_len < 1 {
                    sleep_len = UPDATE_SLEEP_NONE;
                }
                kernel
                    .objects
                    .module_mut(h)
                    .set_next_call_frame(now.wrapping_add(sleep_len as u32));
                kernel.logic.scheduler.rebalance(&mut kernel.objects, 0);
            }
            kernel.logic.frame = now.wrapping_add(1);
        }

        let o = kernel.objects.get(1);
        // after 4 frames of gravity the object has reached the ground (z = 0)
        // and is at rest.
        assert!((o.position.z - 0.0).abs() < 1e-4);
    }
}
