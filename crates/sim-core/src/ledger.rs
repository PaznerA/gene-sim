//! The conserved-energy LEDGER of the joule economy (ADR-013 CHEMOSTAT-J, phase **F0a** — scaffolding).
//!
//! Every joule (`J`, an `i64` quantum) in the substrate is conserved EXACTLY modulo **three named taps**:
//! INFLUX (solar minted per tick), RESPIRED (maintenance + trophic-efficiency loss), and OVERFLOW (the
//! explicit sink for any cap-saturation event, so saturating arithmetic can never *silently* destroy a
//! quantum). The invariant **`ledger_closes`** — `Σ(all live J) == initial_total + influx − respired −
//! overflow` — is a SEMANTIC gate stronger than the bit-hash; the metabolism/pool phases (F0b/F1/F3) will
//! assert it every tick.
//!
//! At **F0a** the ledger is present and inserted as a resource, but no system moves `J` yet (energy is still
//! `f64`, no pools exist), so it closes trivially against an initial total of 0. Adding it is **hash-neutral**
//! — the `Ledger` is never folded into `hash_world` and draws nothing from the `SimRng` stream.

use bevy_ecs::prelude::Resource;

/// The run's conserved-energy account. All amounts are joule quanta (`i64`). `initial_total` is the `J` present
/// in the world at reset; the three taps record cumulative flow across the world boundary since reset.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Total `J` present in the world at reset (pools + organism stores + chem). Zero until F1 seeds pools.
    pub initial_total: i64,
    /// Cumulative `J` minted into the world (solar influx).
    pub influx: i64,
    /// Cumulative `J` dissipated as maintenance + trophic inefficiency.
    pub respired: i64,
    /// Cumulative `J` routed to the overflow sink on cap-saturation — never silently lost.
    pub overflow: i64,
}

impl Ledger {
    /// The `J` the books say should currently be live: `initial + influx − respired − overflow`.
    #[must_use]
    pub fn expected_total(&self) -> i64 {
        self.initial_total + self.influx - self.respired - self.overflow
    }

    /// The `ledger_closes` conservation invariant: does the actually-measured live `J` equal what the taps
    /// say it should be? Later phases call this each tick with the summed live `J` of pools + organisms + chem.
    #[must_use]
    pub fn closes(&self, measured_live_total: i64) -> bool {
        measured_live_total == self.expected_total()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_ledger_closes_at_zero() {
        let l = Ledger::default();
        assert_eq!(l.expected_total(), 0);
        assert!(l.closes(0));
        assert!(!l.closes(1));
    }

    #[test]
    fn taps_conserve() {
        // 1000 seeded, +300 minted, −120 respired, −30 to overflow → 1150 should be live.
        let l = Ledger {
            initial_total: 1000,
            influx: 300,
            respired: 120,
            overflow: 30,
        };
        assert_eq!(l.expected_total(), 1150);
        assert!(l.closes(1150));
        assert!(
            !l.closes(1149),
            "a single lost quantum must break the books"
        );
    }
}
