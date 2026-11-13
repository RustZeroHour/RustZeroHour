// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Property tests for the players/teams/relationships block (the fourth
//! Phase 3 slice): the relationship chain's totality against a
//! brute-force reference, the mask queries against set arithmetic, the
//! interning injectivity, the team-id allocation scheme, and the
//! member-list conservation laws.
//!
//! The relationship chain is deliberately NOT symmetric and NOT
//! transitive (the original assigns per side, per team and per
//! prototype with no closure) — the properties pin the chain's
//! *totality* (every query resolves through the exact precedence) and
//! the query surfaces' agreement with independent specifications, not
//! algebraic laws the original does not have.

use proptest::prelude::*;

use rzh_sim::logic::{Kernel, KernelHost, ObjectSpec};
use rzh_sim::partition::Region2;
use rzh_sim::players::{SideSpec, SidesSpec, TeamSpec};
use rzh_sim::types::{
    ALLOW_ALLIES, ALLOW_ENEMIES, ALLOW_NEUTRAL, ALLOW_SAME_PLAYER, PLAYERMASK_NONE, Relationship,
    TeamID,
};

/// The no-op host.
struct PropHost;
impl KernelHost for PropHost {}

/// A random relationship.
fn arb_relationship() -> impl Strategy<Value = Relationship> {
    prop_oneof![
        Just(Relationship::Enemies),
        Just(Relationship::Neutral),
        Just(Relationship::Allies),
    ]
}

/// Build a fresh world with a random player/team topology: `n_sides`
/// named sides (enemies/allies drawn at random), the standard singleton
/// default teams, and one non-singleton squad per player.
fn build_world(n_sides: usize, rels: &[(usize, usize, Relationship)]) -> (Kernel, PropHost) {
    let mut host = PropHost;
    let mut kernel = Kernel::with_grid(
        10.0,
        Region2 {
            lo_x: 0.0,
            lo_y: 0.0,
            hi_x: 100.0,
            hi_y: 100.0,
        },
        &mut host,
    );

    let names: Vec<String> = (0..n_sides).map(|i| format!("p{i}")).collect();
    let mut sides = Vec::new();
    for name in &names {
        sides.push(SideSpec {
            player_name: name.clone(),
            is_human: false,
            multiplayer_is_local: false,
            observer: false,
            enemies: Vec::new(),
            allies: Vec::new(),
            ..SideSpec::default()
        });
    }
    let mut teams = vec![TeamSpec {
        name: "team".to_string(),
        owner: String::new(),
        singleton: true,
        ..TeamSpec::default()
    }];
    for name in &names {
        teams.push(TeamSpec {
            name: format!("team{name}"),
            owner: name.clone(),
            singleton: true,
            ..TeamSpec::default()
        });
    }
    for name in &names {
        teams.push(TeamSpec {
            name: format!("{name}squad"),
            owner: name.clone(),
            singleton: false,
            ..TeamSpec::default()
        });
    }
    kernel.players.new_game(
        &SidesSpec {
            sides,
            teams,
            has_game_info: false,
            game_info: Default::default(),
            default_starting_cash: 0,
        },
        &mut kernel.objects,
        &mut host,
    );

    // apply the random relations on top (player → player)
    for &(from, to, r) in rels {
        if from < n_sides && to < n_sides {
            kernel.players.list.players[from].set_player_relationship(to, r);
        }
    }
    (kernel, host)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The relationship chain is total: for a random relation assignment,
    /// the player→team resolution agrees with a brute-force reference
    /// that walks the same precedence (team-override, then the target's
    /// owner's entry, then NEUTRAL) over the raw state.
    #[test]
    fn relationship_chain_matches_reference(
        n_sides in 2usize..6,
        rels in prop::collection::vec((0usize..8, 0usize..8, arb_relationship()), 0..12),
    ) {
        let (kernel, _host) = build_world(n_sides, &rels);

        // one query team per player (the default teams)
        for from in 0..n_sides {
            for to in 0..n_sides {
                let dt = kernel.players.list.players[to].default_team;
                if dt == 0 {
                    continue; // a missing default team resolves NEUTRAL (the `if (that)` guard)
                }
                let got = kernel.players.player_get_relationship(from, dt);

                // the brute-force reference over the raw state
                let p = &kernel.players.list.players[from];
                let expect = match p.team_relations.get(&dt).copied() {
                    Some(r) => r,
                    None => {
                        // the target team's owner — resolved through the arena
                        let owner = kernel
                            .players
                            .factory
                            .teams
                            .iter()
                            .flatten()
                            .find(|t| t.id == dt)
                            .and_then(|t| kernel.players.factory.protos[t.proto].as_ref())
                            .map(|pr| pr.owner)
                            .unwrap_or(usize::MAX);
                        p.player_relations
                            .get(&(owner as i32))
                            .copied()
                            .unwrap_or(Relationship::Neutral)
                    }
                };

                prop_assert_eq!(got, expect);
            }
        }
    }

    /// The team-level chain: the team's own overrides first, then its
    /// owner's view — the two-level precedence over the raw state.
    #[test]
    fn team_chain_matches_reference(
        n_sides in 2usize..6,
        rels in prop::collection::vec((0usize..8, 0usize..8, arb_relationship()), 0..12),
    ) {
        let (mut kernel, mut host) = build_world(n_sides, &rels);

        // materialize a squad team per player (the non-singleton creates)
        let mut squads = Vec::new();
        for p in 1..n_sides {
            squads.push(kernel.players.create_team(&format!("p{p}squad"), &mut host).unwrap());
        }
        // and some player default teams
        let defaults: Vec<TeamID> = (0..n_sides)
            .map(|p| kernel.players.list.players[p].default_team)
            .collect();

        for &sa in &squads {
            for &dt in &defaults {
                if dt == 0 {
                    continue;
                }
                let got = kernel.players.team_get_relationship(sa, dt);
                let sa_team = kernel
                    .players
                    .factory
                    .teams
                    .iter()
                    .flatten()
                    .find(|t| t.id == sa)
                    .unwrap();
                let owner = kernel.players.factory.protos[sa_team.proto]
                    .as_ref()
                    .unwrap()
                    .owner;
                let expect = sa_team
                    .team_relations
                    .get(&dt)
                    .copied()
                    .or_else(|| {
                        let dt_owner = kernel
                            .players
                            .factory
                            .teams
                            .iter()
                            .flatten()
                            .find(|t| t.id == dt)
                            .and_then(|t| kernel.players.factory.protos[t.proto].as_ref())
                            .map(|pr| pr.owner)
                            .unwrap_or(usize::MAX);
                        sa_team.player_relations.get(&(dt_owner as i32)).copied()
                    })
                    .unwrap_or_else(|| {
                        // the owner's player-level view of dt
                        kernel.players.player_get_relationship(owner, dt)
                    });
                prop_assert_eq!(got, expect);
            }
        }
    }

    /// `getPlayersWithRelationship` agrees with the brute-force walk over
    /// the players' own resolution of each candidate's DEFAULT TEAM.
    #[test]
    fn players_with_relationship_matches_bruteforce(
        n_sides in 2usize..6,
        rels in prop::collection::vec((0usize..8, 0usize..8, arb_relationship()), 0..12),
    ) {
        let (kernel, _host) = build_world(n_sides, &rels);

        for src in 0..n_sides as i32 {
            for &flags in &[
                ALLOW_SAME_PLAYER,
                ALLOW_ALLIES,
                ALLOW_ENEMIES,
                ALLOW_NEUTRAL,
                ALLOW_SAME_PLAYER | ALLOW_ALLIES | ALLOW_ENEMIES | ALLOW_NEUTRAL,
                0u32,
            ] {
                let got = kernel.players.get_players_with_relationship(src, flags);
                let mut expect = PLAYERMASK_NONE;
                if src >= 0 && (kernel.players.nth_player(src).is_some()) {
                    if (flags & ALLOW_SAME_PLAYER) != 0 {
                        expect |= kernel.players.list.players[src as usize].mask();
                    }
                    for i in 0..kernel.players.list.count {
                        if i == src as usize {
                            continue;
                        }
                        let dt = kernel.players.list.players[i].default_team;
                        let r = kernel.players.player_get_relationship(src as usize, dt);
                        let bit = match r {
                            Relationship::Enemies => ALLOW_ENEMIES,
                            Relationship::Allies => ALLOW_ALLIES,
                            Relationship::Neutral => ALLOW_NEUTRAL,
                        };
                        if (flags & bit) != 0 {
                            expect |= kernel.players.list.players[i].mask();
                        }
                    }
                }
                prop_assert_eq!(got, expect);
            }
        }
    }

    /// The mask peeling partitions: `getEachPlayerFromMask` returns each
    /// player at most once, in index order, and the returned set is
    /// exactly the mask ∩ the players' own bits.
    #[test]
    fn mask_peeling_partitions_the_player_bits(
        mask in any::<u16>(),
    ) {
        let (mut kernel, _host) = build_world(4, &[]);

        let mut remaining = mask;
        let mut seen: Vec<usize> = Vec::new();
        loop {
            let got = kernel.players.get_each_player_from_mask(&mut remaining);
            match got {
                Some(p) => {
                    // index order: strictly increasing
                    prop_assert!(seen.last().is_none_or(|&l| l < p));
                    seen.push(p);
                }
                None => break,
            }
        }
        // the union of the peeled players' bits == the original mask ∩ the live bits
        let mut live_bits = PLAYERMASK_NONE;
        for p in &kernel.players.list.players {
            live_bits |= p.mask();
        }
        let mut peeled_bits = PLAYERMASK_NONE;
        for p in &seen {
            peeled_bits |= kernel.players.list.players[*p].mask();
        }
        prop_assert_eq!(peeled_bits, mask & live_bits);
        prop_assert_eq!(remaining, 0);
    }

    /// The name-key interning is injective over distinct names and stable
    /// over re-interns (the sequential-ordinal scheme).
    #[test]
    fn name_keys_are_injective_and_stable(
        names in prop::collection::vec("[a-zA-Z][a-zA-Z0-9_]{0,11}", 1..24),
    ) {
        let (mut kernel, _host) = build_world(2, &[]);
        let mut first: Vec<(String, u32)> = Vec::new();
        for name in &names {
            let key = kernel.players.name_key(name).ordinal();
            match first.iter().find(|(n, _)| n == name) {
                Some((_, k)) => prop_assert_eq!(*k, key), // the re-intern is stable
                None => first.push((name.clone(), key)),
            }
        }
        // the distinct names got distinct keys
        let mut keys: Vec<u32> = first.iter().map(|(_, k)| *k).collect();
        keys.sort_unstable();
        keys.dedup();
        prop_assert_eq!(keys.len(), first.len());
    }

    /// The team-id allocation scheme: ids are strictly increasing from 1,
    /// the singleton create returns the same id, and `findTeamByID`
    /// resolves every live id (and refuses the invalid).
    #[test]
    fn team_ids_allocate_monotonically(
        n_sides in 2usize..5,
    ) {
        let (mut kernel, mut host) = build_world(n_sides, &[]);

        let mut ids = Vec::new();
        for p in 1..n_sides {
            ids.push(kernel.players.create_team(&format!("p{p}squad"), &mut host).unwrap());
        }
        // strictly increasing
        for w in ids.windows(2) {
            prop_assert!(w[0] < w[1]);
        }
        // the singleton reuse
        let dt = kernel.players.list.players[1].default_team;
        let again = kernel.players.create_team(&format!("team{}", kernel.players.list.players[1].name), &mut host);
        prop_assert_eq!(again, Some(dt));
        // findTeamByID resolves every live id
        for &id in &ids {
            prop_assert_eq!(kernel.players.find_team_by_id(id), Some(id));
        }
        prop_assert_eq!(kernel.players.find_team_by_id(0), None);
        // and an id never allocated does not resolve
        prop_assert_eq!(kernel.players.find_team_by_id(999), None);
    }

    /// The member-list conservation: after random setTeam churn, every
    /// object appears in exactly one team's member list, the member
    /// lists contain only live-team members, and the object's team field
    /// names the list that holds it.
    #[test]
    fn member_lists_stay_conserved(
        n_sides in 2usize..5,
        moves in prop::collection::vec((0usize..12, 0usize..6, any::<bool>()), 1..24),
    ) {
        let (mut kernel, mut host) = build_world(n_sides, &[]);

        // the teams to move between: the default teams + a squad each
        let mut teams = Vec::new();
        for p in 1..n_sides {
            teams.push(kernel.players.list.players[p].default_team);
            teams.push(kernel.players.create_team(&format!("p{p}squad"), &mut host).unwrap());
        }
        // the objects
        let mut objs = Vec::new();
        for i in 0..12 {
            let id = kernel.create_object(
                &ObjectSpec {
                    position: rzh_sim::types::Coord3 {
                        x: 10.0 + (i as f32),
                        y: 10.0,
                        z: 0.0,
                    },
                    ..Default::default()
                },
                &mut host,
            );
            objs.push(id);
        }

        for &(obj_idx, team_idx, to_none) in &moves {
            let obj = objs[obj_idx];
            let team = if to_none {
                None
            } else {
                Some(teams[team_idx % teams.len()])
            };
            kernel.players.set_team(&mut kernel.objects, obj, team, &mut host);
        }

        // the conservation check
        let mut member_count = 0;
        for t in kernel.players.factory.teams.iter().flatten() {
            for &m in &t.members {
                prop_assert_eq!(kernel.objects.get(m).team, t.id);
                member_count += 1;
            }
        }
        let attached = objs
            .iter()
            .filter(|&&o| kernel.objects.get(o).team != 0)
            .count();
        prop_assert_eq!(member_count, attached);
    }
}
