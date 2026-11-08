//! The partition manager's spatial store — the cell grid, the per-object
//! coverage and the dirty-module refresh.
//!
//! Origin: `PartitionManager.cpp` / `PartitionManager.h` at baseline
//! 571a7c5 — `init` (the cell-size clamp, the double-division
//! `cellSizeInv`, the zero-extent patch, the CEIL cell counts),
//! `shutdown`, `update` (the dirty-module refresh with its LIFO
//! processing, the update-cells / collision-check gates, the
//! KINDOF_IMMOBILE collision refusal and the contact-list pair
//! collection), `registerObject` / `unRegisterObject` (the master-list
//! prepend, the double-register refusal), `PartitionData`
//! (`calcMaxCoiForShape`, `allocCoiArray` / `freeCoiArray`,
//! `makeDirty`'s status upgrades, `removeAllTouchedCells`,
//! `addSubPixToCoverage`'s rescan, `updateCellsTouched` with
//! `doSmallFill`, `doCircleFillPrecise` — the non-retail Stubbjax
//! 29/01/2026 branch — and `doRectFill`, the last-cell change hook and
//! the shroudedness invalidation), `CellAndObjectIntersection`'s
//! coverage (the cell-list prepend and ordered unlink) and
//! `PartitionContactList::addToContactList` (the `hash2ints` symmetric
//! bucket hash, the either-order dedup, the LIFO pair list).
//!
//! Structural deviations, behaviour-preserving (stated per plan §3.3
//! rule 2):
//! - the intrusive doubly-linked COI lists become per-cell
//!   `Vec<ObjectID>`s with the same prepend order (an ordered removal
//!   instead of an O(1) unlink — cell lists are a handful of entries);
//! - the dirty and master linked lists become `VecDeque`s with the same
//!   head-at-front order;
//! - the `PartitionData` lives inside its object (the C++ pools it
//!   separately with a back-pointer — the handle is the object id);
//! - the contact list's 5381-bucket hash is replaced by the exact
//!   either-order membership scan it accelerates (the dedup result is
//!   identical);
//! - ghost objects (the fog-memory half of `unRegisterObject`), the
//!   shroud/threat/value cell state beyond the ctor defaults, and
//!   `processContactList`'s collide-test family are later slices.
//!
//! The float paths run on the Hoffesommer conversions
//! ([`crate::fastmath`]) and the pinned GameMath trig
//! (`rzh_math::gmath`) exactly as the deterministic build does.
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use std::collections::VecDeque;

use rzh_math::gmath::{cosf, sinf};
use rzh_math::scalar::sqrt;

use crate::fastmath::{real_to_int_ceil, real_to_int_floor};
use crate::logic::{KernelHost, Objects};
use crate::types::{GeometryType, MAX_PLAYER_COUNT, ObjectID, ObjectShroudStatus};

/// `ShroudLevel` (PartitionManager.h): the per-cell, per-player counters.
/// Defaults are "passive shroud" — 1, 0. The looker/shrouder state
/// transitions are the shroud slice's.
#[derive(Clone, Copy, Debug)]
pub struct ShroudLevel {
    /// `m_currentShroud`.
    pub current_shroud: i32,
    /// `m_activeShroudLevel`.
    pub active_shroud_level: i32,
}

impl Default for ShroudLevel {
    fn default() -> Self {
        Self {
            current_shroud: 1,
            active_shroud_level: 0,
        }
    }
}

/// `PartitionCell`: the fundamental unit of space. The COI list keeps
/// the prepend order (newest first) the original's intrusive list has.
#[derive(Clone, Debug, Default)]
pub struct PartitionCell {
    /// The objects touching this cell, newest first
    /// (`m_firstCoiInCell`'s list).
    pub cois: Vec<ObjectID>,
    /// `m_shroudLevel[MAX_PLAYER_COUNT]` (the ctor defaults are pinned
    /// by the golden dumps; the transitions are the shroud slice's).
    pub shroud_level: [ShroudLevel; MAX_PLAYER_COUNT],
    /// `m_cellX`.
    pub cell_x: i16,
    /// `m_cellY`.
    pub cell_y: i16,
}

/// `PartitionData::DirtyStatus`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DirtyStatus {
    /// `NOT_DIRTY`.
    #[default]
    NotDirty = 0,
    /// `NEED_COLLISION_CHECK`.
    NeedCollisionCheck = 1,
    /// `NEED_CELL_UPDATE_AND_COLLISION_CHECK`.
    NeedCellUpdateAndCollisionCheck = 2,
}

/// `PartitionData`: the part of an object that maintains it in the space
/// partitioning system. Owned by the object; the manager's master and
/// dirty lists reference it by object id.
#[derive(Debug)]
pub struct PartitionData {
    /// `m_coiArray` + `m_coiArrayCount`: the COI slots (the cell each
    /// covers, or `None` when the slot is free). The array length is the
    /// allocated count.
    pub coi_array: Vec<Option<u32>>,
    /// `m_coiInUseCount`.
    pub coi_in_use: usize,
    /// `m_dirtyStatus`.
    pub dirty_status: DirtyStatus,
    /// `m_lastCell`: the last cell the object's center was in (`None`
    /// before the first refresh — the original's null pointer).
    pub last_cell: Option<u32>,
    /// `m_shroudedness[MAX_PLAYER_COUNT]` (born INVALID; the
    /// recomputation is the shroud slice's).
    pub shroudedness: [ObjectShroudStatus; MAX_PLAYER_COUNT],
    /// `m_shroudednessPrevious[MAX_PLAYER_COUNT]`.
    pub shroudedness_previous: [ObjectShroudStatus; MAX_PLAYER_COUNT],
    /// `m_everSeenByPlayer[MAX_PLAYER_COUNT]`.
    pub ever_seen: [bool; MAX_PLAYER_COUNT],
}

impl Default for PartitionData {
    fn default() -> Self {
        Self {
            coi_array: Vec::new(),
            coi_in_use: 0,
            dirty_status: DirtyStatus::NotDirty,
            last_cell: None,
            shroudedness: [ObjectShroudStatus::Invalid; MAX_PLAYER_COUNT],
            shroudedness_previous: [ObjectShroudStatus::Invalid; MAX_PLAYER_COUNT],
            ever_seen: [false; MAX_PLAYER_COUNT],
        }
    }
}

/// A world rectangle (`Region2D` / the terrain extents).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Region2 {
    /// The low corner's x.
    pub lo_x: f32,
    /// The low corner's y.
    pub lo_y: f32,
    /// The high corner's x.
    pub hi_x: f32,
    /// The high corner's y.
    pub hi_y: f32,
}

impl Region2 {
    /// `Region2D::width()`.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.hi_x - self.lo_x
    }
    /// `Region2D::height()`.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.hi_y - self.lo_y
    }
}

/// The collected collision-candidate pairs of one refresh (the
/// transcription's `PartitionContactList`; the pairs land in
/// [`PartitionManager::last_contacts`] in the original's list order).
///
/// The dedup is the original's exact structure: the symmetric
/// `hash2ints` bucket hash (5381 sockets, chained), so "already noted
/// in either order" is the bucket walk — not a linear scan.
pub(crate) struct ContactList {
    /// The pairs in add order; reversed at collection end.
    pairs: Vec<(ObjectID, ObjectID)>,
    /// `m_contactHash[SOCKET_COUNT]`: the bucket heads (node indices,
    /// `u32::MAX` = empty).
    sockets: Vec<u32>,
    /// The chain links (`m_nextHash` per node).
    chain: Vec<u32>,
}

impl ContactList {
    /// A fresh list (the per-update stack object: the bucket array is
    /// the original's memset).
    pub(crate) fn new() -> Self {
        Self {
            pairs: Vec::new(),
            sockets: vec![u32::MAX; crate::types::CONTACT_LIST_SOCKET_COUNT],
            chain: Vec::new(),
        }
    }

    /// `addToContactList(obj, other)`: skip self-pairs; walk the
    /// `hash2ints` bucket for the either-order dedup; otherwise record
    /// (the node prepends to its bucket).
    pub(crate) fn add(&mut self, obj: ObjectID, other: ObjectID) {
        if obj == other {
            return;
        }

        // compute hash index based on object's ids.
        let hash_value = (crate::types::hash2ints(obj, other)
            % crate::types::CONTACT_LIST_SOCKET_COUNT as u32) as usize;

        // make sure given hit has not already been recorded
        let mut node = self.sockets[hash_value];
        while node != u32::MAX {
            let (a, b) = self.pairs[node as usize];
            if (a == obj && b == other) || (a == other && b == obj) {
                // already noted
                return;
            }
            node = self.chain[node as usize];
        }

        // new hit: add to hash table (the bucket prepend) and to the
        // pair list
        let idx = self.pairs.len() as u32;
        self.pairs.push((obj, other));
        self.chain.push(self.sockets[hash_value]);
        self.sockets[hash_value] = idx;
    }
}

/// The partition manager: the grid, the per-object module registry and
/// the dirty refresh.
#[derive(Debug, Default)]
pub struct PartitionManager {
    /// `m_cellSize` (after the < 1 clamp).
    pub cell_size: f32,
    /// `m_cellSizeInv` — `(Real)(1.0 / m_cellSize)`: the double division
    /// then the f32 narrowing.
    pub cell_size_inv: f32,
    /// `m_cellCountX`.
    pub cell_count_x: i32,
    /// `m_cellCountY`.
    pub cell_count_y: i32,
    /// `m_totalCellCount`.
    pub total_cell_count: i32,
    /// `m_cells` (row-major: `y * cellCountX + x`).
    pub cells: Vec<PartitionCell>,
    /// `m_worldExtents`.
    pub world_extents: Region2,
    /// `m_moduleList`: the master list, newest registration first (the
    /// prepend order the queries walk).
    pub module_list: VecDeque<ObjectID>,
    /// `m_dirtyModules`: the refresh worklist, most-recently-dirtied
    /// first (the LIFO processing order).
    pub dirty_modules: VecDeque<ObjectID>,
    /// `m_updatedSinceLastReset`.
    pub updated_since_last_reset: bool,
    /// The pairs the last [`Self::update`] collected, in contact-list
    /// order (the dump observable; `processContactList` is the collision
    /// slice's).
    pub last_contacts: Vec<(ObjectID, ObjectID)>,
}

impl PartitionManager {
    /// `init`: build the grid. `partitionCellSize` is
    /// `TheGlobalData->m_partitionCellSize` (the INI-fed value — the
    /// store takes it as a parameter; the data layer owns the source).
    pub fn init(&mut self, partition_cell_size: f32, world_extents: Region2) {
        self.cell_size = partition_cell_size;
        if self.cell_size < 1.0 {
            self.cell_size = 1.0;
        }

        self.cell_size_inv = (1.0_f64 / f64::from(self.cell_size)) as f32;

        self.world_extents = world_extents;
        // you'd think it wouldn't be legal for terrainlogic to have zero area, but apparently
        // that's what it resets itself to. let's just make the world a simple place and pretend
        // it's nonzero in area, so that we can proceed without crashing.
        if self.world_extents.width() < 1.0 {
            self.world_extents.hi_x = self.world_extents.lo_x + 1.0;
        }
        if self.world_extents.height() < 1.0 {
            self.world_extents.hi_y = self.world_extents.lo_y + 1.0;
        }
        self.cell_count_x = real_to_int_ceil(self.world_extents.width() * self.cell_size_inv);
        self.cell_count_y = real_to_int_ceil(self.world_extents.height() * self.cell_size_inv);
        self.total_cell_count = self.cell_count_x * self.cell_count_y;
        let count = if self.total_cell_count > 0 {
            self.total_cell_count as usize
        } else {
            0
        };
        self.cells.clear();
        self.cells.resize(count, PartitionCell::default());
        for x in 0..self.cell_count_x {
            for y in 0..self.cell_count_y {
                if let Some(cell) = self.cell_at_mut(x, y) {
                    cell.cell_x = x as i16;
                    cell.cell_y = y as i16;
                }
            }
        }
    }

    /// `shutdown`: clear everything.
    pub fn shutdown(&mut self) {
        self.updated_since_last_reset = false;
        self.remove_all_dirty_modules();

        self.cells.clear();

        self.cell_size = 0.0;
        self.cell_size_inv = 0.0;
        self.cell_count_x = 0;
        self.cell_count_y = 0;
        self.total_cell_count = 0;
        self.world_extents = Region2::default();
    }

    /// `worldToCell`: FLOOR of the offset-scaled world point.
    #[must_use]
    pub fn world_to_cell(&self, wx: f32, wy: f32) -> (i32, i32) {
        (
            real_to_int_floor((wx - self.world_extents.lo_x) * self.cell_size_inv),
            real_to_int_floor((wy - self.world_extents.lo_y) * self.cell_size_inv),
        )
    }

    /// `worldToCellDist`: CEIL of the scaled distance.
    #[must_use]
    pub fn world_to_cell_dist(&self, w: f32) -> i32 {
        real_to_int_ceil(w * self.cell_size_inv)
    }

    /// `getCellAt`: the cell index, or `None` off the grid.
    #[must_use]
    pub fn cell_at(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.cell_count_x || y >= self.cell_count_y {
            return None;
        }
        Some(y as usize * self.cell_count_x as usize + x as usize)
    }

    /// `getCellAt` (mutable), internal.
    fn cell_at_mut(&mut self, x: i32, y: i32) -> Option<&mut PartitionCell> {
        let idx = self.cell_at(x, y)?;
        self.cells.get_mut(idx)
    }

    /// The cell's (x, y), for a cell index (the dump coordinate).
    #[must_use]
    pub fn cell_xy(&self, index: u32) -> (i32, i32) {
        let cell = &self.cells[index as usize];
        (i32::from(cell.cell_x), i32::from(cell.cell_y))
    }

    /// `removeAllDirtyModules` (shutdown's).
    fn remove_all_dirty_modules(&mut self) {
        self.dirty_modules.clear();
    }

    /// `removeFromDirtyModules(mod)`: unlink from the worklist and clear
    /// the module's dirty status.
    fn remove_from_dirty(&mut self, objects: &mut Objects, id: ObjectID) {
        if let Some(pos) = self.dirty_modules.iter().position(|&d| d == id) {
            self.dirty_modules.remove(pos);
        }
        objects.partition_mut(id).dirty_status = DirtyStatus::NotDirty;
    }

    /// `registerObject`: the double-register refusal, the master-list
    /// prepend and the attach.
    pub fn register_object(&mut self, objects: &mut Objects, id: ObjectID) {
        // if object is already part of this system get out of here
        if objects.get(id).partition.is_some() {
            return;
        }

        // allocate a new module of partition data and link it to the
        // list in the partition manager (prepend)
        objects.get_mut(id).partition = Some(PartitionData::default());
        self.module_list.push_front(id);

        // add module to object
        self.attach_to_object(objects, id);
    }

    /// `unRegisterObject`: the detach, the master-list unlink and the
    /// destructor's dirty-list cleanup. (The ghost branch — "if any
    /// players have a fogged memory of this object, we can't remove it
    /// from the shroud system just yet" — needs ghost objects and is
    /// never taken in this slice.)
    pub fn unregister_object(&mut self, objects: &mut Objects, id: ObjectID) {
        // get the partition module
        if objects.get(id).partition.is_none() {
            return;
        }

        // detach the module from the object
        self.detach_from_object(objects, id);

        // remove module from master list
        if let Some(pos) = self.module_list.iter().position(|&m| m == id) {
            self.module_list.remove(pos);
        }

        // deleteInstance(mod) → ~PartitionData's dirty-list cleanup (the
        // free_coi_array inside detach_from_object re-dirtied the module)
        if let Some(pos) = self.dirty_modules.iter().position(|&d| d == id) {
            self.dirty_modules.remove(pos);
        }
        objects.get_mut(id).partition = None;
    }

    /// `attachToObject`: remember the owner, (re)calc the COI bound,
    /// (re)alloc the array, mark dirty.
    fn attach_to_object(&mut self, objects: &mut Objects, id: ObjectID) {
        // (the ghost-object creation for IMMOBILE drawables is the
        // fog-memory slice; not taken here)

        // (re)calc maxCoi and (re)alloc cois
        self.free_coi_array(objects, id);
        self.alloc_coi_array(objects, id); // may throw! (allocation failure aborts)

        self.make_dirty(objects, id, true);
    }

    /// `detachFromObject`: the coverage removal and the array free. (The
    /// contact-list cleanup hook only matters mid-refresh, which no
    /// reachable path of this slice produces.)
    fn detach_from_object(&mut self, objects: &mut Objects, id: ObjectID) {
        self.remove_all_touched_cells(objects, id);
        self.free_coi_array(objects, id);

        // no longer attached to object
        objects.get_mut(id).partition = None;
    }

    /// `calcMaxCoiForShape`: the COI slot bound. The geometry code is
    /// the raw C++ ordinal (out-of-range values take the `default:`
    /// branch's 4). The RETAIL_COMPATIBLE_CRC fallthrough from
    /// sphere/cylinder into the BOX diagonal is dropped per plan §3.3
    /// rule 1; so is the commented-out small-shape shortcut
    /// (M Lorenzen 8/26/03).
    #[must_use]
    pub fn calc_max_coi_for_shape(&self, geom: i32, major_radius: f32, minor_radius: f32) -> i32 {
        let result = match geom {
            0 | 1 => {
                // GEOMETRY_SPHERE, GEOMETRY_CYLINDER:
                // note that majorRadius is a radius, not a diameter.
                // this actually allocates a few too many, but that's ok.
                let cells = self.world_to_cell_dist(major_radius * 2.0) + 1;
                cells * cells
            }
            2 => {
                // GEOMETRY_BOX
                let diagonal = sqrt(major_radius * major_radius + minor_radius * minor_radius);
                let cells = self.world_to_cell_dist(diagonal * 2.0) + 1;
                cells * cells
            }
            _ => return 4, // the default branch
        };
        if result < 4 { 4 } else { result }
    }

    /// `allocCoiArray`.
    fn alloc_coi_array(&mut self, objects: &mut Objects, id: ObjectID) {
        let geom = objects.get(id).geometry;
        let count = self.calc_max_coi_for_shape(
            geom.geom_type.bits(),
            geom.major_radius,
            geom.minor_radius,
        );
        let part = objects.partition_mut(id);
        part.coi_array = vec![None; count as usize];
        part.coi_in_use = 0;
        self.make_dirty(objects, id, true);
    }

    /// `freeCoiArray`.
    fn free_coi_array(&mut self, objects: &mut Objects, id: ObjectID) {
        let part = objects.partition_mut(id);
        part.coi_array.clear();
        part.coi_in_use = 0;
        self.make_dirty(objects, id, true);
    }

    /// `removeAllTouchedCells`: every in-use COI leaves its cell's list.
    fn remove_all_touched_cells(&mut self, objects: &mut Objects, id: ObjectID) {
        let cells_to_clear: Vec<u32> = {
            let part = objects.partition(id);
            part.coi_array.iter().flatten().copied().collect()
        };
        for cell in cells_to_clear {
            if let Some(pos) = self.cells[cell as usize].cois.iter().position(|&o| o == id) {
                self.cells[cell as usize].cois.remove(pos);
            }
        }
        let part = objects.partition_mut(id);
        for slot in part.coi_array.iter_mut() {
            *slot = None;
        }
        part.coi_in_use = 0;
    }

    /// `addSubPixToCoverage`: the rescan for an existing COI of this
    /// cell, then the coverage add.
    fn add_sub_pix_to_coverage(&mut self, objects: &mut Objects, id: ObjectID, cell: usize) {
        // see if we already have a coi for this cell.
        let existing = {
            let part = objects.partition(id);
            (0..part.coi_in_use.min(part.coi_array.len()))
                .any(|i| part.coi_array[i] == Some(cell as u32))
        };
        if existing {
            // already covered: the COI is already in the cell's list
            return;
        }
        let free_slot = {
            let part = objects.partition(id);
            part.coi_in_use < part.coi_array.len()
        };
        if free_slot {
            // nope, no coi for this cell, allocate a new one
            let slot = objects.partition(id).coi_in_use;
            objects.partition_mut(id).coi_array[slot] = Some(cell as u32);
            objects.partition_mut(id).coi_in_use += 1;
            // addCoverage: the cell-list prepend
            self.cells[cell].cois.insert(0, id);
        }
        // (the "not enough cois allocated" assert is the out-of-domain
        // contract — the bound calc guarantees the slots)
    }

    /// `doRectFill`: the rotated-box rasterizer with its half-step
    /// fudge ("in theory, should be getCellSize() exactly, but needs to
    /// be smaller to avoid aliasing problems").
    // (the parameter list is the original's verbatim)
    #[allow(clippy::too_many_arguments)]
    fn do_rect_fill(
        &mut self,
        objects: &mut Objects,
        id: ObjectID,
        center_x: f32,
        center_y: f32,
        halfsize_x: f32,
        halfsize_y: f32,
        angle: f32,
    ) {
        let c = cosf(angle); // (Real)Cos(angle) — the gm_cosf branch
        let s = sinf(angle); // (Real)Sin(angle) — the gm_sinf branch

        let actual_cell_size = self.cell_size;
        let step_size = actual_cell_size * 0.5; // in theory, should be getCellSize() exactly, but needs to be smaller to avoid aliasing problems
        let ydx = s * step_size;
        let ydy = -c * step_size;
        let xdx = c * step_size;
        let xdy = s * step_size;

        // GS 12-26-02  Oh my.  The step size has been fudged down to ensure that a cell is not jumped over,
        // but then the original step size was used to determine the number of steps.  This error gets
        // more pronounced on bigger buildings.  ie a 100 X 160 registers cells for a 80 x 100.  If you
        // fudge one, you have to fudge the other.
        let step_size_inv_times2 = 2.0 * (1.0 / step_size);

        // srj fixes subtle bug: need +1 to get all of misaligned geometries accted for.
        // (the +1 fixup is commented out in the original — the GS fix)
        let num_steps_x = real_to_int_ceil(halfsize_x * step_size_inv_times2);
        let num_steps_y = real_to_int_ceil(halfsize_y * step_size_inv_times2);

        let mut tl_x = center_x - halfsize_x * c - halfsize_y * s;
        let mut tl_y = center_y + halfsize_y * c - halfsize_x * s;

        for _iy in 0..num_steps_y {
            let mut x = tl_x;
            let mut y = tl_y;
            for _ix in 0..num_steps_x {
                let (cellx, celly) = self.world_to_cell(x, y);
                if let Some(cell) = self.cell_at(cellx, celly) {
                    // might be null if off the edge
                    self.add_sub_pix_to_coverage(objects, id, cell);
                }
                x += xdx;
                y += xdy;
            }
            tl_x += ydx;
            tl_y += ydy;
        }
    }

    /// `doesCircleOverlapCell`: the closest-point clamp and the STRICT
    /// less-than (an exact boundary touch does not overlap).
    fn does_circle_overlap_cell(
        center_x: f32,
        center_y: f32,
        radius: f32,
        cell_x: f32,
        cell_y: f32,
        cell_size: f32,
    ) -> bool {
        let closest_x = cell_x.max(center_x.min(cell_x + cell_size));
        let closest_y = cell_y.max(center_y.min(cell_y + cell_size));

        let dist_x = center_x - closest_x;
        let dist_y = center_y - closest_y;

        (dist_x * dist_x + dist_y * dist_y) < (radius * radius)
    }

    /// `doCircleFillPrecise` — the non-retail bugfix branch
    /// (TheSuperHackers @bugfix Stubbjax 29/01/2026: "Use precise circle
    /// fill to improve collision accuracy, most notably for objects with
    /// geometry radii >= 20 and < 40").
    fn do_circle_fill_precise(
        &mut self,
        objects: &mut Objects,
        id: ObjectID,
        center_x: f32,
        center_y: f32,
        radius: f32,
    ) {
        let (min_cell_x, min_cell_y) = self.world_to_cell(center_x - radius, center_y - radius);
        let (max_cell_x, max_cell_y) = self.world_to_cell(center_x + radius, center_y + radius);

        let cell_size = self.cell_size;

        for x in min_cell_x..=max_cell_x {
            for y in min_cell_y..=max_cell_y {
                let cell_world_x = x as f32 * cell_size;
                let cell_world_y = y as f32 * cell_size;

                if Self::does_circle_overlap_cell(
                    center_x,
                    center_y,
                    radius,
                    cell_world_x,
                    cell_world_y,
                    cell_size,
                ) && let Some(cell) = self.cell_at(x, y)
                {
                    self.add_sub_pix_to_coverage(objects, id, cell);
                }
            }
        }
    }

    /// `doSmallFill`: the ≤ 4-cell fast path with the half-cell radius
    /// truncation.
    fn do_small_fill(
        &mut self,
        objects: &mut Objects,
        id: ObjectID,
        center_x: f32,
        center_y: f32,
        radius: f32,
    ) {
        let half_cell_size = self.cell_size * 0.5;
        let mut radius = radius;
        if radius > half_cell_size {
            // object is too large to use a 'small' geometry, truncating size to cellsize
            radius = half_cell_size;
        }

        let (cx1, cy1) = self.world_to_cell(center_x - radius, center_y - radius);
        let (cx2, cy2) = self.world_to_cell(center_x + radius, center_y + radius);

        for x in cx1..=cx2 {
            for y in cy1..=cy2 {
                if let Some(cell) = self.cell_at(x, y) {
                    // the direct slot take (no rescan: the ≤ 4 cells are
                    // distinct by construction)
                    let slot = objects.partition(id).coi_in_use;
                    objects.partition_mut(id).coi_array[slot] = Some(cell as u32);
                    objects.partition_mut(id).coi_in_use += 1;
                    self.cells[cell].cois.insert(0, id);
                }
            }
        }
    }

    /// `addPossibleCollisions`: the cell walk that collects the
    /// collision candidate pairs (cells with fewer than two occupants
    /// cannot collide anything).
    fn add_possible_collisions(&self, objects: &Objects, id: ObjectID, ct_list: &mut ContactList) {
        let part = objects.partition(id);
        for slot in 0..part.coi_in_use {
            let cell = &self.cells[part.coi_array[slot].unwrap_or(u32::MAX) as usize];
            if cell.cois.len() < 2 {
                continue;
            }
            for &that in &cell.cois {
                if id != that {
                    ct_list.add(id, that);
                }
            }
        }
    }

    /// `makeDirty(needToUpdateCells)`: the status upgrades and the LIFO
    /// prepend.
    pub fn make_dirty(&mut self, objects: &mut Objects, id: ObjectID, need_to_update_cells: bool) {
        let in_list = self.dirty_modules.iter().any(|&d| d == id);
        if !in_list {
            if need_to_update_cells {
                objects.partition_mut(id).dirty_status =
                    DirtyStatus::NeedCellUpdateAndCollisionCheck;
            } else if objects.partition(id).dirty_status == DirtyStatus::NotDirty {
                objects.partition_mut(id).dirty_status = DirtyStatus::NeedCollisionCheck;
            }
            self.dirty_modules.push_front(id);
        } else if need_to_update_cells {
            // If I am in the list already, I may be in there for a half update, so if my
            // needs have increased I'll upgrade what I want.
            objects.partition_mut(id).dirty_status = DirtyStatus::NeedCellUpdateAndCollisionCheck;
        }
    }

    /// `updateCellsTouched` (the refresh body): recompute the coverage
    /// from the object's current geometry and position.
    fn update_cells_touched(
        &mut self,
        objects: &mut Objects,
        host: &mut dyn KernelHost,
        id: ObjectID,
    ) {
        let obj_geom = objects.get(id).geometry;
        let pos = objects.get(id).position;
        let angle = objects.get(id).orientation;

        self.remove_all_touched_cells(objects, id);
        if obj_geom.is_small {
            self.do_small_fill(objects, id, pos.x, pos.y, obj_geom.major_radius);
        } else {
            match obj_geom.geom_type {
                GeometryType::Sphere | GeometryType::Cylinder => {
                    self.do_circle_fill_precise(objects, id, pos.x, pos.y, obj_geom.major_radius);
                }
                GeometryType::Box => {
                    self.do_rect_fill(
                        objects,
                        id,
                        pos.x,
                        pos.y,
                        obj_geom.major_radius,
                        obj_geom.minor_radius,
                        angle,
                    );
                }
            }
        }

        let (cell_x, cell_y) = self.world_to_cell(pos.x, pos.y);
        let current_cell = self.cell_at(cell_x, cell_y).map(|c| c as u32);
        if current_cell != objects.partition(id).last_cell {
            // To not expose PartitionCells, he will think in terms of points.  He will
            // unlook at a point and look at the new point.  We do the rounding and the
            // changing into PartitionCells
            objects.get_mut(id).cell_change_count += 1;
            host.on_partition_cell_change(id, current_cell.map_or(-1, |c| c as i32));
            objects.partition_mut(id).last_cell = current_cell;
        }

        // if we have moved, our shroudedness status might be different for all players,
        // so it must all be invalidated.
        let part = objects.partition_mut(id);
        for status in part.shroudedness.iter_mut() {
            if *status != ObjectShroudStatus::Invalid
                && *status != ObjectShroudStatus::InvalidButPreviousValid
            {
                *status = ObjectShroudStatus::Invalid;
            }
        }
    }

    /// `update`: the dirty refresh. The LIFO worklist pops until empty;
    /// each module updates its coverage if flagged and contributes
    /// collision candidates if it is not immobile. The pairs land in
    /// [`Self::last_contacts`] in contact-list order.
    pub fn update(&mut self, objects: &mut Objects, host: &mut impl KernelHost) {
        if !self.updated_since_last_reset {
            self.updated_since_last_reset = true;
        }

        let mut ct_list = ContactList::new();
        while !self.dirty_modules.is_empty() {
            // save it.
            let dirty = *self.dirty_modules.front().expect("checked non-empty");

            // get this BEFORE removing from dirty list, since that clears the
            // flag in question.
            let dirty_status = objects.partition(dirty).dirty_status;
            let update_em = dirty_status == DirtyStatus::NeedCellUpdateAndCollisionCheck;
            let collide_em = dirty_status != DirtyStatus::NotDirty; // isInNeedOfCollisionCheck() && getObject()

            // detach it from the dirty list.
            self.remove_from_dirty(objects, dirty);

            if update_em {
                self.update_cells_touched(objects, host, dirty);
            }

            let immobile = objects.get(dirty).is_kind_of_immobile();
            if collide_em && !immobile {
                self.add_possible_collisions(objects, dirty, &mut ct_list);
            }
        }

        // (processContactList — the collide-test family — is the
        // collision slice; the pair collection is this slice's dump)
        ct_list.pairs.reverse(); // the LIFO contact-list order
        self.last_contacts = ct_list.pairs;
        // (processPendingUndoShroudRevealQueue is the shroud slice)
    }
}
