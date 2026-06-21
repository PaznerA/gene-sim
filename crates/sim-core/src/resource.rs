//! Dynamic resource pools — the per-cell ecology substrate the joule economy will consume (ADR-013
//! CHEMOSTAT-J, phase **F1**). Three channels: `light` (solar-influx proxy), `free_nutrient`, `detritus` —
//! each row-major `[0, 1]`.
//!
//! Like the soil field (ADR-008), the INITIAL pools are generated PURELY from the master seed via
//! [`derive_seed`](crate::det::derive_seed) — **zero** `SimRng` draws (invariant #3) — so introducing the
//! field cannot reorder the stream or change the determinism hash. At F1 the field is **present but UNWIRED**:
//! the metabolism that depletes + regenerates it (and couples it into selection) is F3. Inserting it at reset
//! is therefore **hash-neutral**, proven by the unchanged pinned literal (exactly as soil R1.0 was).

use crate::soil::gen_channel;

/// Number of resource channels: `light`, `free_nutrient`, `detritus` (each `[0, 1]`).
pub const RESOURCE_CHANNELS: usize = 3;

/// Default resource-field resolution (matches the world/soil grid 1:1).
pub const RESOURCE_DIMS: (u32, u32) = (32, 32);

/// Disjoint base for the resource `derive_seed` stream family (ASCII "RSRC"), kept far from the soil /
/// placement / climate families (DECISIONS.md stream registry). Off the `SimRng` stream (inv #3).
pub const RESOURCE_STREAM_BASE: u64 = 0x0052_5352_4300_0000;

/// A deterministic per-cell resource field: light / free nutrient / detritus, each row-major `[0, 1]`.
/// Static at F1 (the depletion/regeneration dynamics land in F3); generated off the `SimRng` stream.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceField {
    /// Field width in cells.
    pub width: u32,
    /// Field height in cells.
    pub height: u32,
    /// Per-cell light availability in `[0, 1]`, row-major.
    pub light: Vec<f32>,
    /// Per-cell free (plant-available) nutrient in `[0, 1]`, row-major.
    pub free_nutrient: Vec<f32>,
    /// Per-cell detritus (dead organic matter) in `[0, 1]`, row-major.
    pub detritus: Vec<f32>,
}

impl ResourceField {
    /// Generate the field from the master `seed` — **no `SimRng` draw** (inv #3) — via the shared seed-derived
    /// control lattice (the soil precedent) on the disjoint [`RESOURCE_STREAM_BASE`] family.
    #[must_use]
    pub fn generate(seed: u64, width: u32, height: u32) -> Self {
        assert!(width > 0 && height > 0, "resource field must be non-empty");
        Self {
            width,
            height,
            light: gen_channel(seed, RESOURCE_STREAM_BASE, 0, width, height),
            free_nutrient: gen_channel(seed, RESOURCE_STREAM_BASE, 1, width, height),
            detritus: gen_channel(seed, RESOURCE_STREAM_BASE, 2, width, height),
        }
    }

    /// The row-major plane for channel `ch` (`0` light, `1` free_nutrient, `2` detritus).
    #[must_use]
    pub fn channel(&self, ch: usize) -> &[f32] {
        match ch {
            0 => &self.light,
            1 => &self.free_nutrient,
            _ => &self.detritus,
        }
    }

    /// Nearest-cell value of channel `ch` resampled onto a `(target_w, target_h)` grid at `(tx, ty)` — pure
    /// integer arithmetic (deterministic). The render snapshot will export resource planes through this (F1b).
    #[must_use]
    pub fn sample_to(&self, ch: usize, tx: u32, ty: u32, target_w: u32, target_h: u32) -> f32 {
        let sx = ((u64::from(tx) * u64::from(self.width)) / u64::from(target_w))
            .min(u64::from(self.width) - 1);
        let sy = ((u64::from(ty) * u64::from(self.height)) / u64::from(target_h))
            .min(u64::from(self.height) - 1);
        self.channel(ch)[(sy * u64::from(self.width) + sx) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_in_range() {
        let a = ResourceField::generate(42, 32, 32);
        let b = ResourceField::generate(42, 32, 32);
        assert_eq!(a, b, "same seed → identical field (off-stream, inv #3)");
        for ch in 0..RESOURCE_CHANNELS {
            assert!(
                a.channel(ch).iter().all(|&v| (0.0..=1.0).contains(&v)),
                "channel {ch} in [0,1]"
            );
        }
    }

    #[test]
    fn disjoint_from_soil() {
        // Different disjoint derive_seed family → the resource field must differ from soil for the same seed.
        let r = ResourceField::generate(7, 32, 32);
        let s = crate::soil::SoilField::generate(7, 32, 32);
        assert_ne!(
            r.light, s.moisture,
            "resource light != soil moisture (disjoint stream base)"
        );
    }
}
