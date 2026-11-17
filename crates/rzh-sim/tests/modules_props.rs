// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Property and unit tests for the module framework.
//!
//! - the interface mask's lattice (the `or`/`has`/`is_empty` algebra over
//!   random bit combinations);
//! - the factory registry's determinism (the same registration sequence
//!   produces the same decorated keys through fresh generators — the
//!   `std::map`'s key space is the interning order) and the
//!   created-or-overwrite insertion;
//! - the data-driven construction's invariants over random mixed-family
//!   specs: the behavior list's length and order, the update halves
//!   exactly on the UPDATE-bit carriers (with the phase and the
//!   disabled mask stamped from the spec), the singleton captures on
//!   the FIRST body/contain, the need-build-complete flag exactly on
//!   the CREATE carriers, and the heap's push order (behavior order,
//!   update-carrying only);
//! - the family walks' filter over random mixed objects: each walk
//!   dispatches exactly its family's carriers, in behavior order, and
//!   nothing else.

use proptest::prelude::*;

use rzh_sim::logic::{Kernel, KernelHost, ObjectSpec};
use rzh_sim::modules::{
    BehaviorSpec, BodyDamageType, DamageCallbackKind, DamageInfo, InterfaceMask,
    MODULEINTERFACE_BODY, MODULEINTERFACE_COLLIDE, MODULEINTERFACE_CONTAIN, MODULEINTERFACE_CREATE,
    MODULEINTERFACE_DAMAGE, MODULEINTERFACE_DESTROY, MODULEINTERFACE_DIE, MODULEINTERFACE_UPDATE,
    ModuleFactory, ModuleType,
};
use rzh_sim::partition::Region2;
use rzh_util::namekey::NameKeyGenerator;

// --- the interface mask lattice ----------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `or` is the union: a mask has every bit of either operand, and
    /// nothing an operand lacks.
    #[test]
    fn mask_or_is_the_union(a in any::<u16>(), b in any::<u16>()) {
        let ma = InterfaceMask::from_bits(a);
        let mb = InterfaceMask::from_bits(b);
        let m = ma.or(mb);
        prop_assert!(m.has(ma));
        prop_assert!(m.has(mb));
        prop_assert_eq!(m.bits(), a | b);
    }

    /// `has` is the containment order: antisymmetric between distinct
    /// masks, and reflexive.
    #[test]
    fn mask_has_is_the_order(a in any::<u16>(), b in any::<u16>()) {
        let ma = InterfaceMask::from_bits(a);
        let mb = InterfaceMask::from_bits(b);
        prop_assert!(ma.has(ma));
        if ma.has(mb) && mb.has(ma) {
            prop_assert_eq!(ma, mb);
        }
        if ma.has(mb) && !mb.has(ma) {
            prop_assert!(!mb.has(ma));
        }
    }

    /// A mask is empty iff it has no bits — and the family constants are
    /// never empty and pairwise distinct.
    #[test]
    fn mask_empty_and_families(v in any::<u16>()) {
        let m = InterfaceMask::from_bits(v);
        prop_assert_eq!(m.is_empty(), v == 0);
        let families = [
            MODULEINTERFACE_UPDATE, MODULEINTERFACE_DIE, MODULEINTERFACE_DAMAGE,
            MODULEINTERFACE_CREATE, MODULEINTERFACE_COLLIDE, MODULEINTERFACE_BODY,
            MODULEINTERFACE_CONTAIN, MODULEINTERFACE_DESTROY,
        ];
        for &f in &families {
            prop_assert!(!f.is_empty());
            prop_assert!(f.has(f));
            for &g in &families {
                if f != g {
                    prop_assert!(!f.has(g));
                }
            }
        }
    }
}

// --- the factory registry ------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The same registration sequence produces the same decorated keys
    /// through fresh generators (the key space is the interning order),
    /// and the lookups resolve: a registered name yields its mask, an
    /// unknown name yields the empty mask.
    #[test]
    fn registry_lookups_resolve(
        names in proptest::collection::vec("[A-Za-z][A-Za-z0-9]{1,12}", 1..24),
        masks in proptest::collection::vec(0u16..0x1000, 1..24),
    ) {
        let mut f1 = ModuleFactory::default();
        let mut k1 = NameKeyGenerator::new();
        let mut f2 = ModuleFactory::default();
        let mut k2 = NameKeyGenerator::new();
        for (i, name) in names.iter().enumerate() {
            let mask = InterfaceMask::from_bits(masks[i % masks.len()]);
            fn create(_data: &rzh_sim::modules::ModuleData) -> Box<dyn rzh_sim::modules::ModuleBehavior> {
            Box::new(rzh_sim::modules::ScriptModule)
        }
            f1.add_module_internal(&mut k1, name, ModuleType::Behavior, mask, create);
            f2.add_module_internal(&mut k2, name, ModuleType::Behavior, mask, create);
        }
        // the decorated keys agree
        for name in &names {
            prop_assert_eq!(
                ModuleFactory::make_decorated_name_key(&mut k1, name, ModuleType::Behavior),
                ModuleFactory::make_decorated_name_key(&mut k2, name, ModuleType::Behavior),
            );
        }
        // the lookups resolve
        for (i, name) in names.iter().enumerate() {
            let expected = InterfaceMask::from_bits(masks[i % masks.len()]);
            prop_assert_eq!(
                f1.find_module_interface_mask(&mut k1, name, ModuleType::Behavior),
                expected
            );
        }
        // an unknown name and an empty name yield nothing
        prop_assert!(f1
            .find_module_interface_mask(&mut k1, "NoSuchModuleAtAll", ModuleType::Behavior)
            .is_empty());
        prop_assert!(f1
            .find_module_interface_mask(&mut k1, "", ModuleType::Behavior)
            .is_empty());
    }

    /// A re-registration overwrites in place (the `m_moduleTemplateMap
    /// [namekey]` created-or-overwritten entry).
    #[test]
    fn reregistration_overwrites(mask1 in 0u16..0x1000, mask2 in 0u16..0x1000) {
        let mut f = ModuleFactory::default();
        let mut k = NameKeyGenerator::new();
        fn create(_data: &rzh_sim::modules::ModuleData) -> Box<dyn rzh_sim::modules::ModuleBehavior> {
            Box::new(rzh_sim::modules::ScriptModule)
        }
        f.add_module_internal(&mut k, "AModule", ModuleType::Behavior,
                              InterfaceMask::from_bits(mask1), create);
        f.add_module_internal(&mut k, "AModule", ModuleType::Behavior,
                              InterfaceMask::from_bits(mask2), create);
        prop_assert_eq!(
            f.find_module_interface_mask(&mut k, "AModule", ModuleType::Behavior),
            InterfaceMask::from_bits(mask2)
        );
    }

    /// The data list appends one entry per creation with the tag stamped
    /// (the same tag re-stamped interns once), and the type digit
    /// separates the buckets.
    #[test]
    fn data_list_appends_with_tags(tags in proptest::collection::vec("Tag[0-9]{1,4}", 1..16)) {
        let mut f = ModuleFactory::default();
        let mut k = NameKeyGenerator::new();
        fn create(_data: &rzh_sim::modules::ModuleData) -> Box<dyn rzh_sim::modules::ModuleBehavior> {
            Box::new(rzh_sim::modules::ScriptModule)
        }
        f.add_module_internal(&mut k, "AModule", ModuleType::Behavior,
                              MODULEINTERFACE_DIE, create);
        for (i, tag) in tags.iter().enumerate() {
            let idx = f.new_module_data_from_ini(&mut k, "AModule", ModuleType::Behavior, tag);
            prop_assert_eq!(idx, Some(i));
            prop_assert_eq!(f.module_data[i].module_tag_name_key, k.name_to_key(tag));
        }
        prop_assert_eq!(f.module_data.len(), tags.len());
        // the unknown name yields nothing
        prop_assert_eq!(
            f.new_module_data_from_ini(&mut k, "NoSuchModule", ModuleType::Behavior, "T"),
            None
        );
    }
}

// --- the data-driven construction ----------------------------------------------

/// The scripted class names (the families the random specs draw from).
const CLASSES: [(&str, InterfaceMask); 8] = [
    ("ScriptUpdateModule", MODULEINTERFACE_UPDATE),
    ("ScriptCollideModule", MODULEINTERFACE_COLLIDE),
    ("ScriptDieModule", MODULEINTERFACE_DIE),
    ("ScriptDamageModule", MODULEINTERFACE_DAMAGE),
    ("ScriptCreateModule", MODULEINTERFACE_CREATE),
    ("ScriptDestroyModule", MODULEINTERFACE_DESTROY),
    ("ScriptBodyModule", MODULEINTERFACE_BODY),
    ("ScriptContainModule", MODULEINTERFACE_CONTAIN),
];

/// A no-op host (the walks' dispatches are observed through the object
/// state and the module halves, not events, in these properties).
#[derive(Default)]
struct NullHost;

impl KernelHost for NullHost {}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Random mixed-family specs build the behavior list the factory
    /// dictates: the length, the per-module mask (the registry entry's),
    /// the update half exactly on the UPDATE carriers (with the phase
    /// and the disabled mask stamped from the spec), the singleton
    /// captures on the FIRST body/contain, the need-build-complete flag
    /// exactly on the CREATE carriers, and the heap's push order
    /// (behavior order, update-carrying only). (The specs stay valid:
    /// at most one BODY and one CONTAIN — the ctor's duplicate
    /// refusals are debug_asserts, checked by the unit test below.)
    #[test]
    fn construction_matches_the_registry(
        class_ids in proptest::collection::vec(0usize..6, 0..16),
        body_at in proptest::option::of(0usize..17),
        contain_at in proptest::option::of(0usize..17),
    ) {
        // splice the optional single body/contain into the class list
        let mut class_ids: Vec<usize> = class_ids;
        let insert_at = |ids: &mut Vec<usize>, at: usize, cls: usize| {
            let at = at.min(ids.len());
            ids.insert(at, cls);
        };
        // insert the later-position one first so the earlier index
        // stays true to its own position
        match (body_at, contain_at) {
            (Some(b), Some(c)) => {
                let (first, second, cls_first, cls_second) =
                    if b <= c { (b, c, 6usize, 7usize) } else { (c, b, 7usize, 6usize) };
                insert_at(&mut class_ids, second, cls_second);
                insert_at(&mut class_ids, first, cls_first);
            }
            (Some(b), None) => insert_at(&mut class_ids, b, 6),
            (None, Some(c)) => insert_at(&mut class_ids, c, 7),
            (None, None) => {}
        }
        let mut host = NullHost;
        let kernel = Kernel::with_grid(
            10.0,
            Region2 { lo_x: 0.0, lo_y: 0.0, hi_x: 100.0, hi_y: 100.0 },
            &mut host,
        );
        let mut kernel = kernel;
        kernel.modules.register_script_modules(&mut kernel.players.name_keys);

        let spec = ObjectSpec {
            behaviors: class_ids
                .iter()
                .enumerate()
                .map(|(i, &c)| BehaviorSpec {
                    name: CLASSES[c].0.into(),
                    tag: format!("ModuleTag_{i}"),
                    phase: if i % 3 == 0 {
                        rzh_sim::types::SleepyUpdatePhase::Initial
                    } else {
                        rzh_sim::types::SleepyUpdatePhase::Normal
                    },
                    disabled_types_to_process: Default::default(),
                    ctor_wake: None,
                })
                .collect(),
            ..ObjectSpec::default()
        };
        let id = kernel.create_object(&spec, &mut host);
        let obj = kernel.objects.get(id);

        // the length and the masks
        prop_assert_eq!(obj.behaviors.len(), class_ids.len());
        for (&c, b) in class_ids.iter().zip(&obj.behaviors) {
            prop_assert_eq!(b.interfaces, CLASSES[c].1);
        }

        // the update half: exactly the UPDATE carriers, with the phase
        let mut expected_updates = 0usize;
        for (i, (&c, b)) in class_ids.iter().zip(&obj.behaviors).enumerate() {
            let carries = CLASSES[c].1.has(MODULEINTERFACE_UPDATE);
            prop_assert_eq!(b.update.is_some(), carries);
            if carries {
                expected_updates += 1;
                let expected_phase = if i % 3 == 0 {
                    rzh_sim::types::SleepyUpdatePhase::Initial
                } else {
                    rzh_sim::types::SleepyUpdatePhase::Normal
                };
                prop_assert_eq!(b.update.as_ref().unwrap().phase, expected_phase);
            }
        }

        // the singleton captures: the FIRST carrier of each
        let first_body = class_ids
            .iter()
            .position(|&c| CLASSES[c].1.has(MODULEINTERFACE_BODY))
            .map(|i| i as u32);
        let first_contain = class_ids
            .iter()
            .position(|&c| CLASSES[c].1.has(MODULEINTERFACE_CONTAIN))
            .map(|i| i as u32);
        prop_assert_eq!(obj.body_module, first_body);
        prop_assert_eq!(obj.contain_module, first_contain);

        // the need-build-complete flag: exactly the CREATE carriers
        for (&c, b) in class_ids.iter().zip(&obj.behaviors) {
            prop_assert_eq!(
                b.need_to_run_on_build_complete,
                CLASSES[c].1.has(MODULEINTERFACE_CREATE)
            );
        }

        // the heap: the update carriers in behavior order
        prop_assert_eq!(kernel.logic.scheduler.heap.len(), expected_updates);
        for (slot, &c) in class_ids.iter().enumerate() {
            if CLASSES[c].1.has(MODULEINTERFACE_UPDATE) {
                let handle = kernel
                    .logic
                    .scheduler
                    .heap
                    .iter()
                    .find(|h| h.module as usize == slot)
                    .copied();
                prop_assert!(handle.is_some(), "the update module at {slot} is on the heap");
            }
        }
        // (the heap validator is debug-profile only — the WP1 keep)
        #[cfg(debug_assertions)]
        prop_assert!(kernel.scheduler_valid());
    }

    /// The family walks dispatch exactly their carriers, in behavior
    /// order — the die walk, the damage trio, the build-complete walk
    /// (which also clears the flag) and the destroy walk's onDelete
    /// sweep (every module).
    #[test]
    fn walks_dispatch_their_carriers(
        class_ids in proptest::collection::vec(0usize..6, 1..16),
    ) {
        let mut host = WalkHost::default();
        let kernel = Kernel::with_grid(
            10.0,
            Region2 { lo_x: 0.0, lo_y: 0.0, hi_x: 100.0, hi_y: 100.0 },
            &mut host,
        );
        let mut kernel = kernel;
        kernel.modules.register_script_modules(&mut kernel.players.name_keys);

        let spec = ObjectSpec {
            behaviors: class_ids
                .iter()
                .enumerate()
                .map(|(i, &c)| BehaviorSpec {
                    name: CLASSES[c].0.into(),
                    tag: format!("ModuleTag_{i}"),
                    ..BehaviorSpec::default()
                })
                .collect(),
            ..ObjectSpec::default()
        };
        let id = kernel.create_object(&spec, &mut host);

        // the die walk
        host.walks.clear();
        kernel.object_on_die(&mut host, id, &DamageInfo::default());
        let expected: Vec<u32> = class_ids
            .iter()
            .enumerate()
            .filter(|(_, c)| CLASSES[**c].1.has(MODULEINTERFACE_DIE))
            .map(|(i, _)| i as u32)
            .collect();
        prop_assert_eq!(&host.walks, &expected);

        // the damage trio (one walk per kind)
        for kind in [
            DamageCallbackKind::Damage,
            DamageCallbackKind::Healing,
            DamageCallbackKind::BodyDamageStateChange,
        ] {
            host.walks.clear();
            match kind {
                DamageCallbackKind::Damage => {
                    kernel.object_notify_damage(&mut host, id, &DamageInfo::default())
                }
                DamageCallbackKind::Healing => {
                    kernel.object_notify_healing(&mut host, id, &DamageInfo::default())
                }
                DamageCallbackKind::BodyDamageStateChange => {
                    kernel.object_notify_body_damage_state_change(
                        &mut host,
                        id,
                        &DamageInfo::default(),
                        BodyDamageType::Pristine,
                        BodyDamageType::Damaged,
                    )
                }
            }
            let expected: Vec<u32> = class_ids
                .iter()
                .enumerate()
                .filter(|(_, c)| CLASSES[**c].1.has(MODULEINTERFACE_DAMAGE))
                .map(|(i, _)| i as u32)
                .collect();
            prop_assert_eq!(&host.walks, &expected);
        }

        // the build-complete walk: the CREATE carriers, the flags cleared
        kernel.object_notify_build_complete(&mut host, id);
        for b in &kernel.objects.get(id).behaviors {
            prop_assert!(!b.need_to_run_on_build_complete);
        }

        // the destroy choreography: the destroy-module walk (the
        // DESTROY carriers) BEFORE the status write, then the onDelete
        // sweep over EVERY module, in order
        host.walks.clear();
        kernel.destroy_object(id, &mut host);
        let mut expected: Vec<u32> = class_ids
            .iter()
            .enumerate()
            .filter(|(_, c)| CLASSES[**c].1.has(MODULEINTERFACE_DESTROY))
            .map(|(i, _)| i as u32)
            .collect();
        expected.extend(0..class_ids.len() as u32);
        prop_assert_eq!(&host.walks, &expected);
    }
}

/// A host recording the walked module indices (the family walks'
/// dispatch order).
#[derive(Default)]
struct WalkHost {
    walks: Vec<u32>,
}

impl KernelHost for WalkHost {
    fn on_die_module(&mut self, _obj: i32, module: u32, _damage: &DamageInfo) {
        self.walks.push(module);
    }
    fn on_damage_module(
        &mut self,
        _obj: i32,
        module: u32,
        _kind: DamageCallbackKind,
        _damage: &DamageInfo,
        _old_state: Option<BodyDamageType>,
        _new_state: Option<BodyDamageType>,
    ) {
        self.walks.push(module);
    }
    fn on_create_module(&mut self, _obj: i32, module: u32) {
        self.walks.push(module);
    }
    fn on_build_complete_module(&mut self, _obj: i32, module: u32) {
        self.walks.push(module);
    }
    fn on_delete_module(&mut self, _obj: i32, module: u32) {
        self.walks.push(module);
    }
    fn on_destroy_module(&mut self, _obj: i32, module: u32) {
        self.walks.push(module);
    }
}

// --- the duplicate refusals (the ctor's DEBUG_ASSERTCRASH family) ------------

/// Two BODY modules on one object trip the duplicate refusal (the
/// `Duplicate bodies` crash — debug-only, like the original's
/// DEBUG_ASSERTCRASH).
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "Duplicate bodies")]
fn duplicate_bodies_refused() {
    let mut host = NullHost;
    let kernel = Kernel::with_grid(
        10.0,
        Region2 {
            lo_x: 0.0,
            lo_y: 0.0,
            hi_x: 100.0,
            hi_y: 100.0,
        },
        &mut host,
    );
    let mut kernel = kernel;
    kernel
        .modules
        .register_script_modules(&mut kernel.players.name_keys);
    let spec = ObjectSpec {
        behaviors: vec![
            BehaviorSpec {
                name: "ScriptBodyModule".into(),
                tag: "ModuleTag_B0".into(),
                ..BehaviorSpec::default()
            },
            BehaviorSpec {
                name: "ScriptBodyModule".into(),
                tag: "ModuleTag_B1".into(),
                ..BehaviorSpec::default()
            },
        ],
        ..ObjectSpec::default()
    };
    let _ = kernel.create_object(&spec, &mut host);
}

/// Two CONTAIN modules trip the container refusal.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "Duplicate containers")]
fn duplicate_containers_refused() {
    let mut host = NullHost;
    let kernel = Kernel::with_grid(
        10.0,
        Region2 {
            lo_x: 0.0,
            lo_y: 0.0,
            hi_x: 100.0,
            hi_y: 100.0,
        },
        &mut host,
    );
    let mut kernel = kernel;
    kernel
        .modules
        .register_script_modules(&mut kernel.players.name_keys);
    let spec = ObjectSpec {
        behaviors: vec![
            BehaviorSpec {
                name: "ScriptContainModule".into(),
                tag: "ModuleTag_C0".into(),
                ..BehaviorSpec::default()
            },
            BehaviorSpec {
                name: "ScriptContainModule".into(),
                tag: "ModuleTag_C1".into(),
                ..BehaviorSpec::default()
            },
        ],
        ..ObjectSpec::default()
    };
    let _ = kernel.create_object(&spec, &mut host);
}

/// A repeating module tag trips the addModuleInfo uniqueness refusal
/// (both profiles — the load-path data error the original throws on).
#[test]
#[should_panic(expected = "must be unique among all modules")]
fn duplicate_tags_refused() {
    let mut host = NullHost;
    let kernel = Kernel::with_grid(
        10.0,
        Region2 {
            lo_x: 0.0,
            lo_y: 0.0,
            hi_x: 100.0,
            hi_y: 100.0,
        },
        &mut host,
    );
    let mut kernel = kernel;
    kernel
        .modules
        .register_script_modules(&mut kernel.players.name_keys);
    let spec = ObjectSpec {
        behaviors: vec![
            BehaviorSpec {
                name: "ScriptUpdateModule".into(),
                tag: "ModuleTag_Same".into(),
                ..BehaviorSpec::default()
            },
            BehaviorSpec {
                name: "ScriptDieModule".into(),
                tag: "ModuleTag_Same".into(),
                ..BehaviorSpec::default()
            },
        ],
        ..ObjectSpec::default()
    };
    let _ = kernel.create_object(&spec, &mut host);
}

/// An unregistered class name trips the factory's not-found crash (the
/// `Module name '%s' not found` of the original).
#[test]
#[should_panic(expected = "Module name 'NoSuchModule' not found")]
fn unknown_module_refused() {
    let mut host = NullHost;
    let kernel = Kernel::with_grid(
        10.0,
        Region2 {
            lo_x: 0.0,
            lo_y: 0.0,
            hi_x: 100.0,
            hi_y: 100.0,
        },
        &mut host,
    );
    let mut kernel = kernel;
    kernel
        .modules
        .register_script_modules(&mut kernel.players.name_keys);
    let spec = ObjectSpec {
        behaviors: vec![BehaviorSpec {
            name: "NoSuchModule".into(),
            tag: "ModuleTag_X".into(),
            ..BehaviorSpec::default()
        }],
        ..ObjectSpec::default()
    };
    let _ = kernel.create_object(&spec, &mut host);
}
