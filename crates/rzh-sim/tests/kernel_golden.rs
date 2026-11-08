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
//! refusals; the contact pairs' dedup and LIFO order.

use std::fmt::Write as _;
use std::path::PathBuf;

use rzh_sim::logic::{Kernel, KernelHost, ObjectSpec, Objects, UpdateModuleSpec, UpdateOutcome};
use rzh_sim::partition::Region2;
use rzh_sim::scheduler::ModuleArena;
use rzh_sim::types::{
    DisabledMask, DisabledType, FOREVER, GeometryInfo, GeometryType, INVALID_ID,
    LOGICFRAMES_PER_MSEC_REAL, LOGICFRAMES_PER_SECOND, LOGICFRAMES_PER_SECONDS_REAL,
    MSEC_PER_LOGICFRAME_REAL, SECONDS_PER_LOGICFRAME_REAL, SleepyUpdatePhase, UPDATE_SLEEP_FOREVER,
    UPDATE_SLEEP_NONE, WW_SYNC_MILLISECONDS,
};

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

/// The golden host: the ordered event stream plus the scripted modules.
struct GoldenHost {
    engine_time_frozen: bool,
    events: Vec<(u8, i64, i64)>,
    scripted: Vec<(i32, u32, ScriptedBody)>,
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
}

impl Harness {
    fn new(cell_size: f32, extents: (f32, f32, f32, f32)) -> Self {
        let region = Region2 {
            lo_x: extents.0,
            lo_y: extents.1,
            hi_x: extents.2,
            hi_y: extents.3,
        };
        Self {
            kernel: Kernel::with_grid(cell_size, region),
            host: GoldenHost {
                engine_time_frozen: false,
                events: Vec::new(),
                scripted: Vec::new(),
            },
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
    let a = h.create_object(g, false, 0, &[], (0.0, 0.0), 0.0, &[]);
    let b = h.create_object(g, false, 0, &[], (0.0, 0.0), 0.0, &[]);
    let c = h.create_object(g, false, 0, &[], (0.0, 0.0), 0.0, &[]);
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
    h.create_object(g, false, 1, &[], (0.0, 0.0), 0.0, &[]);

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
    let a = h.create_object(g, false, 2, &[], (0.0, 0.0), 0.0, &[]);
    // object born with constructor wakes (the not-in-list path)
    let b = h.create_object(g, false, 2, &[3, 5], (0.0, 0.0), 0.0, &[]);
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

    let a = h.create_object(g, false, 5, &[], (0.0, 0.0), 0.0, &[]);
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
    let a = h.create_object(g, false, 4, &[], (0.0, 0.0), 0.0, &phases);
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
    let a = h.create_object(g, false, 3, &[], (0.0, 0.0), 0.0, &[]);
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
    let a = h.create_object(g, false, 2, &[1, 1], (50.0, 50.0), 0.0, &[]);
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

    let a = h.create_object(g, false, 3, &[], (50.0, 50.0), 0.0, &[]);
    let b = h.create_object(g, false, 3, &[], (60.0, 60.0), 0.0, &[]);
    let c = h.create_object(g, false, 3, &[], (70.0, 70.0), 0.0, &[]);
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
    let a = h.create_object(g, false, 256, &[], (50.0, 50.0), 0.0, &[]);
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
    let a = h.create_object(g, false, 3, &[], (50.0, 50.0), 0.0, &[]);
    let b = h.create_object(g, false, 2, &[], (60.0, 60.0), 0.0, &[]);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    );
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
    h.dump_partition();
    h.line("end");
}

fn corpus_part_dirty(h: &mut Harness) {
    h.line("corpus part_dirty");
    let g = geom(GeometryType::Sphere, false, 1.0, 3.0, 3.0);
    let a = h.create_object(g, false, 0, &[], (55.0, 55.0), 0.0, &[]);
    let b = h.create_object(g, false, 0, &[], (35.0, 35.0), 0.0, &[]);
    h.dump_partition(); // both dirty, LIFO order

    // the half-update path: makeDirty(false) on a clean module
    h.kernel.pm.make_dirty(&mut h.kernel.objects, a, false);
    h.dump_partition();

    // the upgrade path
    h.kernel.pm.make_dirty(&mut h.kernel.objects, a, true);
    h.dump_partition();

    // the refresh: LIFO processing (b then a)
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
    h.dump_events();
    h.dump_partition();

    // a re-dirty while clean, and the idempotent re-dirty while in-list
    h.kernel.pm.make_dirty(&mut h.kernel.objects, a, true);
    h.kernel.pm.make_dirty(&mut h.kernel.objects, a, true);
    h.dump_partition();
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);

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
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
    h.dump_partition();

    // setGeometryInfo's unregister/re-register refresh
    h.kernel.objects.get_mut(a).geometry = geom(GeometryType::Box, false, 1.0, 8.0, 4.0);
    h.kernel.pm.unregister_object(&mut h.kernel.objects, a);
    h.kernel.pm.register_object(&mut h.kernel.objects, a);
    h.dump_partition();
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    h.create_object(g, false, 0, &[], (50.0, 50.0), 0.0, &[]);
    h.create_object(g, false, 0, &[], (52.0, 52.0), 0.0, &[]);
    h.create_object(g, false, 0, &[], (60.0, 60.0), 0.0, &[]);
    h.create_object(g, true, 0, &[], (50.0, 50.0), 0.0, &[]);
    h.create_object(g, false, 0, &[], (90.0, 90.0), 0.0, &[]);

    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
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
    h.kernel.pm.update(&mut h.kernel.objects, &mut h.host);
    h.dump_contacts();
    h.dump_partition();
    h.line("end");
}

fn corpus_part_register(h: &mut Harness) {
    h.line("corpus part_register");
    let g = geom(GeometryType::Sphere, false, 1.0, 3.0, 3.0);
    let a = h.create_object(g, false, 0, &[], (55.0, 55.0), 0.0, &[]);
    let b = h.create_object(g, false, 0, &[], (35.0, 35.0), 0.0, &[]);
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

fn corpus_roundtrip(h: &mut Harness) {
    h.line("corpus kernel_roundtrip");
    let g = geom(GeometryType::Sphere, true, 1.0, 2.5, 2.5);
    let big = geom(GeometryType::Box, false, 1.0, 8.0, 6.0);

    // a small mover with a module (waking every 2 frames)
    let a = h.create_object(g, false, 1, &[], (51.0, 51.0), 0.0, &[]);
    h.script_sleep(a, 0, 2);
    // a big rotated immobile structure
    h.create_object(big, true, 0, &[], (35.0, 35.0), 0.4, &[]);
    // a far mover with a module (every frame)
    let c = h.create_object(g, false, 1, &[], (90.0, 12.0), 0.0, &[]);
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

#[test]
fn kernel_golden() {
    let mut all = String::new();
    let corpora: [fn(&mut Harness); 19] = [
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
