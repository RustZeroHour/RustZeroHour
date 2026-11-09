// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the simulation-kernel port.
//!
//! `tests/golden/kernel.txt` is a script produced by
//! `tools/differential/gen_kernel_golden.cpp` — a verbatim transcription
//! of the Zero Hour game-logic kernel (the frame loop's ordered
//! subsystem walk, the object registry with the ID counter and the
//! destroy list's two-pass sleepy erase, the sleepy-update scheduler
//! with its packed frame+phase priority heap, the disabled-mask state,
//! and the partition manager's spatial store — the grid, the COI
//! coverage with the small/precise-circle/rect fills, the dirty-module
//! refresh and the contact-pair collection). Each corpus drives the
//! transcription through a scripted world and dumps the discrete state:
//! the heap array (module ids, packed priorities, index stamps), the
//! registry (frames, ids, the object list, the destroy list), the store
//! (the grid, the master and dirty lists, the per-object coverage, the
//! per-cell COI lists), the contact pairs and the ordered event stream.
//!
//! The test replays every corpus through [`rzh_sim`] with the same
//! driver shape (the [`Harness`] mirrors the generator's `World`, the
//! [`GoldenHost`] mirrors its hook counters and scripted module bodies)
//! and asserts line-by-line equality. The pinned quirks include: the
//! 30 Hz frame constants; the ID counter's wrap and the loadPostProcess
//! compaction; the 0→1 initial wake clamp; the same-priority tie order
//! (the heap array's exact state after pushes, requeues and the
//! two-pass destroy erase); the phase ordering; the awaken refusal
//! family (the easy return, the "already awake at now+1" case, the
//! not-in-list initialization) and the FOREVER saturation; the
//! exclusive disabled gate with the 1-frame retry and the sweep's
//! now >= tillFrame re-enable (and the frame > now mask write); the
//! destroy-during-destroy same-pass append; the 256-module erase cap
//! boundary; the heapify rebuild; the grid's cell-size clamp,
//! double-division inverse, zero-extent patch and CEIL counts; the
//! worldToCell floor with the Hoffesommer quirks; the COI bounds (the
//! f32-0.1 CEIL drift included); the small fill's half-cell truncation;
//! the precise circle fill's strict boundary (an exact touch does not
//! cover); the rect fill's half-step fudge with its rotated walk and
//! the degenerate zero-step box; the movement thresholds (0.01); the
//! dirty-list LIFO and status upgrades; the master-list prepend and the
//! refusals; the contact pairs' dedup and LIFO order; the collide-test
//! family — the 3x3 matrix's loc/normal bits, the sphere-sphere touch
//! boundary (the <=) and the coincident zero-normal (the normalize
//! zero-guard), the closeEnough preflight's unwritten loc.x/y and
//! normal (the pinned zero of the original's uninitialized stack
//! local), the rect-circle square promotion (the corner-cap hit a true
//! circle misses, both orders with the flipped normal's -0.0 z), the
//! sphere-slice radii above and below the nonsphere, the
//! geomCollidesWithGeom z-gate that ignores the below-extents (narrower
//! than collidesWith's — the sphere-pair asymmetry), the z-gate
//! boundary itself, the KINDOF_NO_COLLIDE refusal; and
//! processContactList — the dispatch order with the flipped normal and
//! the shared loc, the LIFO pair processing order (the re-dirty
//! cascade's dirty-list order is its observable), the steady-state
//! re-dirty, the NO_COLLISIONS pre-gate, the mid-walk absorb break, the
//! destroyed-absorb (the skipped second call and the refused re-dirty),
//! the unregister mid-processing (the nulled later pairs), the immobile
//! re-dirty refusal, and the z-miss pair (collected but refused).

use std::fmt::Write as _;
use std::path::PathBuf;

use rzh_sim::collide::{
    COLLIDE_TEST_PROCS, CollideInfo, CollideLocAndNormal, geom_collides_with_geom,
};
use rzh_sim::logic::{
    CollideOutcome, Kernel, KernelHost, ObjectSpec, Objects, UpdateModuleSpec, UpdateOutcome,
};
use rzh_sim::partition::Region2;
use rzh_sim::players::{GameInfoSpec, SideSpec, SidesSpec, TeamSpec};
use rzh_sim::query::{
    PartitionFilter, PartitionFilterAcceptByKindOf, PartitionFilterAcceptOnTeam,
    PartitionFilterAlive, PartitionFilterOnMap, PartitionFilterRejectByKindOf,
    PartitionFilterRelationship, PartitionFilterSameMapStatus, PartitionFilterSamePlayer,
    RELATIONSHIP_FILTER_ALLOW_ALLIES, RELATIONSHIP_FILTER_ALLOW_ENEMIES,
    RELATIONSHIP_FILTER_ALLOW_NEUTRAL, SimpleObjectIterator,
};
use rzh_sim::scheduler::ModuleArena;
use rzh_sim::types::{
    Coord3, DisabledMask, DisabledType, FOREVER, GeometryInfo, GeometryType, INVALID_ID,
    KINDOF_NO_COLLIDE_BIT, LOGICFRAMES_PER_MSEC_REAL, LOGICFRAMES_PER_SECOND,
    LOGICFRAMES_PER_SECONDS_REAL, MSEC_PER_LOGICFRAME_REAL, OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT,
    OBJECT_PRIVATE_OFF_MAP_BIT, OBJECT_STATUS_NO_COLLISIONS_BIT, Relationship,
    SECONDS_PER_LOGICFRAME_REAL, SleepyUpdatePhase, UPDATE_SLEEP_FOREVER, UPDATE_SLEEP_NONE,
    WW_SYNC_MILLISECONDS,
};
use rzh_sim::types::{DistanceCalculationType, IterOrderType};

/// The generator's `sideSpec` builder.
fn side_spec(
    name: &str,
    is_human: bool,
    mp_local: bool,
    observer: bool,
    enemies: &[&str],
    allies: &[&str],
) -> SideSpec {
    SideSpec {
        player_name: name.to_string(),
        is_human,
        multiplayer_is_local: mp_local,
        observer,
        enemies: enemies.iter().map(|s| s.to_string()).collect(),
        allies: allies.iter().map(|s| s.to_string()).collect(),
    }
}

/// The generator's `teamSpec` builder.
fn team_spec(name: &str, owner: &str, singleton: bool) -> TeamSpec {
    TeamSpec {
        name: name.to_string(),
        owner: owner.to_string(),
        singleton,
        ..TeamSpec::default()
    }
}

/// The corpus driver: the generator's `World` (the logic, the partition
/// manager, the object arena) plus its dump string.
struct Harness {
    kernel: Kernel,
    host: GoldenHost,
    out: String,
}

/// The scripted module body (the generator's `UpdateModuleT` scripting
/// fields).
#[derive(Clone, Copy)]
struct ScriptedBody {
    sleep: i32,
    wake: Option<(i32, u32, u32)>,
    destroy: Option<i32>,
}

/// The scripted collide-module body (the generator's `CollideModuleT`
/// scripting fields).
#[derive(Clone, Copy, Default)]
struct ScriptedCollideBody {
    absorb: bool,
    destroy: Option<i32>,
    unregister: Option<i32>,
}

/// One collide-module dispatch record (the generator's
/// `GameLogicHooks::CollideEvent`).
#[derive(Clone, Copy)]
struct CollideEvent {
    obj: i32,
    other: i32,
    module: i32,
    loc: Coord3,
    normal: Coord3,
}

/// The golden host: the ordered event streams plus the scripted modules.
struct GoldenHost {
    engine_time_frozen: bool,
    events: Vec<(u8, i64, i64)>,
    scripted: Vec<(i32, u32, ScriptedBody)>,
    collide_events: Vec<CollideEvent>,
    scripted_collide: Vec<(i32, u32, ScriptedCollideBody)>,
    /// The players/teams event stream (the generator's `m_teamEvents`:
    /// the 4-int entries, uniform).
    team_events: Vec<[i64; 4]>,
}

// The event ids (the generator's `GameLogicHooks::Ev`).
const EV_CLIENT_FRAME: u8 = 0;
const EV_SCRIPTS: u8 = 1;
const EV_FROZEN: u8 = 2;
const EV_TERRAIN: u8 = 3;
const EV_RECORDER: u8 = 4;
const EV_COMMANDS: u8 = 5;
const EV_UPDATE: u8 = 6;
const EV_AI: u8 = 7;
const EV_BUILD: u8 = 8;
const EV_PARTITION: u8 = 9;
const EV_DESTROY: u8 = 10;
const EV_COMMANDS_RESET: u8 = 11;
const EV_WEAPONSTORE: u8 = 12;
const EV_LOCOMOTORSTORE: u8 = 13;
const EV_VICTORY: u8 = 14;
const EV_SWEEP: u8 = 15;
const EV_FRAME_INC: u8 = 16;
const EV_STARTNEWGAME: u8 = 17;
const EV_OBJ_CREATED: u8 = 18;
const EV_OBJ_DESTROYED: u8 = 19;
const EV_CELL_CHANGE: u8 = 20;

// The team-event ids (the generator's `GameLogicHooks::TeamEv`).
const TEV_PLAYER_UPDATE: i64 = 0;
const TEV_LOCAL: i64 = 1;
const TEV_TEAM_MEMBER: i64 = 2;
const TEV_CAPTURE: i64 = 3;
const TEV_TEAM_SCRIPT: i64 = 4;
const TEV_TEAM_DESTROYED: i64 = 5;

#[allow(clippy::trivially_copy_pass_by_ref)]
fn tev_name(e: i64) -> &'static str {
    match e {
        TEV_PLAYER_UPDATE => "pupdate",
        TEV_LOCAL => "local",
        TEV_TEAM_MEMBER => "teammember",
        TEV_CAPTURE => "capture",
        TEV_TEAM_SCRIPT => "script",
        TEV_TEAM_DESTROYED => "teamdestroyed",
        _ => "?",
    }
}

fn script_kind_bits(kind: rzh_sim::players::TeamScriptKind) -> i64 {
    kind as i64
}

fn ev_name(e: u8) -> &'static str {
    match e {
        EV_CLIENT_FRAME => "clientframe",
        EV_SCRIPTS => "scripts",
        EV_FROZEN => "frozen",
        EV_TERRAIN => "terrain",
        EV_RECORDER => "recorder",
        EV_COMMANDS => "commands",
        EV_UPDATE => "update",
        EV_AI => "ai",
        EV_BUILD => "build",
        EV_PARTITION => "partition",
        EV_DESTROY => "destroy",
        EV_COMMANDS_RESET => "commandsreset",
        EV_WEAPONSTORE => "weaponstore",
        EV_LOCOMOTORSTORE => "locomotorstore",
        EV_VICTORY => "victory",
        EV_SWEEP => "sweep",
        EV_FRAME_INC => "frameinc",
        EV_STARTNEWGAME => "startnewgame",
        EV_OBJ_CREATED => "objcreated",
        EV_OBJ_DESTROYED => "objdestroyed",
        EV_CELL_CHANGE => "cellchange",
        _ => "?",
    }
}

impl KernelHost for GoldenHost {
    fn is_time_frozen(&mut self) -> bool {
        self.engine_time_frozen
    }

    fn on_start_new_game(&mut self) {
        self.events.push((EV_STARTNEWGAME, -1, -1));
    }

    fn on_client_frame(&mut self, frame: u32) {
        self.events.push((EV_CLIENT_FRAME, i64::from(frame), -1));
    }

    fn on_scripts_update(&mut self) {
        self.events.push((EV_SCRIPTS, -1, -1));
    }

    fn on_frozen_gate(&mut self, frozen: bool) {
        self.events.push((EV_FROZEN, i64::from(frozen), -1));
    }

    fn on_terrain_update(&mut self) {
        self.events.push((EV_TERRAIN, -1, -1));
    }

    fn on_recorder_update(&mut self) {
        self.events.push((EV_RECORDER, -1, -1));
    }

    fn on_commands_update(&mut self) {
        self.events.push((EV_COMMANDS, -1, -1));
    }

    fn on_update_module(&mut self, id: i32, module: u32) {
        self.events
            .push((EV_UPDATE, i64::from(id), i64::from(module)));
    }

    fn on_ai_update(&mut self) {
        self.events.push((EV_AI, -1, -1));
    }

    fn on_build_assistant_update(&mut self) {
        self.events.push((EV_BUILD, -1, -1));
    }

    fn on_partition_update(&mut self) {
        self.events.push((EV_PARTITION, -1, -1));
    }

    fn on_destroy_list(&mut self, count: usize) {
        self.events.push((EV_DESTROY, count as i64, -1));
    }

    fn on_commands_reset(&mut self) {
        self.events.push((EV_COMMANDS_RESET, -1, -1));
    }

    fn on_weapon_store_update(&mut self) {
        self.events.push((EV_WEAPONSTORE, -1, -1));
    }

    fn on_locomotor_store_update(&mut self) {
        self.events.push((EV_LOCOMOTORSTORE, -1, -1));
    }

    fn on_victory_conditions_update(&mut self) {
        self.events.push((EV_VICTORY, -1, -1));
    }

    fn on_disabled_sweep(&mut self) {
        self.events.push((EV_SWEEP, -1, -1));
    }

    fn on_frame_increment(&mut self) {
        self.events.push((EV_FRAME_INC, -1, -1));
    }

    fn on_object_created(&mut self, id: i32) {
        self.events.push((EV_OBJ_CREATED, i64::from(id), -1));
    }

    fn on_object_destroyed(&mut self, id: i32) {
        self.events.push((EV_OBJ_DESTROYED, i64::from(id), -1));
    }

    fn on_partition_cell_change(&mut self, id: i32, cell: i32) {
        self.events
            .push((EV_CELL_CHANGE, i64::from(id), i64::from(cell)));
    }

    fn run_update(&mut self, id: i32, module: u32) -> UpdateOutcome {
        let body = self
            .scripted
            .iter()
            .find(|(s_obj, s_mod, _)| *s_obj == id && *s_mod == module)
            .map(|(_, _, body)| *body);
        let mut outcome = UpdateOutcome::default();
        if let Some(body) = body {
            // the transcription's runUpdate order: destroy, then wake,
            // then the return code
            outcome.destroy = body.destroy;
            outcome.wake = body.wake;
            outcome.sleep = body.sleep;
        }
        outcome
    }

    fn on_collide_module(
        &mut self,
        id: i32,
        module: u32,
        other: i32,
        loc: &Coord3,
        normal: &Coord3,
    ) {
        self.collide_events.push(CollideEvent {
            obj: id,
            other,
            module: module as i32,
            loc: *loc,
            normal: *normal,
        });
    }

    fn run_collide(
        &mut self,
        id: i32,
        module: u32,
        _other: i32,
        _loc: &Coord3,
        _normal: &Coord3,
    ) -> CollideOutcome {
        let body = self
            .scripted_collide
            .iter()
            .find(|(s_obj, s_mod, _)| *s_obj == id && *s_mod == module)
            .map(|(_, _, body)| *body);
        let mut outcome = CollideOutcome::default();
        if let Some(body) = body {
            outcome.absorb = body.absorb;
            outcome.destroy = body.destroy;
            outcome.unregister = body.unregister;
        }
        outcome
    }

    fn on_player_update(&mut self, player: usize) {
        self.team_events
            .push([TEV_PLAYER_UPDATE, player as i64, -1, -1]);
    }

    fn on_becoming_local_player(&mut self, player: usize, yes: bool) {
        self.team_events
            .push([TEV_LOCAL, player as i64, i64::from(yes), -1]);
    }

    fn on_becoming_team_member(&mut self, obj: i32, player: usize, yes: bool) {
        self.team_events.push([
            TEV_TEAM_MEMBER,
            i64::from(obj),
            player as i64,
            i64::from(yes),
        ]);
    }

    fn on_capture(&mut self, obj: i32, old_player: usize, new_player: usize) {
        self.team_events.push([
            TEV_CAPTURE,
            i64::from(obj),
            old_player as i64,
            new_player as i64,
        ]);
    }

    fn on_team_script(&mut self, team: u32, kind: rzh_sim::players::TeamScriptKind) {
        self.team_events
            .push([TEV_TEAM_SCRIPT, i64::from(team), script_kind_bits(kind), -1]);
    }

    fn on_production_script(&mut self, proto: u32) {
        self.team_events.push([
            TEV_TEAM_SCRIPT,
            -1,
            6, // SCRIPT_PRODUCTION_CONDITION
            i64::from(proto),
        ]);
    }

    fn on_team_destroyed(&mut self, team: u32) {
        self.team_events
            .push([TEV_TEAM_DESTROYED, i64::from(team), -1, -1]);
    }
}

impl Harness {
    fn new(cell_size: f32, extents: (f32, f32, f32, f32)) -> Self {
        let region = Region2 {
            lo_x: extents.0,
            lo_y: extents.1,
            hi_x: extents.2,
            hi_y: extents.3,
        };
        let mut host = GoldenHost {
            engine_time_frozen: false,
            events: Vec::new(),
            scripted: Vec::new(),
            collide_events: Vec::new(),
            scripted_collide: Vec::new(),
            team_events: Vec::new(),
        };
        let kernel = Kernel::with_grid(cell_size, region, &mut host);
        Self {
            kernel,
            host,
            out: String::new(),
        }
    }

    fn line(&mut self, text: &str) {
        writeln!(self.out, "{text}").expect("write");
    }

    /// The generator's `createObject` (the ThingFactory::newObject
    /// shape).
    #[allow(clippy::too_many_arguments)]
    fn create_object(
        &mut self,
        geometry: GeometryInfo,
        immobile: bool,
        num_updates: usize,
        ctor_wakes: &[i64],
        pos: (f32, f32),
        angle: f32,
        phases: &[SleepyUpdatePhase],
        colliders: usize,
    ) -> i32 {
        let spec = ObjectSpec {
            geometry,
            immobile,
            modules: (0..num_updates)
                .map(|m| UpdateModuleSpec {
                    phase: phases.get(m).copied().unwrap_or_default(),
                    disabled_types_to_process: DisabledMask::default(),
                    ctor_wake: ctor_wakes
                        .get(m)
                        .copied()
                        .and_then(|w| if w >= 0 { Some(w as u32) } else { None }),
                })
                .collect(),
            colliders,
            position: rzh_sim::types::Coord3 {
                x: pos.0,
                y: pos.1,
                z: 0.0,
            },
            orientation: angle,
        };
        self.kernel.create_object(&spec, &mut self.host)
    }

    fn script_sleep(&mut self, obj: i32, module: u32, sleep: i32) {
        self.host.scripted.push((
            obj,
            module,
            ScriptedBody {
                sleep,
                wake: None,
                destroy: None,
            },
        ));
    }

    fn script_destroy(&mut self, obj: i32, module: u32, victim: i32) {
        // the transcription sets m_onUpdateDestroyObj alone — the
        // scripted sleep (and any wake) stay as they were
        let existing = self
            .host
            .scripted
            .iter()
            .position(|(s_obj, s_mod, _)| *s_obj == obj && *s_mod == module);
        match existing {
            Some(idx) => self.host.scripted[idx].2.destroy = Some(victim),
            None => self.host.scripted.push((
                obj,
                module,
                ScriptedBody {
                    sleep: UPDATE_SLEEP_NONE,
                    wake: None,
                    destroy: Some(victim),
                },
            )),
        }
    }

    fn update(&mut self) {
        self.kernel.update(&mut self.host);
    }

    fn dump_events(&mut self) {
        let count = self.host.events.len();
        self.line(&format!("events {count}"));
        for (e, a, b) in std::mem::take(&mut self.host.events) {
            if a < 0 {
                self.line(&format!("ev {}", ev_name(e)));
            } else if b < 0 {
                self.line(&format!("ev {} {a}", ev_name(e)));
            } else {
                self.line(&format!("ev {} {a} {b}", ev_name(e)));
            }
        }
    }

    fn dump_heap(&mut self) {
        let mut lines = Vec::new();
        let count = self.kernel.logic.scheduler.heap.len();
        lines.push(format!("heap {count}"));
        for &h in &self.kernel.logic.scheduler.heap {
            let state = self.kernel.objects.module(h);
            lines.push(format!(
                "u o{} m{} pri={:08x} idx={}",
                h.obj,
                h.module,
                state.priority(),
                state.index_in_logic
            ));
        }
        for l in lines {
            self.line(&l);
        }
    }

    fn dump_registry(&mut self) {
        let mut lines = Vec::new();
        lines.push(format!("frame {}", self.kernel.logic.frame));
        lines.push(format!("nextid {}", self.kernel.logic.next_obj_id));
        lines.push(format!("objlist {}", self.kernel.logic.obj_list.len()));
        let obj_lines: Vec<String> = self
            .kernel
            .logic
            .obj_list
            .iter()
            .map(|&id| {
                let obj = self.kernel.objects.get(id);
                let part = if obj.partition.is_some() {
                    id
                } else {
                    INVALID_ID
                };
                format!(
                    "o id={} upd={} part={} dis={:04x} cellchg={}",
                    obj.id,
                    obj.updates.len(),
                    part,
                    obj.disabled_mask.bits,
                    obj.cell_change_count
                )
            })
            .collect();
        lines.extend(obj_lines);
        lines.push(format!(
            "destroylist {}",
            self.kernel.logic.objects_to_destroy.len()
        ));
        let destroy_lines: Vec<String> = self
            .kernel
            .logic
            .objects_to_destroy
            .iter()
            .map(|&id| format!("d id={id}"))
            .collect();
        lines.extend(destroy_lines);
        for l in lines {
            self.line(&l);
        }
    }

    fn dump_partition(&mut self) {
        let mut lines = Vec::new();
        let pm = &self.kernel.pm;
        lines.push(format!(
            "grid size={:08x} inv={:08x} {}x{} total={} lo={:08x},{:08x} hi={:08x},{:08x} updated={}",
            pm.cell_size.to_bits(),
            pm.cell_size_inv.to_bits(),
            pm.cell_count_x,
            pm.cell_count_y,
            pm.total_cell_count,
            pm.world_extents.lo_x.to_bits(),
            pm.world_extents.lo_y.to_bits(),
            pm.world_extents.hi_x.to_bits(),
            pm.world_extents.hi_y.to_bits(),
            i32::from(pm.updated_since_last_reset)
        ));

        // the master list order
        lines.push("modules".to_string());
        for &id in &pm.module_list {
            let part = self.kernel.objects.partition_of(id);
            lines.push(format!(
                "m o{} coi={}/{} dirty={} last={} shr={},{},{} seen={},{}",
                id,
                part.coi_in_use,
                part.coi_array.len(),
                part.dirty_status as i32,
                part.last_cell.map_or(-1, |c| c as i32),
                part.shroudedness[0] as i32,
                part.shroudedness[1] as i32,
                part.shroudedness[15] as i32,
                i32::from(part.ever_seen[0]),
                i32::from(part.ever_seen[1])
            ));
        }

        // the dirty list order
        lines.push("dirtylist".to_string());
        for &id in &pm.dirty_modules {
            let part = self.kernel.objects.partition_of(id);
            lines.push(format!("dl o{} status={}", id, part.dirty_status as i32));
        }

        // the per-object COI coverage, in slot order
        for &id in &pm.module_list {
            lines.push(format!("coi o{id}"));
            let part = self.kernel.objects.partition_of(id);
            for slot in 0..part.coi_in_use {
                let cell = part.coi_array[slot].expect("in-use slot");
                let (x, y) = pm.cell_xy(cell);
                lines.push(format!("c {x},{y}"));
            }
        }

        // the per-cell COI lists, in list order (newest first)
        lines.push(format!("cells {}", pm.cells.len()));
        for cell in &pm.cells {
            if cell.cois.is_empty() {
                continue;
            }
            lines.push(format!(
                "cell {},{} n={} shr={},{},{}",
                cell.cell_x,
                cell.cell_y,
                cell.cois.len(),
                cell.shroud_level[0].current_shroud,
                cell.shroud_level[1].current_shroud,
                cell.shroud_level[0].active_shroud_level
            ));
            for &id in &cell.cois {
                lines.push(format!("k o{id}"));
            }
        }
        for l in lines {
            self.line(&l);
        }
    }

    fn dump_contacts(&mut self) {
        let mut lines = Vec::new();
        let count = self.kernel.pm.last_contacts.len();
        lines.push(format!("contacts {count}"));
        for &(a, b) in &self.kernel.pm.last_contacts {
            lines.push(format!("p o{a} o{b}"));
        }
        for l in lines {
            self.line(&l);
        }
    }

    /// The generator's `dumpCollideEvents` (the collide-module dispatch
    /// stream, in dispatch order, with the loc and normal bits).
    fn dump_collide_events(&mut self) {
        let mut lines = Vec::new();
        let count = self.host.collide_events.len();
        lines.push(format!("collideevents {count}"));
        for e in std::mem::take(&mut self.host.collide_events) {
            lines.push(format!(
                "cv o{} o{} m{} loc={:08x},{:08x},{:08x} nrm={:08x},{:08x},{:08x}",
                e.obj,
                e.other,
                e.module,
                e.loc.x.to_bits(),
                e.loc.y.to_bits(),
                e.loc.z.to_bits(),
                e.normal.x.to_bits(),
                e.normal.y.to_bits(),
                e.normal.z.to_bits()
            ));
        }
        for l in lines {
            self.line(&l);
        }
    }

    /// The generator's `createObject` with the full position (the collide
    /// corpora's z-carrying objects; no update modules, no wakes).
    fn create_colliding(
        &mut self,
        geometry: GeometryInfo,
        immobile: bool,
        pos: (f32, f32, f32),
        colliders: usize,
    ) -> i32 {
        let spec = ObjectSpec {
            geometry,
            immobile,
            modules: Vec::new(),
            colliders,
            position: Coord3 {
                x: pos.0,
                y: pos.1,
                z: pos.2,
            },
            orientation: 0.0,
        };
        self.kernel.create_object(&spec, &mut self.host)
    }

    /// The generator's `CollideModuleT` scripting: the absorb.
    fn script_collide_absorb(&mut self, obj: i32, module: u32) {
        self.host.scripted_collide.push((
            obj,
            module,
            ScriptedCollideBody {
                absorb: true,
                destroy: None,
                unregister: None,
            },
        ));
    }

    /// The generator's `CollideModuleT` scripting: the destroy request.
    fn script_collide_destroy(&mut self, obj: i32, module: u32, victim: i32) {
        self.host.scripted_collide.push((
            obj,
            module,
            ScriptedCollideBody {
                absorb: false,
                destroy: Some(victim),
                unregister: None,
            },
        ));
    }

    /// The generator's `CollideModuleT` scripting: the unregister request.
    fn script_collide_unregister(&mut self, obj: i32, module: u32, victim: i32) {
        self.host.scripted_collide.push((
            obj,
            module,
            ScriptedCollideBody {
                absorb: false,
                destroy: None,
                unregister: Some(victim),
            },
        ));
    }

    /// The generator's `freshWorld`: a brand-new World on the same dumper
    /// (the sub-corpora with their own grids).
    fn fresh_world(&mut self, cell_size: f32, extents: (f32, f32, f32, f32)) {
        let region = Region2 {
            lo_x: extents.0,
            lo_y: extents.1,
            hi_x: extents.2,
            hi_y: extents.3,
        };
        self.host = GoldenHost {
            engine_time_frozen: false,
            events: Vec::new(),
            scripted: Vec::new(),
            collide_events: Vec::new(),
            scripted_collide: Vec::new(),
            team_events: Vec::new(),
        };
        self.kernel = Kernel::with_grid(cell_size, region, &mut self.host);
    }

    /// The generator's z-carrying, angle-carrying object creation (no
    /// update modules, no colliders — the query corpora's objects).
    fn create_query_obj(&mut self, geometry: GeometryInfo, pos: (f32, f32, f32)) -> i32 {
        self.create_query_obj_angle(geometry, pos, 0.0)
    }

    // ---- the players/teams block's dump helpers (the generator's
    // dumpTeamEvents / dumpPlayers / dumpTeams / dumpObjectTeams). ----

    /// `dumpTeamEvents`: the uniform 4-int entries (the script events
    /// name their kind; the production event's 4th is the prototype id).
    fn dump_team_events(&mut self) {
        let n = self.host.team_events.len();
        self.line(&format!("teamevents {n}"));
        let events = std::mem::take(&mut self.host.team_events);
        for e in &events {
            if e[0] == TEV_TEAM_SCRIPT {
                // the kind ordinal → the name (the generator's
                // scriptKindName)
                let kind = match e[2] {
                    0 => "oncreate",
                    1 => "ondestroyed",
                    2 => "onidle",
                    3 => "onenemysighted",
                    4 => "onallclear",
                    5 => "onunitdestroyed",
                    _ => "production",
                };
                self.line(&format!("tv script {} {} {}", e[1], kind, e[3]));
            } else {
                self.line(&format!("tv {} {} {} {}", tev_name(e[0]), e[1], e[2], e[3]));
            }
        }
    }

    /// `dumpPlayers`: the count, the local player, every slot (all 16),
    /// the relation maps in key order, the default team, the prototype
    /// list and the slot bookkeeping.
    fn dump_players(&mut self) {
        // (the kernel read and the output buffer are disjoint fields, so
        // both borrows coexist)
        let players = &self.kernel.players;
        let out = &mut self.out;
        let count = players.list.count;
        let local = players.list.local.map(|p| p as i32).unwrap_or(-1);
        let _ = writeln!(out, "players count={count} local={local}");
        for i in 0..rzh_sim::types::MAX_PLAYER_COUNT {
            let p = &players.list.players[i];
            let _ = writeln!(
                out,
                "pl {} name={} key={} type={} dead={} obs={} act={} dflt={} slot={} npr={}",
                i,
                p.name,
                p.name_key.ordinal(),
                p.player_type.bits(),
                i32::from(p.is_player_dead),
                i32::from(p.observer),
                i32::from(p.is_player_active()),
                p.default_team,
                players.get_slot_index(i),
                p.team_prototypes.len()
            );
            for (&k, &r) in &p.player_relations {
                let _ = writeln!(out, "pr {i}->{k} {}", r.name());
            }
            for (&k, &r) in &p.team_relations {
                let _ = writeln!(out, "tr {i}->t{k} {}", r.name());
            }
            for (j, &slot) in p.team_prototypes.iter().enumerate() {
                let proto_id = players.factory.protos[slot]
                    .as_ref()
                    .map(|tp| tp.id)
                    .unwrap_or(0);
                let _ = writeln!(out, "pt {i} {j} tp{proto_id}");
            }
        }
        for s in 0..rzh_sim::types::MAX_SLOTS {
            let _ = writeln!(
                out,
                "slot {s} p{}",
                players.get_player_index_from_slot_index(s as i32)
            );
        }
    }

    /// `dumpTeams`: the unique-id counters, the prototype map in KEY
    /// order, each prototype's template flags, and each live team's
    /// state + members (newest first).
    fn dump_teams(&mut self) {
        let players = &self.kernel.players;
        let out = &mut self.out;
        let _ = writeln!(
            out,
            "teams nextproto={} nextid={} protos={}",
            players.factory.unique_team_proto_id,
            players.factory.unique_team_id,
            players.factory.map.len()
        );
        for (&key, &slot) in &players.factory.map {
            let tp = players.factory.protos[slot].as_ref().expect("live proto");
            let _ = writeln!(
                out,
                "tp {} name={} key={} owner={} single={} inst={}",
                tp.id,
                tp.name,
                key.ordinal(),
                tp.owner,
                i32::from(tp.singleton),
                tp.instances.len()
            );
            let info = &tp.template_;
            let _ = writeln!(
                out,
                "tpi {} oc={} od={} oi={} oes={} oac={} oud={} thr={:08x} ea={} pc={} pe={}",
                tp.id,
                i32::from(!info.script_on_create.is_empty()),
                i32::from(!info.script_on_destroyed.is_empty()),
                i32::from(!info.script_on_idle.is_empty()),
                i32::from(!info.script_on_enemy_sighted.is_empty()),
                i32::from(!info.script_on_all_clear.is_empty()),
                i32::from(!info.script_on_unit_destroyed.is_empty()),
                info.destroyed_threshold.to_bits(),
                i32::from(info.execute_actions),
                info.production_condition,
                i32::from(info.production_script_exists)
            );
            for &ts in tp.instances.iter().rev() {
                let t = players.factory.teams[ts].as_ref().expect("live team");
                let st = &t.state;
                let _ = writeln!(
                    out,
                    "t {} tp{} act={} cre={} eo={} see={} psee={} chk={} cur={} thr={} idle={} mem={}",
                    t.id,
                    players.factory.protos[t.proto]
                        .as_ref()
                        .expect("live proto")
                        .id,
                    i32::from(st.active),
                    i32::from(st.created),
                    i32::from(st.entered_or_exited),
                    i32::from(st.see_enemy),
                    i32::from(st.prev_see_enemy),
                    i32::from(st.check_enemy_sighted),
                    st.cur_units,
                    st.destroy_threshold,
                    i32::from(st.was_idle),
                    t.members.len()
                );
                for (&k, &r) in &t.team_relations {
                    let _ = writeln!(out, "tot t{}->t{k} {}", t.id, r.name());
                }
                for (&k, &r) in &t.player_relations {
                    let _ = writeln!(out, "top t{}->{k} {}", t.id, r.name());
                }
                for &m in t.members.iter().rev() {
                    let _ = writeln!(out, "mem t{} o{}", t.id, m);
                }
            }
        }
    }

    /// `dumpObjectTeams`: the object→team attachment state (the arena
    /// order).
    fn dump_object_teams(&mut self) {
        let ids: Vec<i32> = self.kernel.objects.slot_ids();
        let out = &mut self.out;
        let _ = writeln!(out, "objteams {}", ids.len());
        for id in ids {
            let o = self.kernel.objects.get(id);
            let ctrl = self
                .kernel
                .players
                .controlling_player_of(&self.kernel.objects, id)
                .map(|p| p as i32)
                .unwrap_or(-1);
            let _ = writeln!(
                out,
                "ot o{} t{} ctrl={} vis={:08x} ai={}/{}",
                o.id,
                o.team,
                ctrl,
                o.vision_range.to_bits(),
                i32::from(o.has_ai),
                i32::from(o.ai_idle)
            );
        }
    }

    fn create_query_obj_angle(
        &mut self,
        geometry: GeometryInfo,
        pos: (f32, f32, f32),
        angle: f32,
    ) -> i32 {
        let spec = ObjectSpec {
            geometry,
            immobile: false,
            modules: Vec::new(),
            colliders: 0,
            position: Coord3 {
                x: pos.0,
                y: pos.1,
                z: pos.2,
            },
            orientation: angle,
        };
        self.kernel.create_object(&spec, &mut self.host)
    }

    /// The generator's `drainIter`: the pre-reset next (the fresh
    /// cursor's null — the original's callers must first()/reset()), the
    /// reset, then the full drain with numerics.
    fn drain_iter(&mut self, iter: &mut SimpleObjectIterator, tag: &str) {
        let (pre_obj, pre_num) = iter.next_with_numeric();
        self.line(&format!("{tag} pre {} {:08x}", pre_obj, pre_num.to_bits()));
        iter.reset();
        let mut count = 0;
        loop {
            let (o, num) = iter.next_with_numeric();
            if o == INVALID_ID {
                break;
            }
            self.line(&format!("{tag} n o{o} {:08x}", num.to_bits()));
            count += 1;
        }
        self.line(&format!("{tag} count {count}"));
    }

    /// The generator's `dumpRadiusVec`.
    fn dump_radius_vec(&mut self) {
        let mut lines = Vec::new();
        let pm = &self.kernel.pm;
        lines.push(format!(
            "maxgco {} rings {}",
            pm.max_gco_radius,
            pm.radius_vec.len()
        ));
        for (r, v) in pm.radius_vec.iter().enumerate() {
            lines.push(format!("ring {r} n {}", v.len()));
            for &(x, y) in v {
                lines.push(format!("o {x},{y}"));
            }
        }
        for l in lines {
            self.line(&l);
        }
    }

    /// The store refresh (the query corpora's `w.pm.update(&w.hooks)`).
    fn refresh_store(&mut self) {
        self.kernel.pm.update(
            &mut self.kernel.objects,
            &mut self.kernel.logic,
            &mut self.host,
        );
    }
}

/// The `Objects::partition` accessor (the harness side).
trait PartitionOf {
    fn partition_of(&self, id: i32) -> &rzh_sim::partition::PartitionData;
}

impl PartitionOf for Objects {
    fn partition_of(&self, id: i32) -> &rzh_sim::partition::PartitionData {
        self.get(id).partition.as_ref().expect("partition data")
    }
}

/// The `theCollideTestProcs` matrix entry `(this, that)` — the original's
/// `(geom - GEOMETRY_FIRST) * GEOMETRY_NUM_TYPES + (that - GEOMETRY_FIRST)`
/// indexing, kept legible at the one-off call sites.
fn matrix_proc(this: usize, that: usize) -> rzh_sim::collide::CollideTestProc {
    COLLIDE_TEST_PROCS[this * 3 + that]
}

fn f32b(x: f32) -> u32 {
    x.to_bits()
}

fn geom(
    geom_type: GeometryType,
    is_small: bool,
    height: f32,
    major: f32,
    minor: f32,
) -> GeometryInfo {
    GeometryInfo {
        geom_type,
        is_small,
        height,
        major_radius: major,
        minor_radius: minor,
    }
}

// ============================================================================
// The corpora (the generator's, one for one).
// ============================================================================

fn corpus_ids(h: &mut Harness) {
    h.line("corpus kernel_ids");
    // the frame constants (the WP1 "30 Hz, to verify" item)
    h.line(&format!(
        "lfps {} msync {}",
        LOGICFRAMES_PER_SECOND, WW_SYNC_MILLISECONDS
    ));
    h.line(&format!(
        "reals {:08x} {:08x} {:08x} {:08x}",
        f32b(LOGICFRAMES_PER_MSEC_REAL),
        f32b(MSEC_PER_LOGICFRAME_REAL),
        f32b(LOGICFRAMES_PER_SECONDS_REAL),
        f32b(SECONDS_PER_LOGICFRAME_REAL)
    ));
    let g = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);
    let alloc1 = h.kernel.logic.allocate_object_id();
    h.line(&format!("alloc1 {alloc1}"));
    h.line(&format!("nextid {}", h.kernel.logic.next_obj_id));
    // the init reset (GameLogic::init: m_nextObjID = 1)
    h.kernel.logic.set_object_id_counter(1);
    let a = h.create_object(g, false, 0, &[], (0.0, 0.0), 0.0, &[], 0);
    let b = h.create_object(g, false, 0, &[], (0.0, 0.0), 0.0, &[], 0);
    let c = h.create_object(g, false, 0, &[], (0.0, 0.0), 0.0, &[], 0);
    h.line(&format!("ids {a} {b} {c}"));
    // destroy b (frame 0), then allocate — no reuse
    h.kernel.destroy_object(b, &mut h.host);
    h.kernel.process_destroy_list(&mut h.host);
    h.line(&format!(
        "afterdestroy nextid {} list {}",
        h.kernel.logic.next_obj_id,
        h.kernel.logic.obj_list.len()
    ));
    let e = h.kernel.logic.allocate_object_id();
    h.line(&format!("alloc4 {e}"));
    // the setObjectIDCounter mid-flight write (the save/load hook)
    h.kernel.logic.set_object_id_counter(100);
    let alloc5 = h.kernel.logic.allocate_object_id();
    h.line(&format!("alloc5 {alloc5}"));
    // the compaction: max(id)+1 walked from INVALID_ID
    h.kernel.logic.set_object_id_counter(500);
    h.kernel.load_post_process();
    h.line(&format!("compacted {}", h.kernel.logic.next_obj_id));
    // the wrap: the counter at 0x7fffffff wraps into the negative range
    h.kernel.logic.set_object_id_counter(0x7fff_ffff);
    let wrapalloc = h.kernel.logic.allocate_object_id();
    h.line(&format!("wrapalloc {wrapalloc}"));
    h.line(&format!("wrapnext {}", h.kernel.logic.next_obj_id));
    h.dump_events();
    h.line("end");
}

fn corpus_frame(h: &mut Harness) {
    h.line("corpus kernel_frame");
    let g = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);
    h.create_object(g, false, 1, &[], (0.0, 0.0), 0.0, &[], 0);

    // a plain frame: the full ordered walk
    h.kernel.pre_update();
    h.update();
    h.line(&format!(
        "f1 frame {} hasupd {}",
        h.kernel.frame(),
        i32::from(h.kernel.logic.has_updated)
    ));
    h.dump_events();

    // the freeze gate: scripts froze the engine
    h.host.engine_time_frozen = true;
    h.kernel.pre_update();
    h.update();
    h.line(&format!("f2 frame {}", h.kernel.frame()));
    h.dump_events();
    h.host.engine_time_frozen = false;

    // the startNewGame gate: the kick runs, the frame still advances
    h.kernel.request_new_game();
    h.kernel.pre_update();
    h.update();
    h.line(&format!("f3 frame {}", h.kernel.frame()));
    h.dump_events();

    // the pause-frame transition
    h.kernel.set_pause_frame(h.kernel.frame() + 1);
    h.kernel.pre_update();
    h.update();
    h.kernel.pre_update();
    h.line(&format!(
        "paused {} pauseFrame {}",
        i32::from(h.kernel.game_paused()),
        h.kernel.logic.pause_frame
    ));
    h.line("end");
}

fn corpus_sched_basic(h: &mut Harness) {
    h.line("corpus sched_basic");
    let g = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);

    // object born at frame 0: the modules' initial wake clamps to 1
    let a = h.create_object(g, false, 2, &[], (0.0, 0.0), 0.0, &[], 0);
    // object born with constructor wakes (the not-in-list path)
    let b = h.create_object(g, false, 2, &[3, 5], (0.0, 0.0), 0.0, &[], 0);
    h.dump_heap();

    h.script_sleep(a, 0, 1);
    h.script_sleep(a, 1, 7);
    h.script_sleep(b, 0, 3);
    h.script_sleep(b, 1, UPDATE_SLEEP_FOREVER);

    for _ in 0..6 {
        h.kernel.pre_update();
        h.update();
        h.dump_events();
    }
    h.dump_heap();
    h.dump_registry();
    h.line("end");
}

fn corpus_sched_ties(h: &mut Harness) {
    h.line("corpus sched_ties");
    let g = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);

    let a = h.create_object(g, false, 5, &[], (0.0, 0.0), 0.0, &[], 0);
    h.dump_heap();
    for m in 0..5 {
        h.script_sleep(a, m, 2);
    }

    h.update(); // 0 → 1 (nothing runs)
    h.update(); // 1 → 2: the five run here
    h.dump_events();
    h.dump_heap();
    h.update(); // 2 → 3 (nothing)
    h.update(); // 3 → 4: the five run again
    h.dump_events();
    h.dump_heap();
    h.line("end");
}

fn corpus_sched_phases(h: &mut Harness) {
    h.line("corpus sched_phases");
    let g = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);

    let phases = [
        SleepyUpdatePhase::Final,
        SleepyUpdatePhase::Initial,
        SleepyUpdatePhase::Normal,
        SleepyUpdatePhase::Physics,
    ];
    let a = h.create_object(g, false, 4, &[], (0.0, 0.0), 0.0, &phases, 0);
    for m in 0..4 {
        h.script_sleep(a, m, 5);
    }

    h.update(); // 0 → 1
    h.update(); // 1 → 2
    for m in 0..4 {
        h.kernel.set_wake_frame(a, m, 1);
    }
    h.dump_heap();
    h.update(); // 2 → 3: all four run, in phase order
    h.dump_events();
    h.dump_heap();
    h.line("end");
}

fn corpus_sched_awaken(h: &mut Harness) {
    h.line("corpus sched_awaken");
    let g = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);
    let a = h.create_object(g, false, 3, &[], (0.0, 0.0), 0.0, &[], 0);
    for m in 0..3 {
        h.script_sleep(a, m, 10);
    }

    h.update(); // frame 0 → 1
    h.dump_heap();

    let now = h.kernel.frame();
    // the easy return: waking exactly at the current wake frame
    h.kernel.set_wake_frame(a, 0, 10);
    h.line(&format!(
        "m0frame {}",
        h.kernel
            .objects
            .module(rzh_sim::scheduler::ModuleHandle { obj: a, module: 0 })
            .next_call_frame()
    ));
    // the subtle case: nextCallFrame == now && when == now + 1 → refusal
    h.kernel.awaken_module(a, 1, now);
    h.line(&format!(
        "m1frame {}",
        h.kernel
            .objects
            .module(rzh_sim::scheduler::ModuleHandle { obj: a, module: 1 })
            .next_call_frame()
    ));
    h.kernel.awaken_module(a, 1, now.wrapping_add(1)); // refused
    h.line(&format!(
        "m1frame {}",
        h.kernel
            .objects
            .module(rzh_sim::scheduler::ModuleHandle { obj: a, module: 1 })
            .next_call_frame()
    ));
    // a wake further out: accepted, rebalanced
    h.kernel.awaken_module(a, 2, now + 30);
    h.dump_heap();
    // the saturation: waking past FOREVER clamps to FOREVER
    h.kernel.awaken_module(a, 0, 0x7fff_fff0);
    h.line(&format!(
        "m0pri {:08x}",
        h.kernel
            .objects
            .module(rzh_sim::scheduler::ModuleHandle { obj: a, module: 0 })
            .priority()
    ));
    // frameToSleepTime: the min-of-four and the past-frame fallback
    h.line(&format!(
        "fts {}",
        h.kernel.logic.frame_to_sleep_time(5, 9, 3, 7)
    ));
    h.line(&format!(
        "fts {}",
        h.kernel
            .logic
            .frame_to_sleep_time(now, FOREVER, FOREVER, FOREVER)
    ));
    h.line(&format!(
        "fts {}",
        h.kernel
            .logic
            .frame_to_sleep_time(now.wrapping_sub(1), FOREVER, FOREVER, FOREVER)
    ));
    h.line("end");
}

fn corpus_sched_disabled(h: &mut Harness) {
    h.line("corpus sched_disabled");
    let g = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);
    let a = h.create_object(g, false, 2, &[1, 1], (50.0, 50.0), 0.0, &[], 0);
    h.script_sleep(a, 0, 4);
    h.script_sleep(a, 1, 4);
    // module 1 processes EMP-disabled objects; module 0 processes nothing
    let mut emp = DisabledMask::default();
    emp.set(DisabledType::Emp, true);
    h.kernel.objects.get_mut(a).updates[1].disabled_types_to_process = emp;

    // disable by EMP for 3 frames (from frame 0: mask set, till 3)
    h.kernel.set_disabled_until(a, DisabledType::Emp, 3);
    h.line(&format!(
        "dis {:04x} till {}",
        h.kernel.objects.get(a).disabled_mask.bits,
        h.kernel.objects.get(a).disabled_till_frame[DisabledType::Emp.index()]
    ));

    h.update(); // frame 1
    h.dump_events();
    h.dump_heap();
    h.update(); // frame 2
    h.dump_events();
    h.update(); // frame 3: the sweep clears
    h.dump_events();
    h.line(&format!(
        "dis {:04x}",
        h.kernel.objects.get(a).disabled_mask.bits
    ));
    h.dump_heap();
    h.update(); // frame 4: module 0 runs again
    h.dump_events();
    h.update(); // frame 5: module 1 runs too
    h.dump_events();

    // setDisabledUntil at now: the mask stays clear
    h.kernel
        .set_disabled_until(a, DisabledType::Held, h.kernel.frame());
    h.line(&format!(
        "held {:04x}",
        h.kernel.objects.get(a).disabled_mask.bits
    ));
    // an unknown type index: refused (the port's enum makes this
    // unrepresentable; the original's bounds check pins it — the value
    // is the unchanged mask)
    h.line(&format!(
        "bad {:04x}",
        h.kernel.objects.get(a).disabled_mask.bits
    ));
    h.line("end");
}

fn corpus_sched_destroy(h: &mut Harness) {
    h.line("corpus sched_destroy");
    let g = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);

    let a = h.create_object(g, false, 3, &[], (50.0, 50.0), 0.0, &[], 0);
    let b = h.create_object(g, false, 3, &[], (60.0, 60.0), 0.0, &[], 0);
    let c = h.create_object(g, false, 3, &[], (70.0, 70.0), 0.0, &[], 0);
    for obj in [a, b, c] {
        for m in 0..3 {
            h.script_sleep(obj, m, 100);
        }
    }
    // stagger the wakes so the heap interleaves them
    h.kernel.awaken_module(a, 0, 5);
    h.kernel.awaken_module(b, 0, 4);
    h.kernel.awaken_module(c, 0, 6);
    h.kernel.awaken_module(b, 1, 3);
    h.kernel.awaken_module(a, 1, 7);
    h.dump_heap();

    // destroy b: the destroy list processes at the next update's tail
    h.kernel.destroy_object(b, &mut h.host);
    h.update();
    h.dump_events();
    h.dump_heap();
    h.dump_registry();
    h.dump_partition();

    // destroy-during-destroy: a's module 0 requests c's destruction
    h.script_destroy(a, 0, c);
    h.kernel.awaken_module(a, 0, h.kernel.frame());
    h.update();
    h.dump_events();
    h.dump_registry();
    h.dump_heap();
    h.line("end");
}

fn corpus_sched_destroy_cap(h: &mut Harness) {
    h.line("corpus sched_destroy_cap");
    let g = geom(GeometryType::Sphere, true, 0.0, 0.0, 0.0);
    let a = h.create_object(g, false, 256, &[], (50.0, 50.0), 0.0, &[], 0);
    for m in 0..256 {
        h.script_sleep(a, m, 100);
    }
    h.line(&format!("heap {}", h.kernel.logic.scheduler.heap.len()));
    h.kernel.destroy_object(a, &mut h.host);
    h.update();
    h.line(&format!("heap {}", h.kernel.logic.scheduler.heap.len()));
    h.dump_registry();
    h.line("end");
}

fn corpus_sched_loadpost(h: &mut Harness) {
    h.line("corpus sched_loadpost");
    let g = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);
    let a = h.create_object(g, false, 3, &[], (50.0, 50.0), 0.0, &[], 0);
    let b = h.create_object(g, false, 2, &[], (60.0, 60.0), 0.0, &[], 0);
    // run a few frames to shuffle the heap (the default sleep-1 bodies)
    for _ in 0..3 {
        h.update();
        h.host.events.clear();
    }
    // set raw wake frames as a save would load them
    h.kernel.awaken_module(a, 0, 40);
    h.kernel.awaken_module(a, 1, 1);
    h.kernel.awaken_module(a, 2, 12);
    h.kernel.awaken_module(b, 0, 9);
    h.kernel.awaken_module(b, 1, 2);
    h.dump_heap();
    // the legacy zero frame (a save with when == 0)
    h.kernel.objects.get_mut(a).updates[1].set_next_call_frame(0);
    h.kernel.objects.get_mut(b).updates[1].set_next_call_frame(0);
    h.kernel.load_post_process();
    h.line(&format!(
        "after loadpost nextid {}",
        h.kernel.logic.next_obj_id
    ));
    h.dump_heap();
    h.line("end");
}

fn corpus_part_grid(h: &mut Harness) {
    h.line("corpus part_grid");
    {
        h.dump_partition();
        let (cx, cy) = h.kernel.pm.world_to_cell(55.0, 55.0);
        h.line(&format!("wtc {cx} {cy}"));
        let (cx, cy) = h.kernel.pm.world_to_cell(-0.001, 0.0);
        h.line(&format!("wtc {cx} {cy}"));
        let (cx, cy) = h.kernel.pm.world_to_cell(99.999, 100.0);
        h.line(&format!("wtc {cx} {cy}"));
        h.line(&format!(
            "wtd {} {} {}",
            h.kernel.pm.world_to_cell_dist(0.0),
            h.kernel.pm.world_to_cell_dist(10.0),
            h.kernel.pm.world_to_cell_dist(10.001)
        ));
        h.line(&format!(
            "cellat {} {} {} {}",
            i32::from(h.kernel.pm.cell_at(0, 0).is_some()),
            i32::from(h.kernel.pm.cell_at(9, 9).is_some()),
            i32::from(h.kernel.pm.cell_at(10, 0).is_some()),
            i32::from(h.kernel.pm.cell_at(-1, 0).is_some())
        ));
    }
    {
        // the cell-size clamp (< 1 → 1) and the zero-extent patch
        let mut h2 = Harness::new(0.5, (0.0, 0.0, 0.0, 0.0));
        h2.dump_partition();
        h.out.push_str(&h2.out);
    }
    {
        // the double-division cellSizeInv: 1/3 in f64 then narrowed
        let mut h2 = Harness::new(3.0, (2.0, 5.0, 32.0, 35.0));
        h2.dump_partition();
        let (cx, cy) = h2.kernel.pm.world_to_cell(2.0, 5.0);
        h2.line(&format!("wtc {cx} {cy}"));
        let (cx, cy) = h2.kernel.pm.world_to_cell(31.9999, 34.9999);
        h2.line(&format!("wtc {cx} {cy}"));
        h.out.push_str(&h2.out);
    }
    {
        // shutdown clears everything
        let mut h2 = Harness::new(10.0, (0.0, 0.0, 100.0, 100.0));
        h2.kernel.pm.shutdown();
        h2.dump_partition();
        h.out.push_str(&h2.out);
    }
    h.line("end");
}

fn corpus_part_maxcoi(h: &mut Harness) {
    h.line("corpus part_maxcoi");
    h.line(&format!(
        "sph {} {} {} {}",
        h.kernel
            .pm
            .calc_max_coi_for_shape(GeometryType::Sphere.bits(), 1.0, 0.0),
        h.kernel
            .pm
            .calc_max_coi_for_shape(GeometryType::Sphere.bits(), 5.0, 0.0),
        h.kernel
            .pm
            .calc_max_coi_for_shape(GeometryType::Cylinder.bits(), 12.5, 0.0),
        h.kernel
            .pm
            .calc_max_coi_for_shape(GeometryType::Sphere.bits(), 0.0, 0.0),
    ));
    h.line(&format!(
        "box {} {}",
        h.kernel
            .pm
            .calc_max_coi_for_shape(GeometryType::Box.bits(), 30.0, 40.0),
        h.kernel
            .pm
            .calc_max_coi_for_shape(GeometryType::Box.bits(), 1.0, 1.0),
    ));
    h.line(&format!(
        "bad {}",
        h.kernel.pm.calc_max_coi_for_shape(99, 5.0, 5.0)
    ));
    h.line("end");
}

fn corpus_part_small(h: &mut Harness) {
    h.line("corpus part_small");
    // a small object centered mid-cell
    h.create_object(
        geom(GeometryType::Sphere, true, 1.0, 2.5, 2.5),
        false,
        0,
        &[],
        (51.0, 52.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // straddling a corner: four cells
    h.create_object(
        geom(GeometryType::Sphere, true, 1.0, 4.9, 4.9),
        false,
        0,
        &[],
        (50.0, 50.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // the truncation: a small radius beyond the half-cell clamps down
    h.create_object(
        geom(GeometryType::Box, true, 1.0, 9.0, 9.0),
        false,
        0,
        &[],
        (25.0, 25.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // off-grid center: the cells clip at the boundary
    h.create_object(
        geom(GeometryType::Sphere, true, 1.0, 3.0, 3.0),
        false,
        0,
        &[],
        (1.0, 1.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();
    h.line("end");
}

fn corpus_part_circle(h: &mut Harness) {
    h.line("corpus part_circle");
    // radius 0: the strict overlap fails — no coverage at all
    h.create_object(
        geom(GeometryType::Cylinder, false, 1.0, 0.0, 0.0),
        false,
        0,
        &[],
        (55.0, 55.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // an exact boundary touch: distance == radius → strict < fails
    h.create_object(
        geom(GeometryType::Sphere, false, 1.0, 5.0, 5.0),
        false,
        0,
        &[],
        (50.0, 55.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // a mid radius spanning several cells, plus off-grid clipping
    h.create_object(
        geom(GeometryType::Sphere, false, 1.0, 7.3, 7.3),
        false,
        0,
        &[],
        (17.0, 13.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // a big radius (>= 20: the Stubbjax bugfix range)
    h.create_object(
        geom(GeometryType::Sphere, false, 1.0, 24.5, 24.5),
        false,
        0,
        &[],
        (50.0, 50.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();
    h.line("end");
}

fn corpus_part_rect(h: &mut Harness) {
    h.line("corpus part_rect");
    // axis-aligned: halfsizes exactly on the step grid
    h.create_object(
        geom(GeometryType::Box, false, 1.0, 5.0, 5.0),
        false,
        0,
        &[],
        (55.0, 55.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // rotated 45 degrees
    h.create_object(
        geom(GeometryType::Box, false, 1.0, 10.0, 10.0),
        false,
        0,
        &[],
        (50.0, 50.0),
        std::f32::consts::FRAC_PI_4,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // a tiny box: one step per axis → exactly the center cell
    h.create_object(
        geom(GeometryType::Box, false, 1.0, 1.0, 1.0),
        false,
        0,
        &[],
        (55.0, 55.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // a degenerate box: halfsize 0 → zero steps → no cells at all
    h.create_object(
        geom(GeometryType::Box, false, 1.0, 0.0, 0.0),
        false,
        0,
        &[],
        (55.0, 55.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // a long thin rotated box spanning the grid edge
    h.create_object(
        geom(GeometryType::Box, false, 1.0, 23.0, 3.0),
        false,
        0,
        &[],
        (12.0, 12.0),
        0.3,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // halfsize just over a step multiple (the fudge)
    h.create_object(
        geom(GeometryType::Box, false, 1.0, 2.6, 2.6),
        false,
        0,
        &[],
        (65.0, 65.0),
        0.0,
        &[],
        0,
    );
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();
    h.line("end");
}

fn corpus_part_dirty(h: &mut Harness) {
    h.line("corpus part_dirty");
    let g = geom(GeometryType::Sphere, false, 1.0, 3.0, 3.0);
    let a = h.create_object(g, false, 0, &[], (55.0, 55.0), 0.0, &[], 0);
    let b = h.create_object(g, false, 0, &[], (35.0, 35.0), 0.0, &[], 0);
    h.dump_partition(); // both dirty, LIFO order

    // the half-update path: makeDirty(false) on a clean module
    h.kernel.pm.make_dirty(&mut h.kernel.objects, a, false);
    h.dump_partition();

    // the upgrade path
    h.kernel.pm.make_dirty(&mut h.kernel.objects, a, true);
    h.dump_partition();

    // the refresh: LIFO processing (b then a)
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_events();
    h.dump_partition();

    // a re-dirty while clean, and the idempotent re-dirty while in-list
    h.kernel.pm.make_dirty(&mut h.kernel.objects, a, true);
    h.kernel.pm.make_dirty(&mut h.kernel.objects, a, true);
    h.dump_partition();
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);

    // movement thresholds: below 0.01 no dirty; above it dirty
    let old = h.kernel.objects.get(a).position;
    h.kernel.objects.get_mut(a).position = rzh_sim::types::Coord3 {
        x: 55.005,
        y: 55.0,
        z: 0.0,
    };
    h.kernel
        .react_to_transform_change(a, old, h.kernel.objects.get(a).orientation);
    h.line(&format!(
        "dirty-after-tiny {}",
        i32::from(!h.kernel.pm.dirty_modules.is_empty())
    ));
    h.kernel.objects.get_mut(a).position = rzh_sim::types::Coord3 {
        x: 56.0,
        y: 55.0,
        z: 0.0,
    };
    h.kernel
        .react_to_transform_change(a, old, h.kernel.objects.get(a).orientation);
    h.line(&format!(
        "dirty-after-move {}",
        i32::from(!h.kernel.pm.dirty_modules.is_empty())
    ));
    // the angle threshold
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    let old = h.kernel.objects.get(a).position;
    h.kernel.objects.get_mut(a).orientation = 0.005;
    h.kernel.react_to_transform_change(a, old, 0.0);
    h.line(&format!(
        "dirty-after-tiny-angle {}",
        i32::from(!h.kernel.pm.dirty_modules.is_empty())
    ));
    h.kernel.objects.get_mut(a).orientation = 0.02;
    h.kernel.react_to_transform_change(a, old, 0.0);
    h.line(&format!(
        "dirty-after-angle {}",
        i32::from(!h.kernel.pm.dirty_modules.is_empty())
    ));
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();

    // setGeometryInfo's unregister/re-register refresh
    h.kernel.objects.get_mut(a).geometry = geom(GeometryType::Box, false, 1.0, 8.0, 4.0);
    h.kernel.pm.unregister_object(&mut h.kernel.objects, a);
    h.kernel.pm.register_object(&mut h.kernel.objects, a);
    h.dump_partition();
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_partition();
    h.line(&format!(
        "cellchg {} {}",
        h.kernel.objects.get(a).cell_change_count,
        h.kernel.objects.get(b).cell_change_count
    ));
    h.line("end");
}

fn corpus_part_contact(h: &mut Harness) {
    h.line("corpus part_contact");
    let g = geom(GeometryType::Sphere, false, 1.0, 6.0, 6.0);
    // three overlapping movers + one immobile structure + one far away
    h.create_object(g, false, 0, &[], (50.0, 50.0), 0.0, &[], 0);
    h.create_object(g, false, 0, &[], (52.0, 52.0), 0.0, &[], 0);
    h.create_object(g, false, 0, &[], (60.0, 60.0), 0.0, &[], 0);
    h.create_object(g, true, 0, &[], (50.0, 50.0), 0.0, &[], 0);
    h.create_object(g, false, 0, &[], (90.0, 90.0), 0.0, &[], 0);

    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_contacts();
    h.dump_partition();

    // a move that stops overlapping: the pairs drop
    let b = 2;
    let old = h.kernel.objects.get(b).position;
    h.kernel.objects.get_mut(b).position = rzh_sim::types::Coord3 {
        x: 80.0,
        y: 80.0,
        z: 0.0,
    };
    h.kernel
        .react_to_transform_change(b, old, h.kernel.objects.get(b).orientation);
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_contacts();
    h.dump_partition();
    h.line("end");
}

fn corpus_part_register(h: &mut Harness) {
    h.line("corpus part_register");
    let g = geom(GeometryType::Sphere, false, 1.0, 3.0, 3.0);
    let a = h.create_object(g, false, 0, &[], (55.0, 55.0), 0.0, &[], 0);
    let b = h.create_object(g, false, 0, &[], (35.0, 35.0), 0.0, &[], 0);
    h.dump_partition();

    // the double-register refusal (the master list is the observable)
    h.kernel.pm.register_object(&mut h.kernel.objects, a);
    let master_count = h.kernel.pm.module_list.len();
    h.line(&format!("dbl {master_count}"));

    // the unregister: the module leaves the master list
    h.kernel.pm.unregister_object(&mut h.kernel.objects, b);
    h.dump_partition();
    // the unregister of an unregistered object: refused
    h.kernel.pm.unregister_object(&mut h.kernel.objects, b);
    let master_count = h.kernel.pm.module_list.len();
    h.line(&format!("again {master_count}"));
    // the destroy path unregisters (the ~Object flow)
    h.kernel.destroy_object(a, &mut h.host);
    h.kernel.process_destroy_list(&mut h.host);
    h.dump_partition();
    h.line("end");
}

// ============================================================================
// The collide corpora (the generator's, one for one).
// ============================================================================

/// `corpus collide_matrix`: the collide-test family over crafted geometry
/// pairs — the 3x3 matrix (the boolean through `geomCollidesWithGeom`,
/// the loc/normal bits through the proc table), the null-cinfo guard, the
/// sphere-sphere touch boundary and the coincident zero-normal, the
/// closeEnough preflight with its unwritten loc/normal, the rect-circle
/// square promotion (both orders), the sphere-slice cases, and the z-gate
/// families (the below-extent asymmetry between `collidesWith` and
/// `geomCollidesWithGeom`, the boundary, the KINDOF_NO_COLLIDE refusal).
fn corpus_collide_matrix(h: &mut Harness) {
    h.line("corpus collide_matrix");

    // the three geometry flavors: sphere (r=10), cylinder (r=10, h=8),
    // box (10x6, h=8)
    let sphere = geom(GeometryType::Sphere, false, 10.0, 10.0, 10.0);
    let cyl = geom(GeometryType::Cylinder, false, 8.0, 10.0, 10.0);
    let bx = geom(GeometryType::Box, false, 8.0, 10.0, 6.0);
    let geoms = [sphere, cyl, bx];

    // (a) the 3x3 matrix at an overlapping offset (the second geom rotated
    // 0.3 rad), through geomCollidesWithGeom — the boolean
    let p1 = Coord3 {
        x: 100.0,
        y: 100.0,
        z: 0.0,
    };
    let p2 = Coord3 {
        x: 104.0,
        y: 100.0,
        z: 0.0,
    };
    for (i, gi) in geoms.iter().enumerate() {
        for (j, gj) in geoms.iter().enumerate() {
            let res = geom_collides_with_geom(&p1, gi, 0.0, &p2, gj, 0.3);
            h.line(&format!("m {i} {j} {}", i32::from(res)));
        }
    }
    // ... and at a miss offset
    let p3 = Coord3 {
        x: 140.0,
        y: 100.0,
        z: 0.0,
    };
    for (i, gi) in geoms.iter().enumerate() {
        for (j, gj) in geoms.iter().enumerate() {
            let res = geom_collides_with_geom(&p1, gi, 0.0, &p3, gj, 0.3);
            h.line(&format!("miss {i} {j} {}", i32::from(res)));
        }
    }

    // (b) the same matrix through the proc table directly, with the cinfo —
    // the float observables (loc and normal)
    for (i, gi) in geoms.iter().enumerate() {
        for (j, gj) in geoms.iter().enumerate() {
            let a = CollideInfo::new(p1, *gi, 0.0);
            let b = CollideInfo::new(p2, *gj, 0.3);
            let mut cinfo = CollideLocAndNormal::default();
            let res = COLLIDE_TEST_PROCS[i * 3 + j](&a, &b, Some(&mut cinfo));
            h.line(&format!(
                "d {i} {j} {} loc={:08x},{:08x},{:08x} nrm={:08x},{:08x},{:08x}",
                i32::from(res),
                cinfo.loc.x.to_bits(),
                cinfo.loc.y.to_bits(),
                cinfo.loc.z.to_bits(),
                cinfo.normal.x.to_bits(),
                cinfo.normal.y.to_bits(),
                cinfo.normal.z.to_bits()
            ));
        }
    }

    // (c) the null-cinfo guard: every proc takes a null cinfo fine
    {
        let a = CollideInfo::new(p1, sphere, 0.0);
        let b = CollideInfo::new(p2, sphere, 0.0);
        let res = COLLIDE_TEST_PROCS[0](&a, &b, None);
        h.line(&format!("nullcinfo {}", i32::from(res)));
    }

    // (d) the sphere-sphere touch boundary: dist == r1+r2 (the <=) and
    // the step past it; plus the coincident spheres (the normalize
    // zero-guard leaves the normal zero)
    {
        let s15 = geom(GeometryType::Sphere, false, 15.0, 15.0, 15.0);
        let a = CollideInfo::new(
            Coord3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            s15,
            0.0,
        );
        let b = CollideInfo::new(
            Coord3 {
                x: 30.0,
                y: 0.0,
                z: 0.0,
            },
            s15,
            0.0,
        );
        let mut cinfo = CollideLocAndNormal::default();
        let res = COLLIDE_TEST_PROCS[0](&a, &b, Some(&mut cinfo));
        h.line(&format!(
            "touch {} loc={:08x},{:08x},{:08x} nrm={:08x},{:08x},{:08x}",
            i32::from(res),
            cinfo.loc.x.to_bits(),
            cinfo.loc.y.to_bits(),
            cinfo.loc.z.to_bits(),
            cinfo.normal.x.to_bits(),
            cinfo.normal.y.to_bits(),
            cinfo.normal.z.to_bits()
        ));
        let c = CollideInfo::new(
            Coord3 {
                x: 30.1,
                y: 0.0,
                z: 0.0,
            },
            s15,
            0.0,
        );
        let past = COLLIDE_TEST_PROCS[0](&a, &c, None);
        h.line(&format!("past {}", i32::from(past)));
        let z = CollideInfo::new(
            Coord3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            s15,
            0.0,
        );
        let mut cz = CollideLocAndNormal::default();
        let resz = COLLIDE_TEST_PROCS[0](&a, &z, Some(&mut cz));
        h.line(&format!(
            "coincident {} loc={:08x},{:08x},{:08x} nrm={:08x},{:08x},{:08x}",
            i32::from(resz),
            cz.loc.x.to_bits(),
            cz.loc.y.to_bits(),
            cz.loc.z.to_bits(),
            cz.normal.x.to_bits(),
            cz.normal.y.to_bits(),
            cz.normal.z.to_bits()
        ));
    }

    // (e) the closeEnough preflight: a thin 10x1 box rotated 90 degrees
    // versus an unrotated 10x6 box, centers 0.9 apart — every corner of
    // either misses the other's frame (the plate's corners stand 10 off
    // the box's minor axis; the box's corners stand 9.1 off the plate's
    // minor axis), so the rect test says false; the minRadius=1 preflight
    // (0.9^2 < 1^2) forces the true — with loc.x/y and the normal left
    // unwritten (the pinned zero), loc.z the side-to-side average
    {
        let plate = geom(GeometryType::Box, false, 8.0, 10.0, 1.0);
        let boxg = geom(GeometryType::Box, false, 8.0, 10.0, 6.0);
        let a = CollideInfo::new(
            Coord3 {
                x: 50.0,
                y: 50.0,
                z: 0.0,
            },
            plate,
            std::f32::consts::FRAC_PI_2,
        );
        let c = CollideInfo::new(
            Coord3 {
                x: 50.9,
                y: 50.0,
                z: 0.0,
            },
            boxg,
            0.0,
        );
        let mut cinfo = CollideLocAndNormal::default();
        let res = COLLIDE_TEST_PROCS[8](&a, &c, Some(&mut cinfo)); // Box_Box
        h.line(&format!(
            "closeenough {} loc={:08x},{:08x},{:08x} nrm={:08x},{:08x},{:08x}",
            i32::from(res),
            cinfo.loc.x.to_bits(),
            cinfo.loc.y.to_bits(),
            cinfo.loc.z.to_bits(),
            cinfo.normal.x.to_bits(),
            cinfo.normal.y.to_bits(),
            cinfo.normal.z.to_bits()
        ));
        // the same pair one step further apart: the preflight misses too,
        // and the rect test still says false
        let c2 = CollideInfo::new(
            Coord3 {
                x: 51.1,
                y: 50.0,
                z: 0.0,
            },
            boxg,
            0.0,
        );
        let toofar = COLLIDE_TEST_PROCS[8](&a, &c2, None);
        h.line(&format!("toofar {}", i32::from(toofar)));
        // and through geomCollidesWithGeom (the z-gate passes at z=0, so
        // the same verdict)
        let ce_geom = geom_collides_with_geom(
            &a.position,
            &plate,
            std::f32::consts::FRAC_PI_2,
            &c.position,
            &boxg,
            0.0,
        );
        h.line(&format!("closeenough-geom {}", i32::from(ce_geom)));
    }

    // (f) the rect-circle square promotion: a cylinder r=5 diagonal from an
    // unrotated 10x6 box — the true circle misses (the closest box corner
    // is sqrt(3.5^2+4.5^2) > 5 away) but the promoted 10x10 square's
    // corner cap catches the box corner. Both orders (the flipped normal
    // of the Circle_Rect wrapper).
    {
        let small = geom(GeometryType::Cylinder, false, 8.0, 5.0, 5.0);
        let boxg = geom(GeometryType::Box, false, 8.0, 10.0, 6.0);
        let cyl_i = CollideInfo::new(
            Coord3 {
                x: 63.5,
                y: 60.5,
                z: 0.0,
            },
            small,
            0.0,
        );
        let box_i = CollideInfo::new(
            Coord3 {
                x: 50.0,
                y: 50.0,
                z: 0.0,
            },
            boxg,
            0.0,
        );
        let mut c1 = CollideLocAndNormal::default();
        let r1 = matrix_proc(1, 2)(&cyl_i, &box_i, Some(&mut c1)); // Cylinder_Box
        h.line(&format!(
            "promo-cyl-box {} loc={:08x},{:08x},{:08x} nrm={:08x},{:08x},{:08x}",
            i32::from(r1),
            c1.loc.x.to_bits(),
            c1.loc.y.to_bits(),
            c1.loc.z.to_bits(),
            c1.normal.x.to_bits(),
            c1.normal.y.to_bits(),
            c1.normal.z.to_bits()
        ));
        let mut c2 = CollideLocAndNormal::default();
        let r2 = matrix_proc(2, 1)(&box_i, &cyl_i, Some(&mut c2)); // Box_Cylinder
        h.line(&format!(
            "promo-box-cyl {} loc={:08x},{:08x},{:08x} nrm={:08x},{:08x},{:08x}",
            i32::from(r2),
            c2.loc.x.to_bits(),
            c2.loc.y.to_bits(),
            c2.loc.z.to_bits(),
            c2.normal.x.to_bits(),
            c2.normal.y.to_bits(),
            c2.normal.z.to_bits()
        ));
        // the true circle would miss: the same cylinder moved past the
        // box — both the circle and the promoted square miss
        let far = CollideInfo::new(
            Coord3 {
                x: 66.0,
                y: 63.0,
                z: 0.0,
            },
            small,
            0.0,
        );
        let promo_far = matrix_proc(1, 2)(&far, &box_i, None);
        h.line(&format!("promo-far {}", i32::from(promo_far)));
    }

    // (g) the sphere-slice cases of z_collideTest_Sphere_Nonsphere: a
    // sphere above (2b) and below (2a) the nonsphere, within radius of its
    // top/bottom — the slice radius sqrt(r^2 - dz^2) decides
    {
        let sph = geom(GeometryType::Sphere, false, 10.0, 10.0, 10.0);
        let small = geom(GeometryType::Cylinder, false, 8.0, 6.0, 6.0);
        // 2b: sphere center z=15 above the cylinder top (z=8), 7 within
        // the radius 10 → slice radius sqrt(100-49)≈7.14; xy distance 6.5
        // → collides (6.5 <= 7.14+6); at 14 it does not
        let above = CollideInfo::new(
            Coord3 {
                x: 0.0,
                y: 0.0,
                z: 15.0,
            },
            sph,
            0.0,
        );
        let cyl_i = CollideInfo::new(
            Coord3 {
                x: 6.5,
                y: 0.0,
                z: 0.0,
            },
            small,
            0.0,
        );
        let mut c1 = CollideLocAndNormal::default();
        let r1 = matrix_proc(0, 1)(&above, &cyl_i, Some(&mut c1)); // Sphere_Cylinder
        h.line(&format!(
            "slice-above {} loc={:08x},{:08x},{:08x} nrm={:08x},{:08x},{:08x}",
            i32::from(r1),
            c1.loc.x.to_bits(),
            c1.loc.y.to_bits(),
            c1.loc.z.to_bits(),
            c1.normal.x.to_bits(),
            c1.normal.y.to_bits(),
            c1.normal.z.to_bits()
        ));
        let far_c = CollideInfo::new(
            Coord3 {
                x: 14.0,
                y: 0.0,
                z: 0.0,
            },
            small,
            0.0,
        );
        let slice_above_far = matrix_proc(0, 1)(&above, &far_c, None);
        h.line(&format!("slice-above-far {}", i32::from(slice_above_far)));
        // 2a: sphere center z=-5 below the cylinder bottom (z=0), 5 within
        // the radius 10 → slice radius sqrt(100-25)≈8.66; xy distance 7
        let below = CollideInfo::new(
            Coord3 {
                x: 0.0,
                y: 0.0,
                z: -5.0,
            },
            sph,
            0.0,
        );
        let cyl2 = CollideInfo::new(
            Coord3 {
                x: 7.0,
                y: 0.0,
                z: 0.0,
            },
            small,
            0.0,
        );
        let mut c2 = CollideLocAndNormal::default();
        let r2 = matrix_proc(0, 1)(&below, &cyl2, Some(&mut c2));
        h.line(&format!(
            "slice-below {} loc={:08x},{:08x},{:08x} nrm={:08x},{:08x},{:08x}",
            i32::from(r2),
            c2.loc.x.to_bits(),
            c2.loc.y.to_bits(),
            c2.loc.z.to_bits(),
            c2.normal.x.to_bits(),
            c2.normal.y.to_bits(),
            c2.normal.z.to_bits()
        ));
        // the reversed order (Cylinder_Sphere): the same path with the
        // flipped normal
        let mut c3 = CollideLocAndNormal::default();
        let r3 = matrix_proc(1, 0)(&cyl2, &below, Some(&mut c3));
        h.line(&format!(
            "slice-rev {} loc={:08x},{:08x},{:08x} nrm={:08x},{:08x},{:08x}",
            i32::from(r3),
            c3.loc.x.to_bits(),
            c3.loc.y.to_bits(),
            c3.loc.z.to_bits(),
            c3.normal.x.to_bits(),
            c3.normal.y.to_bits(),
            c3.normal.z.to_bits()
        ));
    }

    // (h) the z-gates. collidesWith (the object path, with the
    // getMaxHeightBelowPosition extents) vs geomCollidesWithGeom (the
    // raw-z gate — narrower). The sphere-pair below-extent asymmetry: two
    // spheres r=10 at z=5 and z=-6, xy 3 apart — collidesWith's gate
    // passes (thisBot=-5 <= thatTop=4), geomCollidesWithGeom's does not
    // (this.z=5 > thatTop=4). Also the collidesWith boundary itself:
    // sphere z=18 vs cylinder h=8 (thisBot=8 == thatTop=8 passes; z=18.5
    // does not).
    {
        let sph = geom(GeometryType::Sphere, false, 10.0, 10.0, 10.0);
        let s1 = h.create_colliding(sph, false, (10.0, 10.0, 5.0), 0);
        let s2 = h.create_colliding(sph, false, (13.0, 10.0, -6.0), 0);
        let asym_collides = rzh_sim::collide::collides_with(&h.kernel.objects, s1, s2).is_some();
        h.line(&format!("asym-collides {}", i32::from(asym_collides)));
        let p1 = h.kernel.objects.get(s1).position;
        let p2 = h.kernel.objects.get(s2).position;
        let asym_geom = geom_collides_with_geom(&p1, &sph, 0.0, &p2, &sph, 0.0);
        h.line(&format!("asym-geom {}", i32::from(asym_geom)));
        let asym_geom_rev = geom_collides_with_geom(&p2, &sph, 0.0, &p1, &sph, 0.0);
        h.line(&format!("asym-geom-rev {}", i32::from(asym_geom_rev)));

        let cy = geom(GeometryType::Cylinder, false, 8.0, 6.0, 6.0);
        let s3 = h.create_colliding(sph, false, (30.0, 10.0, 18.0), 0);
        let cyl_o = h.create_colliding(cy, false, (31.0, 10.0, 0.0), 0);
        let zbound = rzh_sim::collide::collides_with(&h.kernel.objects, s3, cyl_o).is_some();
        h.line(&format!("zbound {}", i32::from(zbound)));
        let s4 = h.create_colliding(sph, false, (40.0, 10.0, 18.5), 0);
        let cyl_o2 = h.create_colliding(cy, false, (41.0, 10.0, 0.0), 0);
        let zbound_past = rzh_sim::collide::collides_with(&h.kernel.objects, s4, cyl_o2).is_some();
        h.line(&format!("zbound-past {}", i32::from(zbound_past)));
        // the KINDOF_NO_COLLIDE refusal (either side)
        let nc = h.create_colliding(sph, false, (50.0, 10.0, 0.0), 0);
        let plain = h.create_colliding(sph, false, (52.0, 10.0, 0.0), 0);
        h.kernel.objects.get_mut(nc).kindof |= 1 << KINDOF_NO_COLLIDE_BIT;
        let nocol = rzh_sim::collide::collides_with(&h.kernel.objects, nc, plain).is_some();
        h.line(&format!("nocol {}", i32::from(nocol)));
        let nocol_rev = rzh_sim::collide::collides_with(&h.kernel.objects, plain, nc).is_some();
        h.line(&format!("nocol-rev {}", i32::from(nocol_rev)));
    }
    h.line("end");
}

/// `corpus collide_process`: `processContactList` through the store — the
/// dispatch order with the flipped normal, the steady-state re-dirty, the
/// NO_COLLISIONS pre-gate, the mid-walk absorb break, the
/// destroyed-absorb (the skipped second call and the refused re-dirty),
/// the unregister mid-processing (the nulled later pairs), the immobile
/// re-dirty refusal, and the z-miss pair.
fn corpus_collide_process(h: &mut Harness) {
    h.line("corpus collide_process");
    let sph = geom(GeometryType::Sphere, false, 6.0, 6.0, 6.0);

    // (a) the dispatch order and the flipped normal: two overlapping
    // spheres, two plain colliders each
    let a = h.create_colliding(sph, false, (50.0, 50.0, 0.0), 2);
    let b = h.create_colliding(sph, false, (52.0, 52.0, 0.0), 2);
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_contacts();
    h.dump_collide_events();
    h.dump_partition();

    // (b) the steady state: the re-dirtied pair re-processes every frame
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_contacts();
    h.dump_collide_events();

    // (c) the NO_COLLISIONS pre-gate: the pair is still collected, no
    // dispatch
    h.kernel.objects.get_mut(b).status |= 1 << OBJECT_STATUS_NO_COLLISIONS_BIT;
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_contacts();
    h.dump_collide_events();

    // (d) the mid-walk absorb: a's first collider sets the bit — a's
    // second is skipped, b's walk is unaffected; the NEXT update's
    // pre-gate refuses the pair. (The pair must be re-dirtied first: the
    // pre-gate refusal of part (c) left both clean.)
    h.kernel.objects.get_mut(b).status &= !(1 << OBJECT_STATUS_NO_COLLISIONS_BIT);
    h.kernel.pm.make_dirty(&mut h.kernel.objects, a, true);
    h.kernel.pm.make_dirty(&mut h.kernel.objects, b, true);
    h.script_collide_absorb(a, 0);
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_collide_events();
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_collide_events();
    h.dump_partition();
    h.kernel.objects.get_mut(a).status &= !(1 << OBJECT_STATUS_NO_COLLISIONS_BIT);

    // (e) the destroyed-absorb: c's first collider destroys e during the
    // FIRST dispatch — e's onCollide is skipped (the isDestroyed gate),
    // e's re-dirty is refused; the destroy list processes at the corpus's
    // hand. (The first update after registration collects the pair from
    // the first-registered object's walk, so the pair is (c, e) — c
    // dispatches first.)
    let c = h.create_colliding(sph, false, (20.0, 50.0, 0.0), 1);
    let e = h.create_colliding(sph, false, (22.0, 52.0, 0.0), 1);
    h.script_collide_destroy(c, 0, e);
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_collide_events();
    h.dump_registry();
    h.dump_partition();
    h.kernel.process_destroy_list(&mut h.host);
    h.dump_partition();

    // (f) the unregister mid-processing: g's first collider unregisters h
    // during the (f,g) pair's dispatch — removeSpecificPartitionData nulls
    // the later (h,g) and (h,f) nodes, and the walk skips them. h stays a
    // live object with no partition data (its re-dirty is refused)
    let _f = h.create_colliding(sph, false, (20.0, 20.0, 0.0), 1);
    let g = h.create_colliding(sph, false, (25.0, 25.0, 0.0), 1);
    let hobj = h.create_colliding(sph, false, (30.0, 30.0, 0.0), 1);
    h.script_collide_unregister(g, 0, hobj);
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_contacts();
    h.dump_collide_events();
    h.dump_registry();
    h.dump_partition();

    // (g) the immobile re-dirty refusal: a mobile-immobile pair — both
    // dispatch, only the mobile side re-dirties (the steady state below
    // collects the pair through the mobile side alone)
    let _mob = h.create_colliding(sph, false, (70.0, 70.0, 0.0), 1);
    let _imm = h.create_colliding(sph, true, (72.0, 72.0, 0.0), 1);
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_collide_events();
    h.dump_partition();
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_contacts();
    h.dump_collide_events();

    // (h) the z-miss: xy-overlapping but z-disjoint — the pair is
    // collected (the cells are 2d), the collide test refuses
    h.create_colliding(sph, false, (80.0, 80.0, 0.0), 1);
    h.create_colliding(sph, false, (82.0, 82.0, 50.0), 1);
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host);
    h.dump_contacts();
    h.dump_collide_events();
    h.line("end");
}

// --- players_lifecycle: the ctor/init scheme, the neutral birth, the
// interning, the update walk's 16-slot quirk, the reset's stale relations --
fn corpus_players_lifecycle(h: &mut Harness) {
    h.line("corpus players_lifecycle");
    h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));

    // the birth state: count 1, local 0 (the becomingLocalPlayer(0,true)
    // transition), the neutral self-relation, the empty-string key, the
    // -1 slot arrays
    h.dump_players();
    h.dump_team_events();

    // the name-key interning: the sequential ordinals, the case
    // sensitivity, the re-intern of the same name
    let nk_empty = h.kernel.players.name_key("");
    h.line(&format!("nk empty={}", nk_empty.ordinal()));
    let nk_a = h.kernel.players.name_key("player1");
    let nk_b = h.kernel.players.name_key("player2");
    let nk_cap = h.kernel.players.name_key("Player1");
    let nk_a2 = h.kernel.players.name_key("player1");
    let nk_p = h.kernel.players.name_key("player1");
    h.line(&format!(
        "nk a={} b={} A={} a2={} players={}",
        nk_a.ordinal(),
        nk_b.ordinal(),
        nk_cap.ordinal(),
        nk_a2.ordinal(),
        nk_p.ordinal()
    ));

    // the 16-slot update walk (the count is 1 — every slot updates anyway)
    h.kernel.players.update(&mut h.host);
    h.dump_team_events();

    // getNthPlayer's bounds refusal
    h.line(&format!(
        "nth {} {}",
        i32::from(h.kernel.players.nth_player(-1).is_some()),
        i32::from(
            h.kernel
                .players
                .nth_player(rzh_sim::types::MAX_PLAYER_COUNT as i32)
                .is_some()
        )
    ));

    // the stale-relation quirk: relations set on a player, then reset() —
    // the maps survive init (they are born in the ctor and die in the
    // dtor). The team 77 is born outside the factory's map (the
    // transcription's raw teamNew) so the relation resolves across the
    // reset; the Rust mirror drives the same through a bare birth.
    let team77 = {
        // the transcription's teamNew(&w.teamFactory, nullptr, 77, &w):
        // a team with no prototype, id 77, in the arena
        let players = &mut h.kernel.players;
        players.factory.teams.push(Some(rzh_sim::players::Team {
            id: 77,
            proto: usize::MAX, // no prototype (the transcription's nullptr)
            state: rzh_sim::players::TeamRuntimeState::default(),
            team_relations: std::collections::BTreeMap::new(),
            player_relations: std::collections::BTreeMap::new(),
            members: Vec::new(),
            recruitable_set: false,
            recruitable: false,
        }));
        77
    };
    {
        let players = &mut h.kernel.players;
        players.list.players[1].set_player_relationship(2, Relationship::Enemies);
        players.list.players[1].set_team_relationship(team77, Relationship::Allies);
    }
    h.line("setrel");
    h.dump_players();
    h.kernel.players.reset(&mut h.kernel.objects, &mut h.host);
    h.line("afterreset");
    h.dump_players();
    h.dump_team_events();

    // setLocalPlayer's transitions and the null refusal (null → neutral)
    h.kernel.players.set_local_player(Some(3), &mut h.host);
    h.kernel.players.set_local_player(None, &mut h.host);
    h.dump_team_events();
    h.dump_players();
}

// --- players_newgame: the sides walk, the local-selection precedence, the
// enemies/allies assignment, the default teams, the slots -------------
fn corpus_players_newgame(h: &mut Harness) {
    h.line("corpus players_newgame");
    {
        // a 4-side map: neutral, a human, two computers; the multiplayer
        // local flag on a LATER side beats the earlier human
        h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));
        let sides = SidesSpec {
            sides: vec![
                side_spec("", false, false, false, &[], &[]), // the neutral side
                side_spec("america", true, false, false, &["china"], &[]),
                side_spec("china", false, false, false, &[], &["america"]),
                side_spec(
                    "gla",
                    false,
                    true,
                    false,
                    &["america", "china", "nosuch"],
                    &[],
                ),
            ],
            teams: vec![
                team_spec("team", "neutral", true),
                team_spec("teamamerica", "america", true),
                team_spec("teamchina", "china", true),
            ],
            has_game_info: true,
            game_info: GameInfoSpec {
                occupied_slots: vec![0, 2, 3],
            },
        };

        h.kernel
            .players
            .new_game(&sides, &mut h.kernel.objects, &mut h.host);
        h.dump_players();
        h.dump_teams();
        h.dump_team_events();

        // the local-selection precedence: the multiplayerIsLocal side
        // (gla, player 3) won over the earlier human (america, player 1)
        h.line(&format!(
            "local {}",
            h.kernel.players.list.local.map(|p| p as i32).unwrap_or(-1)
        ));

        // the asymmetric relations: america→china ENEMIES, china→america
        // ALLIES (the walk assigns per side, no symmetry)
        let rel = |h: &Harness, from: usize, to: usize| {
            let dt = h.kernel.players.list.players[to].default_team;
            h.kernel.players.player_get_relationship(from, dt).name()
        };
        h.line(&format!(
            "rel a->c {} c->a {} a->g {} g->a {}",
            rel(h, 1, 2),
            rel(h, 2, 1),
            rel(h, 1, 3),
            rel(h, 3, 1)
        ));

        // the slot lookups (the "player%d" probe missed our names)
        h.line(&format!(
            "slots {} {} {} {}",
            h.kernel.players.get_slot_index(1),
            h.kernel.players.get_slot_index(2),
            h.kernel.players.get_player_index_from_slot_index(0),
            h.kernel.players.get_player_index_from_slot_index(4)
        ));
    }
    {
        // the same map with the network present: no human wins, the
        // multiplayerIsLocal flag is the only local source
        h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));
        h.kernel.players.network_present = true;
        let sides = SidesSpec {
            sides: vec![
                side_spec("", false, false, false, &[], &[]),
                side_spec("america", true, false, false, &[], &[]),
                side_spec("china", false, true, false, &[], &[]),
            ],
            teams: Vec::new(),
            has_game_info: false,
            game_info: GameInfoSpec::default(),
        };
        h.kernel
            .players
            .new_game(&sides, &mut h.kernel.objects, &mut h.host);
        h.line(&format!(
            "netlocal {}",
            h.kernel.players.list.local.map(|p| p as i32).unwrap_or(-1)
        ));
        h.dump_team_events();
    }
    {
        // no local at all (all computers, no network): the fallback
        // forces the first non-neutral player HUMAN and local; and a
        // neutral side with enemies of its own (the second loop processes
        // the neutral player too — its enemies assignment lands)
        h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));
        let sides = SidesSpec {
            sides: vec![
                side_spec("", false, false, false, &["america"], &[]),
                side_spec("america", false, false, false, &[], &[]),
            ],
            teams: vec![team_spec("team", "neutral", true)],
            has_game_info: false,
            game_info: GameInfoSpec::default(),
        };
        h.kernel
            .players
            .new_game(&sides, &mut h.kernel.objects, &mut h.host);
        h.line(&format!(
            "forcedlocal {} type0={} type1={}",
            h.kernel.players.list.local.map(|p| p as i32).unwrap_or(-1),
            h.kernel.players.list.players[0].player_type.bits(),
            h.kernel.players.list.players[1].player_type.bits()
        ));
        h.dump_players();
        h.dump_team_events();
    }
    {
        // the missing default-team prototype: setDefaultTeam's null path
        // (the release build continues; the default team stays 0)
        h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));
        let sides = SidesSpec {
            sides: vec![
                side_spec("", false, false, false, &[], &[]),
                side_spec("america", true, false, false, &[], &[]),
            ],
            teams: Vec::new(), // no teams at all
            has_game_info: false,
            game_info: GameInfoSpec::default(),
        };
        h.kernel
            .players
            .new_game(&sides, &mut h.kernel.objects, &mut h.host);
        h.line(&format!(
            "nodefault {}",
            h.kernel.players.list.players[1].default_team
        ));
    }
}

// --- teams_factory: the id scheme, the singleton reuse, the auto-create,
// the duplicate-name refusal, the map order -----------------------
fn corpus_teams_factory(h: &mut Harness) {
    h.line("corpus teams_factory");
    h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));
    let squad = TeamSpec {
        script_on_destroyed: "sd".to_string(),
        destroyed_threshold: 0.5,
        ..team_spec("americaSquad", "america", false)
    };
    let prod = TeamSpec {
        execute_actions: true,
        production_condition: "condA".to_string(),
        production_script_exists: true,
        ..team_spec("prodteam", "china", false)
    };
    let prod_miss = TeamSpec {
        execute_actions: true,
        production_condition: "condB".to_string(),
        production_script_exists: false,
        ..team_spec("prodmiss", "china", true)
    };
    let sides = SidesSpec {
        sides: vec![
            side_spec("", false, false, false, &[], &[]),
            side_spec("america", true, false, false, &[], &[]),
            side_spec("china", false, false, false, &[], &[]),
        ],
        teams: vec![
            team_spec("team", "neutral", true),
            team_spec("teamamerica", "america", true),
            team_spec("teamchina", "china", true),
            squad,
            team_spec("teamamerica", "china", true), // the duplicate name
            team_spec("stray", "nosuchplayer", true),
            prod,
            prod_miss,
        ],
        has_game_info: false,
        game_info: GameInfoSpec::default(),
    };

    h.kernel
        .players
        .new_game(&sides, &mut h.kernel.objects, &mut h.host);
    h.dump_teams();
    h.dump_team_events();

    // the ++id scheme and the singleton reuse: createTeam on a singleton
    // returns the SAME instance (no new id); createInactiveTeam ditto
    let ta = h.kernel.players.create_team("teamamerica", &mut h.host);
    let tb = h
        .kernel
        .players
        .create_inactive_team("teamamerica", &mut h.host);
    h.line(&format!(
        "singleton {} {}",
        ta.unwrap_or(0),
        tb.unwrap_or(0)
    ));

    // the non-singleton auto-create of findTeam, and the explicit creates
    let s1 = h.kernel.players.find_team("americaSquad", &mut h.host);
    let s1_active = s1
        .and_then(|id| {
            h.kernel
                .players
                .factory
                .teams
                .iter()
                .flatten()
                .find(|t| t.id == id)
                .map(|t| t.state.active)
        })
        .unwrap_or(false);
    h.line(&format!(
        "find1 {} act={}",
        s1.unwrap_or(0),
        i32::from(s1_active)
    ));
    let s2 = h.kernel.players.create_team("americaSquad", &mut h.host);
    let s3 = h.kernel.players.create_team("americaSquad", &mut h.host);
    h.line(&format!("creates {} {}", s2.unwrap_or(0), s3.unwrap_or(0)));
    let s4 = h.kernel.players.find_team("americaSquad", &mut h.host);
    h.line(&format!("find2 {}", s4.unwrap_or(0)));

    // createTeamOnPrototype: the singleton's reuse and the null refusal
    let tp_a = h
        .kernel
        .players
        .find_team_prototype_id("teamamerica")
        .unwrap_or(0);
    let c1 = h
        .kernel
        .players
        .create_team_on_prototype("teamamerica", &mut h.host);
    h.line(&format!(
        "onproto {} null={}",
        c1.unwrap_or(0),
        i32::from(tp_a == 0)
    ));

    // the unknown prototype refusal
    let unk_create = h
        .kernel
        .players
        .create_inactive_team("nosuch", &mut h.host)
        .is_some();
    let unk_find = h.kernel.players.find_team("nosuch", &mut h.host).is_some();
    h.line(&format!(
        "unknown {} {}",
        i32::from(unk_create),
        i32::from(unk_find)
    ));

    // findTeamByID: the prototype-map walk order (key order — the
    // interning order of the names), the INVALID refusal
    h.line(&format!(
        "byid t1={} t2={} t0={}",
        h.kernel.players.find_team_by_id(1).unwrap_or(0),
        h.kernel.players.find_team_by_id(2).unwrap_or(0),
        i32::from(h.kernel.players.find_team_by_id(0).is_some())
    ));
    // findTeamPrototypeByID: the map walk
    let pb1 = h
        .kernel
        .players
        .find_team_prototype_by_id(1)
        .map(|id| {
            h.kernel
                .players
                .factory
                .protos
                .iter()
                .flatten()
                .find(|p| p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let pb2 = h
        .kernel
        .players
        .find_team_prototype_by_id(2)
        .map(|id| {
            h.kernel
                .players
                .factory
                .protos
                .iter()
                .flatten()
                .find(|p| p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    h.line(&format!(
        "protobyid 1={pb1} 2={pb2} 99={}",
        i32::from(h.kernel.players.find_team_prototype_by_id(99).is_some())
    ));

    h.dump_teams();
    h.dump_team_events();
}

// --- relationships: the override chain, the precedence ladder, the removal
// family, the defector gates, the setControllingPlayer move ------------
fn corpus_relationships(h: &mut Harness) {
    h.line("corpus relationships");
    h.fresh_world(10.0, (0.0, 0.0, 200.0, 200.0));
    let sides = SidesSpec {
        sides: vec![
            side_spec("", false, false, false, &[], &[]),
            side_spec("america", true, false, false, &[], &[]),
            side_spec("china", false, false, false, &[], &[]),
            side_spec("gla", false, false, false, &[], &[]),
        ],
        teams: vec![
            team_spec("team", "neutral", true),
            team_spec("teamamerica", "america", true),
            team_spec("teamchina", "china", true),
            team_spec("teamgla", "gla", true),
            team_spec("americaSquad", "america", false),
            team_spec("chinaSquad", "china", false),
        ],
        has_game_info: false,
        game_info: GameInfoSpec::default(),
    };
    h.kernel
        .players
        .new_game(&sides, &mut h.kernel.objects, &mut h.host);
    h.dump_team_events();

    let ta = h
        .kernel
        .players
        .find_team("teamamerica", &mut h.host)
        .unwrap();
    let tc = h
        .kernel
        .players
        .find_team("teamchina", &mut h.host)
        .unwrap();
    let tg = h.kernel.players.find_team("teamgla", &mut h.host).unwrap();
    let sa = h
        .kernel
        .players
        .create_team("americaSquad", &mut h.host)
        .unwrap();
    let sc = h
        .kernel
        .players
        .create_team("chinaSquad", &mut h.host)
        .unwrap();

    // the ladder: player→team (through the default team), the team override,
    // the player override on the team, the owning player's view
    let rel_name = |h: &Harness, from: usize, that: u32| -> &'static str {
        h.kernel.players.player_get_relationship(from, that).name()
    };
    h.kernel.players.list.players[1].set_player_relationship(2, Relationship::Enemies);
    let l0 = rel_name(h, 1, tc);
    h.line(&format!("l0 {l0}"));
    h.kernel.players.list.players[1].set_team_relationship(tc, Relationship::Allies);
    let l1 = rel_name(h, 1, tc);
    h.line(&format!("l1 {l1}"));
    h.kernel.players.list.players[1].set_team_relationship(tg, Relationship::Neutral);
    let l2 = rel_name(h, 1, tg);
    h.line(&format!("l2 {l2}"));
    // the team-level chain: team override > team's player override > owner
    let trel_name = |h: &Harness, team: u32, that: u32| -> &'static str {
        h.kernel.players.team_get_relationship(team, that).name()
    };
    let t0 = trel_name(h, sa, sc);
    h.line(&format!("t0 {t0}"));
    {
        let t = h
            .kernel
            .players
            .factory
            .teams
            .iter_mut()
            .flatten()
            .find(|t| t.id == sa)
            .unwrap();
        t.set_override_team_relationship(sc, Relationship::Allies);
    }
    let t1 = trel_name(h, sa, sc);
    h.line(&format!("t1 {t1}"));
    {
        let t = h
            .kernel
            .players
            .factory
            .teams
            .iter_mut()
            .flatten()
            .find(|t| t.id == sa)
            .unwrap();
        t.set_override_player_relationship(2, Relationship::Neutral);
    }
    let t2 = trel_name(h, sa, sc);
    h.line(&format!("t2 {t2}"));
    {
        let t = h
            .kernel
            .players
            .factory
            .teams
            .iter_mut()
            .flatten()
            .find(|t| t.id == sa)
            .unwrap();
        let _ = t.remove_override_team_relationship(sc);
    }
    let t3 = trel_name(h, sa, sc);
    h.line(&format!("t3 {t3}"));
    // the other team's side (asymmetry)
    let t4 = trel_name(h, sc, sa);
    h.line(&format!("t4 {t4}"));

    // the null-Team forms: NEUTRAL (the `if (that)` guard)
    h.line(&format!(
        "n0 {} {}",
        h.kernel.players.player_get_relationship(1, 0).name(),
        h.kernel.players.team_get_relationship(sa, 0).name()
    ));

    // the removal family: the empty-map refusal, the miss, the null
    // clear-alls, the INVALID clear-alls on the team overrides
    let r0 = h.kernel.players.list.players[0].remove_player_relationship(Some(2));
    let r1 = h.kernel.players.list.players[1].remove_player_relationship(Some(3));
    let r2 = h.kernel.players.list.players[1].remove_team_relationship(None);
    let r3 = h.kernel.players.list.players[1].remove_player_relationship(None);
    let (r4a, r4b) = {
        let t = h
            .kernel
            .players
            .factory
            .teams
            .iter_mut()
            .flatten()
            .find(|t| t.id == sa)
            .unwrap();
        (
            t.remove_override_team_relationship(rzh_sim::types::TEAM_ID_INVALID),
            t.remove_override_player_relationship(rzh_sim::types::PLAYER_INDEX_INVALID),
        )
    };
    let r5 = {
        let t = h
            .kernel
            .players
            .factory
            .teams
            .iter_mut()
            .flatten()
            .find(|t| t.id == sa)
            .unwrap();
        t.remove_override_team_relationship(999)
    };
    h.line(&format!("r0 {}", i32::from(r0)));
    h.line(&format!("r1 {}", i32::from(r1)));
    h.line(&format!("r2 {}", i32::from(r2)));
    h.line(&format!("r3 {}", i32::from(r3)));
    h.line(&format!("r4 {} {}", i32::from(r4a), i32::from(r4b)));
    h.line(&format!("r5 {}", i32::from(r5)));
    let r6 = trel_name(h, sa, sc);
    h.line(&format!("r6 {r6}"));
    h.dump_players();
    h.dump_teams();

    // Object::getRelationship: the defector gates over the object chain
    // (re-establish the base enemy so each gate's verdict differs)
    h.kernel.players.list.players[1].set_player_relationship(2, Relationship::Enemies);
    let sph = GeometryInfo {
        geom_type: GeometryType::Sphere,
        is_small: true,
        height: 1.0,
        major_radius: 1.0,
        minor_radius: 1.0,
    };
    let oa = h.create_query_obj(sph, (50.0, 50.0, 0.0));
    let oc = h.create_query_obj(sph, (60.0, 50.0, 0.0));
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, oa, Some(ta), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, oc, Some(tc), &mut h.host);
    let orel = |h: &Harness, a: i32, b: i32| -> &'static str {
        h.kernel
            .players
            .object_get_relationship(&h.kernel.objects, a, b)
            .name()
    };
    let d0 = orel(h, oa, oc);
    h.line(&format!("d0 {d0}"));
    h.kernel.objects.get_mut(oa).private_status |=
        1 << rzh_sim::types::OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT;
    let d1 = orel(h, oa, oc);
    h.line(&format!("d1 {d1}"));
    h.kernel.objects.get_mut(oa).private_status &=
        !(1 << rzh_sim::types::OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT);
    h.kernel.objects.get_mut(oc).private_status |=
        1 << rzh_sim::types::OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT;
    let d2 = orel(h, oa, oc);
    h.line(&format!("d2 {d2}"));
    h.kernel.objects.get_mut(oc).private_status &=
        !(1 << rzh_sim::types::OBJECT_PRIVATE_UNDETECTED_DEFECTOR_BIT);
    // a teamless source: NEUTRAL regardless
    let onull = h.create_query_obj(sph, (70.0, 50.0, 0.0));
    let d3 = orel(h, onull, oc);
    h.line(&format!("d3 {d3}"));

    // setControllingPlayer: the prototype's owner move, the lists, the
    // member capture events, and the null refusal
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, oa, Some(sa), &mut h.host);
    h.dump_team_events();
    h.kernel
        .players
        .set_controlling_player(sa, Some(2), &mut h.host);
    let ctrl = h
        .kernel
        .players
        .factory
        .teams
        .iter()
        .flatten()
        .find(|t| t.id == sa)
        .and_then(|t| h.kernel.players.factory.protos[t.proto].as_ref())
        .map(|p| p.owner)
        .unwrap_or(usize::MAX);
    h.line(&format!("ctrl {}", ctrl as i32));
    h.dump_team_events();
    h.kernel
        .players
        .set_controlling_player(sa, None, &mut h.host);
    let ctrl_null = h
        .kernel
        .players
        .factory
        .teams
        .iter()
        .flatten()
        .find(|t| t.id == sa)
        .map(|t| {
            h.kernel.players.factory.protos[t.proto]
                .as_ref()
                .map(|p| p.owner)
                .unwrap_or(usize::MAX)
        })
        .unwrap_or(usize::MAX);
    h.line(&format!("ctrlnull {}", ctrl_null as i32));
    h.dump_players();
    h.dump_teams();
    h.dump_object_teams();
}

// --- players_masks: getPlayerFromMask, the getEachPlayerFromMask peeling,
// getPlayersWithRelationship -------------------------------
fn corpus_players_masks(h: &mut Harness) {
    h.line("corpus players_masks");
    h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));
    let sides = SidesSpec {
        sides: vec![
            side_spec("", false, false, false, &[], &[]),
            side_spec("america", true, false, false, &["china"], &["gla"]),
            side_spec("china", false, false, false, &[], &[]),
            side_spec("gla", false, false, false, &[], &[]),
            side_spec("observer", false, false, true, &[], &[]),
        ],
        teams: vec![
            team_spec("team", "neutral", true),
            team_spec("teamamerica", "america", true),
            team_spec("teamchina", "china", true),
            team_spec("teamgla", "gla", true),
            team_spec("teamobserver", "observer", true),
        ],
        has_game_info: false,
        game_info: GameInfoSpec::default(),
    };
    h.kernel
        .players
        .new_game(&sides, &mut h.kernel.objects, &mut h.host);
    h.dump_team_events();

    // getPlayerFromMask: the hit and the two misses (an unused bit and
    // the multi-bit mask — exact equality only)
    h.line(&format!(
        "m0 {}",
        h.kernel
            .players
            .get_player_from_mask(1u16 << 2)
            .map(|p| p.index() as i32)
            .unwrap_or(-1)
    ));
    h.line(&format!(
        "m1 {} {}",
        i32::from(h.kernel.players.get_player_from_mask(1u16 << 9).is_some()),
        i32::from(h.kernel.players.get_player_from_mask(0x0006).is_some())
    ));

    // the peeling walk: index order, the mask erasure, the exhaustion
    // (mask zeroed + null)
    let mut peel: rzh_sim::types::PlayerMaskType = 0x000e; // players 1, 2, 3
    for i in 0..5 {
        let p = h.kernel.players.get_each_player_from_mask(&mut peel);
        h.line(&format!(
            "peel {i} {} {:04x}",
            p.map(|p| p as i32).unwrap_or(-1),
            peel
        ));
    }
    // a mask with bits no player carries: the exhaustion zeroes it
    let mut dead: rzh_sim::types::PlayerMaskType = 0x8000;
    let p = h.kernel.players.get_each_player_from_mask(&mut dead);
    h.line(&format!(
        "peeldead {} {:04x}",
        p.map(|p| p as i32).unwrap_or(-1),
        dead
    ));

    // getPlayersWithRelationship: the selector combos, the invalid src,
    // the zero-flags early exit, the observer's dead flag (still a player
    // with a mask — relationship NEUTRAL toward everyone)
    use rzh_sim::types::{ALLOW_ALLIES, ALLOW_ENEMIES, ALLOW_NEUTRAL, ALLOW_SAME_PLAYER};
    let same = ALLOW_SAME_PLAYER;
    let al = ALLOW_ALLIES;
    let en = ALLOW_ENEMIES;
    let ne = ALLOW_NEUTRAL;
    h.line(&format!(
        "g0 {:04x}",
        h.kernel.players.get_players_with_relationship(1, same | al)
    ));
    h.line(&format!(
        "g1 {:04x}",
        h.kernel.players.get_players_with_relationship(1, en)
    ));
    h.line(&format!(
        "g2 {:04x}",
        h.kernel.players.get_players_with_relationship(1, ne)
    ));
    h.line(&format!(
        "g3 {:04x}",
        h.kernel
            .players
            .get_players_with_relationship(1, same | al | en | ne)
    ));
    h.line(&format!(
        "g4 {:04x}",
        h.kernel.players.get_players_with_relationship(1, 0)
    ));
    h.line(&format!(
        "g5 {:04x}",
        h.kernel
            .players
            .get_players_with_relationship(-1, same | en)
    ));
    h.line(&format!(
        "g6 {:04x}",
        h.kernel
            .players
            .get_players_with_relationship(99, same | en)
    ));
    h.line(&format!(
        "g7 {:04x}",
        h.kernel.players.get_players_with_relationship(0, same | ne)
    ));
    h.line(&format!(
        "g8 {:04x}",
        h.kernel
            .players
            .get_players_with_relationship(4, same | ne | al | en)
    ));
    h.dump_players();
}

// --- team_members: the setTeam paths, the member-list choreography,
// updateState's latches and thresholds, the empty-team sweep ---------
fn corpus_team_members(h: &mut Harness) {
    h.line("corpus team_members");
    h.fresh_world(10.0, (0.0, 0.0, 200.0, 200.0));
    let squad = TeamSpec {
        script_on_create: "oc1".to_string(),
        script_on_destroyed: "od1".to_string(),
        script_on_idle: "oi1".to_string(),
        script_on_enemy_sighted: "oes1".to_string(),
        script_on_all_clear: "oac1".to_string(),
        script_on_unit_destroyed: "oud1".to_string(),
        destroyed_threshold: 0.4,
        ..team_spec("americaSquad", "america", false)
    };
    let sides = SidesSpec {
        sides: vec![
            side_spec("", false, false, false, &[], &[]),
            side_spec("america", true, false, false, &["china"], &[]),
            side_spec("china", false, false, false, &["america"], &[]),
            side_spec("dead", false, false, false, &[], &[]),
        ],
        teams: vec![
            team_spec("team", "neutral", true),
            team_spec("teamamerica", "america", true),
            team_spec("teamchina", "china", true),
            team_spec("teamdead", "dead", true),
            squad,
        ],
        has_game_info: false,
        game_info: GameInfoSpec::default(),
    };
    h.kernel
        .players
        .new_game(&sides, &mut h.kernel.objects, &mut h.host);
    h.dump_team_events();

    h.kernel.players.list.players[3].is_player_dead = true; // kill the player (the isPlayerActive gate)
    let ta = h
        .kernel
        .players
        .find_team("teamamerica", &mut h.host)
        .unwrap();
    let sa = h
        .kernel
        .players
        .create_team("americaSquad", &mut h.host)
        .unwrap();
    let td = h.kernel.players.find_team("teamdead", &mut h.host).unwrap();

    let sph = GeometryInfo {
        geom_type: GeometryType::Sphere,
        is_small: true,
        height: 1.0,
        major_radius: 1.0,
        minor_radius: 1.0,
    };
    let o1 = h.create_query_obj(sph, (50.0, 50.0, 0.0));
    let o2 = h.create_query_obj(sph, (52.0, 50.0, 0.0));
    let o3 = h.create_query_obj(sph, (54.0, 50.0, 0.0));
    let o4 = h.create_query_obj(sph, (56.0, 50.0, 0.0));
    for &o in &[o1, o2, o3, o4] {
        let obj = h.kernel.objects.get_mut(o);
        obj.vision_range = 8.0;
    }
    h.kernel.objects.get_mut(o1).has_ai = true;
    h.kernel.objects.get_mut(o2).has_ai = true;
    h.kernel.objects.get_mut(o3).has_ai = true;
    // the onIdle latch: warm on the first update, fire on the second
    h.kernel.objects.get_mut(o1).ai_idle = true;
    h.kernel.objects.get_mut(o2).ai_idle = true;

    // the isPlayerActive refusal: assigning to the dead player's team
    // redirects to the NEUTRAL player's default team
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, o1, Some(td), &mut h.host);
    h.line(&format!("refused o1 t{}", h.kernel.objects.get(o1).team));
    h.dump_team_events();

    // the member choreography: the prepend order, the transitions, the
    // same-team no-op, the capture on a cross-player move
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, o1, Some(sa), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, o2, Some(sa), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, o2, Some(sa), &mut h.host); // no-op
    h.dump_teams();
    h.dump_team_events();

    // updateState #1: the created latch (the onCreate script, the
    // threshold compute over 2 members: 2 - 2*0.4 = 1.2 → 1), the
    // enemy-sighted check against the new chinese object
    let o5 = h.create_query_obj(sph, (57.0, 50.0, 0.0));
    h.kernel.objects.get_mut(o5).vision_range = 8.0;
    let tc = h
        .kernel
        .players
        .find_team("teamchina", &mut h.host)
        .unwrap();
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, o5, Some(tc), &mut h.host);
    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host); // settle the store
    h.kernel
        .players
        .update_team_states(&mut h.kernel.objects, &mut h.kernel.pm, &mut h.host);
    h.dump_teams();
    h.dump_team_events();

    // updateState #2: the enemy sighted (o1 sees o5 at distance 7 < 8),
    // the onIdle latch firing, then the all-clear path after o5's death
    h.kernel
        .players
        .update_team_states(&mut h.kernel.objects, &mut h.kernel.pm, &mut h.host);
    h.dump_teams();
    h.dump_team_events();
    h.kernel.objects.get_mut(o5).private_status |=
        1 << rzh_sim::types::OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT;
    h.kernel
        .players
        .update_team_states(&mut h.kernel.objects, &mut h.kernel.pm, &mut h.host);
    h.dump_teams();
    h.dump_team_events();

    // the onDestroyed trigger: mark o1/o2 dead → curUnits 0 ≤ threshold 1
    // fires, the -1 latch refuses the second fire;
    // notifyTeamOfObjectDeath on a unit death
    h.kernel.objects.get_mut(o1).private_status |=
        1 << rzh_sim::types::OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT;
    h.kernel
        .players
        .notify_team_of_object_death(sa, &mut h.host);
    h.kernel.objects.get_mut(o2).private_status |=
        1 << rzh_sim::types::OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT;
    h.kernel
        .players
        .update_team_states(&mut h.kernel.objects, &mut h.kernel.pm, &mut h.host);
    h.kernel
        .players
        .update_team_states(&mut h.kernel.objects, &mut h.kernel.pm, &mut h.host);
    h.dump_teams();
    h.dump_team_events();

    // the detach choreography: the swap back to the default team (the
    // capture event with the players), the null detach
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, o3, Some(sa), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, o3, Some(ta), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, o3, None, &mut h.host);
    h.dump_object_teams();
    h.dump_team_events();

    // the empty-team sweep: detach the (dead) members, then the sweep
    // deletes sa (non-singleton, not a default team, active, empty) —
    // with the preTeamDestroy walk, the override cleanup on the remaining
    // teams and the player relation cleanup
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, o1, None, &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, o2, None, &mut h.host);
    h.kernel
        .players
        .update_team_states(&mut h.kernel.objects, &mut h.kernel.pm, &mut h.host);
    h.dump_teams();
    h.dump_team_events();

    // a player-owning default team is spared by the sweep (the singleton
    // refusal and the default-team refusal): the inactive non-singleton
    // is spared too
    let _s2 = h
        .kernel
        .players
        .create_inactive_team("americaSquad", &mut h.host);
    h.kernel
        .players
        .update_team_states(&mut h.kernel.objects, &mut h.kernel.pm, &mut h.host);
    h.dump_teams();
}

// --- players_roundtrip: the integration corpus — objects on teams, the
// filters over the player state, the local switch, validateTeam -------
fn corpus_players_roundtrip(h: &mut Harness) {
    h.line("corpus players_roundtrip");
    h.fresh_world(10.0, (0.0, 0.0, 200.0, 200.0));
    let sides = SidesSpec {
        sides: vec![
            side_spec("", false, false, false, &[], &[]),
            side_spec("america", true, false, false, &["china"], &[]),
            side_spec("china", false, false, false, &["america"], &[]),
            side_spec("gla", false, false, false, &["america", "china"], &[]),
        ],
        teams: vec![
            team_spec("team", "neutral", true),
            team_spec("teamamerica", "america", true),
            team_spec("teamchina", "china", true),
            team_spec("teamgla", "gla", true),
            team_spec("americaSquad", "america", false),
        ],
        has_game_info: false,
        game_info: GameInfoSpec::default(),
    };
    h.kernel
        .players
        .new_game(&sides, &mut h.kernel.objects, &mut h.host);
    h.dump_team_events();

    let ta = h
        .kernel
        .players
        .find_team("teamamerica", &mut h.host)
        .unwrap();
    let tc = h
        .kernel
        .players
        .find_team("teamchina", &mut h.host)
        .unwrap();
    let tg = h.kernel.players.find_team("teamgla", &mut h.host).unwrap();
    let tneutral = h.kernel.players.find_team("team", &mut h.host).unwrap();
    let sa = h
        .kernel
        .players
        .create_team("americaSquad", &mut h.host)
        .unwrap();

    // the objects: three american (two on the default team, one on the
    // squad), two chinese, one gla, one neutral
    let sph = GeometryInfo {
        geom_type: GeometryType::Sphere,
        is_small: true,
        height: 1.0,
        major_radius: 1.0,
        minor_radius: 1.0,
    };
    let mut objs = Vec::new();
    for i in 0..7 {
        objs.push(h.create_query_obj(sph, (40.0 + 4.0 * (i as f32), 50.0, 0.0)));
    }
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, objs[0], Some(ta), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, objs[1], Some(ta), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, objs[2], Some(sa), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, objs[3], Some(tc), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, objs[4], Some(tc), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, objs[5], Some(tg), &mut h.host);
    h.kernel
        .players
        .set_team(&mut h.kernel.objects, objs[6], Some(tneutral), &mut h.host);
    h.dump_object_teams();
    h.dump_team_events();

    h.kernel
        .pm
        .update(&mut h.kernel.objects, &mut h.kernel.logic, &mut h.host); // settle the store

    // the filters over the player state: SamePlayer, Relationship (each
    // direction), AcceptOnTeam — over a range query around a1
    {
        let same_player = PartitionFilterSamePlayer::new(1);
        let allies = PartitionFilterRelationship::new(objs[0], RELATIONSHIP_FILTER_ALLOW_ALLIES);
        let enemies = PartitionFilterRelationship::new(objs[0], RELATIONSHIP_FILTER_ALLOW_ENEMIES);
        let neutral = PartitionFilterRelationship::new(objs[0], RELATIONSHIP_FILTER_ALLOW_NEUTRAL);
        let on_team = PartitionFilterAcceptOnTeam::new(sa);

        let fs1: [&dyn PartitionFilter; 1] = [&same_player];
        let fs2: [&dyn PartitionFilter; 1] = [&allies];
        let fs3: [&dyn PartitionFilter; 1] = [&enemies];
        let fs4: [&dyn PartitionFilter; 1] = [&neutral];
        let fs5: [&dyn PartitionFilter; 1] = [&on_team];

        // the same-player query: a1, a2 (a3 is on the squad team — same
        // controlling player, so it passes too)
        let (hit, dist, _) = h.kernel.pm.get_closest_object_from_object(
            &mut h.kernel.objects,
            &h.kernel.players,
            objs[0],
            100.0,
            DistanceCalculationType::FromCenter2d,
            &fs1,
        );
        h.line(&format!(
            "q same {} d={:08x}",
            if hit.is_some() { "hit" } else { "miss" },
            dist.to_bits()
        ));
        // the allies-of-a1 query: america considers china an enemy — the
        // same-player objects and (unassigned!) the gla one pass
        let (hit, dist, _) = h.kernel.pm.get_closest_object_from_object(
            &mut h.kernel.objects,
            &h.kernel.players,
            objs[0],
            100.0,
            DistanceCalculationType::FromCenter2d,
            &fs2,
        );
        h.line(&format!(
            "q allies {} d={:08x}",
            if hit.is_some() { "hit" } else { "miss" },
            dist.to_bits()
        ));
        // the enemies query: the chinese objects
        let (hit, dist, _) = h.kernel.pm.get_closest_object_from_object(
            &mut h.kernel.objects,
            &h.kernel.players,
            objs[0],
            100.0,
            DistanceCalculationType::FromCenter2d,
            &fs3,
        );
        h.line(&format!(
            "q enemies {} o{} d={:08x}",
            if hit.is_some() { "hit" } else { "miss" },
            hit.unwrap_or(-1),
            dist.to_bits()
        ));
        // the neutral query: the gla (unassigned) and neutral objects
        let (hit, dist, _) = h.kernel.pm.get_closest_object_from_object(
            &mut h.kernel.objects,
            &h.kernel.players,
            objs[0],
            100.0,
            DistanceCalculationType::FromCenter2d,
            &fs4,
        );
        h.line(&format!(
            "q neutral {} o{} d={:08x}",
            if hit.is_some() { "hit" } else { "miss" },
            hit.unwrap_or(-1),
            dist.to_bits()
        ));
        // the on-team query: the squad member
        let (hit, dist, _) = h.kernel.pm.get_closest_object_from_object(
            &mut h.kernel.objects,
            &h.kernel.players,
            objs[0],
            100.0,
            DistanceCalculationType::FromCenter2d,
            &fs5,
        );
        h.line(&format!(
            "q onteam {} o{} d={:08x}",
            if hit.is_some() { "hit" } else { "miss" },
            hit.unwrap_or(-1),
            dist.to_bits()
        ));

        // the enemy-sighted path over the squad (the team updateState
        // query with the same filters): a3 sees the chinese pair
        h.kernel.objects.get_mut(objs[2]).vision_range = 12.0;
        h.kernel
            .players
            .update_team_states(&mut h.kernel.objects, &mut h.kernel.pm, &mut h.host);
        h.dump_teams();
        h.dump_team_events();
    }

    // the local switch: the transitions fire; validateTeam resolves the
    // name (auto-creating the non-singleton) and falls back to the
    // neutral default team on a miss
    h.kernel.players.set_local_player(Some(2), &mut h.host);
    let v0 = h.kernel.players.validate_team("americaSquad", &mut h.host);
    let v1 = h.kernel.players.validate_team("nosuchteam", &mut h.host);
    let v2 = h.kernel.players.validate_team("teamchina", &mut h.host);
    h.line(&format!("v0 {v0}"));
    h.line(&format!("v1 {v1}"));
    h.line(&format!("v2 {v2}"));
    h.dump_team_events();
    h.dump_players();

    // the stale-relation reset over a populated world: the factory clear
    // walks the prototypes in key order (the map pre-emptied — only the
    // player half of teamAboutToBeDeleted runs), the player↔player
    // relations survive
    h.kernel.players.reset(&mut h.kernel.objects, &mut h.host);
    h.dump_players();
    h.dump_teams();
    h.dump_team_events();
    h.dump_object_teams();
}

fn corpus_roundtrip(h: &mut Harness) {
    h.line("corpus kernel_roundtrip");
    let g = geom(GeometryType::Sphere, true, 1.0, 2.5, 2.5);
    let big = geom(GeometryType::Box, false, 1.0, 8.0, 6.0);

    // a small mover with a module (waking every 2 frames)
    let a = h.create_object(g, false, 1, &[], (51.0, 51.0), 0.0, &[], 0);
    h.script_sleep(a, 0, 2);
    // a big rotated immobile structure
    h.create_object(big, true, 0, &[], (35.0, 35.0), 0.4, &[], 0);
    // a far mover with a module (every frame)
    let c = h.create_object(g, false, 1, &[], (90.0, 12.0), 0.0, &[], 0);
    h.script_sleep(c, 0, 1);

    for _ in 0..5 {
        // move a little each frame
        let old = h.kernel.objects.get(a).position;
        let (x, y) = (old.x + 3.0, old.y + 1.0);
        h.kernel.objects.get_mut(a).position = rzh_sim::types::Coord3 { x, y, z: 0.0 };
        h.kernel
            .react_to_transform_change(a, old, h.kernel.objects.get(a).orientation);
        h.kernel.pre_update();
        h.update();
        h.dump_events();
        h.dump_contacts();
    }
    h.dump_heap();
    h.dump_registry();
    h.dump_partition();

    // destroy c mid-run
    h.kernel.destroy_object(c, &mut h.host);
    h.update();
    h.dump_events();
    h.dump_heap();
    h.dump_registry();
    h.line("end");
}

// ============================================================================
// The driver.

// ============================================================================
// The query corpora (the third Phase 3 slice: the partition query family).
// ============================================================================

/// --- query_radiusvec: the FASTER_GCO ring precompute (calcMinRadius's
/// corner-pair minimum with the corner-touch and edge-share radii, the
/// maxGcoRadius diagonal bound, the y-outer/x-inner offset scan order) --
#[allow(clippy::too_many_lines)]
fn corpus_query_radiusvec(h: &mut Harness) {
    h.line("corpus query_radiusvec");
    {
        // (a) the 1x1-cell world: the whole offset space is the single
        // cell (corner-touch); rings 1..2 exist but stay empty
        h.fresh_world(10.0, (0.0, 0.0, 10.0, 10.0));
        let (cx, cy) = (h.kernel.pm.cell_count_x, h.kernel.pm.cell_count_y);
        h.line(&format!("a cells {cx},{cy}"));
        h.dump_radius_vec();
        // calcMinRadius probes: the same cell, the orthogonal neighbors
        // (edge share), the diagonal neighbors (corner touch), and the
        // beyond-diagonal offsets
        let probes: [(i32, i32); 15] = [
            (0, 0),
            (1, 0),
            (0, 1),
            (-1, 0),
            (0, -1),
            (1, 1),
            (-1, 1),
            (1, -1),
            (-1, -1),
            (2, 0),
            (2, 1),
            (2, 2),
            (3, 2),
            (-2, 1),
            (5, 7),
        ];
        for (x, y) in probes {
            let r = h.kernel.pm.calc_min_radius((x, y));
            h.line(&format!("minr {x},{y} {r}"));
        }
    }
    {
        // (b) a non-square fractional-cell grid (the f32 CEIL drift of
        // the count math rides along, pinned by both sides computing it)
        h.fresh_world(3.5, (0.0, 0.0, 21.0, 10.5));
        let (cx, cy) = (h.kernel.pm.cell_count_x, h.kernel.pm.cell_count_y);
        h.line(&format!("b cells {cx},{cy}"));
        h.dump_radius_vec();
    }
    {
        // (c) a mid-size square grid: the ring counts only (the ring
        // sum is the offset space) plus the first/last offsets of the
        // early rings — the scan order the walk takes
        h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));
        let (cx, cy) = (h.kernel.pm.cell_count_x, h.kernel.pm.cell_count_y);
        h.line(&format!("c cells {cx},{cy}"));
        let mut lines = Vec::new();
        {
            let pm = &h.kernel.pm;
            lines.push(format!(
                "c maxgco {} rings {}",
                pm.max_gco_radius,
                pm.radius_vec.len()
            ));
            for (r, v) in pm.radius_vec.iter().enumerate() {
                lines.push(format!("ring {r} n {}", v.len()));
            }
            for r in 1..=3 {
                let v = &pm.radius_vec[r];
                for &(x, y) in v.iter().take(4) {
                    lines.push(format!("r{r} head {x},{y}"));
                }
                for &(x, y) in v[v.len().saturating_sub(3)..].iter() {
                    lines.push(format!("r{r} tail {x},{y}"));
                }
            }
        }
        for l in lines {
            h.line(&l);
        }
    }
    h.line("end");
}

/// --- query_closest: getClosestObjects — the ring walk's early exit, the
/// strict < boundary (the exact-distance reject and the no-match
/// prefill), the same-cell tie (first walked wins), the boundary measures
/// (the clamped-zero and the shrunk-vector cases), the filters, and the
/// measure helpers --
#[allow(clippy::too_many_lines)]
// the rotated source's angle literal 1.5707963 is one ulp UNDER
// std::f32::consts::FRAC_PI_2 — clippy's approx_constant wants the
// constant, but the transcription's corpus (and the committed golden)
// pinned the literal's value; switching would desync the two sides
#[allow(clippy::approx_constant)]
fn corpus_query_closest(h: &mut Harness) {
    h.line("corpus query_closest");
    h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));
    let sph = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);
    let cyl3 = geom(GeometryType::Cylinder, false, 4.0, 3.0, 3.0);
    let cyl10 = geom(GeometryType::Cylinder, false, 4.0, 10.0, 10.0);

    let a = h.create_query_obj(sph, (50.0, 50.0, 0.0));
    let b = h.create_query_obj(sph, (65.0, 50.0, 0.0));
    h.create_query_obj(sph, (50.0, 35.0, 0.0)); // c: 15 south of a
    h.create_query_obj(sph, (60.0, 50.0, 0.0)); // d: 10 east of a, ring 1
    let e = h.create_query_obj(sph, (85.0, 85.0, 0.0));
    // the boundary-measure cylinders, kept away from the center so the
    // center queries stay unpolluted: m1 the r=3 pair partner, m2 the
    // r=10 overlap clamper, m3 the z-stepped 3d target
    h.create_query_obj(cyl3, (76.0, 12.0, 0.0)); // m1
    h.create_query_obj(cyl10, (56.0, 12.0, 0.0)); // m2
    let m3 = h.create_query_obj(cyl10, (70.0, 50.0, 5.0));
    // the same-cell equidistant pair (both exactly 5 from the query
    // position, both in cell (3,2) — the COI-list order decides)
    h.create_query_obj(sph, (30.0, 30.0, 0.0)); // s1 (registered first)
    h.create_query_obj(sph, (30.0, 20.0, 0.0)); // s2 (prepended: walked first)
    // the filter objects: f1 dies, e leaves the map, b carries an
    // anonymous kindof bit (bit 5 — the bit's meaning is the data
    // layer's; the filter semantics is what is pinned)
    let f1 = h.create_query_obj(sph, (42.0, 42.0, 0.0));
    h.kernel.objects.get_mut(f1).private_status |= 1 << OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT;
    h.kernel.objects.get_mut(e).private_status |= 1 << OBJECT_PRIVATE_OFF_MAP_BIT;
    h.kernel.objects.get_mut(b).kindof |= 1 << 5;

    // the store refresh: the COI coverage the queries walk
    h.refresh_store();

    // (a) from-object, wide: d wins at 10 — found in ring 1 (its cell is
    // the orthogonal neighbor), so the walk stops after ring 1 and the
    // farther b/c/e are never even walked
    {
        let (o, dist, vec) = h.kernel.pm.get_closest_object_from_object(
            &mut h.kernel.objects,
            &h.kernel.players,
            a,
            100.0,
            DistanceCalculationType::FromCenter2d,
            &[],
        );
        h.line(&format!(
            "a o{} dist={:08x} vec={:08x},{:08x},{:08x}",
            o.unwrap_or(INVALID_ID),
            dist.to_bits(),
            vec.x.to_bits(),
            vec.y.to_bits(),
            vec.z.to_bits()
        ));
    }
    // (b) from-position at a's spot: no self-exclusion — a itself wins at
    // distance zero (replacing m1's earlier ring-0 find within the walk)
    {
        let (o, dist, vec) = h.kernel.pm.get_closest_object_from_position(
            &mut h.kernel.objects,
            &h.kernel.players,
            Coord3 {
                x: 50.0,
                y: 50.0,
                z: 0.0,
            },
            100.0,
            DistanceCalculationType::FromCenter2d,
            &[],
        );
        h.line(&format!(
            "b o{} dist={:08x} vec={:08x},{:08x},{:08x}",
            o.unwrap_or(INVALID_ID),
            dist.to_bits(),
            vec.x.to_bits(),
            vec.y.to_bits(),
            vec.z.to_bits()
        ));
    }
    // (c) the strict boundary: from (55,50) both a (5 west) and d (5
    // east) are exactly maxDist away — 25 < 25 fails for both, so no
    // match; the outputs carry the sqrt of the initial bound and the
    // non-retail prefill of maxDist^3
    {
        let (o, dist, vec) = h.kernel.pm.get_closest_object_from_position(
            &mut h.kernel.objects,
            &h.kernel.players,
            Coord3 {
                x: 55.0,
                y: 50.0,
                z: 0.0,
            },
            5.0,
            DistanceCalculationType::FromCenter2d,
            &[],
        );
        h.line(&format!(
            "c o{} dist={:08x} vec={:08x},{:08x},{:08x}",
            o.unwrap_or(INVALID_ID),
            dist.to_bits(),
            vec.x.to_bits(),
            vec.y.to_bits(),
            vec.z.to_bits()
        ));
    }
    // (d) the same-cell tie: from (30,25) s1 and s2 are both exactly 5
    // away in cell (3,2); the COI list is newest-first, so s2 is walked
    // first and wins — s1 then fails the strict < against the tightened
    // bound
    {
        let (o, dist, vec) = h.kernel.pm.get_closest_object_from_position(
            &mut h.kernel.objects,
            &h.kernel.players,
            Coord3 {
                x: 30.0,
                y: 25.0,
                z: 0.0,
            },
            10.0,
            DistanceCalculationType::FromCenter2d,
            &[],
        );
        h.line(&format!(
            "d o{} dist={:08x} vec={:08x},{:08x},{:08x}",
            o.unwrap_or(INVALID_ID),
            dist.to_bits(),
            vec.x.to_bits(),
            vec.y.to_bits(),
            vec.z.to_bits()
        ));
    }
    // (e) the boundary-2d measure, the shrunk case: from the position
    // (70,12) m1's r=3 cylinder sits 6 away — the position side
    // contributes no radius, so the 3 shrinks the distance to 3 and the
    // vector by half (m2's 14-away r=10 pair shrinks to 4 — farther)
    {
        let (o, dist, vec) = h.kernel.pm.get_closest_object_from_position(
            &mut h.kernel.objects,
            &h.kernel.players,
            Coord3 {
                x: 70.0,
                y: 12.0,
                z: 0.0,
            },
            20.0,
            DistanceCalculationType::FromBoundingsphere2d,
            &[],
        );
        h.line(&format!(
            "e o{} dist={:08x} vec={:08x},{:08x},{:08x}",
            o.unwrap_or(INVALID_ID),
            dist.to_bits(),
            vec.x.to_bits(),
            vec.y.to_bits(),
            vec.z.to_bits()
        ));
    }
    // (f) the boundary-2d measure, the clamped case: from (56,20) the
    // r=10 cylinder m2 is 8 away — well inside the 11 of combined radii
    // — so the distance clamps to zero and the vector scales by the
    // zero factor (the -0.0 of the southward y is bit-visible)
    {
        let (o, dist, vec) = h.kernel.pm.get_closest_object_from_position(
            &mut h.kernel.objects,
            &h.kernel.players,
            Coord3 {
                x: 56.0,
                y: 20.0,
                z: 0.0,
            },
            12.0,
            DistanceCalculationType::FromBoundingsphere2d,
            &[],
        );
        h.line(&format!(
            "f o{} dist={:08x} vec={:08x},{:08x},{:08x}",
            o.unwrap_or(INVALID_ID),
            dist.to_bits(),
            vec.x.to_bits(),
            vec.y.to_bits(),
            vec.z.to_bits()
        ));
    }
    // (g) the boundary-3d measure: from a the d pair (two r=1 spheres
    // 10 apart) shrinks to 8 — beating m3's 11.2 (the z=5 cylinder's
    // 21.2 diagonal minus the 11 of combined spheres; its z-delta path
    // is pinned by the measure helpers below)
    {
        let (o, dist, vec) = h.kernel.pm.get_closest_object_from_object(
            &mut h.kernel.objects,
            &h.kernel.players,
            a,
            100.0,
            DistanceCalculationType::FromBoundingsphere3d,
            &[],
        );
        h.line(&format!(
            "g o{} dist={:08x} vec={:08x},{:08x},{:08x}",
            o.unwrap_or(INVALID_ID),
            dist.to_bits(),
            vec.x.to_bits(),
            vec.y.to_bits(),
            vec.z.to_bits()
        ));
    }
    // (h) the filters: the dead f1 is refused by Alive (and the raw
    // query finds it at zero), the off-map e by OnMap, everyone but b by
    // the kindof accept, b by the kindof reject, and everyone but e by
    // the same-map-status view from e
    {
        let q =
            |h: &mut Harness, pos: (f32, f32), max_dist: f32, filters: &[&dyn PartitionFilter]| {
                h.kernel
                    .pm
                    .get_closest_object_from_position(
                        &mut h.kernel.objects,
                        &h.kernel.players,
                        Coord3 {
                            x: pos.0,
                            y: pos.1,
                            z: 0.0,
                        },
                        max_dist,
                        DistanceCalculationType::FromCenter2d,
                        filters,
                    )
                    .0
                    .unwrap_or(INVALID_ID)
            };
        let raw = q(h, (42.0, 42.0), 6.0, &[]);
        h.line(&format!("h raw o{raw}"));
        let alive = PartitionFilterAlive;
        let f_alive: [&dyn PartitionFilter; 1] = [&alive];
        let got = q(h, (42.0, 42.0), 6.0, &f_alive);
        h.line(&format!("h alive o{got}"));
        let on_map = PartitionFilterOnMap;
        let f_on_map: [&dyn PartitionFilter; 1] = [&on_map];
        let got = q(h, (80.0, 80.0), 100.0, &f_on_map);
        h.line(&format!("h onmap o{got}"));
        let accept = PartitionFilterAcceptByKindOf::new(1 << 5, 0);
        let f_accept: [&dyn PartitionFilter; 1] = [&accept];
        let got = q(h, (65.0, 50.0), 100.0, &f_accept);
        h.line(&format!("h accept o{got}"));
        let reject = PartitionFilterRejectByKindOf::new(1 << 5, 0);
        let f_reject: [&dyn PartitionFilter; 1] = [&reject];
        let got = q(h, (65.0, 50.0), 100.0, &f_reject);
        h.line(&format!("h reject o{got}"));
        let same_map = PartitionFilterSameMapStatus::new(e);
        let f_same: [&dyn PartitionFilter; 1] = [&same_map];
        let got = q(h, (84.0, 84.0), 100.0, &f_same);
        h.line(&format!("h samemap o{got}"));
    }
    // (i) the measure helpers over all four dc types: the vector, the
    // distance and the goal distance to the z-stepped cylinder, and the
    // relative angles (the four quadrants, the coincident early-out and
    // a rotated source)
    {
        for dc in 0..4 {
            let dc = DistanceCalculationType::from_bits(dc);
            let vec = h.kernel.pm.get_vector_to(&h.kernel.objects, a, m3, dc);
            h.line(&format!(
                "i vec dc{} ={:08x},{:08x},{:08x}",
                dc.bits(),
                vec.x.to_bits(),
                vec.y.to_bits(),
                vec.z.to_bits()
            ));
            let (ds, vec2) = h
                .kernel
                .pm
                .get_distance_squared(&h.kernel.objects, a, m3, dc);
            h.line(&format!(
                "i ds dc{} {:08x} ={:08x},{:08x},{:08x}",
                dc.bits(),
                ds.to_bits(),
                vec2.x.to_bits(),
                vec2.y.to_bits(),
                vec2.z.to_bits()
            ));
            let pos = Coord3 {
                x: 70.0,
                y: 50.0,
                z: 5.0,
            };
            let (ds2, _) = h
                .kernel
                .pm
                .get_distance_squared_to_pos(&h.kernel.objects, a, &pos, dc);
            h.line(&format!("i dsp dc{} {:08x}", dc.bits(), ds2.to_bits()));
            let goal = Coord3 {
                x: 60.0,
                y: 60.0,
                z: 0.0,
            };
            let (gs, vec3) =
                h.kernel
                    .pm
                    .get_goal_distance_squared(&h.kernel.objects, a, &goal, m3, dc);
            h.line(&format!(
                "i gs dc{} {:08x} ={:08x},{:08x},{:08x}",
                dc.bits(),
                gs.to_bits(),
                vec3.x.to_bits(),
                vec3.y.to_bits(),
                vec3.z.to_bits()
            ));
            let (gs2, _) =
                h.kernel
                    .pm
                    .get_goal_distance_squared_to_pos(&h.kernel.objects, a, &goal, &pos, dc);
            h.line(&format!("i gsp dc{} {:08x}", dc.bits(), gs2.to_bits()));
        }
        let rot = h.create_query_obj_angle(sph, (30.0, 30.0, 0.0), 1.5707963);
        let angle_queries: [(i32, (f32, f32), &str); 7] = [
            (a, (60.0, 50.0), "east"),
            (a, (50.0, 60.0), "north"),
            (a, (50.0, 40.0), "south"),
            (a, (40.0, 50.0), "west"),
            (a, (50.0, 50.0), "same"),
            (rot, (40.0, 30.0), "roteast"),
            (rot, (30.0, 40.0), "rotnorth"),
        ];
        for (src, pos, tag) in angle_queries {
            let ang = h.kernel.pm.get_relative_angle_2d_to_pos(
                &h.kernel.objects,
                src,
                &Coord3 {
                    x: pos.0,
                    y: pos.1,
                    z: 0.0,
                },
            );
            h.line(&format!("i ang {tag} {:08x}", ang.to_bits()));
            let ango = h.kernel.pm.get_relative_angle_2d(&h.kernel.objects, src, a);
            h.line(&format!("i ango {tag} {:08x}", ango.to_bits()));
        }
    }
    h.line("end");
}

/// --- query_iterate: iterateObjectsInRange over the SimpleObjectIterator
/// (the FASTEST insertion order, the mergesort's tie stability in all four
/// sorted orders, the pre-reset empty quirk, reset), iterateAllObjects
/// (the master-list order and the filter set) and iteratePotentialCollisions
/// (the 1.1 slop gate vs the would-collide filter, the 2d/3d numeric split) --
#[allow(clippy::too_many_lines)]
fn corpus_query_iterate(h: &mut Harness) {
    h.line("corpus query_iterate");
    {
        h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));
        let sph = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);
        let big = geom(GeometryType::Sphere, false, 1.0, 25.0, 25.0);

        // the cluster: a at the center, b/c/d/e at 12 on the axes (the
        // four-way tie), f the big multi-cell sphere (the done-flag
        // dedup), g far out, h the kindof carrier; e dead, g off-map
        let a = h.create_query_obj(sph, (50.0, 50.0, 0.0));
        let b = h.create_query_obj(sph, (50.0, 62.0, 0.0));
        let c = h.create_query_obj(sph, (62.0, 50.0, 0.0));
        let d = h.create_query_obj(sph, (38.0, 50.0, 0.0));
        let e = h.create_query_obj(sph, (50.0, 38.0, 0.0));
        let f = h.create_query_obj(big, (55.0, 55.0, 0.0));
        let g = h.create_query_obj(sph, (95.0, 95.0, 0.0));
        let hh = h.create_query_obj(sph, (30.0, 70.0, 0.0));
        // the corpus programs the template-supplied inputs post-creation
        for (id, cost) in [
            (a, 1000.0),
            (b, 100.0),
            (c, 50.0),
            (d, 100.0),
            (e, 25.0),
            (f, 500.0),
            (g, 10.0),
            (hh, 50.0),
        ] {
            h.kernel.objects.get_mut(id).build_cost = cost;
        }
        h.kernel.objects.get_mut(e).private_status |= 1 << OBJECT_PRIVATE_EFFECTIVELY_DEAD_BIT;
        h.kernel.objects.get_mut(g).private_status |= 1 << OBJECT_PRIVATE_OFF_MAP_BIT;
        h.kernel.objects.get_mut(hh).kindof |= 1 << 5;

        h.refresh_store();

        let center = Coord3 {
            x: 50.0,
            y: 50.0,
            z: 0.0,
        };
        let range_from_pos = |h: &mut Harness,
                              pos: Coord3,
                              max_dist: f32,
                              dc: DistanceCalculationType,
                              order: IterOrderType|
         -> SimpleObjectIterator {
            h.kernel.pm.iterate_objects_in_range_from_position(
                &mut h.kernel.objects,
                &h.kernel.players,
                pos,
                max_dist,
                dc,
                &[],
                order,
            )
        };

        // (a) FASTEST: the ring-walk insertion order, numerics attached
        {
            let mut it = range_from_pos(
                h,
                center,
                20.0,
                DistanceCalculationType::FromCenter2d,
                IterOrderType::Fastest,
            );
            h.drain_iter(&mut it, "a fastest");
        }
        // (b) NEAR_TO_FAR: the sorted drain with the tie order (a 0,
        // f 50, then the four at 144 in the mergesort's stable order)
        {
            let mut it = range_from_pos(
                h,
                center,
                20.0,
                DistanceCalculationType::FromCenter2d,
                IterOrderType::SortedNearToFar,
            );
            h.drain_iter(&mut it, "b near");
        }
        // (c) FAR_TO_NEAR
        {
            let mut it = range_from_pos(
                h,
                center,
                20.0,
                DistanceCalculationType::FromCenter2d,
                IterOrderType::SortedFarToNear,
            );
            h.drain_iter(&mut it, "c far");
        }
        // (d) the cost orders over the same cluster
        {
            let mut it = range_from_pos(
                h,
                center,
                20.0,
                DistanceCalculationType::FromCenter2d,
                IterOrderType::SortedCheapToExpensive,
            );
            h.drain_iter(&mut it, "d cheap");
        }
        {
            let mut it = range_from_pos(
                h,
                center,
                20.0,
                DistanceCalculationType::FromCenter2d,
                IterOrderType::SortedExpensiveToCheap,
            );
            h.drain_iter(&mut it, "d exp");
        }
        // (e) the no-clamp branch: maxDist >= HUGE_DIST walks every ring
        // of the map — the whole cluster with its distances (the
        // master-list-free ring walk reaches everything)
        {
            let mut it = range_from_pos(
                h,
                center,
                1_000_000.0,
                DistanceCalculationType::FromCenter2d,
                IterOrderType::Fastest,
            );
            h.drain_iter(&mut it, "e huge");
        }
        // (f) iterateAllObjects: the master-list prepend order (newest
        // first), then the filter refusals over the same walk
        {
            let mut it =
                h.kernel
                    .pm
                    .iterate_all_objects(&mut h.kernel.objects, &h.kernel.players, &[]);
            h.drain_iter(&mut it, "f all");
        }
        {
            let on_map = PartitionFilterOnMap;
            let alive = PartitionFilterAlive;
            let filters: [&dyn PartitionFilter; 2] = [&on_map, &alive];
            let mut it =
                h.kernel
                    .pm
                    .iterate_all_objects(&mut h.kernel.objects, &h.kernel.players, &filters);
            h.drain_iter(&mut it, "f onmapalive");
        }
        {
            let accept = PartitionFilterAcceptByKindOf::new(1 << 5, 0);
            let filters: [&dyn PartitionFilter; 1] = [&accept];
            let mut it =
                h.kernel
                    .pm
                    .iterate_all_objects(&mut h.kernel.objects, &h.kernel.players, &filters);
            h.drain_iter(&mut it, "f accept");
        }
        {
            let reject = PartitionFilterRejectByKindOf::new(1 << 5, 0);
            let filters: [&dyn PartitionFilter; 1] = [&reject];
            let mut it =
                h.kernel
                    .pm
                    .iterate_all_objects(&mut h.kernel.objects, &h.kernel.players, &filters);
            h.drain_iter(&mut it, "f reject");
        }
        // (g) the from-object form: a excludes itself, everything else
        // within 20 stays
        {
            let mut it = h.kernel.pm.iterate_objects_in_range_from_object(
                &mut h.kernel.objects,
                &h.kernel.players,
                a,
                20.0,
                DistanceCalculationType::FromCenter2d,
                &[],
                IterOrderType::Fastest,
            );
            h.drain_iter(&mut it, "g fromobj");
        }
        // (h) the boundary measure in the range query: the 2d sphere
        // radii pull the far-apart big sphere in (its boundary touches
        // the center's)
        {
            let mut it = range_from_pos(
                h,
                center,
                30.0,
                DistanceCalculationType::FromBoundingsphere2d,
                IterOrderType::SortedNearToFar,
            );
            h.drain_iter(&mut it, "h bnd");
        }
    }
    {
        // (i) iteratePotentialCollisions: the small rotated box against a
        // z-stepped cluster — the 1.1-slop dist gate (the far sphere
        // never reaches the filter; the at-gate sphere reaches it and is
        // refused), the would-collide pass at z=3.9 with the 2d/3d
        // numeric split, and the centered overlap
        h.fresh_world(10.0, (0.0, 0.0, 100.0, 100.0));
        let sph = geom(GeometryType::Sphere, true, 1.0, 1.0, 1.0);
        let bx = geom(GeometryType::Box, false, 4.0, 3.0, 2.0);

        h.create_query_obj(sph, (50.0, 50.0, 0.0)); // a: the centered overlap
        h.create_query_obj(sph, (53.0, 50.0, 3.9)); // k1: the z-stepped overlap
        h.create_query_obj(sph, (56.0, 50.0, 0.0)); // k2: past the box edge — filter refuses
        h.create_query_obj(sph, (60.0, 50.0, 0.0)); // k3: past the slop gate — never filtered

        h.refresh_store();

        let pos = Coord3 {
            x: 50.0,
            y: 50.0,
            z: 0.0,
        };
        {
            let mut it = h.kernel.pm.iterate_potential_collisions(
                &mut h.kernel.objects,
                &h.kernel.players,
                pos,
                bx,
                0.3,
                true,
            );
            h.drain_iter(&mut it, "i 2d");
        }
        {
            let mut it = h.kernel.pm.iterate_potential_collisions(
                &mut h.kernel.objects,
                &h.kernel.players,
                pos,
                bx,
                0.3,
                false,
            );
            h.drain_iter(&mut it, "i 3d");
        }
    }
    h.line("end");
}

// ============================================================================

#[test]
fn kernel_golden() {
    let mut all = String::new();
    let corpora: [fn(&mut Harness); 31] = [
        corpus_ids,
        corpus_frame,
        corpus_sched_basic,
        corpus_sched_ties,
        corpus_sched_phases,
        corpus_sched_awaken,
        corpus_sched_disabled,
        corpus_sched_destroy,
        corpus_sched_destroy_cap,
        corpus_sched_loadpost,
        corpus_part_grid,
        corpus_part_maxcoi,
        corpus_part_small,
        corpus_part_circle,
        corpus_part_rect,
        corpus_part_dirty,
        corpus_part_contact,
        corpus_part_register,
        corpus_collide_matrix,
        corpus_collide_process,
        corpus_query_radiusvec,
        corpus_query_closest,
        corpus_query_iterate,
        corpus_players_lifecycle,
        corpus_players_newgame,
        corpus_teams_factory,
        corpus_relationships,
        corpus_players_masks,
        corpus_team_members,
        corpus_players_roundtrip,
        corpus_roundtrip,
    ];
    for corpus in corpora {
        let mut h = Harness::new(10.0, (0.0, 0.0, 100.0, 100.0));
        corpus(&mut h);
        all.push_str(&h.out);
    }

    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("kernel.txt");
    let golden = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", golden_path.display()));

    let golden_lines: Vec<&str> = golden.lines().collect();
    let produced_lines: Vec<&str> = all.lines().collect();
    assert_eq!(
        golden_lines.len(),
        produced_lines.len(),
        "golden has {} lines, produced {}",
        golden_lines.len(),
        produced_lines.len()
    );
    let mut mismatches = 0;
    for (i, (g, p)) in golden_lines.iter().zip(produced_lines.iter()).enumerate() {
        if g != p {
            mismatches += 1;
            if mismatches <= 40 {
                eprintln!("line {}:\n  golden:   {g}\n  produced: {p}", i + 1);
            }
        }
    }
    assert_eq!(mismatches, 0, "{mismatches} mismatches against the golden");
}
