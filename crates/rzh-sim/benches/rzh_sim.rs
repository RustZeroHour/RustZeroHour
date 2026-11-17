// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! The §6 budget benches for the simulation kernel, compared against the
//! C++ baseline (`tools/differential/bench_wrap_kernel.cpp`, the same
//! formula-generated corpus, run through
//! `tools/differential/bench_baseline.sh`).
//!
//! - `kernel/frame_loop`: one full `Kernel::update()` walk per frame over
//!   the mixed corpus — the sleepy dispatch with its requeue rebalances,
//!   the partition refresh of every moved object (the small, precise
//!   -circle and rotated-rect fills), the contact-pair collection and
//!   the destroy-list pass — 180 frames per measurement.
//! - `kernel/dispatch`: the same corpus with no movement — the sleepy
//!   churn alone (the heap's requeue path; the dirty list stays empty
//!   after the first frame).
//! - `kernel/query_x300`: 300 mixed spatial queries over the settled
//!   corpus — the closest-object ring walk (twice: center-2d and the
//!   boundary-2d measure behind an on-map filter) and the
//!   near-to-far-sorted range iterator drained to exhaustion, the
//!   checksum folding every id and distance bit.
//! - `kernel/players_x300`: 300 mixed players/teams operations over the
//!   settled corpus — a third the relationship-to-mask sweeps (all five
//!   players, the flag combos keyed by the iteration), a third the
//!   relationship chains (twelve object pairs through the defector
//!   gates, the team overrides and the owning players' maps), a third
//!   the full `updateTeamStates` walk (the state half with the
//!   enemy-sighted partition queries), the checksum folding every
//!   verdict, mask and team-state word.
//! - `kernel/messages_x300`: 300 GameMessage cycles — each posts 12
//!   mixed messages to the stream (the pixel/int/bool arguments, the
//!   destroy/create group pairs, the replay cameras and the CRC
//!   messages of the rotating players), propagates them through the
//!   three translators (the id-formula dispositions and the mid-body
//!   posts), walks the command list (the dispatch and the CRC handlers
//!   with the validation tail over the connected slots) and resets,
//!   the checksum folding the survivors, the cache and the latch every
//!   cycle.
//! - `kernel/purse_x300`: 300 mixed money/energy/ranks/sciences
//!   operations over the same corpus promoted with the block's data
//!   stores and energy values — a quarter the money streams (the
//!   withdrawal/deposit cycles with the sound and tracking flags, the
//!   bucket rollovers), a quarter the energy influence walks (the
//!   entering and leaving alternating, the adjustPower sign matrix,
//!   the brownout sweeps over the POWERED members), a quarter the rank
//!   ladder (the skill-point gains with the Hoffesommer ceil's
//!   arithmetic and the periodic resets), a quarter the science
//!   purchases (the gate chain and the purchasable walks), the
//!   checksum folding every player's wallet, rank, science and power
//!   state plus the objects' disabled masks.
//!
//! Both benches assert the final-state checksum (the heap array's packed
//! priorities in order, the object list's coverage and cell-change
//! counts, the contact-pair count) — the same value the C++ wrapper
//! computes, locking the two sides to identical bytes.

// The criterion macro generates an undocumented `main`.
#![allow(missing_docs)]

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

use rzh_sim::logic::{Kernel, KernelHost, ObjectSpec, UpdateOutcome};
use rzh_sim::messages::GameMessageType;
use rzh_sim::modules::BehaviorSpec;
use rzh_sim::partition::Region2;
use rzh_sim::query::{PartitionFilter, PartitionFilterOnMap};
use rzh_sim::scheduler::ModuleArena;
use rzh_sim::types::{
    Coord3, DisabledMask, DistanceCalculationType, GeometryInfo, GeometryType, IterOrderType,
    SleepyUpdatePhase, UpdateSleepTime,
};

/// The bench corpus size (the scale of the other map benches' corpora).
const BENCH_OBJECTS: usize = 120;
/// The frames one measurement runs.
const BENCH_FRAMES: u32 = 180;

/// The mixed-run checksum (pinned at authoring time against the C++
/// wrapper — both sides build the identical corpus).
const FRAME_LOOP_CHECKSUM: u64 = 16_055_563_144_769_613_415;
/// The no-movement checksum (ditto).
const DISPATCH_CHECKSUM: u64 = 14_539_779_273_077_219_524;

/// The scripted module bodies (the formula corpus's sleep patterns).
struct BenchHost {
    sleeps: Vec<(i32, u32, UpdateSleepTime)>,
    // the GameMessage block's state (the C++ wrapper's hooks): the net
    // connectivity and latch, the scripted translate dispositions and
    // posts (the id formulas the wrapper fills per cycle)
    net_connected: [bool; 8],
    net_saw_crc_mismatch: bool,
    translate_disposition: Vec<(u32, u32)>,
    translate_posts: Vec<(u32, u32)>,
}

impl KernelHost for BenchHost {
    fn run_update(&mut self, id: i32, module: u32) -> UpdateOutcome {
        let mut outcome = UpdateOutcome::default();
        for &(s_obj, s_mod, sleep) in &self.sleeps {
            if s_obj == id && s_mod == module {
                outcome.sleep = sleep;
            }
        }
        outcome
    }

    fn run_translate(
        &mut self,
        translator: u32,
        msg: u32,
        _data: &rzh_sim::messages::MessageData,
    ) -> rzh_sim::messages::TranslateOutcome {
        let mut outcome = rzh_sim::messages::TranslateOutcome::default();
        if self.translate_disposition.contains(&(translator, msg)) {
            outcome.disposition = rzh_sim::messages::GameMessageDisposition::DESTROY_MESSAGE;
        }
        if self.translate_posts.contains(&(translator, msg)) {
            // the wrapper's single scripted post type
            outcome.posts = vec![rzh_sim::messages::MessagePost {
                ty: GameMessageType::MSG_RAW_KEY_DOWN,
                args: Vec::new(),
            }];
        }
        outcome
    }

    fn is_player_connected(&mut self, slot: i32) -> bool {
        (0..8).contains(&slot) && self.net_connected[slot as usize]
    }

    fn saw_crc_mismatch(&mut self) -> bool {
        self.net_saw_crc_mismatch
    }

    fn on_crc_mismatch(&mut self) {
        self.net_saw_crc_mismatch = true;
    }
}

/// The formula corpus: identical on both sides (the C++ wrapper builds
/// it through the same formulas; the checksums lock it).
fn build_corpus(cell_size: f32, extents: (f32, f32, f32, f32)) -> (Kernel, BenchHost) {
    let region = Region2 {
        lo_x: extents.0,
        lo_y: extents.1,
        hi_x: extents.2,
        hi_y: extents.3,
    };
    let mut host = BenchHost {
        sleeps: Vec::new(),
        net_connected: [false; 8],
        net_saw_crc_mismatch: false,
        translate_disposition: Vec::new(),
        translate_posts: Vec::new(),
    };
    let mut kernel = Kernel::with_grid(cell_size, region, &mut host);
    // TheModuleFactory's scripted-module registrations (every corpus
    // object resolves its classes through the factory)
    kernel
        .modules
        .register_script_modules(&mut kernel.players.name_keys);

    for i in 0..BENCH_OBJECTS as i32 {
        let geometry = match i % 4 {
            0 => GeometryInfo {
                geom_type: GeometryType::Sphere,
                is_small: true,
                height: 1.0,
                major_radius: 2.5,
                minor_radius: 2.5,
            },
            1 => GeometryInfo {
                geom_type: GeometryType::Sphere,
                is_small: false,
                height: 1.0,
                major_radius: 7.0,
                minor_radius: 7.0,
            },
            2 => GeometryInfo {
                geom_type: GeometryType::Cylinder,
                is_small: false,
                height: 4.0,
                major_radius: 12.5,
                minor_radius: 12.5,
            },
            _ => GeometryInfo {
                geom_type: GeometryType::Box,
                is_small: false,
                height: 4.0,
                major_radius: 10.0,
                minor_radius: 6.0,
            },
        };
        let immobile = i % 37 == 0;
        let module_count = if i % 2 == 0 { 1 } else { 2 };
        let phase = match i % 4 {
            0 => SleepyUpdatePhase::Initial,
            1 => SleepyUpdatePhase::Physics,
            2 => SleepyUpdatePhase::Normal,
            _ => SleepyUpdatePhase::Final,
        };
        let mut modules = Vec::new();
        for m in 0..module_count {
            modules.push(BehaviorSpec {
                name: "ScriptUpdateModule".into(),
                tag: format!("ModuleTag_Update{m}"),
                phase,
                disabled_types_to_process: DisabledMask::default(),
                ctor_wake: None,
            });
            let sleep = if m == 0 { 1 + (i % 30) } else { 5 + (i % 17) };
            // the ids run 1..=BENCH_OBJECTS (the counter starts at 1)
            host.sleeps.push((i + 1, m as u32, sleep));
        }
        let spec = ObjectSpec {
            geometry,
            immobile,
            behaviors: modules,
            position: Coord3 {
                x: 20.0 + (i % 20) as f32 * 13.0 + (i / 20) as f32 * 2.5,
                y: 20.0 + (i / 20) as f32 * 11.5,
                z: 0.0,
            },
            orientation: if i % 4 == 3 { 0.3 } else { 0.0 },
            ..ObjectSpec::default()
        };
        kernel.create_object(&spec, &mut host);
    }
    (kernel, host)
}

/// The final-state checksum: the heap array's packed priorities in
/// order, the object list's coverage and cell-change counts, and the
/// contact-pair count of the last frame. The C++ wrapper computes the
/// same value (both sides assert it at authoring time).
fn checksum(kernel: &Kernel) -> u64 {
    let mut sum: u64 = 1_000_003;
    sum = sum
        .wrapping_mul(31)
        .wrapping_add(kernel.logic.scheduler.heap.len() as u64);
    for &h in &kernel.logic.scheduler.heap {
        let packed = u64::from(kernel.objects.module(h).priority());
        sum = sum.wrapping_mul(31).wrapping_add(packed);
    }
    for &id in &kernel.logic.obj_list {
        let obj = kernel.objects.get(id);
        let part = obj.partition.as_ref().expect("registered");
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(part.coi_in_use as u64)
            .wrapping_mul(31)
            .wrapping_add(part.last_cell.map_or(u64::MAX, u64::from))
            .wrapping_mul(31)
            .wrapping_add(obj.cell_change_count as u64);
    }
    sum = sum
        .wrapping_mul(31)
        .wrapping_add(kernel.pm.last_contacts.len() as u64);
    sum
}

/// The mixed corpus run: every third object moves each frame (crossing
/// cells as the offsets accumulate), two objects die at frame 90.
fn run_frames(kernel: &mut Kernel, host: &mut BenchHost, frames: u32, moving: bool) -> u64 {
    for f in 0..frames {
        if moving {
            // the ids run 1..=BENCH_OBJECTS (the counter starts at 1)
            for i in 1..=BENCH_OBJECTS as i32 {
                if kernel.objects.try_get(i).is_none() {
                    continue; // destroyed at frame 90
                }
                if (f as i32 + i) % 3 == 0 {
                    let (old, angle) = {
                        let obj = kernel.objects.get(i);
                        (obj.position, obj.orientation)
                    };
                    let (x, y) = (old.x + 1.7, old.y + 0.9);
                    kernel.objects.get_mut(i).position = Coord3 { x, y, z: 0.0 };
                    kernel.react_to_transform_change(i, old, angle);
                }
            }
        }
        if f == 90 {
            kernel.destroy_object(11, host);
            kernel.destroy_object(71, host);
        }
        kernel.update(host);
    }
    checksum(kernel)
}

/// The corpus shared by both benches (built once, measured many times).
fn corpus() -> (Kernel, BenchHost) {
    build_corpus(10.0, (0.0, 0.0, 300.0, 300.0))
}

fn bench_frame_loop(c: &mut Criterion) {
    // the authoring-time lockstep check: the corpus is deterministic, so
    // the checksum is too (the C++ wrapper asserts the same value)
    let (mut kernel, mut host) = corpus();
    let expected = run_frames(&mut kernel, &mut host, BENCH_FRAMES, true);
    // the authoring-time lockstep value (the C++ wrapper asserts the same)
    assert_eq!(expected, FRAME_LOOP_CHECKSUM);

    c.bench_function("kernel/frame_loop", |b| {
        b.iter_batched(
            corpus,
            |(mut kernel, mut host)| run_frames(&mut kernel, &mut host, BENCH_FRAMES, true),
            BatchSize::PerIteration,
        )
    });
}

fn bench_dispatch(c: &mut Criterion) {
    let (mut kernel, mut host) = corpus();
    let expected = run_frames(&mut kernel, &mut host, BENCH_FRAMES, false);
    // the authoring-time lockstep value (the C++ wrapper asserts the same)
    assert_eq!(expected, DISPATCH_CHECKSUM);

    c.bench_function("kernel/dispatch", |b| {
        b.iter_batched(
            corpus,
            |(mut kernel, mut host)| run_frames(&mut kernel, &mut host, BENCH_FRAMES, false),
            BatchSize::PerIteration,
        )
    });
}

/// The query bench's checksum (pinned at authoring time against the C++
/// wrapper — both sides build the identical corpus and query stream).
const QUERY_CHECKSUM: u64 = 1_025_416_407_242_357_139;

/// The query corpus: the frame-loop corpus settled by one full update
/// (the COI coverage the ring walk reads), then the 300-query stream —
/// a third closest-object center-2d queries, a third boundary-2d
/// closest behind an on-map filter, a third near-to-far-sorted range
/// iterators drained to exhaustion. The checksum folds every id and
/// distance bit, locking the two sides together.
fn run_queries(kernel: &mut Kernel) -> u64 {
    let mut sum: u64 = 1_000_003;
    for i in 0..300u64 {
        let pos = Coord3 {
            x: 20.0 + ((i * 7) % 260) as f32,
            y: 15.0 + ((i * 13) % 270) as f32,
            z: 0.0,
        };
        match i % 3 {
            0 => {
                let (id, dist, _) = kernel.pm.get_closest_object_from_position(
                    &mut kernel.objects,
                    &kernel.players,
                    pos,
                    45.0,
                    DistanceCalculationType::FromCenter2d,
                    &[],
                );
                sum = sum
                    .wrapping_mul(31)
                    .wrapping_add(id.unwrap_or(0) as u64)
                    .wrapping_mul(31)
                    .wrapping_add(u64::from(dist.to_bits()));
            }
            1 => {
                let on_map = PartitionFilterOnMap;
                let filters: [&dyn PartitionFilter; 1] = [&on_map];
                let (id, dist, _) = kernel.pm.get_closest_object_from_position(
                    &mut kernel.objects,
                    &kernel.players,
                    pos,
                    60.0,
                    DistanceCalculationType::FromBoundingsphere2d,
                    &filters,
                );
                sum = sum
                    .wrapping_mul(31)
                    .wrapping_add(id.unwrap_or(0) as u64)
                    .wrapping_mul(31)
                    .wrapping_add(u64::from(dist.to_bits()));
            }
            _ => {
                let mut it = kernel.pm.iterate_objects_in_range_from_position(
                    &mut kernel.objects,
                    &kernel.players,
                    pos,
                    35.0,
                    DistanceCalculationType::FromCenter2d,
                    &[],
                    IterOrderType::SortedNearToFar,
                );
                it.reset();
                loop {
                    let (o, num) = it.next_with_numeric();
                    if o == 0 {
                        break;
                    }
                    sum = sum
                        .wrapping_mul(31)
                        .wrapping_add(o as u64)
                        .wrapping_mul(31)
                        .wrapping_add(u64::from(num.to_bits()));
                }
            }
        }
    }
    sum
}

/// The settled query corpus (the coverage filled by one full update).
fn query_corpus() -> (Kernel, BenchHost) {
    let (mut kernel, mut host) = build_corpus(10.0, (0.0, 0.0, 300.0, 300.0));
    kernel.update(&mut host);
    (kernel, host)
}

fn bench_query(c: &mut Criterion) {
    // the authoring-time lockstep check: the corpus and the query stream
    // are deterministic, so the checksum is too (the C++ wrapper asserts
    // the same value)
    let (mut kernel, _host) = query_corpus();
    let expected = run_queries(&mut kernel);
    assert_eq!(expected, QUERY_CHECKSUM, "re-pin QUERY_CHECKSUM");

    c.bench_function("kernel/query_x300", |b| {
        b.iter_batched(
            query_corpus,
            |(mut kernel, _host)| run_queries(&mut kernel),
            BatchSize::PerIteration,
        )
    });
}

/// The players/teams block's checksum (pinned at authoring time against
/// the C++ wrapper — both sides build the identical corpus).
const PLAYERS_CHECKSUM: u64 = 10_479_098_065_696_129_437;

/// The players corpus: the 4-side world (the enemies/allies assignment
/// included) with the 120 objects distributed over the default teams
/// (every fifth object to its player's squad instead), the vision
/// ranges and AI bits set, settled by one full update.
fn players_corpus() -> (Kernel, BenchHost) {
    let (mut kernel, mut host) = build_corpus(10.0, (0.0, 0.0, 300.0, 300.0));

    // the distilled sides: four players, the first the human and the
    // local one; p1→p2 enemies, p3→p1 allies
    let sides = rzh_sim::players::SidesSpec {
        sides: vec![
            rzh_sim::players::SideSpec {
                player_name: String::new(),
                ..Default::default()
            },
            rzh_sim::players::SideSpec {
                player_name: "p1".to_string(),
                is_human: true,
                enemies: vec!["p2".to_string()],
                ..Default::default()
            },
            rzh_sim::players::SideSpec {
                player_name: "p2".to_string(),
                ..Default::default()
            },
            rzh_sim::players::SideSpec {
                player_name: "p3".to_string(),
                allies: vec!["p1".to_string()],
                ..Default::default()
            },
        ],
        teams: {
            let mut teams = vec![rzh_sim::players::TeamSpec {
                name: "team".to_string(),
                owner: String::new(),
                singleton: true,
                ..Default::default()
            }];
            for p in 1..=3 {
                teams.push(rzh_sim::players::TeamSpec {
                    name: format!("teamp{p}"),
                    owner: format!("p{p}"),
                    singleton: true,
                    ..Default::default()
                });
            }
            for p in 1..=3 {
                teams.push(rzh_sim::players::TeamSpec {
                    name: format!("p{p}squad"),
                    owner: format!("p{p}"),
                    singleton: false,
                    ..Default::default()
                });
            }
            teams
        },
        has_game_info: false,
        game_info: Default::default(),
        default_starting_cash: 0,
    };
    // the block's data FIRST (the new_game's init walk runs the ladder —
    // a reset after the team distribution would wipe it): the science
    // catalog (chains, costs, the non-grantable) and the 5-rung ladder
    let science_specs: Vec<rzh_sim::sciences::ScienceSpec> = (0..8)
        .map(|i| rzh_sim::sciences::ScienceSpec {
            name: format!("S{i}"),
            prereq_sciences: if i % 3 == 0 {
                Vec::new()
            } else {
                vec![format!("S{}", i - 1)]
            },
            science_purchase_point_cost: 1 + (i % 3),
            grantable: i % 5 != 0,
        })
        .collect();
    let rank_specs: Vec<rzh_sim::sciences::RankSpec> = [0i32, 100, 300, 600, 1000]
        .iter()
        .enumerate()
        .map(|(i, needed)| rzh_sim::sciences::RankSpec {
            skill_points_needed: *needed,
            science_purchase_points_granted: 1 + i as i32,
            sciences_granted: vec![format!("S{}", i.min(7))],
        })
        .collect();
    kernel.players.load_stores(&science_specs, &rank_specs);

    kernel
        .players
        .new_game(&sides, &mut kernel.objects, &mut host);

    // the team distribution: object i belongs to player (i % 3) + 1's
    // default team, every fifth to that player's squad instead
    let default_teams: Vec<u32> = (1..=3)
        .map(|p| kernel.players.list.players[p].default_team)
        .collect();
    let squads: Vec<u32> = (1..=3)
        .map(|p| {
            kernel
                .players
                .create_team(&format!("p{p}squad"), &mut host)
                .unwrap()
        })
        .collect();
    for i in 1..=BENCH_OBJECTS {
        let player = (i % 3) + 1;
        let team = if i % 5 == 0 {
            squads[player - 1]
        } else {
            default_teams[player - 1]
        };
        kernel
            .players
            .set_team(&mut kernel.objects, i as i32, Some(team), &mut host);
        let obj = kernel.objects.get_mut(i as i32);
        obj.vision_range = 5.0 + ((i % 7) as f32);
        obj.has_ai = (i % 3) != 0;
        obj.ai_idle = (i % 2) == 0;
    }

    kernel.update(&mut host); // the settle: the COI coverage the queries walk
    (kernel, host)
}

/// The timed body: 300 mixed players/teams operations (the C++
/// wrapper's bench_kernel_players, formula for formula).
fn run_players(kernel: &mut Kernel, host: &mut BenchHost) -> u64 {
    use rzh_sim::types::{ALLOW_ALLIES, ALLOW_ENEMIES, ALLOW_NEUTRAL, ALLOW_SAME_PLAYER};

    let mut sum: u64 = 1_000_003;
    for i in 0..300u64 {
        match i % 3 {
            0 => {
                // the relationship-to-mask sweeps: all five players, the
                // flag combo keyed by the iteration
                let same = if (i & 1) != 0 { ALLOW_SAME_PLAYER } else { 0 };
                let flags: u32 = ALLOW_ALLIES | ALLOW_ENEMIES | ALLOW_NEUTRAL | same;
                for p in 0..5i32 {
                    let mask = kernel.players.get_players_with_relationship(p, flags);
                    sum = sum.wrapping_mul(31).wrapping_add(u64::from(mask));
                }
            }
            1 => {
                // the relationship chains: twelve object pairs
                for k in 0..12u64 {
                    let a = ((i * 7 + k * 13) % 120 + 1) as i32;
                    let b = ((i * 11 + k * 29) % 120 + 1) as i32;
                    let r = kernel
                        .players
                        .object_get_relationship(&kernel.objects, a, b);
                    sum = sum.wrapping_mul(31).wrapping_add(r.bits() as u64);
                    let ta = kernel.objects.get(a).team;
                    let tb = kernel.objects.get(b).team;
                    let tr = kernel.players.team_get_relationship(ta, tb);
                    sum = sum.wrapping_mul(31).wrapping_add(tr.bits() as u64);
                }
            }
            _ => {
                // the full updateTeamStates walk (the state half with
                // the enemy-sighted partition queries), then the fold
                kernel
                    .players
                    .update_team_states(&mut kernel.objects, &mut kernel.pm, host);
                let mut team_fold: u64 = 0;
                for slot in kernel.players.factory.map.values() {
                    if let Some(proto) = kernel.players.factory.protos[*slot].as_ref() {
                        for &ts in proto.instances.iter().rev() {
                            if let Some(t) = kernel.players.factory.teams[ts].as_ref() {
                                team_fold = team_fold
                                    .wrapping_mul(31)
                                    .wrapping_add(u64::from(t.id))
                                    .wrapping_mul(31)
                                    .wrapping_add(t.state.cur_units as u64)
                                    .wrapping_mul(31)
                                    .wrapping_add(u64::from(t.state.see_enemy));
                            }
                        }
                    }
                }
                sum = sum.wrapping_mul(31).wrapping_add(team_fold);
            }
        }
    }
    sum
}

fn bench_players(c: &mut Criterion) {
    // the authoring-time lockstep check: the corpus and the operation
    // stream are deterministic, so the checksum is too (the C++ wrapper
    // asserts the same value)
    let (mut kernel, mut host) = players_corpus();
    let expected = run_players(&mut kernel, &mut host);
    assert_eq!(expected, PLAYERS_CHECKSUM, "re-pin PLAYERS_CHECKSUM");

    c.bench_function("kernel/players_x300", |b| {
        b.iter_batched(
            players_corpus,
            |(mut kernel, mut host)| run_players(&mut kernel, &mut host),
            BatchSize::PerIteration,
        )
    });
}

/// The money/energy/ranks/sciences block's checksum (pinned at authoring
/// time against the C++ wrapper — both sides build the identical corpus
/// and operation stream).
const PURSE_CHECKSUM: u64 = 15_734_376_335_415_262_704;

/// The purse corpus: the players corpus promoted with the block's data
/// (the science catalog with chains and costs, the 5-rung rank ladder),
/// the objects carrying energy values (producers, consumers and bonus
/// carriers on the teams), the players' wallets and radars — settled by
/// one full update.
fn purse_corpus() -> (Kernel, BenchHost) {
    let (mut kernel, mut host) = build_corpus(10.0, (0.0, 0.0, 300.0, 300.0));

    // the distilled sides: four players, the first the human and the
    // local one; p1→p2 enemies, p3→p1 allies
    let sides = rzh_sim::players::SidesSpec {
        sides: vec![
            rzh_sim::players::SideSpec {
                player_name: String::new(),
                ..Default::default()
            },
            rzh_sim::players::SideSpec {
                player_name: "p1".to_string(),
                is_human: true,
                enemies: vec!["p2".to_string()],
                ..Default::default()
            },
            rzh_sim::players::SideSpec {
                player_name: "p2".to_string(),
                ..Default::default()
            },
            rzh_sim::players::SideSpec {
                player_name: "p3".to_string(),
                allies: vec!["p1".to_string()],
                ..Default::default()
            },
        ],
        teams: {
            let mut teams = vec![rzh_sim::players::TeamSpec {
                name: "team".to_string(),
                owner: String::new(),
                singleton: true,
                ..Default::default()
            }];
            for p in 1..=3 {
                teams.push(rzh_sim::players::TeamSpec {
                    name: format!("teamp{p}"),
                    owner: format!("p{p}"),
                    singleton: true,
                    ..Default::default()
                });
            }
            for p in 1..=3 {
                teams.push(rzh_sim::players::TeamSpec {
                    name: format!("p{p}squad"),
                    owner: format!("p{p}"),
                    singleton: false,
                    ..Default::default()
                });
            }
            teams
        },
        has_game_info: false,
        game_info: Default::default(),
        default_starting_cash: 0,
    };
    // the block's data FIRST (the new_game's init walk runs the ladder —
    // a reset after the team distribution would wipe it): the science
    // catalog (chains, costs, the non-grantable) and the 5-rung ladder
    let science_specs: Vec<rzh_sim::sciences::ScienceSpec> = (0..8)
        .map(|i| rzh_sim::sciences::ScienceSpec {
            name: format!("S{i}"),
            prereq_sciences: if i % 3 == 0 {
                Vec::new()
            } else {
                vec![format!("S{}", i - 1)]
            },
            science_purchase_point_cost: 1 + (i % 3),
            grantable: i % 5 != 0,
        })
        .collect();
    let rank_specs: Vec<rzh_sim::sciences::RankSpec> = [0i32, 100, 300, 600, 1000]
        .iter()
        .enumerate()
        .map(|(i, needed)| rzh_sim::sciences::RankSpec {
            skill_points_needed: *needed,
            science_purchase_points_granted: 1 + i as i32,
            sciences_granted: vec![format!("S{}", i.min(7))],
        })
        .collect();
    kernel.players.load_stores(&science_specs, &rank_specs);

    kernel
        .players
        .new_game(&sides, &mut kernel.objects, &mut host);

    // the team distribution: object i belongs to player (i % 3) + 1's
    // default team, every fifth to that player's squad instead
    let default_teams: Vec<u32> = (1..=3)
        .map(|p| kernel.players.list.players[p].default_team)
        .collect();
    let squads: Vec<u32> = (1..=3)
        .map(|p| {
            kernel
                .players
                .create_team(&format!("p{p}squad"), &mut host)
                .unwrap()
        })
        .collect();
    for i in 1..=BENCH_OBJECTS {
        let player = (i % 3) + 1;
        let team = if i % 5 == 0 {
            squads[player - 1]
        } else {
            default_teams[player - 1]
        };
        kernel
            .players
            .set_team(&mut kernel.objects, i as i32, Some(team), &mut host);
        let obj = kernel.objects.get_mut(i as i32);
        obj.vision_range = 5.0 + ((i % 7) as f32);
        obj.has_ai = (i % 3) != 0;
        obj.ai_idle = (i % 2) == 0;
    }

    // the energy values: producers, consumers and bonus carriers over
    // the teams (the template fields the corpus programs)
    for i in 1..=BENCH_OBJECTS {
        let obj = kernel.objects.get_mut(i as i32);
        obj.energy_production = match i % 4 {
            0 => 30,
            1 => -20,
            2 => 25,
            _ => -10,
        };
        obj.energy_bonus = if i % 3 == 0 { 50 } else { 0 };
    }
    // the POWERED kindof bit rides the spec; re-create is not needed —
    // program the bit directly (the spec's powered flag's equivalent)
    for i in 1..=BENCH_OBJECTS {
        if i % 2 == 0 {
            kernel.objects.get_mut(i as i32).kindof |= 1u128 << rzh_sim::types::KINDOF_POWERED_BIT;
        }
    }

    // the wallets and the radars
    for p in 1..=3 {
        let cash = 10_000 + (p as u32 * 13);
        kernel.players.list.players[p].money.set_starting_cash(cash);
        kernel.players.list.players[p].add_radar(p % 2 == 0);
    }

    kernel.update(&mut host); // the settle
    (kernel, host)
}

/// The timed body: 300 mixed money/energy/ranks/sciences operations (the
/// C++ wrapper's bench_kernel_purse, formula for formula).
fn run_purse(kernel: &mut Kernel, host: &mut BenchHost) -> u64 {
    let mut sum: u64 = 1_000_003;

    // the science keys, resolved once (the catalog's ordinals)
    let sciences: Vec<rzh_sim::sciences::ScienceType> = (0..8)
        .map(|i| kernel.players.science_from_name(&format!("S{i}")))
        .collect();

    for i in 0..300u64 {
        match i % 4 {
            0 => {
                // the money stream: withdrawals and deposits with the
                // tracked/untracked and sound flags keyed by the
                // iteration, the bucket rollover at the frame
                for p in 1..=3u32 {
                    let sound = (i + u64::from(p)) % 2 == 0;
                    let track = (i + u64::from(p)) % 3 == 0;
                    let wallet = &mut kernel.players.list.players[p as usize].money;
                    wallet.withdraw(50 + ((i + u64::from(p)) % 97) as u32, sound, host);
                    wallet.deposit(30 + ((i * u64::from(p)) % 53) as u32, sound, track, host);
                    wallet.update_income_bucket(i as u32);
                    sum = sum
                        .wrapping_mul(31)
                        .wrapping_add(u64::from(wallet.count_money()))
                        .wrapping_mul(31)
                        .wrapping_add(u64::from(wallet.get_cash_per_minute()));
                }
            }
            1 => {
                // the energy stream: the influence walks (entering and
                // leaving alternating), the adjustPower matrix, the reads
                for p in 1..=3usize {
                    let obj = ((i * 7 + p as u64 * 29) % 120 + 1) as i32;
                    if i % 2 == 0 {
                        kernel.players.energy_object_entering_influence(
                            &mut kernel.objects,
                            p,
                            obj,
                            i as u32,
                        );
                    } else {
                        kernel.players.energy_object_leaving_influence(
                            &mut kernel.objects,
                            p,
                            obj,
                            i as u32,
                        );
                    }
                    let delta = ((i % 5) as i32 - 2) * 7;
                    kernel.players.energy_adjust_power(
                        &mut kernel.objects,
                        p,
                        delta,
                        i % 2 == 0,
                        i as u32,
                    );
                    sum = sum
                        .wrapping_mul(31)
                        .wrapping_add(kernel.players.energy_get_production(p, i as u32) as u64)
                        .wrapping_mul(31)
                        .wrapping_add(kernel.players.energy_get_consumption(p) as u64)
                        .wrapping_mul(31)
                        .wrapping_add(u64::from(
                            kernel.players.energy_supply_ratio(p, i as u32).to_bits(),
                        ));
                }
            }
            2 => {
                // the rank stream: the skill-point gains (the Hoffesommer
                // ceil's arithmetic included), the periodic resets
                let limit = kernel.logic.rank_level_limit;
                for p in 1..=3usize {
                    if i % 16 == 15 {
                        kernel.players.reset_rank(p, host);
                    } else {
                        kernel
                            .players
                            .add_skill_points(p, 17 + (i % 23) as i32, limit, host);
                    }
                    let pl = &kernel.players.list.players[p];
                    sum = sum
                        .wrapping_mul(31)
                        .wrapping_add(pl.rank_level as u64)
                        .wrapping_mul(31)
                        .wrapping_add(pl.skill_points as u64)
                        .wrapping_mul(31)
                        .wrapping_add(pl.science_purchase_points as u64);
                }
            }
            _ => {
                // the science stream: the purchases (the gate chain), the
                // purchasable walk, the point top-ups
                for p in 1..=3usize {
                    let science = sciences[((i + p as u64) % 8) as usize];
                    kernel
                        .players
                        .attempt_to_purchase_science(p, science, &mut *host);
                    if i % 3 == 0 {
                        kernel.players.add_science_purchase_points(p, 2);
                    }
                    let (can, pot) = kernel.players.get_purchasable_sciences(p);
                    sum = sum
                        .wrapping_mul(31)
                        .wrapping_add(can.len() as u64)
                        .wrapping_mul(31)
                        .wrapping_add(pot.len() as u64);
                    for s in can {
                        sum = sum.wrapping_mul(31).wrapping_add(u64::from(s.ordinal()));
                    }
                }
            }
        }
    }

    // the final fold: every player's block state and the objects' masks
    for p in 0..16usize {
        let pl = &kernel.players.list.players[p];
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(u64::from(pl.money.count_money()))
            .wrapping_mul(31)
            .wrapping_add(u64::from(pl.money.get_cash_per_minute()))
            .wrapping_mul(31)
            .wrapping_add(pl.rank_level as u64)
            .wrapping_mul(31)
            .wrapping_add(pl.skill_points as u64)
            .wrapping_mul(31)
            .wrapping_add(pl.science_purchase_points as u64)
            .wrapping_mul(31)
            .wrapping_add(pl.sciences.len() as u64)
            .wrapping_mul(31)
            .wrapping_add(pl.energy.energy_production as u64)
            .wrapping_mul(31)
            .wrapping_add(pl.energy.energy_consumption as u64)
            .wrapping_mul(31)
            .wrapping_add(pl.radar_count as u64);
    }
    for &id in &kernel.logic.obj_list {
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(u64::from(kernel.objects.get(id).disabled_mask.bits));
    }
    sum
}

fn bench_purse(c: &mut Criterion) {
    // the authoring-time lockstep check: the corpus and the operation
    // stream are deterministic, so the checksum is too (the C++ wrapper
    // asserts the same value)
    let (mut kernel, mut host) = purse_corpus();
    let expected = run_purse(&mut kernel, &mut host);
    assert_eq!(expected, PURSE_CHECKSUM, "re-pin PURSE_CHECKSUM");

    c.bench_function("kernel/purse_x300", |b| {
        b.iter_batched(
            purse_corpus,
            |(mut kernel, mut host)| run_purse(&mut kernel, &mut host),
            BatchSize::PerIteration,
        )
    });
}

/// The GameMessage block's checksum (pinned at authoring time against
/// the C++ wrapper — both sides build the identical corpus and cycle
/// stream).
const MESSAGES_CHECKSUM: u64 = 6_887_319_234_910_174_045;

/// The messages corpus: the base corpus's world with a 4-player
/// networked block (the local human p1, two other humans, the
/// neutral), the slots connected and three translators attached
/// (priorities 1, 5, 5 — the stable tie).
fn messages_corpus() -> (Kernel, BenchHost) {
    let (mut kernel, mut host) = build_corpus(10.0, (0.0, 0.0, 300.0, 300.0));
    let sides = rzh_sim::players::SidesSpec {
        sides: vec![
            rzh_sim::players::SideSpec::default(), // the neutral side
            rzh_sim::players::SideSpec {
                player_name: "p1".to_string(),
                is_human: true,
                multiplayer_is_local: true,
                ..Default::default()
            },
            rzh_sim::players::SideSpec {
                player_name: "p2".to_string(),
                is_human: true,
                ..Default::default()
            },
            rzh_sim::players::SideSpec {
                player_name: "p3".to_string(),
                is_human: true,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    kernel.players.network_present = true;
    kernel
        .players
        .new_game(&sides, &mut kernel.objects, &mut host);
    kernel.players.set_slot_index(1, 0);
    kernel.players.set_slot_index(2, 1);
    kernel.players.set_slot_index(3, 2);
    host.net_connected = [true, true, true, false, false, false, false, false];
    kernel.stream.attach_translator(1);
    kernel.stream.attach_translator(5);
    kernel.stream.attach_translator(5);
    (kernel, host)
}

/// The timed body: 300 message cycles — each posts 12 mixed messages to
/// the stream, propagates them through the three translators (the
/// id-formula dispositions and the mid-body posts), walks the command
/// list (the dispatch and the CRC handlers with the validation tail)
/// and resets — the checksum folding the survivors, the cache and the
/// latch every cycle (the C++ wrapper's `run_messages`, formula for
/// formula).
fn run_messages(kernel: &mut Kernel, host: &mut BenchHost) -> u64 {
    let mut sum: u64 = 1_000_003;
    for c in 0..300u64 {
        for j in 0..12u32 {
            let id = match j % 6 {
                0 => {
                    let id = kernel.post_stream_message(GameMessageType::MSG_RAW_MOUSE_POSITION);
                    kernel
                        .msg_arena
                        .get_mut(id)
                        .append_pixel_argument(rzh_sim::types::ICoord2 {
                            x: (j * 17) as i32,
                            y: (j * 31) as i32,
                        });
                    id
                }
                1 => {
                    let id = kernel.post_stream_message(GameMessageType::MSG_PURCHASE_SCIENCE);
                    kernel
                        .msg_arena
                        .get_mut(id)
                        .append_integer_argument((c * 31 + u64::from(j)) as i32);
                    id
                }
                2 => kernel.post_stream_message(GameMessageType::MSG_DESTROY_SELECTED_GROUP),
                3 => {
                    let id = kernel.post_stream_message(GameMessageType::MSG_CREATE_SELECTED_GROUP);
                    kernel
                        .msg_arena
                        .get_mut(id)
                        .append_boolean_argument(j % 2 == 0);
                    id
                }
                4 => kernel.post_stream_message(GameMessageType::MSG_SET_REPLAY_CAMERA),
                _ => {
                    let id = kernel.post_stream_message(GameMessageType::MSG_LOGIC_CRC);
                    let m = kernel.msg_arena.get_mut(id);
                    m.player_index = 1 + (c % 3) as i32;
                    m.append_integer_argument((0x1000_0000u64 + c * 31 + u64::from(j)) as i32);
                    id
                }
            };
            // the scripted translator bodies (the id formulas — the C++
            // wrapper drives the identical ones)
            if id % 5 == 2 {
                host.translate_disposition.push((3, id)); // t3, the last
            }
            if id % 7 == 3 {
                host.translate_posts.push((1, id)); // t1 posts the RAW_KEY_DOWN
            }
        }
        kernel.propagate_messages(host);
        let survivors = kernel.commands.ids(&kernel.msg_arena);
        sum = sum.wrapping_mul(31).wrapping_add(survivors.len() as u64);
        for &id in &survivors {
            sum = sum.wrapping_mul(31).wrapping_add(u64::from(id));
            sum = sum
                .wrapping_mul(31)
                .wrapping_add(kernel.msg_arena.get(id).ty.ordinal() as u64);
        }
        kernel.process_command_list(host);
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(kernel.logic.cached_crcs.len() as u64);
        for (&player, &crc) in &kernel.logic.cached_crcs {
            sum = sum.wrapping_mul(31).wrapping_add(player as u64);
            sum = sum.wrapping_mul(31).wrapping_add(u64::from(crc));
        }
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(u64::from(host.net_saw_crc_mismatch));
        kernel.commands.reset(&mut kernel.msg_arena);
        host.translate_disposition.clear();
        host.translate_posts.clear();
    }
    sum
}

fn bench_messages(c: &mut Criterion) {
    // the authoring-time lockstep check: the corpus and the cycle
    // stream are deterministic, so the checksum is too (the C++
    // wrapper asserts the same value)
    let (mut kernel, mut host) = messages_corpus();
    let expected = run_messages(&mut kernel, &mut host);
    assert_eq!(expected, MESSAGES_CHECKSUM, "re-pin MESSAGES_CHECKSUM");

    c.bench_function("kernel/messages_x300", |b| {
        b.iter_batched(
            messages_corpus,
            |(mut kernel, mut host)| run_messages(&mut kernel, &mut host),
            BatchSize::PerIteration,
        )
    });
}

/// The module framework's checksum (pinned at authoring time against
/// the C++ wrapper — both sides build the identical corpus and walk
/// cycle).
const MODULES_CHECKSUM: u64 = 1_632_563_042_329_548_879;

/// The module-framework corpus: 300 objects built through the factory
/// with mixed family lists by formula (the body/contain singletons on
/// the divisibility gates, the update halves with formula phases and
/// constructor wakes, the collide/die/damage/create families likewise).
fn modules_corpus() -> (Kernel, BenchHost) {
    let region = Region2 {
        lo_x: 0.0,
        lo_y: 0.0,
        hi_x: 300.0,
        hi_y: 300.0,
    };
    let mut host = BenchHost {
        sleeps: Vec::new(),
        net_connected: [false; 8],
        net_saw_crc_mismatch: false,
        translate_disposition: Vec::new(),
        translate_posts: Vec::new(),
    };
    let mut kernel = Kernel::with_grid(10.0, region, &mut host);
    kernel
        .modules
        .register_script_modules(&mut kernel.players.name_keys);
    for i in 0..300i32 {
        let mut behaviors: Vec<BehaviorSpec> = Vec::new();
        let mut tag = 0i32;
        let mut mut_tag = |name: &str| {
            let t = format!("ModuleTag_{i}_{tag}");
            tag += 1;
            BehaviorSpec {
                name: name.into(),
                tag: t,
                ..BehaviorSpec::default()
            }
        };
        if i % 5 == 0 {
            behaviors.push(mut_tag("ScriptBodyModule"));
        }
        if i % 6 == 0 {
            behaviors.push(mut_tag("ScriptContainModule"));
        }
        let updates = 1 + (i % 2) as usize;
        for u in 0..updates {
            let phase = match (i + u as i32) % 4 {
                0 => SleepyUpdatePhase::Initial,
                1 => SleepyUpdatePhase::Physics,
                3 => SleepyUpdatePhase::Final,
                _ => SleepyUpdatePhase::Normal,
            };
            let mut spec = mut_tag("ScriptUpdateModule");
            spec.phase = phase;
            spec.ctor_wake = if i % 10 == 0 {
                Some(u as u32 + 3)
            } else {
                None
            };
            behaviors.push(spec);
        }
        if i % 3 == 0 {
            behaviors.push(mut_tag("ScriptCollideModule"));
        }
        if i % 4 == 0 {
            behaviors.push(mut_tag("ScriptDieModule"));
        }
        if i % 7 == 0 {
            behaviors.push(mut_tag("ScriptDamageModule"));
        }
        if i % 11 == 0 {
            behaviors.push(mut_tag("ScriptCreateModule"));
        }
        let x = 10.0 + (i % 30) as f32 * 9.0;
        let y = 10.0 + (i / 30) as f32 * 9.0;
        let spec = ObjectSpec {
            geometry: GeometryInfo {
                geom_type: GeometryType::Sphere,
                is_small: true,
                height: 1.0,
                major_radius: 2.5,
                minor_radius: 2.5,
            },
            immobile: false,
            behaviors,
            position: Coord3 { x, y, z: 0.0 },
            ..ObjectSpec::default()
        };
        let id = kernel.create_object(&spec, &mut host);
        // the scripted sleeps by formula (the update halves)
        for m in 0..updates {
            host.sleeps.push((id, m as u32, 1 + ((i + m as i32) % 9)));
        }
    }
    (kernel, host)
}

/// One module-framework cycle: three rounds of the family walks over
/// every tenth object (the die walk, the damage trio, build-complete),
/// a destroy of every 97th object, then ten update frames (the sleepy
/// dispatch through the factory-built modules). Returns the state
/// checksum (the heap's packed priorities in order, the behavior
/// counts, the singleton captures).
fn run_modules(kernel: &mut Kernel, host: &mut BenchHost) -> u64 {
    let mut sum = 0u64;
    for round in 0..3 {
        let ids: Vec<i32> = kernel.objects.slot_ids();
        for (n, &id) in ids.iter().enumerate() {
            if n % 10 != 0 {
                continue;
            }
            let damage = rzh_sim::modules::DamageInfo {
                source_id: id,
                death_type: rzh_sim::modules::DeathType::Normal,
                amount: (round * 7) as f32,
                ..Default::default()
            };
            kernel.object_on_die(host, id, &damage);
            kernel.object_notify_damage(host, id, &damage);
            kernel.object_notify_healing(host, id, &damage);
            kernel.object_notify_body_damage_state_change(
                host,
                id,
                &damage,
                rzh_sim::modules::BodyDamageType::Pristine,
                rzh_sim::modules::BodyDamageType::Damaged,
            );
            kernel.object_notify_build_complete(host, id);
        }
        let ids: Vec<i32> = kernel.objects.slot_ids();
        for (n, &id) in ids.iter().enumerate() {
            if n % 97 == 1 {
                kernel.destroy_object(id, host);
            }
        }
        kernel.process_destroy_list(host);
        for _ in 0..10 {
            kernel.pre_update();
            kernel.update(host);
        }
    }
    // the checksum: the heap's packed priorities in slot order, the
    // live objects' behavior counts and singleton captures
    for h in &kernel.logic.scheduler.heap {
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(u64::from(kernel.objects.module(*h).priority()));
    }
    for id in kernel.objects.slot_ids() {
        let obj = kernel.objects.get(id);
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(obj.behaviors.len() as u64);
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(u64::from(obj.body_module.map_or(-1, |m| m as i32) as u32));
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(u64::from(obj.contain_module.map_or(-1, |m| m as i32) as u32));
    }
    sum
}

fn bench_modules(c: &mut Criterion) {
    // the authoring-time lockstep check: the corpus and the walk cycle
    // are deterministic, so the checksum is too (the C++ wrapper
    // asserts the same value)
    let (mut kernel, mut host) = modules_corpus();
    let expected = run_modules(&mut kernel, &mut host);
    assert_eq!(expected, MODULES_CHECKSUM, "re-pin MODULES_CHECKSUM");

    c.bench_function("kernel/modules_x300", |b| {
        b.iter_batched(
            modules_corpus,
            |(mut kernel, mut host)| run_modules(&mut kernel, &mut host),
            BatchSize::PerIteration,
        )
    });
}

criterion_group!(
    benches,
    bench_frame_loop,
    bench_dispatch,
    bench_query,
    bench_players,
    bench_purse,
    bench_messages,
    bench_modules
);
criterion_main!(benches);
