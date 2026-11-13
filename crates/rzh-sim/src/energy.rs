//! The energy block — `Energy.cpp`, the fifth Phase 3 slice's power
//! half.
//!
//! Origin: `GeneralsMD/Code/GameEngine/Source/Common/RTS/Energy.cpp`
//! and `Include/Common/Energy.h` at baseline 571a7c5 — the per-player
//! power pool: the sabotage-gated production read
//! ([`Energy::get_production`]), the consumption read, the sufficiency
//! verdict ([`Energy::has_sufficient_power`]) and the supply ratio
//! ([`Energy::get_energy_supply_ratio`]) with its zero-consumption
//! branch.
//!
//! The pinned quirks:
//!
//! - "for consistent nomenclature, we'll arbitrarily call energy units
//!   'kilowatts' (though that may have no bearing on reality)";
//! - the sabotage gate reads `TheGameLogic->getFrame() <
//!   m_powerSabotagedTillFrame` — a sabotaged player produces NOTHING
//!   (production reads 0, the ratio reads 0, sufficiency reads false)
//!   even with a full plant park;
//! - [`Energy::get_energy_supply_ratio`]'s zero-consumption branch
//!   returns the raw production as a `Real` (a ratio above 1 for a
//!   producer with no consumers — the UI clamps, the value doesn't);
//! - the owner is carried as the player index (`Player*` in the
//!   original); `None` is the pre-`init` state whose
//!   no-brownout-notify branch is unreachable in every real sequence
//!   (the `Player` ctor's `init` sets it) — carried faithfully.
//!
//! The mutation choreography — [`crate::players::Players`]'s
//! `energy_adjust_power` family and the brownout walk — lives in
//! `players.rs`, where the owner's team topology and the object arena
//! are reachable (the original's `addProduction`/`addConsumption` end
//! in `m_owner->onPowerBrownOutChange`, which sweeps the owner's
//! objects). The `xfer` save-load half is Phase 8's (plan ADR-010).
//!
//! Original: Copyright 2025 Electronic Arts Inc.
//! Rust port Copyright 2026 the RustZeroHour contributors.

/// `Energy` (Energy.h): "This class is used to encapsulate the Player's
/// energy use and production."
#[derive(Clone, Debug)]
pub struct Energy {
    /// `m_energyProduction` — level of energy production, in kw.
    pub energy_production: i32,
    /// `m_energyConsumption` — level of energy consumption, in kw.
    pub energy_consumption: i32,
    /// `m_powerSabotagedTillFrame` — "If power is sabotaged, the frame
    /// will be greater than now."
    pub power_sabotaged_till_frame: u32,
    /// `m_owner` — the owning player's index (`Player*` in the
    /// original, a tight pointer; `None` only before the owning
    /// `Player::init`).
    pub owner: Option<usize>,
}

impl Energy {
    /// `Energy::Energy` — the raw ctor (zeros, no owner).
    #[must_use]
    pub fn new() -> Self {
        Self {
            energy_production: 0,
            energy_consumption: 0,
            power_sabotaged_till_frame: 0,
            owner: None,
        }
    }

    /// `Energy::init(Player*)` — "reset energy information to base
    /// values."
    pub fn init(&mut self, owner: usize) {
        self.energy_production = 0;
        self.energy_consumption = 0;
        self.power_sabotaged_till_frame = 0;
        self.owner = Some(owner);
    }

    /// `Energy::getProduction` — the sabotage gate: no power while the
    /// sabotage lasts.
    #[must_use]
    pub fn get_production(&self, frame: u32) -> i32 {
        if frame < self.power_sabotaged_till_frame {
            // Power sabotaged, therefore no power.
            return 0;
        }
        self.energy_production
    }

    /// `Energy::getConsumption`.
    #[must_use]
    pub fn get_consumption(&self) -> i32 {
        self.energy_consumption
    }

    /// `Energy::hasSufficientPower` — the sabotage gate, then the
    /// `>=` verdict.
    #[must_use]
    pub fn has_sufficient_power(&self, frame: u32) -> bool {
        if frame < self.power_sabotaged_till_frame {
            // Power sabotaged, therefore no power.
            return false;
        }
        self.energy_production >= self.energy_consumption
    }

    /// `Energy::getEnergySupplyRatio` — "return the percentage of
    /// energy needed that we actually produce, as a 0.0 ... 1.0
    /// fraction" (the zero-consumption branch notwithstanding).
    #[must_use]
    pub fn get_energy_supply_ratio(&self, frame: u32) -> f32 {
        // (DEBUG_ASSERTCRASH(m_energyProduction >= 0 &&
        // m_energyConsumption >= 0, ("neg Energy numbers")) — the
        // corpora keep the contract)

        if frame < self.power_sabotaged_till_frame {
            // Power sabotaged, therefore no power, no ratio.
            return 0.0;
        }

        if self.energy_consumption == 0 {
            return self.energy_production as f32;
        }

        (self.energy_production as f32) / (self.energy_consumption as f32)
    }

    /// `Energy::setPowerSabotagedTillFrame`.
    pub fn set_power_sabotaged_till_frame(&mut self, frame: u32) {
        self.power_sabotaged_till_frame = frame;
    }

    /// `Energy::getPowerSabotagedTillFrame`.
    #[must_use]
    pub fn get_power_sabotaged_till_frame(&self) -> u32 {
        self.power_sabotaged_till_frame
    }
}

impl Default for Energy {
    fn default() -> Self {
        Self::new()
    }
}
