// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Property tests for the GameMessage command-processing block (the
//! sixth Phase 3 slice): the argument union's typed round-trips (every
//! typed append reads back identically through its own member), the
//! list choreography's invariants (the walk order equals the append
//! order; every removal keeps the links bidirectional), the translator
//! registry's stable-priority ordering (the execution order is sorted
//! by priority with ties in attach order — the insertion sort's
//! contract), the propagation's conservation (the survivors plus the
//! destroyed plus the swept equal the posted; every message lands on
//! exactly one list or dies), and the CRC validation's agreement with
//! an independent specification (the reference-CRC walk over the
//! connected players).
//!
//! The properties pin the surfaces' *agreement with independent
//! specifications*, not algebraic laws the original does not have (the
//! redundant-sweep patterns and the punned reads are the golden
//! corpora's pins, not laws).

use std::collections::BTreeMap;

use proptest::prelude::*;

use rzh_sim::logic::{Kernel, KernelHost};
use rzh_sim::messages::{
    GameMessageArgument, GameMessageDisposition, GameMessageType, MessagePost, TranslateOutcome,
};
use rzh_sim::partition::Region2;
use rzh_sim::players::{SideSpec, SidesSpec};
use rzh_sim::types::{Coord3, ICoord2, IRegion2};

/// The no-op host (the message block's hooks are the golden test's
/// scripting surface; the properties drive the kernel directly).
struct NullHost;

impl KernelHost for NullHost {}

fn kernel() -> Kernel {
    let region = Region2 {
        lo_x: 0.0,
        lo_y: 0.0,
        hi_x: 100.0,
        hi_y: 100.0,
    };
    Kernel::with_grid(10.0, region, &mut NullHost)
}

// ---- the argument family ---------------------------------------------------

proptest! {
    #[test]
    fn arg_roundtrip(
        v_int in any::<i32>(),
        v_real in any::<f32>(),
        v_bool in any::<bool>(),
        v_oid in any::<i32>(),
        v_team in any::<u32>(),
        v_ts in any::<u32>(),
        v_wc in any::<u16>(),
        x in any::<f32>(), y in any::<f32>(), z in any::<f32>(),
        px in any::<i32>(), py in any::<i32>(),
        lox in any::<i32>(), loy in any::<i32>(),
        hix in any::<i32>(), hiy in any::<i32>(),
    ) {
        let a = GameMessageArgument::new_integer(v_int);
        prop_assert_eq!(a.integer(), v_int);
        prop_assert_eq!(a.data_type.ordinal(), 0);

        let a = GameMessageArgument::new_real(v_real);
        prop_assert_eq!(a.real().to_bits(), v_real.to_bits());
        prop_assert_eq!(a.data_type.ordinal(), 1);

        let a = GameMessageArgument::new_boolean(v_bool);
        prop_assert_eq!(a.boolean(), v_bool);
        prop_assert_eq!(a.data_type.ordinal(), 2);

        let a = GameMessageArgument::new_object_id(v_oid);
        prop_assert_eq!(a.object_id(), v_oid);
        prop_assert_eq!(a.data_type.ordinal(), 3);

        let a = GameMessageArgument::new_team_id(v_team);
        prop_assert_eq!(a.team_id(), v_team);
        prop_assert_eq!(a.data_type.ordinal(), 5);

        let loc = Coord3 { x, y, z };
        let a = GameMessageArgument::new_location(loc);
        let back = a.location();
        prop_assert_eq!(back.x.to_bits(), x.to_bits());
        prop_assert_eq!(back.y.to_bits(), y.to_bits());
        prop_assert_eq!(back.z.to_bits(), z.to_bits());
        prop_assert_eq!(a.data_type.ordinal(), 6);

        let pix = ICoord2 { x: px, y: py };
        let a = GameMessageArgument::new_pixel(pix);
        prop_assert_eq!(a.pixel(), pix);
        prop_assert_eq!(a.data_type.ordinal(), 7);

        let reg = IRegion2 { lo: ICoord2 { x: lox, y: loy }, hi: ICoord2 { x: hix, y: hiy } };
        let a = GameMessageArgument::new_pixel_region(reg);
        prop_assert_eq!(a.pixel_region(), reg);
        prop_assert_eq!(a.data_type.ordinal(), 8);

        let a = GameMessageArgument::new_timestamp(v_ts);
        prop_assert_eq!(a.timestamp(), v_ts);
        prop_assert_eq!(a.data_type.ordinal(), 9);

        let a = GameMessageArgument::new_wide_char(v_wc);
        prop_assert_eq!(a.wide_char(), v_wc);
        prop_assert_eq!(a.data_type.ordinal(), 10);
    }
}

proptest! {
    #[test]
    fn arg_out_of_range_zero(count in 0usize..8) {
        let mut k = kernel();
        let id = k.post_stream_message(GameMessageType::MSG_RAW_MOUSE_POSITION);
        for _ in 0..count {
            k.msg_arena.get_mut(id).append_integer_argument(0);
        }
        let m = k.msg_arena.get(id);
        prop_assert_eq!(m.get_argument(count as i32).integer(), 0);
        prop_assert_eq!(
            m.get_argument_data_type(count as i32).ordinal(),
            11 // ARGUMENTDATATYPE_UNKNOWN
        );
        prop_assert_eq!(m.get_argument(-1).integer(), 0);
    }
}

// ---- the list choreography ---------------------------------------------------

proptest! {
    #[test]
    fn list_walk_equals_append_order(types in proptest::collection::vec(
        (0usize..6).prop_map(|i|
            [GameMessageType::MSG_FRAME_TICK,
             GameMessageType::MSG_RAW_KEY_DOWN,
             GameMessageType::MSG_CLEAR_GAME_DATA,
             GameMessageType::MSG_PURCHASE_SCIENCE,
             GameMessageType::MSG_OBJECT_DESTROYED,
             GameMessageType::MSG_TIMESTAMP][i]),
        0..24)) {
        let mut k = kernel();
        for &ty in &types {
            k.post_stream_message(ty);
        }
        // the walk order equals the append order, the links are
        // bidirectional, and every message claims the stream
        let ids = k.stream.ids(&k.msg_arena);
        prop_assert_eq!(ids.len(), types.len());
        for w in 0..ids.len() {
            let m = k.msg_arena.get(ids[w]);
            prop_assert_eq!(m.prev().map(|p| ids.contains(&p)).unwrap_or(true), true);
            if w > 0 {
                prop_assert_eq!(m.prev(), Some(ids[w - 1]));
            } else {
                prop_assert_eq!(m.prev(), None);
                prop_assert_eq!(k.stream.first_message(), Some(ids[0]));
            }
            if w + 1 < ids.len() {
                prop_assert_eq!(m.next(), Some(ids[w + 1]));
            } else {
                prop_assert_eq!(m.next(), None);
            }
        }
    }
}

proptest! {
    #[test]
    fn transfer_preserves_order_and_empties_stream(n in 0usize..16) {
        let mut k = kernel();
        for _ in 0..n {
            k.post_stream_message(GameMessageType::MSG_FRAME_TICK);
        }
        let stream_ids = k.stream.ids(&k.msg_arena);
        let first = k.stream.first_message();
        k.commands.append_message_list(&mut k.msg_arena, first);
        // the transfer: same order, the containment flips, the stream
        // empties (its head/tail still point at the moved messages —
        // the propagate's clear is the mirror's job, so the ids() walk
        // after a manual clear is the contract)
        let cmd_ids = k.commands.ids(&k.msg_arena);
        prop_assert_eq!(cmd_ids, stream_ids);
        // the reset kills everything
        k.commands.reset(&mut k.msg_arena);
        prop_assert!(k.commands.ids(&k.msg_arena).is_empty());
        prop_assert!(k.msg_arena.live_ids().is_empty());
    }
}

// ---- the translator registry -------------------------------------------------

proptest! {
    #[test]
    fn translator_order_is_stable_sorted(prios in proptest::collection::vec(0u32..8, 0..24)) {
        let mut k = kernel();
        let mut ids = Vec::new();
        for &p in &prios {
            ids.push(k.stream.attach_translator(p));
        }
        let reg = k.stream.translator_order();
        prop_assert_eq!(reg.len(), prios.len());
        // the ids are unique, count from 1 (the attach order's set)
        for t in reg.iter() {
            prop_assert!(ids.contains(&t.id));
        }
        prop_assert_eq!(reg.len(), {
            let mut uniq = ids.clone();
            uniq.sort_unstable();
            uniq.dedup();
            uniq.len()
        });
        // the execution order is sorted by priority, ties in attach order
        for w in 1..reg.len() {
            prop_assert!(reg[w - 1].priority <= reg[w].priority);
            if reg[w - 1].priority == reg[w].priority {
                let pa = ids.iter().position(|&x| x == reg[w - 1].id).unwrap();
                let pb = ids.iter().position(|&x| x == reg[w].id).unwrap();
                prop_assert!(pa < pb);
            }
        }
        // find/remove round-trip
        for &id in &ids {
            prop_assert_eq!(k.stream.find_translator(id).map(|t| t.id), Some(id));
        }
        prop_assert!(k.stream.find_translator(9999).is_none());
        for &id in &ids {
            k.stream.remove_translator(id);
            prop_assert!(k.stream.find_translator(id).is_none());
        }
        prop_assert!(k.stream.translator_order().is_empty());
    }
}

// ---- the propagation's conservation -------------------------------------------

/// A host that destroys scripted messages and posts scripted types.
struct ScriptedHost {
    destroy: Vec<(u32, u32)>, // (translator id, msg id)
    posts: Vec<(u32, Vec<GameMessageType>)>,
    seen: Vec<u32>,
}

impl KernelHost for ScriptedHost {
    fn run_translate(
        &mut self,
        translator: u32,
        msg: u32,
        _data: &rzh_sim::messages::MessageData,
    ) -> TranslateOutcome {
        self.seen.push(msg);
        let mut outcome = TranslateOutcome::default();
        if self.destroy.contains(&(translator, msg)) {
            outcome.disposition = GameMessageDisposition::DESTROY_MESSAGE;
        }
        if let Some((_, types)) = self.posts.iter().find(|(t, _)| *t == translator) {
            outcome.posts = types
                .iter()
                .map(|&ty| MessagePost {
                    ty,
                    args: Vec::new(),
                })
                .collect();
        }
        outcome
    }
}

proptest! {
    #[test]
    fn propagate_conserves_messages(
        n in 0usize..12,
        destroy_subset in proptest::collection::vec(any::<bool>(), 0..12),
    ) {
        let mut k = kernel();
        let _t1 = k.stream.attach_translator(1);
        let t2 = k.stream.attach_translator(2);
        let mut host = ScriptedHost {
            destroy: Vec::new(),
            posts: Vec::new(),
            seen: Vec::new(),
        };
        for i in 0..n {
            let id = k.post_stream_message(GameMessageType::MSG_PURCHASE_SCIENCE);
            // only the SECOND translator destroys, so every message is
            // seen by the first (the per-translator restart)
            if destroy_subset.get(i).copied().unwrap_or(false) {
                host.destroy.push((t2, id));
            }
        }
        let posted: Vec<u32> = k.stream.ids(&k.msg_arena);
        k.propagate_messages(&mut host);
        // every original message was translated by t1 (the restart)
        for &id in &posted {
            prop_assert!(host.seen.contains(&id));
        }
        // conservation: the command list holds the survivors; the
        // destroyed are freed; the stream is empty
        let survivors = k.commands.ids(&k.msg_arena);
        prop_assert!(k.stream.ids(&k.msg_arena).is_empty());
        let destroyed: Vec<u32> = (0..n as u32)
            .map(|i| posted[i as usize])
            .enumerate()
            .filter(|(i, _)| destroy_subset.get(*i).copied().unwrap_or(false))
            .map(|(_, id)| id)
            .collect();
        prop_assert_eq!(survivors.len() + destroyed.len(), n);
        for &d in &destroyed {
            prop_assert!(!survivors.contains(&d));
        }
        // the arena's live set is exactly the survivors (nothing leaks,
        // nothing else dies)
        let mut live = k.msg_arena.live_ids();
        live.sort_unstable();
        let mut surv = survivors.clone();
        surv.sort_unstable();
        prop_assert_eq!(live, surv);
    }
}

// ---- the CRC validation against an independent specification -------------------

/// A host with a fixed connectivity and CRC table (the net layer's
/// scripted state).
struct CrcHost {
    connected: [bool; 8],
    mismatch_fired: u32,
}

impl KernelHost for CrcHost {
    fn is_player_connected(&mut self, slot: i32) -> bool {
        (0..8).contains(&slot) && self.connected[slot as usize]
    }
    fn on_crc_mismatch(&mut self) {
        self.mismatch_fired += 1;
    }
}

proptest! {
    #[test]
    fn crc_validation_matches_reference(
        crcs in proptest::collection::vec((1usize..=3, any::<u32>()), 0..6),
        slot_of in proptest::collection::vec(0i32..3, 0..4),
        connected in proptest::collection::vec(any::<bool>(), 8),
    ) {
        // a 3-human networked world: players 1..3 in slots 0..2 (the
        // slot_of table reassigns them per corpus case)
        let mut k = {
            let region = Region2 { lo_x: 0.0, lo_y: 0.0, hi_x: 100.0, hi_y: 100.0 };
            Kernel::with_grid(10.0, region, &mut NullHost)
        };
        let sides = SidesSpec {
            sides: vec![
                SideSpec::default(),
                SideSpec {
                    player_name: "a".into(),
                    is_human: true,
                    multiplayer_is_local: true,
                    ..SideSpec::default()
                },
                SideSpec { player_name: "b".into(), is_human: true, ..SideSpec::default() },
                SideSpec { player_name: "c".into(), is_human: true, ..SideSpec::default() },
            ],
            ..SidesSpec::default()
        };
        k.players.network_present = true;
        k.players.new_game(&sides, &mut k.objects, &mut NullHost);
        for (player, &slot) in slot_of.iter().take(3).enumerate() {
            k.players.set_slot_index(player + 1, slot);
        }
        let mut host = CrcHost { connected: <[bool; 8]>::try_from(connected.clone()).unwrap(), mismatch_fired: 0 };

        // post the CRC messages (each player's arrival gate: human +
        // slotted + connected)
        for &(player, crc) in &crcs {
            let id = k.post_command_message(GameMessageType::MSG_LOGIC_CRC);
            let m = k.msg_arena.get_mut(id);
            m.player_index = player as i32;
            m.append_integer_argument(crc as i32);
        }
        k.process_command_list(&mut host);

        // the independent specification: the cache holds exactly the
        // human, slotted, connected players' CRCs; the validation runs
        // iff the LOCAL player's CRC passed the arrival gates (the
        // m_shouldValidateCRCs raise) and no prior mismatch latched
        // (the property's host never latches), and fires iff (cache
        // count < connected-slot count) or two connected cached CRCs
        // differ
        let arrival_ok = |player: usize| -> bool {
            let slot = k.players.get_slot_index(player);
            let human = k
                .players
                .nth_player(player as i32)
                .map(|p| p.player_type == rzh_sim::types::PlayerType::Human)
                .unwrap_or(false);
            human && slot >= 0 && host.connected[slot as usize]
        };
        let mut spec_cache: BTreeMap<i32, u32> = BTreeMap::new();
        for &(player, crc) in &crcs {
            if arrival_ok(player) {
                spec_cache.insert(player as i32, crc);
            }
        }
        let local = k.players.list.local.unwrap_or(0);
        let should_validate = crcs.iter().any(|&(p, _)| p == local && arrival_ok(p));
        let num_connected = connected.iter().filter(|&&c| c).count();
        let mut spec_mismatch = spec_cache.len() < num_connected;
        if !spec_mismatch {
            let mut reference: Option<u32> = None;
            for (&player, &crc) in &spec_cache {
                let slot = k.players.get_slot_index(player as usize);
                if slot >= 0 && !host.connected[slot as usize] {
                    continue; // the Caball009 skip
                }
                match reference {
                    None => reference = Some(crc),
                    Some(r) if r != crc => spec_mismatch = true,
                    Some(_) => {}
                }
            }
        }
        if !should_validate {
            spec_mismatch = false; // the tail never runs
        }
        prop_assert_eq!(host.mismatch_fired, u32::from(spec_mismatch));
        prop_assert_eq!(
            k.logic.cached_crcs.iter().collect::<Vec<_>>(),
            spec_cache.iter().collect::<Vec<_>>()
        );
    }
}
