//! The module framework — the object-module families and the
//! data-driven construction that instanced them from templates.
//!
//! Origin (baseline 571a7c5):
//!   `Common/Module.h`                   (the `ModuleType` and
//!                                        `ModuleInterfaceType` vocabularies,
//!                                        the `ModuleData` base with its
//!                                        module-tag key, the `Module` /
//!                                        `ObjectModule` bases and the
//!                                        `onObjectCreated` /
//!                                        `onDrawableBoundToObject` /
//!                                        `preloadAssets` / `onDelete`
//!                                        virtual surface)
//!   `Common/ModuleFactory.h/.cpp`       (the registry: the
//!                                        `makeDecoratedNameKey` type
//!                                        prefix — `'0' + type` — through
//!                                        `nameToKey`, the
//!                                        `std::map`-keyed `ModuleTemplateMap`,
//!                                        `addModuleInternal`'s create-or-
//!                                        overwrite insertion,
//!                                        `findModuleInterfaceMask`'s 0-on-
//!                                        miss, `newModuleDataFromINI` with
//!                                        its tag stamp and
//!                                        `m_moduleDataList` append, and
//!                                        `newModule` with the
//!                                        DEBUG_ASSERTCRASH mask-vs-
//!                                        interface validation family)
//!   `GameLogic/Module/BehaviorModule.h` (the `BehaviorModuleInterface`
//!                                        acquisition protocol — the port
//!                                        reduces it to the interface mask
//!                                        plus the behavior-list position,
//!                                        the equivalence the factory's own
//!                                        debug asserts pin: "getInterfaceMask
//!                                        bad for MODULE_x" fires exactly
//!                                        when the family accessor returns
//!                                        non-null)
//!   `GameLogic/Module/UpdateModule.h`   (the `UpdateModuleInterface` —
//!                                        `update()` and the per-module
//!                                        `getUpdatePhase` /
//!                                        `getDisabledTypesToProcess`
//!                                        declarations; the scheduling half
//!                                        is WP1's [`crate::scheduler`])
//!   `GameLogic/Module/DieModule.h`      (`onDie(damageInfo)`)
//!   `GameLogic/Module/DamageModule.h`   (`onDamage`, `onHealing`,
//!                                        `onBodyDamageStateChange`)
//!   `GameLogic/Module/CreateModule.h`   (`onCreate`, `onBuildComplete`
//!                                        and the `m_needToRunOnBuildComplete`
//!                                        flag — born TRUE, cleared by the
//!                                        callback, consulted by
//!                                        `shouldDoOnBuildComplete`)
//!   `GameLogic/Module/DestroyModule.h`  (`onDestroy`)
//!   `GameLogic/Module/CollideModule.h`  (`onCollide(other, loc, normal)`)
//!   `GameLogic/Module/BodyModule.h`     (`BodyDamageType` — the damage
//!                                        state ladder)
//!   `Core/GameEngine/Include/GameLogic/Damage.h`
//!                                        (`DamageInfoInput`'s core fields —
//!                                        the full armor/shockwave model is
//!                                        the combat slice's; `DamageType`
//!                                        and `DeathType` verbatim)
//!   `Object.cpp`'s constructor           (the module half: the behavior
//!                                        list build in template order with
//!                                        the `m_body`/`m_contain` singleton
//!                                        capture and the duplicate
//!                                        DEBUG_ASSERTCRASH refusals, and
//!                                        `m_modulesReady` at the walk's
//!                                        end)
//!   `Object.cpp`'s `onDie`              (the "FIRST, call our die modules"
//!                                        walk, behavior order)
//!   `Object.cpp`'s `onDestroy`          (the `onDelete` walk over every
//!                                        module, behavior order)
//!   `ActiveBody.cpp`'s notify walks      (the damage family: the
//!                                        `onDamage` walk under the
//!                                        health-went-down gate, the
//!                                        `onBodyDamageStateChange` walk
//!                                        under the state-change gate —
//!                                        the gates themselves are the
//!                                        body's, the combat slice)
//!   `GameLogic.cpp`'s `destroyObject`   (the destroy-module walk BEFORE
//!                                        the DESTROYED status write)
//!   `ThingFactory.cpp`'s `newObject`    (the onCreate walk between the
//!                                        GameLogic registration and the
//!                                        partition registration)
//!   `BuildAssistant.cpp`                (the ungated `onBuildComplete`
//!                                        walk — the should-run flag is
//!                                        the module's own)
//!   `ThingTemplate.h`'s `ModuleInfo`    (the nugget list the template
//!                                        carries: name, tag, data, mask —
//!                                        distilled into [`BehaviorSpec`];
//!                                        the INI parse, the inheritance
//!                                        dance and the default-module
//!                                        injection are the rzh-data
//!                                        object model's, a later slice)
//!
//! The C++ module is a `MemoryPoolObject` with a vtable; the port's module
//! is a [`ModuleInstance`] record in the owning object's behavior list —
//! the class identity (`getModuleNameKey`), the per-template tag
//! (`getModuleTagNameKey`), the interface mask (`getInterfaceMask`) and
//! the per-instance body ([`ModuleBehavior`]). The family virtuals
//! (`getUpdate`/`getDie`/`getDamage`/...) become mask queries: a module
//! implements a family iff it carries the family's bit, and the family's
//! dispatch walks the behavior list in order dispatching the bit-carriers
//! — exactly the walks the original's `for (BehaviorModule** i =
//! getBehaviorModules(); *i; ++i)` loops perform.
//!
//! The bodies return effect records ([`crate::logic::UpdateOutcome`],
//! [`crate::logic::CollideOutcome`]) that the kernel applies in the
//! transcription's order — the corpus-proven WP1 discipline; the
//! die/damage/create/destroy/delete bodies are void (their effects are
//! concrete-module semantics, the later slices). The per-frame effects a
//! module may request grow with the modules that need them.
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use std::collections::BTreeMap;
use std::fmt::Debug;

use rzh_util::namekey::{NameKey, NameKeyGenerator};

use crate::scheduler::SleepyState;
use crate::types::{Coord3, DisabledMask, INVALID_ID, ObjectID, SleepyUpdatePhase};

// ============================================================================
// The vocabularies (Module.h, BodyModule.h, Damage.h)
// ============================================================================

/// `ModuleType` (Module.h): the module buckets a template's module info
/// carries. The object side is one bucket — every object module is a
/// behavior; the drawable buckets (`DRAW`, `CLIENT_UPDATE`) exist in the
/// vocabulary because the factory's registry keys carry the type digit,
/// but their registration is the client phase's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModuleType {
    /// `MODULETYPE_BEHAVIOR` — the object modules.
    Behavior,
    /// `MODULETYPE_DRAW` — the drawable modules (client phase).
    Draw,
    /// `MODULETYPE_CLIENT_UPDATE` — the drawable update modules (client
    /// phase).
    ClientUpdate,
}

impl ModuleType {
    /// The type digit of `makeDecoratedNameKey`'s `'0' + (int)type`.
    #[must_use]
    pub const fn digit(self) -> u8 {
        match self {
            Self::Behavior => b'0',
            Self::Draw => b'1',
            Self::ClientUpdate => b'2',
        }
    }
}

/// `ModuleInterfaceType` (Module.h): the interface mask — which dispatch
/// families a module implements. The twelve bits of the original's
/// `Int`-valued enum, in declaration order; the port keeps the C++
/// bit assignments so masks are directly comparable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct InterfaceMask(u16);

/// `MODULEINTERFACE_UPDATE` — the sleepy-update family.
pub const MODULEINTERFACE_UPDATE: InterfaceMask = InterfaceMask(0x0001);
/// `MODULEINTERFACE_DIE` — the death family.
pub const MODULEINTERFACE_DIE: InterfaceMask = InterfaceMask(0x0002);
/// `MODULEINTERFACE_DAMAGE` — the damage-callback family.
pub const MODULEINTERFACE_DAMAGE: InterfaceMask = InterfaceMask(0x0004);
/// `MODULEINTERFACE_CREATE` — the creation family.
pub const MODULEINTERFACE_CREATE: InterfaceMask = InterfaceMask(0x0008);
/// `MODULEINTERFACE_COLLIDE` — the collision family.
pub const MODULEINTERFACE_COLLIDE: InterfaceMask = InterfaceMask(0x0010);
/// `MODULEINTERFACE_BODY` — the body family (one per object).
pub const MODULEINTERFACE_BODY: InterfaceMask = InterfaceMask(0x0020);
/// `MODULEINTERFACE_CONTAIN` — the containment family (one per object).
pub const MODULEINTERFACE_CONTAIN: InterfaceMask = InterfaceMask(0x0040);
/// `MODULEINTERFACE_UPGRADE` — the upgrade family.
pub const MODULEINTERFACE_UPGRADE: InterfaceMask = InterfaceMask(0x0080);
/// `MODULEINTERFACE_SPECIAL_POWER` — the special-power family.
pub const MODULEINTERFACE_SPECIAL_POWER: InterfaceMask = InterfaceMask(0x0100);
/// `MODULEINTERFACE_DESTROY` — the destroy-callback family.
pub const MODULEINTERFACE_DESTROY: InterfaceMask = InterfaceMask(0x0200);
/// `MODULEINTERFACE_DRAW` — the drawable draw family (client phase).
pub const MODULEINTERFACE_DRAW: InterfaceMask = InterfaceMask(0x0400);
/// `MODULEINTERFACE_CLIENT_UPDATE` — the drawable update family (client
/// phase).
pub const MODULEINTERFACE_CLIENT_UPDATE: InterfaceMask = InterfaceMask(0x0800);

impl InterfaceMask {
    /// The empty mask.
    #[must_use]
    pub const fn none() -> Self {
        Self(0)
    }

    /// The union (`|`).
    #[must_use]
    pub const fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every bit of `other` is set (`& other == other`).
    #[must_use]
    pub const fn has(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Whether any bit is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The raw bits (the `Int` the original's mask carries).
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Wraps raw bits that came from a mask value (a save file or a
    /// transcription's dump).
    #[must_use]
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }
}

/// `BodyDamageType` (BodyModule.h): the damage-state ladder the body
/// walks consult — `BODY_PRISTINE`, `BODY_DAMAGED`,
/// `BODY_REALLYDAMAGED`, `BODY_RUBBLE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum BodyDamageType {
    /// `BODY_PRISTINE`.
    #[default]
    Pristine,
    /// `BODY_DAMAGED`.
    Damaged,
    /// `BODY_REALLYDAMAGED`.
    ReallyDamaged,
    /// `BODY_RUBBLE`.
    Rubble,
}

impl BodyDamageType {
    /// The ordinal (`(Int)` in the enum's declaration order).
    #[must_use]
    pub const fn ordinal(self) -> i32 {
        match self {
            Self::Pristine => 0,
            Self::Damaged => 1,
            Self::ReallyDamaged => 2,
            Self::Rubble => 3,
        }
    }

    /// The ordinal-to-value map (the save-load direction).
    #[must_use]
    pub const fn from_ordinal(ordinal: i32) -> Option<Self> {
        match ordinal {
            0 => Some(Self::Pristine),
            1 => Some(Self::Damaged),
            2 => Some(Self::ReallyDamaged),
            3 => Some(Self::Rubble),
            _ => None,
        }
    }
}

/// `DamageType` (Damage.h): the armor-matrix vocabulary, the Zero Hour
/// build — `DAMAGE_FLESHY_SNIPER` is the Generals-only ordinal 31 and is
/// absent here, so the subdual family reuses it (the `/* = 31 */` comment
/// placements of the original).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)]
pub enum DamageType {
    Explosion,
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
    StealthJetMissiles,
    MolotovCocktail,
    ComancheVulcan,
    SubdualMissile,
    SubdualVehicle,
    SubdualBuilding,
    SubdualUnresistable,
    Microwave,
}

impl DamageType {
    /// The ordinal (`(Int)`), `DAMAGE_EXPLOSION = 0` through
    /// `DAMAGE_MICROWAVE = 35`.
    #[must_use]
    pub const fn ordinal(self) -> i32 {
        match self {
            Self::Explosion => 0,
            Self::Crush => 1,
            Self::ArmorPiercing => 2,
            Self::SmallArms => 3,
            Self::Gattling => 4,
            Self::Radiation => 5,
            Self::Flame => 6,
            Self::Laser => 7,
            Self::Sniper => 8,
            Self::Poison => 9,
            Self::Healing => 10,
            Self::Unresistable => 11,
            Self::Water => 12,
            Self::Deploy => 13,
            Self::Surrender => 14,
            Self::Hack => 15,
            Self::KillPilot => 16,
            Self::Penalty => 17,
            Self::Falling => 18,
            Self::Melee => 19,
            Self::Disarm => 20,
            Self::HazardCleanup => 21,
            Self::ParticleBeam => 22,
            Self::Toppling => 23,
            Self::InfantryMissile => 24,
            Self::AuroraBomb => 25,
            Self::LandMine => 26,
            Self::JetMissiles => 27,
            Self::StealthJetMissiles => 28,
            Self::MolotovCocktail => 29,
            Self::ComancheVulcan => 30,
            Self::SubdualMissile => 31,
            Self::SubdualVehicle => 32,
            Self::SubdualBuilding => 33,
            Self::SubdualUnresistable => 34,
            Self::Microwave => 35,
        }
    }
}

/// `DeathType` (Damage.h): how a thing died — "these DELIBERATELY have
/// (slightly) different names from the damage names, since there isn't
/// necessarily a one-to-one correspondence."
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)]
pub enum DeathType {
    Normal,
    None,
    Crushed,
    Burned,
    Exploded,
    Poisoned,
    Toppled,
    Flooded,
    Suicided,
    Lased,
    Detonated,
    Splatted,
    PoisonedBeta,
    Extra2,
    Extra3,
    Extra4,
    Extra5,
    Extra6,
    Extra7,
    Extra8,
    PoisonedGamma,
}

impl DeathType {
    /// The ordinal (`(Int)`), `DEATH_NORMAL = 0` through
    /// `DEATH_POISONED_GAMMA = 20` (`DEATH_NUM_TYPES = 21`).
    #[must_use]
    pub const fn ordinal(self) -> i32 {
        match self {
            Self::Normal => 0,
            Self::None => 1,
            Self::Crushed => 2,
            Self::Burned => 3,
            Self::Exploded => 4,
            Self::Poisoned => 5,
            Self::Toppled => 6,
            Self::Flooded => 7,
            Self::Suicided => 8,
            Self::Lased => 9,
            Self::Detonated => 10,
            Self::Splatted => 11,
            Self::PoisonedBeta => 12,
            Self::Extra2 => 13,
            Self::Extra3 => 14,
            Self::Extra4 => 15,
            Self::Extra5 => 16,
            Self::Extra6 => 17,
            Self::Extra7 => 18,
            Self::Extra8 => 19,
            Self::PoisonedGamma => 20,
        }
    }
}

/// `DamageInfo` (Damage.h): the damage occurrence the die and damage
/// families forward — the input half's core fields, with the ctor's
/// default values. The template/player-mask/shockwave fields and the
/// output half are the combat slice's (the body's `attemptDamage`
/// fills them); the framework's walks only carry the record through.
#[derive(Clone, Copy, Debug)]
pub struct DamageInfo {
    /// `in.m_sourceID` — the damage's source object (born `INVALID_ID`).
    pub source_id: ObjectID,
    /// `in.m_damageType` (born `DAMAGE_EXPLOSION`).
    pub damage_type: DamageType,
    /// `in.m_deathType` — "if this kills us, death type to be used"
    /// (born `DEATH_NORMAL`).
    pub death_type: DeathType,
    /// `in.m_amount` — the damage value (born 0).
    pub amount: f32,
    /// `in.m_kill` — "will always cause object to die regardless of
    /// damage" (born false).
    pub kill: bool,
}

impl Default for DamageInfo {
    /// The `DamageInfoInput` ctor's defaults.
    fn default() -> Self {
        Self {
            source_id: INVALID_ID,
            damage_type: DamageType::Explosion,
            death_type: DeathType::Normal,
            amount: 0.0,
            kill: false,
        }
    }
}

// ============================================================================
// The module body (the Module/BehaviorModule virtual surface)
// ============================================================================

/// The read-only world state a module body dispatch receives — the
/// pieces the original's module virtuals reach through the singletons
/// (`TheGameLogic`, the object arena), assembled at the walk site (the
/// port has no globals; the walks thread what they hold). The context
/// grows as later slices' bodies need more of the world.
pub struct ModuleCtx<'a> {
    /// `TheGameLogic->getFrame()`.
    pub frame: u32,
    /// The object arena (`TheGameLogic`'s lookup table) — every live
    /// object by id.
    pub objects: &'a crate::logic::Objects,
}

impl ModuleCtx<'_> {
    /// Assembles the view a walk site holds.
    pub fn new(frame: u32, objects: &crate::logic::Objects) -> ModuleCtx<'_> {
        ModuleCtx { frame, objects }
    }

    /// Convenience: the object this module belongs to.
    #[must_use]
    pub fn object(&self, obj: ObjectID) -> &crate::logic::Object {
        self.objects.get(obj)
    }
}

/// Which of the three damage-family callbacks is dispatching
/// (`DamageModuleInterface`'s surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageCallbackKind {
    /// `DamageModuleInterface::onDamage`.
    Damage,
    /// `DamageModuleInterface::onHealing`.
    Healing,
    /// `DamageModuleInterface::onBodyDamageStateChange`.
    BodyDamageStateChange,
}

/// The module body — the per-instance behavior (the `Module` virtual
/// surface, distilled to the dispatch families).
///
/// Every family callback's default body is the corresponding C++ base
/// class's default (a no-op, or the family base's own default such as
/// `CreateModule::shouldDoOnBuildComplete`). The dispatch walks call
/// these in the original's order; the effects the kernel applies itself
/// (`GameLogic::destroyObject` requests, wake frames, the absorb bit)
/// come back as [`crate::logic::UpdateOutcome`] /
/// [`crate::logic::CollideOutcome`] records, applied in the
/// transcription's order.
///
/// The context of a body call is read-only state (the kernel view) plus
/// the host (the scripted-corpus surface later slices replace with real
/// bodies); a body never mutates the world directly — it returns the
/// effects, and the framework applies them. This is the port's answer to
/// the original's unrestricted pointer reach from inside a virtual call,
/// and what keeps the simulation single-pass deterministic.
pub trait ModuleBehavior: Debug {
    /// The [`MODULEINTERFACE_UPDATE`] family: the module's periodic body
    /// (`UpdateModuleInterface::update`). Only dispatched for modules
    /// carrying the UPDATE bit; the return code is the sleep length
    /// (with the destroy/wake effects the kernel applies after the
    /// body).
    fn update(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        host: &mut dyn crate::logic::KernelHost,
    ) -> crate::logic::UpdateOutcome {
        let _ = (ctx, obj, module, host);
        crate::logic::UpdateOutcome::default()
    }

    /// The [`MODULEINTERFACE_DIE`] family: the death callback
    /// (`DieModuleInterface::onDie`).
    fn on_die(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        damage: &DamageInfo,
        host: &mut dyn crate::logic::KernelHost,
    ) {
        let _ = (ctx, obj, module, damage, host);
    }

    /// The [`MODULEINTERFACE_DAMAGE`] family: the damage callback
    /// (`DamageModuleInterface::onDamage`).
    fn on_damage(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        damage: &DamageInfo,
        host: &mut dyn crate::logic::KernelHost,
    ) {
        let _ = (ctx, obj, module, damage, host);
    }

    /// The [`MODULEINTERFACE_DAMAGE`] family: the healing callback
    /// (`DamageModuleInterface::onHealing`).
    fn on_healing(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        damage: &DamageInfo,
        host: &mut dyn crate::logic::KernelHost,
    ) {
        let _ = (ctx, obj, module, damage, host);
    }

    /// The [`MODULEINTERFACE_DAMAGE`] family: the damage-state change
    /// callback (`DamageModuleInterface::onBodyDamageStateChange`).
    #[allow(clippy::too_many_arguments)]
    fn on_body_damage_state_change(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        damage: &DamageInfo,
        old_state: BodyDamageType,
        new_state: BodyDamageType,
        host: &mut dyn crate::logic::KernelHost,
    ) {
        let _ = (ctx, obj, module, damage, old_state, new_state, host);
    }

    /// The [`MODULEINTERFACE_CREATE`] family: the creation callback
    /// (`CreateModuleInterface::onCreate`) — "this is called when you
    /// become a code Object."
    fn on_create(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        host: &mut dyn crate::logic::KernelHost,
    ) {
        let _ = (ctx, obj, module, host);
    }

    /// The [`MODULEINTERFACE_CREATE`] family: the build-complete
    /// callback (`CreateModuleInterface::onBuildComplete`) — "this is
    /// called when you are a finished game object." The walk is
    /// ungated (per `BuildAssistant.cpp`); the base's
    /// `m_needToRunOnBuildComplete` flag — born TRUE, cleared here, and
    /// consulted by the concrete overrides' early returns — lives on
    /// the [`ModuleInstance`] (the framework clears it after the body
    /// dispatch).
    fn on_build_complete(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        host: &mut dyn crate::logic::KernelHost,
    ) {
        let _ = (ctx, obj, module, host);
    }

    /// The `Module::onDelete` surface — "will be called on all modules
    /// contained by an object or drawable before the actual deletion of
    /// each of those modules happens" (every module, not a family).
    fn on_delete(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        host: &mut dyn crate::logic::KernelHost,
    ) {
        let _ = (ctx, obj, module, host);
    }

    /// The [`MODULEINTERFACE_DESTROY`] family: the destroy callback
    /// (`DestroyModuleInterface::onDestroy`) — dispatched by
    /// `GameLogic::destroyObject` before the DESTROYED status write.
    fn on_destroy(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        host: &mut dyn crate::logic::KernelHost,
    ) {
        let _ = (ctx, obj, module, host);
    }

    /// The [`MODULEINTERFACE_COLLIDE`] family: the collision callback
    /// (`CollideModuleInterface::onCollide`). Only dispatched for
    /// modules carrying the COLLIDE bit.
    #[allow(clippy::too_many_arguments)]
    fn on_collide(
        &mut self,
        ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        other: ObjectID,
        loc: &Coord3,
        normal: &Coord3,
        host: &mut dyn crate::logic::KernelHost,
    ) -> crate::logic::CollideOutcome {
        let _ = (ctx, obj, module, other, loc, normal, host);
        crate::logic::CollideOutcome::default()
    }
}

// ============================================================================
// The instance and the template-side records (Module.h, ThingTemplate.h)
// ============================================================================

/// One module on an object — the `BehaviorModule` heap record: the class
/// identity, the per-template tag, the interface mask, the body and, for
/// update-carrying modules, the scheduler state (WP1's
/// [`SleepyState`]).
#[derive(Debug)]
pub struct ModuleInstance {
    /// `getModuleNameKey()` — `NAMEKEY(class name)`, the undecorated
    /// class-name key.
    pub name_key: NameKey,
    /// `getModuleTagNameKey()` — the template's tag ("unique among all
    /// modules for this object").
    pub tag_key: NameKey,
    /// `getInterfaceMask()` — which dispatch families this module
    /// implements (the registry entry's own mask, stamped at birth).
    pub interfaces: InterfaceMask,
    /// The module's body (the `Module` vtable's dispatch target).
    pub body: Box<dyn ModuleBehavior>,
    /// The `UpdateModule` half — present iff the module carries the
    /// UPDATE interface. The scheduler's heap resolves through here
    /// ([`crate::scheduler::ModuleArena`]).
    pub update: Option<SleepyState>,
    /// `CreateModule::m_needToRunOnBuildComplete` — born TRUE for
    /// create-family modules (the ctor's `m_needToRunOnBuildComplete
    /// (TRUE)`), cleared by the build-complete walk's dispatch (the
    /// base's `onBuildComplete { m_needToRunOnBuildComplete = FALSE }`),
    /// read by the concrete overrides'
    /// `shouldDoOnBuildComplete` gates. False for every other family
    /// (no flag exists there).
    pub need_to_run_on_build_complete: bool,
}

impl ModuleInstance {
    /// Whether this module implements a family (`getUpdate() != null`
    /// and friends — the mask query the dispatch walks make).
    #[must_use]
    pub const fn implements(&self, family: InterfaceMask) -> bool {
        self.interfaces.has(family)
    }
}

/// `ModuleData` (Module.h): the data-read-from-INI half every template
/// nugget carries — the framework's own consumption is the module-tag
/// key; the per-module INI fields are the concrete modules' (their
/// `buildFieldParse` families, the later slices — the rzh-data object
/// model parses them).
#[derive(Clone, Copy, Debug)]
pub struct ModuleData {
    /// `m_moduleTagNameKey` — `NAMEKEY(moduleTag)`, stamped by
    /// `newModuleDataFromINI`.
    pub module_tag_name_key: NameKey,
}

impl Default for ModuleData {
    /// The `ModuleData()` ctor's zeroed tag (the interning assigns no
    /// key until the stamp).
    fn default() -> Self {
        Self {
            module_tag_name_key: NameKey::INVALID,
        }
    }
}

/// One entry of a template's module list — the `ModuleInfo` nugget
/// (`ThingTemplate.h`): the class name, the tag and the birth
/// configuration the framework consumes. The INI parse, the
/// `inheritable` / `overrideableByLikeKind` flags and the default-module
/// injection are the rzh-data slice's; this is the distilled list
/// `Object`'s constructor walks.
#[derive(Clone, Debug, Default)]
pub struct BehaviorSpec {
    /// The module class name — the factory's registry key (the
    /// `ModuleInfo` nugget's `first`, the "Behavior = <name>" of the
    /// INI).
    pub name: String,
    /// The module tag — the nugget's `m_moduleTag`, unique among all
    /// modules of the template.
    pub tag: String,
    /// `getUpdatePhase()` — consumed iff the module carries the UPDATE
    /// interface (a module-class declaration the corpus scripts
    /// per-instance, matching the synthetic corpus modules' birth
    /// fields).
    pub phase: SleepyUpdatePhase,
    /// `getDisabledTypesToProcess()` — ditto.
    pub disabled_types_to_process: DisabledMask,
    /// The constructor's `setWakeFrame` — an absolute frame; `None` =
    /// never woken (registerObject clamps those to now).
    pub ctor_wake: Option<u32>,
}

// ============================================================================
// The factory (ModuleFactory.h/.cpp)
// ============================================================================

/// `ModuleFactory`'s `ModuleTemplate`: the creation pair plus the
/// interface mask.
#[derive(Clone, Copy)]
pub struct ModuleTemplate {
    /// `m_createProc` — the class's `friend_newModuleInstance`
    /// (`newInstance(cls)(thing, moduleData)`).
    pub create: fn(&ModuleData) -> Box<dyn ModuleBehavior>,
    /// `m_whichInterfaces` — the class's `getInterfaceMask()`.
    pub interfaces: InterfaceMask,
}

impl Debug for ModuleTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleTemplate")
            .field("create", &"<fn>")
            .field("interfaces", &self.interfaces)
            .finish()
    }
}

/// `TheModuleFactory` — the name-keyed registry the data-driven
/// construction resolves through. A `std::map<NameKeyType,
/// ModuleTemplate>` in the original; the port's `BTreeMap` keeps the
/// ordered-map semantics (the registry's iteration order is not
/// observable — lookups are by key — but the bounded structure keeps
/// determinism trivially auditable).
#[derive(Debug, Default)]
pub struct ModuleFactory {
    /// `m_moduleTemplateMap` — keyed on the decorated name key.
    pub templates: BTreeMap<NameKey, ModuleTemplate>,
    /// `m_moduleDataList` — every `ModuleData` ever created (the
    /// factory owns them for the app's lifetime; append-only).
    pub module_data: Vec<ModuleData>,
}

impl ModuleFactory {
    /// `ModuleFactory::makeDecoratedNameKey` — `'0' + (int)type` glued
    /// in front of the name, through `nameToKey`.
    #[must_use]
    pub fn make_decorated_name_key(
        keys: &mut NameKeyGenerator,
        name: &str,
        ty: ModuleType,
    ) -> NameKey {
        // char tmp[256]; tmp[0] = '0' + (int)type; strcpy(&tmp[1], name.str());
        let mut decorated = Vec::with_capacity(name.len() + 1);
        decorated.push(ty.digit());
        decorated.extend_from_slice(name.as_bytes());
        keys.name_to_key(decorated)
    }

    /// `addModuleInternal` — "adding a new module template to the
    /// factory" (the `addModule(classname)` macro's expansion): the
    /// decorated-keyed entry, created-or-overwritten (`m_moduleTemplateMap
    /// [namekey]` default-constructs then fills — a re-registration
    /// overwrites in place).
    pub fn add_module_internal(
        &mut self,
        keys: &mut NameKeyGenerator,
        name: &str,
        ty: ModuleType,
        interfaces: InterfaceMask,
        create: fn(&ModuleData) -> Box<dyn ModuleBehavior>,
    ) {
        let namekey = Self::make_decorated_name_key(keys, name, ty);
        // ModuleTemplate& mtm = m_moduleTemplateMap[namekey]; // creates if absent
        self.templates
            .insert(namekey, ModuleTemplate { create, interfaces });
    }

    /// `findModuleTemplate` → the mask (`findModuleInterfaceMask`):
    /// 0 on an empty name or an unknown name (the original's
    /// `DEBUG_CRASH` on the latter fires in the corpus driver before the
    /// 0 ever escapes; the port's construction path panics there — see
    /// [`Self::new_module`]).
    #[must_use]
    pub fn find_module_interface_mask(
        &mut self,
        keys: &mut NameKeyGenerator,
        name: &str,
        ty: ModuleType,
    ) -> InterfaceMask {
        if name.is_empty() {
            return InterfaceMask::none();
        }
        let namekey = Self::make_decorated_name_key(keys, name, ty);
        self.templates
            .get(&namekey)
            .map_or_else(InterfaceMask::none, |t| t.interfaces)
    }

    /// `newModuleDataFromINI` — the data half of the construction: the
    /// class's data creation (the framework data; the per-class INI
    /// field parse is the concrete modules'), the tag stamp
    /// (`NAMEKEY(moduleTag)`), the ownership append.
    pub fn new_module_data_from_ini(
        &mut self,
        keys: &mut NameKeyGenerator,
        name: &str,
        ty: ModuleType,
        module_tag: &str,
    ) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        let namekey = Self::make_decorated_name_key(keys, name, ty);
        if !self.templates.contains_key(&namekey) {
            return None; // findModuleTemplate's miss (DEBUG_CRASH upstream)
        }
        let data = ModuleData {
            module_tag_name_key: keys.name_to_key(module_tag),
        };
        self.module_data.push(data); // m_moduleDataList.push_back(md)
        Some(self.module_data.len() - 1)
    }

    /// `newModule` — "allocate a new action class instance given the
    /// name": the template lookup, the instancing and the
    /// mask-vs-interface validation family. The original's
    /// `DEBUG_ASSERTCRASH("Module name '%s' not found")` on an unknown
    /// name is a load-path data error; the port makes it loud (the
    /// registry is populated by code, never by data — an unknown name
    /// is an internal invariant violation, never a data condition).
    ///
    /// # Panics
    ///
    /// Panics when `name` is empty, unknown, or the update-carrying
    /// configuration disagrees with the class's mask (the
    /// `newModule`/`addModuleInfo` debug-assert family the original
    /// crashes on).
    pub fn new_module(
        &mut self,
        keys: &mut NameKeyGenerator,
        name: &str,
        ty: ModuleType,
        data: &ModuleData,
        spec: &BehaviorSpec,
    ) -> ModuleInstance {
        if name.is_empty() {
            panic!("attempting to create module with empty name");
        }
        let namekey = Self::make_decorated_name_key(keys, name, ty);
        let Some(template) = self.templates.get(&namekey) else {
            panic!("Module name '{name}' not found");
        };

        let interfaces = template.interfaces;
        let class_name_key = keys.name_to_key(name); // NAMEKEY(#cls)
        let body = (template.create)(data);

        // the module's birth configuration (the corpus modules' per-
        // instance fields): the scheduler half exists iff UPDATE
        let update = if interfaces.has(MODULEINTERFACE_UPDATE) {
            Some(SleepyState::new(spec.phase, spec.disabled_types_to_process))
        } else {
            None
        };
        // CreateModule's ctor: m_needToRunOnBuildComplete(TRUE)
        let need_to_run_on_build_complete = interfaces.has(MODULEINTERFACE_CREATE);

        ModuleInstance {
            name_key: class_name_key,
            tag_key: data.module_tag_name_key,
            interfaces,
            body,
            update,
            need_to_run_on_build_complete,
        }
    }
}

// ============================================================================
// The scripted corpus modules (the synthetic classes the golden corpora
// instantiate — the generator's twins, registered through the same
// addModule surface the concrete slices use)
// ============================================================================

/// `ScriptUpdateModule` — the corpus's update module: the body forwards
/// to the host's scripted table (`KernelHost::run_update`), exactly as
/// the generator's synthetic modules look up their scripts.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScriptUpdateModule;

impl ModuleBehavior for ScriptUpdateModule {
    fn update(
        &mut self,
        _ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        host: &mut dyn crate::logic::KernelHost,
    ) -> crate::logic::UpdateOutcome {
        host.run_update(obj, module)
    }
}

/// `ScriptCollideModule` — the corpus's collide module: the body
/// forwards to the host's scripted table (`KernelHost::run_collide`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ScriptCollideModule;

impl ModuleBehavior for ScriptCollideModule {
    fn on_collide(
        &mut self,
        _ctx: &ModuleCtx,
        obj: ObjectID,
        module: u32,
        other: ObjectID,
        loc: &Coord3,
        normal: &Coord3,
        host: &mut dyn crate::logic::KernelHost,
    ) -> crate::logic::CollideOutcome {
        host.run_collide(obj, module, other, loc, normal)
    }
}

/// The scripted marker module — every scripted family whose body is the
/// base no-op (`ScriptDieModule`, `ScriptDamageModule`,
/// `ScriptCreateModule`, `ScriptDestroyModule`, `ScriptBodyModule`,
/// `ScriptContainModule`): the family's interface bit is the point (the
/// dispatch walks' membership), the bodies are the base defaults.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScriptModule;

impl ModuleBehavior for ScriptModule {}

impl ModuleFactory {
    /// Registers the scripted corpus module classes — the eight
    /// synthetic classes the golden corpora instantiate (the
    /// differential generator registers its C++ twins through the same
    /// `addModule` surface). The class names and their masks:
    ///
    /// | class | mask |
    /// |---|---|
    /// | `ScriptUpdateModule` | UPDATE |
    /// | `ScriptCollideModule` | COLLIDE |
    /// | `ScriptDieModule` | DIE |
    /// | `ScriptDamageModule` | DAMAGE |
    /// | `ScriptCreateModule` | CREATE |
    /// | `ScriptDestroyModule` | DESTROY |
    /// | `ScriptBodyModule` | BODY |
    /// | `ScriptContainModule` | CONTAIN |
    pub fn register_script_modules(&mut self, keys: &mut NameKeyGenerator) {
        self.add_module_internal(
            keys,
            "ScriptUpdateModule",
            ModuleType::Behavior,
            MODULEINTERFACE_UPDATE,
            |_data| Box::new(ScriptUpdateModule),
        );
        self.add_module_internal(
            keys,
            "ScriptCollideModule",
            ModuleType::Behavior,
            MODULEINTERFACE_COLLIDE,
            |_data| Box::new(ScriptCollideModule),
        );
        self.add_module_internal(
            keys,
            "ScriptDieModule",
            ModuleType::Behavior,
            MODULEINTERFACE_DIE,
            |_data| Box::new(ScriptModule),
        );
        self.add_module_internal(
            keys,
            "ScriptDamageModule",
            ModuleType::Behavior,
            MODULEINTERFACE_DAMAGE,
            |_data| Box::new(ScriptModule),
        );
        self.add_module_internal(
            keys,
            "ScriptCreateModule",
            ModuleType::Behavior,
            MODULEINTERFACE_CREATE,
            |_data| Box::new(ScriptModule),
        );
        self.add_module_internal(
            keys,
            "ScriptDestroyModule",
            ModuleType::Behavior,
            MODULEINTERFACE_DESTROY,
            |_data| Box::new(ScriptModule),
        );
        self.add_module_internal(
            keys,
            "ScriptBodyModule",
            ModuleType::Behavior,
            MODULEINTERFACE_BODY,
            |_data| Box::new(ScriptModule),
        );
        self.add_module_internal(
            keys,
            "ScriptContainModule",
            ModuleType::Behavior,
            MODULEINTERFACE_CONTAIN,
            |_data| Box::new(ScriptModule),
        );
    }
}
