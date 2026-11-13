// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Property tests for the money/energy/ranks/sciences block (the fifth
//! Phase 3 slice): the wallet's conservation and its income-bucket
//! invariants (the per-minute figure always equals the bucket array's
//! sum; single-second walks age exactly the 60-second window), the
//! energy pool's agreement with a signed reference and the sabotage
//! gate, the rank ladder's totality (the level is exactly the highest
//! threshold the points cover, the points sit inside the level's band),
//! and the science purchase family against a brute-force gate
//! evaluation.
//!
//! The properties pin the surfaces' *agreement with independent
//! specifications*, not algebraic laws the original does not have (the
//! unsigned wraps, the double-notify and the stale Money index are the
//! golden corpora's pins, not laws).

use proptest::prelude::*;

use rzh_sim::logic::{Kernel, KernelHost, ObjectSpec};
use rzh_sim::money::Money;
use rzh_sim::partition::Region2;
use rzh_sim::players::SideSpec;
use rzh_sim::sciences::{RankSpec, SCIENCE_AVAILABILITY_NAMES, ScienceSpec, ScienceType};
use rzh_sim::types::MAX_PLAYER_COUNT;

/// A host that counts the money/energy/ranks/sciences hooks (the kinds
/// the block fires: 6 money-audio, 7 academy-income, 8 academy-spent,
/// 9 acquired-science — the golden test's TEV ordinals).
struct CountingHost {
    counts: [u64; 10],
}

impl CountingHost {
    fn new() -> Self {
        Self { counts: [0; 10] }
    }
}

impl KernelHost for CountingHost {
    fn on_money_audio(&mut self, _player: usize, _withdraw: bool) {
        self.counts[6] += 1;
    }
    fn on_academy_income(&mut self, _player: usize) {
        self.counts[7] += 1;
    }
    fn on_academy_points_spent(&mut self, _player: usize, _points: i32) {
        self.counts[8] += 1;
    }
    fn on_acquired_science(&mut self, _player: usize, _science: ScienceType) {
        self.counts[9] += 1;
    }
}

/// A fresh kernel.
fn fresh() -> (Kernel, CountingHost) {
    let mut host = CountingHost::new();
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
    (kernel, host)
}

/// A neutral side spec builder (the money fields default to zero).
fn neutral_side(name: &str) -> SideSpec {
    SideSpec {
        player_name: name.to_string(),
        ..SideSpec::default()
    }
}

/// One money operation: (withdraw?, play-sound?, tracked?, amount).
type MoneyOp = (bool, bool, bool, u32);

// ---- the money block ---------------------------------------------------------

/// The wallet: random deposit/withdraw streams conserve the balance
/// against the wrap arithmetic (deposits minus the actual withdrawals —
/// the over-draw clamps), every withdrawal returns at most the request
/// and at most the balance, and the hooks fire exactly per their gates.
#[test]
fn money_conserves_the_balance() {
    proptest!(|(ops in prop::collection::vec(any::<MoneyOp>(), 1..64))| {
        let mut host = CountingHost::new();
        let mut wallet = Money::default();
        wallet.set_player_index(3);

        // the model: the wrapping balance, the tracked total, the hook counts
        let mut model_balance: u32 = 0;
        let mut tracked: u64 = 0;
        let mut audio = 0u64;
        let mut income_hooks = 0u64;

        let mut frame: u32 = 0;
        for (want_withdraw, play_sound, track, amount) in ops {
            if want_withdraw {
                let before = wallet.count_money();
                let got = wallet.withdraw(amount, play_sound, &mut host);
                let expected = amount.min(before);
                prop_assert_eq!(got, expected);
                prop_assert!(got <= amount);
                if expected > 0 && play_sound {
                    audio += 1;
                }
                model_balance -= expected;
            } else if amount > 0 {
                wallet.deposit(amount, play_sound, track, &mut host);
                if play_sound {
                    audio += 1;
                }
                if track {
                    tracked += u64::from(amount);
                }
                // the academy income hook fires for EVERY deposit (the
                // index is in range)
                income_hooks += 1;
                model_balance = model_balance.wrapping_add(amount);
            }
            prop_assert_eq!(wallet.count_money(), model_balance);
            // the per-minute never exceeds the tracked total (the
            // rollover only removes)
            prop_assert!(u64::from(wallet.get_cash_per_minute()) <= tracked);
            frame += 7; // within-second steps mostly
            wallet.update_income_bucket(frame);
        }

        prop_assert_eq!(host.counts[6], audio);
        prop_assert_eq!(host.counts[7], income_hooks);
    });
}

/// The income-bucket invariant: the per-minute figure always equals the
/// bucket array's sum (every tracked deposit lands in exactly one
/// bucket; the rollover removes exactly the landed bucket's sum), and —
/// for walks that advance one second at a time — the figure is the
/// deposits of the last 60 seconds.
#[test]
fn money_buckets_stay_consistent() {
    proptest!(|(ops in prop::collection::vec(any::<(u32, bool)>(), 1..48), stepwise in any::<bool>())| {
        let mut host = CountingHost::new();
        let mut wallet = Money::default();
        wallet.set_player_index(0);

        // the landing log for the window model
        let mut landings: Vec<(u32, u32)> = Vec::new();
        let mut frame: u32 = 0;
        for (amount, jump) in ops {
            frame += if stepwise || !jump { 30 } else { 30 * 7 };
            if amount > 0 {
                wallet.deposit(amount, false, true, &mut host);
                landings.push((frame / 30, amount));
            }
            wallet.update_income_bucket(frame);
            // the invariant: the per-minute equals the bucket array's sum
            // (the buckets wrap like the wallet — the unsigned adds)
            let array_sum: u32 = wallet
                .income_buckets
                .iter()
                .fold(0u32, |a, b| a.wrapping_add(*b));
            prop_assert_eq!(wallet.get_cash_per_minute(), array_sum);
        }

        if stepwise {
            // the window model: a deposit ages out exactly when the walk
            // reaches its second + 60
            let cur_second = frame / 30;
            let expected: u32 = landings
                .iter()
                .filter(|&&(s, _)| s + 60 > cur_second)
                .map(|&(_, a)| a)
                .fold(0u32, |a, b| a.wrapping_add(b));
            prop_assert_eq!(wallet.get_cash_per_minute(), expected);
        }
    });
}

// ---- the energy block --------------------------------------------------------

/// The energy pool: random adjustPower/influence sequences keep the
/// production and consumption in agreement with a signed reference (the
/// adjustPower matrix's reversal included), the sufficiency verdict is
/// the comparison, and the sabotage gate zeroes every read while it
/// lasts.
#[test]
fn energy_matches_the_signed_reference() {
    proptest!(|(ops in prop::collection::vec((-40i32..40, any::<bool>(), any::<bool>()), 1..48), sabotage_frame in 1u32..500)| {
        let (mut kernel, mut host) = fresh();
        let sides = rzh_sim::players::SidesSpec {
            sides: vec![neutral_side(""), neutral_side("p1")],
            teams: Vec::new(),
            ..Default::default()
        };
        kernel.players.new_game(&sides, &mut kernel.objects, &mut host);

        // the reference: the raw production/consumption the signed deltas build
        let mut model_production = 0i64;
        let mut model_consumption = 0i64;

        let mut frame = 0u32;
        for (delta, adding, use_object) in ops {
            if delta == 0 && !use_object {
                continue; // adjustPower's zero refusal
            }
            if use_object {
                // an object with this template production entering
                let spec = ObjectSpec {
                    energy_production: delta,
                    ..ObjectSpec::default()
                };
                let id = kernel.create_object(&spec, &mut host);
                kernel
                    .players
                    .energy_object_entering_influence(&mut kernel.objects, 1, id, frame);
                if delta > 0 {
                    model_production += i64::from(delta);
                } else if delta < 0 {
                    model_consumption += i64::from(-delta);
                }
            } else {
                kernel
                    .players
                    .energy_adjust_power(&mut kernel.objects, 1, delta, adding, frame);
                // adjustPower's sign table: positive is production,
                // negative is consumption ("Negative power is positive
                // consumption"), both sign-flipped by !adding
                if delta > 0 {
                    if adding {
                        model_production += i64::from(delta);
                    } else {
                        model_production -= i64::from(delta);
                    }
                } else if adding {
                    model_consumption += i64::from(-delta);
                } else {
                    model_consumption += i64::from(delta);
                }
            }
            frame += 3;

            let prod = kernel.players.energy_get_production(1, frame);
            let cons = kernel.players.energy_get_consumption(1);
            prop_assert_eq!(i64::from(prod), model_production);
            prop_assert_eq!(i64::from(cons), model_consumption);
            prop_assert_eq!(
                kernel.players.energy_has_sufficient_power(1, frame),
                model_production >= model_consumption
            );
        }

        // the sabotage gate: every read zeroes while the frame is below
        // the till-frame (the strict <), then the real values return
        kernel
            .players
            .energy_set_power_sabotaged_till_frame(1, sabotage_frame);
        prop_assert_eq!(
            kernel
                .players
                .energy_get_production(1, sabotage_frame - 1),
            0
        );
        prop_assert!(!kernel
            .players
            .energy_has_sufficient_power(1, sabotage_frame - 1));
        let ratio = kernel.players.energy_supply_ratio(1, sabotage_frame - 1);
        prop_assert_eq!(ratio.to_bits(), 0.0f32.to_bits());
        prop_assert_eq!(
            i64::from(kernel.players.energy_get_production(1, sabotage_frame)),
            model_production
        );
    });
}

// ---- the ranks block ---------------------------------------------------------

/// The rank ladder: for sorted thresholds, the level after any
/// addSkillPoints stream is exactly the highest threshold the (capped)
/// points cover (clamped by the limit), and the purchase points equal
/// the sum of the granted rungs (never negative).
#[test]
fn rank_level_covers_the_points() {
    proptest!(|(thresholds in prop::collection::vec(0i32..2000, 1..8), gains in prop::collection::vec(-800i32..1500, 1..16), limit_low in 1i32..9)| {
        let (mut kernel, mut host) = fresh();

        // the monotone ladder (ties allowed — the level-up loop skips
        // the equal ones)
        let mut sorted = thresholds.clone();
        sorted.sort_unstable();
        let science_specs: Vec<ScienceSpec> = sorted
            .iter()
            .enumerate()
            .map(|(i, _)| ScienceSpec {
                name: format!("R{i}"),
                ..Default::default()
            })
            .collect();
        let rank_specs: Vec<RankSpec> = sorted
            .iter()
            .enumerate()
            .map(|(i, needed)| RankSpec {
                skill_points_needed: *needed,
                science_purchase_points_granted: i as i32,
                sciences_granted: vec![format!("R{i}")],
            })
            .collect();
        kernel.players.load_stores(&science_specs, &rank_specs);
        kernel.players.reset(&mut kernel.objects, &mut host);

        let count = sorted.len() as i32;
        let limit = limit_low.min(count);
        kernel.logic.set_rank_level_limit(limit);
        let point_cap = i64::from(sorted[(limit - 1) as usize]);

        let mut model_points = 0i64;
        let mut saw_negative = false;
        for g in gains {
            if g < 0 {
                saw_negative = true;
            }
            let level_before = kernel.players.list.players[1].rank_level;
            let gained = kernel.players.add_skill_points(1, g, limit, &mut host);

            // the model: the delta through the pinned Hoffesommer ceil
            // (exact positive integers land one high — the rzh-math
            // goldens pin the trick; this property checks the
            // choreography around it), then the cap (the points may go
            // NEGATIVE — the original clamps only against the cap)
            let delta = i64::from(rzh_sim::fastmath::real_to_int_ceil(1.0f32 * (g as f32)));
            model_points = (model_points + delta).min(point_cap);
            // the level the covered thresholds give (the level never
            // DROPS through addSkillPoints — only the level-up loop runs)
            let mut covered = 1;
            for (i, &needed) in sorted.iter().enumerate() {
                let level = i as i32 + 1;
                if level <= limit && model_points >= i64::from(needed) {
                    covered = level;
                }
            }
            let expected_level = level_before.max(covered);
            // the floor-raise: only the rungs the level-up walk crosses
            // lift the points (each setRankLevel raises them to the
            // walked rung's threshold)
            if expected_level > level_before {
                model_points = model_points.max(i64::from(sorted[(expected_level - 1) as usize]));
            }

            let p1 = &kernel.players.list.players[1];
            prop_assert_eq!(p1.rank_level, expected_level);
            prop_assert_eq!(i32::from(gained), i32::from(p1.rank_level > level_before));
            prop_assert_eq!(i64::from(p1.skill_points), model_points);
        }

        // the final invariant: the points sit inside the level's band (the
        // top's INT_MAX sentinel excepted) — but only for streams without
        // negative gains (a negative drift leaves the points below
        // levelDown, where the original's own debug assert ("hmm, wrong")
        // fires; the release build just carries them — the corpora pin the
        // release behaviour)
        let p1 = &kernel.players.list.players[1];
        let level = p1.rank_level;
        if !saw_negative {
            prop_assert!(i64::from(p1.skill_points) >= i64::from(p1.level_down));
            if p1.level_up != i32::MAX {
                prop_assert!(i64::from(p1.skill_points) < i64::from(p1.level_up));
            }
        }
        let spp_model: i32 = (1..=level).map(|i| i - 1).sum();
        prop_assert_eq!(p1.science_purchase_points, spp_model);
        // the sciences are exactly the granted set of the walked rungs
        prop_assert_eq!(p1.sciences.len(), level as usize);
    });
}

// ---- the sciences block ------------------------------------------------------

/// The purchase family: the purchasable and potentially-purchasable
/// walks agree with brute-force evaluations over the catalog, and
/// `isCapableOfPurchasingScience` is exactly the gate chain.
#[test]
fn purchasable_sets_match_the_brute_force() {
    proptest!(|(n_sciences in 2usize..10, prereq_unknown in prop::collection::vec(any::<bool>(), 0..10), budget in 0i32..8, buy_picks in prop::collection::vec(0u32..10, 0..12))| {
        let (mut kernel, mut host) = fresh();

        // the catalog: roots and chains with costs and grantable flags
        let science_specs: Vec<ScienceSpec> = (0..n_sciences)
            .enumerate()
            .map(|(i, _)| {
                let prereq = if i == 0 || i % 2 == 0 {
                    Vec::new()
                } else if prereq_unknown.get(i).copied().unwrap_or(false) {
                    vec!["NoSuchScience".to_string()]
                } else {
                    vec![format!("S{}", i - 1)]
                };
                ScienceSpec {
                    name: format!("S{i}"),
                    prereq_sciences: prereq,
                    science_purchase_point_cost: (i as i32 % 4),
                    grantable: i % 3 != 0,
                }
            })
            .collect();
        kernel.players.load_stores(&science_specs, &[]);
        kernel.players.reset(&mut kernel.objects, &mut host);

        // the player buys at random while it can
        kernel.players.add_science_purchase_points(1, budget);
        for pick in buy_picks {
            let science = kernel.players.science_from_name(&format!("S{}", pick as usize % n_sciences));
            let _ = kernel
                .players
                .attempt_to_purchase_science(1, science, &mut host);
        }

        // the brute force over the catalog
        let owned: Vec<ScienceType> = kernel.players.list.players[1].sciences.clone();
        let spp = kernel.players.list.players[1].science_purchase_points;
        let mut expected_can = Vec::new();
        let mut expected_pot = Vec::new();
        for spec in &science_specs {
            if spec.science_purchase_point_cost == 0 {
                continue;
            }
            let science = kernel.players.science_from_name(&spec.name);
            if owned.contains(&science) {
                continue;
            }
            let prereqs_met = spec.prereq_sciences.iter().all(|p| {
                let key = kernel.players.science_from_name(p);
                kernel.players.science_store.is_valid_science(key) && owned.contains(&key)
            });
            if prereqs_met {
                expected_can.push(science);
            } else {
                // the roots: the parse-time computation (the unknown
                // prereqs skip; the chains bottom out at the roots)
                let roots_met = kernel
                    .players
                    .science_store
                    .find_science_info(science)
                    .is_some_and(|si| si.root_sciences.iter().all(|&r| owned.contains(&r)));
                if roots_met {
                    expected_pot.push(science);
                }
            }
        }

        let (can, pot) = kernel.players.get_purchasable_sciences(1);
        let mut can_sorted = can;
        can_sorted.sort_by_key(|s| s.ordinal());
        let mut expected_can_sorted = expected_can;
        expected_can_sorted.sort_by_key(|s| s.ordinal());
        prop_assert_eq!(can_sorted, expected_can_sorted);
        let mut pot_sorted = pot;
        pot_sorted.sort_by_key(|s| s.ordinal());
        let mut expected_pot_sorted = expected_pot;
        expected_pot_sorted.sort_by_key(|s| s.ordinal());
        prop_assert_eq!(pot_sorted, expected_pot_sorted);

        // is_capable == the gate chain for every catalog entry
        for spec in &science_specs {
            let science = kernel.players.science_from_name(&spec.name);
            let capable = kernel.players.is_capable_of_purchasing_science(1, science);
            let gate = science != rzh_sim::sciences::SCIENCE_INVALID
                && !owned.contains(&science)
                && spec.science_purchase_point_cost != 0
                && spec.science_purchase_point_cost <= spp
                && spec.prereq_sciences.iter().all(|p| {
                    let key = kernel.players.science_from_name(p);
                    kernel.players.science_store.is_valid_science(key) && owned.contains(&key)
                })
                && !kernel.players.list.players[1].is_science_disabled(science)
                && !kernel.players.list.players[1].is_science_hidden(science);
            prop_assert_eq!(capable, gate);
        }
    });
}

/// The science availability choreography: DISABLED and HIDDEN are
/// mutually exclusive states (the remove-from-both before the one-list
/// add), and AVAILABLE clears both. Also pins the names-table parse
/// (`getScienceAvailabilityTypeFromString`).
#[test]
fn science_availability_states_are_exclusive() {
    proptest!(|(n in 1usize..8, transitions in prop::collection::vec(0u8..3u8, 1..24), sci_idx in 0u8..8u8)| {
        let (mut kernel, mut host) = fresh();
        let science_specs: Vec<ScienceSpec> = (0..n)
            .map(|i| ScienceSpec {
                name: format!("S{i}"),
                science_purchase_point_cost: 1,
                grantable: true,
                ..Default::default()
            })
            .collect();
        kernel.players.load_stores(&science_specs, &[]);
        kernel.players.reset(&mut kernel.objects, &mut host);

        // the names-table parse agrees with the ordinals
        for (i, name) in SCIENCE_AVAILABILITY_NAMES.iter().enumerate() {
            prop_assert_eq!(
                rzh_sim::sciences::ScienceAvailabilityType::from_string(name) as i32,
                i as i32
            );
        }
        prop_assert_eq!(
            rzh_sim::sciences::ScienceAvailabilityType::from_string("nosuchmode")
                as i32,
            -1
        );

        let science = kernel.players.science_from_name(&format!("S{}", sci_idx as usize % n));
        use rzh_sim::sciences::ScienceAvailabilityType as Av;
        for t in transitions {
            let ty = match t {
                0 => Av::Disabled,
                1 => Av::Hidden,
                _ => Av::Available,
            };
            kernel.players.list.players[1].set_science_availability(science, ty);
            let p = &kernel.players.list.players[1];
            match ty {
                Av::Disabled => {
                    prop_assert!(p.is_science_disabled(science));
                    prop_assert!(!p.is_science_hidden(science));
                }
                Av::Hidden => {
                    prop_assert!(!p.is_science_disabled(science));
                    prop_assert!(p.is_science_hidden(science));
                }
                Av::Available => {
                    prop_assert!(!p.is_science_disabled(science));
                    prop_assert!(!p.is_science_hidden(science));
                }
                Av::Invalid => {}
            }
            // the one-science availability is exclusive: at most one list
            // carries it
            prop_assert!(p.sciences_disabled.len() + p.sciences_hidden.len() <= 1);
        }
    });
}

/// The science identity: `science_from_name` interns without validation
/// (the same name is the same key; distinct names are distinct keys —
/// the interning's injectivity), and the catalog lookups resolve exactly
/// the loaded names.
#[test]
fn science_interning_is_injective() {
    proptest!(|(names in prop::collection::vec("[A-Za-z][A-Za-z0-9_]{0,8}", 1..16))| {
        let (mut kernel, mut host) = fresh();
        // distinct names only (a repeated name is one science — the
        // catalog's duplicate handling is the loader's contract)
        let mut names = names;
        names.sort_unstable();
        names.dedup();
        let science_specs: Vec<ScienceSpec> = names
            .iter()
            .map(|n| ScienceSpec {
                name: n.clone(),
                science_purchase_point_cost: 1,
                grantable: true,
                ..Default::default()
            })
            .collect();
        kernel.players.load_stores(&science_specs, &[]);
        kernel.players.reset(&mut kernel.objects, &mut host);

        // injectivity: distinct loaded names carry distinct keys
        let keys: Vec<u32> = names
            .iter()
            .map(|n| kernel.players.science_from_name(n).ordinal())
            .collect();
        let mut distinct = keys.clone();
        distinct.sort_unstable();
        distinct.dedup();
        prop_assert_eq!(keys.len(), distinct.len());

        // the catalog resolves exactly the loaded names
        for n in &names {
            let key = kernel.players.science_from_name(n);
            prop_assert!(kernel.players.science_store.is_valid_science(key));
            prop_assert_eq!(kernel.players.internal_name_for_science(key), Some(n.as_bytes()));
        }
        let nosuch = kernel.players.science_from_name("NoSuchScienceAtAll");
        prop_assert!(!kernel.players.science_store.is_valid_science(nosuch));

        // every player slot exists (the dump's 16-slot contract)
        prop_assert!(kernel.players.list.players.len() == MAX_PLAYER_COUNT);
    });
}
