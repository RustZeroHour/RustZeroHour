//! The money block — `Money.cpp`, the fifth Phase 3 slice's wealth half.
//!
//! Origin: `GeneralsMD/Code/GameEngine/Source/Common/RTS/Money.cpp` and
//! `Include/Common/Money.h` at baseline 571a7c5 — the per-player
//! wallet: [`Money::withdraw`] (the over-draw clamp, "sorry, can't go
//! into debt"), [`Money::deposit`] (the tracked-income bucket
//! accounting), [`Money::set_starting_cash`] (the bucket reset),
//! [`Money::update_income_bucket`] (the frame-derived circular
//! rollover) and [`Money::cash_per_minute`].
//!
//! The pinned quirks:
//!
//! - the wallet and the income buckets are `UnsignedInt` — the adds
//!   wrap (defined behaviour in the original, `wrapping_add` here; the
//!   corpora pin a wrap);
//! - the income tracking keeps a 60-second circular buffer of
//!   per-second buckets; `updateIncomeBucket` rolls it by
//!   `(frame / LOGICFRAMES_PER_SECOND) % 60` — SECOND resolution, so
//!   deposits within one second share a bucket no matter when they
//!   land, and a rolled bucket's sum leaves `m_cashPerMinute` when the
//!   second turns (the subtraction is the wrapped unsigned one);
//! - `deposit` fires the academy-stats `recordIncome` hook even for
//!   the no-sound, no-track form (the new-game starting-cash deposit
//!   rides it — the TheSuperHackers @bugfix "correctly deposits the
//!   money and fixes its audio and academy issues" path);
//! - `withdraw`'s `buildsForFree` refusal exists only in RTS_DEBUG
//!   builds (`m_DEMO_freeBuild` is `#if defined(RTS_DEBUG)` in
//!   Player.h); the port models the release build and states the
//!   debug-only branch.
//!
//! The bodies this slice does not own are hooks:
//! [`crate::logic::KernelHost::on_money_audio`] stands for
//! `triggerAudioEvent` (the volume math — `m_preferredMoneyTransaction
//! Volume` times the event's volume, the `<= 0` early return — is the
//! audio phase's), and [`crate::logic::KernelHost::on_academy_income`]
//! for `AcademyStats::recordIncome` (the academy slice's). The
//! `xfer`/`crc` save-load halves are Phase 8's (plan ADR-010);
//! `parseMoneyAmount` is the INI loader's (rzh-ini/rzh-data).
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

use crate::logic::KernelHost;
use crate::types::{LOGICFRAMES_PER_SECOND, MAX_PLAYER_COUNT};

/// `Money.h`'s `m_incomeBuckets` size: "circular buffer of 60 seconds
/// for income tracking".
pub const INCOME_BUCKET_COUNT: usize = 60;

/// `Money` (Money.h): "How much 'money' (Tiberium, Gems, Magic Resource
/// Boxes, whatever) the Player has. This is currently a Very Simple
/// Class but is encapsulated in anticipation of future expansion."
///
/// The audio hooks and the academy hook ride the [`KernelHost`] the
/// mutating methods take; the frame for
/// [`Money::update_income_bucket`] is passed in (the original reads
/// `TheGameLogic->getFrame()`).
#[derive(Clone, Debug)]
pub struct Money {
    /// `m_money` — amount of money.
    pub money: u32,
    /// `m_playerIndex` — what is my player index? (the audio events
    /// and the academy-stats lookup carry it; the raw value, always a
    /// live slot in contract).
    pub player_index: i32,
    /// `m_incomeBuckets` — the circular per-second income buffer.
    pub income_buckets: [u32; INCOME_BUCKET_COUNT],
    /// `m_currentBucket`.
    pub current_bucket: u32,
    /// `m_cashPerMinute`.
    pub cash_per_minute: u32,
}

impl Default for Money {
    fn default() -> Self {
        // Money() : m_playerIndex(0) { init(); }
        let mut m = Self {
            money: 0,
            player_index: 0,
            income_buckets: [0; INCOME_BUCKET_COUNT],
            current_bucket: 0,
            cash_per_minute: 0,
        };
        m.init();
        m
    }
}

impl Money {
    /// `Money::init`: `setStartingCash(0)`.
    pub fn init(&mut self) {
        self.set_starting_cash(0);
    }

    /// `Money::countMoney`.
    #[must_use]
    pub fn count_money(&self) -> u32 {
        self.money
    }

    /// `Money::withdraw` — "returns the actual amount withdrawn, which
    /// may be less than you want. (sorry, can't go into debt...)".
    ///
    /// (The RTS_DEBUG `buildsForFree` refusal — return 0 without
    /// touching the wallet — is the debug build's cheat guard; stated,
    /// not modelled.)
    pub fn withdraw(
        &mut self,
        mut amount_to_withdraw: u32,
        play_sound: bool,
        host: &mut dyn KernelHost,
    ) -> u32 {
        if amount_to_withdraw > self.money {
            amount_to_withdraw = self.money;
        }

        if amount_to_withdraw == 0 {
            return amount_to_withdraw;
        }

        if play_sound {
            // triggerAudioEvent(TheAudio->getMiscAudio()->m_moneyWithdrawSound)
            // — the volume math is the audio phase's; the hook pins the
            // call and the player the event carries.
            host.on_money_audio(self.player_index as usize, true);
        }

        self.money -= amount_to_withdraw;

        amount_to_withdraw
    }

    /// `Money::deposit` — the zero refusal, the tracked-income bucket
    /// accounting, the academy-stats hook.
    pub fn deposit(
        &mut self,
        amount_to_deposit: u32,
        play_sound: bool,
        track_income: bool,
        host: &mut dyn KernelHost,
    ) {
        if amount_to_deposit == 0 {
            return;
        }

        if play_sound {
            // triggerAudioEvent(...m_moneyDepositSound) — ditto.
            host.on_money_audio(self.player_index as usize, false);
        }

        if track_income {
            self.income_buckets[self.current_bucket as usize] =
                self.income_buckets[self.current_bucket as usize].wrapping_add(amount_to_deposit);
            self.cash_per_minute = self.cash_per_minute.wrapping_add(amount_to_deposit);
        }

        self.money = self.money.wrapping_add(amount_to_deposit);

        // (the original's `if (amountToDeposit > 0)` is always true
        // here — the zero form returned above; the leftover guard
        // rides along)
        if amount_to_deposit > 0 {
            // Player *player = ThePlayerList->getNthPlayer(m_playerIndex);
            // if (player) player->getAcademyStats()->recordIncome();
            // — the bounds refusal is getNthPlayer's (PlayerList.cpp:
            // i < 0 || i >= MAX_PLAYER_COUNT → null).
            if (0..MAX_PLAYER_COUNT as i32).contains(&self.player_index) {
                host.on_academy_income(self.player_index as usize);
            }
        }
    }

    /// `Money::setStartingCash` — the wallet AND the income tracking
    /// reset (the buckets zero, the cursor home, the per-minute zero).
    pub fn set_starting_cash(&mut self, amount: u32) {
        self.money = amount;
        self.income_buckets = [0; INCOME_BUCKET_COUNT];
        self.current_bucket = 0;
        self.cash_per_minute = 0;
    }

    /// `Money::updateIncomeBucket` — the second-resolution rollover:
    /// the bucket for the frame's second takes over, its aged sum
    /// leaves the per-minute figure, and it starts clean.
    pub fn update_income_bucket(&mut self, frame: u32) {
        let next_bucket = (frame / LOGICFRAMES_PER_SECOND) % INCOME_BUCKET_COUNT as u32;
        if next_bucket != self.current_bucket {
            self.cash_per_minute = self
                .cash_per_minute
                .wrapping_sub(self.income_buckets[next_bucket as usize]);
            self.current_bucket = next_bucket;
            self.income_buckets[self.current_bucket as usize] = 0;
        }
    }

    /// `Money::getCashPerMinute`.
    #[must_use]
    pub fn get_cash_per_minute(&self) -> u32 {
        self.cash_per_minute
    }

    /// `Money::setPlayerIndex`.
    pub fn set_player_index(&mut self, ndx: i32) {
        self.player_index = ndx;
    }

    /// `Money::amountEqual` — "Does the amount of this == the amount of
    /// that (compare everything except m_playerIndex)".
    #[must_use]
    pub fn amount_equal(&self, that: &Money) -> bool {
        self.money == that.money
    }
}
