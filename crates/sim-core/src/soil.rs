//! Deterministic per-cell environment substrate — terrain / soil composition (roadmap R1.0).
//!
//! INVARIANT #3 (the load-bearing one here): the field is generated PURELY from the master seed via
//! [`derive_seed`](crate::det::derive_seed) (the stateless splitmix64), drawing **zero** from the threaded
//! `SimRng`. So introducing soil cannot reorder the RNG stream or change the determinism hash — it is
//! **off the hash path** exactly like the snapshot layout. Generation uses only integer / bit-mix /
//! multiply-add `f64` ops (no transcendentals) so it is byte-reproducible across platforms.
//!
//! The field is **static** for a run (R1.0). It couples to nothing yet: the [`EnvironmentModifier`] trait
//! below is the pluggable science seam (invariant #5) that a later phase (R1.1+) and ultimately Stage-5
//! LLM-generated modifiers will wire into selection — but in R1.0 it is **present and unwired**.
//!
//! ## derive_seed stream registry (keep disjoint — see DECISIONS.md)
//! * streams `1`, `2` — snapshot organism placement (`x`/`y`), used by [`Simulation::snapshot`].
//! * streams `[SOIL_STREAM_BASE, SOIL_STREAM_BASE + SOIL_CHANNELS*LATTICE*LATTICE)` — soil control points.
//!   `SOIL_STREAM_BASE` is a large disjoint constant so the soil family can never collide with placement.

use crate::det::derive_seed;
use crate::unit_f64;

/// Number of soil channels: `moisture`, `nutrients`, `pH` (each `[0, 1]`).
pub const SOIL_CHANNELS: usize = 3;

/// Default soil-field resolution. Deliberately a constant (not a `SimConfig` field) in R1.0 so it is
/// decoupled from the renderer's runtime snapshot grid and adds no constructor churn; the snapshot
/// resamples soil onto its own `(width, height)` at export time.
pub const SOIL_DIMS: (u32, u32) = (32, 32);

/// Coarse control-point lattice per channel; bilinear interpolation between points yields smooth gradients
/// (so a future spatial coupling sees clines, not white noise).
const LATTICE: usize = 5;

/// Disjoint base for the soil `derive_seed` stream family (ASCII "SOIL" tagged, far from placement 1/2).
pub const SOIL_STREAM_BASE: u64 = 0x0050_4F49_4C00_0000;

/// A static, deterministic per-cell environment field: moisture / nutrients / pH, each row-major `[0, 1]`.
#[derive(Debug, Clone, PartialEq)]
pub struct SoilField {
    /// Field width in soil cells.
    pub width: u32,
    /// Field height in soil cells.
    pub height: u32,
    /// Per-cell moisture in `[0, 1]`, row-major (`width * height`).
    pub moisture: Vec<f32>,
    /// Per-cell nutrient level in `[0, 1]`, row-major.
    pub nutrients: Vec<f32>,
    /// Per-cell pH (normalized to `[0, 1]`), row-major.
    pub ph: Vec<f32>,
}

impl SoilField {
    /// Generate the field from the master `seed` — **no `SimRng` draw** (invariant #3). Each channel is a
    /// `LATTICE×LATTICE` grid of seed-derived control points, bilinearly interpolated to `width×height`.
    #[must_use]
    pub fn generate(seed: u64, width: u32, height: u32) -> Self {
        assert!(width > 0 && height > 0, "soil field must be non-empty");
        Self {
            width,
            height,
            moisture: gen_channel(seed, 0, width, height),
            nutrients: gen_channel(seed, 1, width, height),
            ph: gen_channel(seed, 2, width, height),
        }
    }

    /// The row-major plane for channel `ch` (`0` moisture, `1` nutrients, `2` pH).
    #[must_use]
    pub fn channel(&self, ch: usize) -> &[f32] {
        match ch {
            0 => &self.moisture,
            1 => &self.nutrients,
            _ => &self.ph,
        }
    }

    /// The field-wide mean of each channel as a [`SoilSample`] — the per-run scalar the **global** soil
    /// coupling (R1.1) feeds to the [`EnvironmentModifier`]. Deterministic (ordered sum).
    #[must_use]
    pub fn mean_sample(&self) -> SoilSample {
        let mean = |v: &[f32]| -> f64 {
            if v.is_empty() {
                0.0
            } else {
                v.iter().map(|&x| f64::from(x)).sum::<f64>() / v.len() as f64
            }
        };
        SoilSample {
            moisture: mean(&self.moisture),
            nutrients: mean(&self.nutrients),
            ph: mean(&self.ph),
        }
    }

    /// Nearest-cell value of channel `ch` resampled onto a `(target_w, target_h)` grid at `(tx, ty)`.
    /// Pure integer arithmetic (deterministic); used by [`Simulation::snapshot`] to export soil planes.
    #[must_use]
    pub fn sample_to(&self, ch: usize, tx: u32, ty: u32, target_w: u32, target_h: u32) -> f32 {
        // Map target cell -> nearest soil cell (clamp at the high edge).
        let sx = ((u64::from(tx) * u64::from(self.width)) / u64::from(target_w))
            .min(u64::from(self.width) - 1);
        let sy = ((u64::from(ty) * u64::from(self.height)) / u64::from(target_h))
            .min(u64::from(self.height) - 1);
        let idx = (sy * u64::from(self.width) + sx) as usize;
        self.channel(ch)[idx]
    }
}

/// Build one bilinearly-interpolated channel from a seed-derived control lattice. Determinism: control
/// points come from `derive_seed(seed, SOIL_STREAM_BASE + ch*LATTICE² + point)` (no `SimRng`), and the
/// interpolation is multiply-add only.
fn gen_channel(seed: u64, ch: usize, width: u32, height: u32) -> Vec<f32> {
    let mut ctrl = [[0.0f64; LATTICE]; LATTICE];
    for (ly, row) in ctrl.iter_mut().enumerate() {
        for (lx, v) in row.iter_mut().enumerate() {
            let point = (ly * LATTICE + lx) as u64;
            let stream = SOIL_STREAM_BASE + (ch as u64) * (LATTICE * LATTICE) as u64 + point;
            *v = unit_f64(derive_seed(seed, stream));
        }
    }

    let mut out = vec![0.0f32; (width as usize) * (height as usize)];
    let span = (LATTICE - 1) as f64;
    for y in 0..height {
        for x in 0..width {
            // Cell centre mapped into lattice space [0, LATTICE-1].
            let fx = (f64::from(x) + 0.5) / f64::from(width) * span;
            let fy = (f64::from(y) + 0.5) / f64::from(height) * span;
            let x0 = fx.floor() as usize;
            let y0 = fy.floor() as usize;
            let x1 = (x0 + 1).min(LATTICE - 1);
            let y1 = (y0 + 1).min(LATTICE - 1);
            let dx = fx - x0 as f64;
            let dy = fy - y0 as f64;
            let top = ctrl[y0][x0] + (ctrl[y0][x1] - ctrl[y0][x0]) * dx;
            let bot = ctrl[y1][x0] + (ctrl[y1][x1] - ctrl[y1][x0]) * dx;
            let v = top + (bot - top) * dy;
            out[(y as usize) * (width as usize) + (x as usize)] = v.clamp(0.0, 1.0) as f32;
        }
    }
    out
}

/// A single cell's soil reading, handed to an [`EnvironmentModifier`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoilSample {
    /// Moisture in `[0, 1]`.
    pub moisture: f64,
    /// Nutrient level in `[0, 1]`.
    pub nutrients: f64,
    /// pH (normalized `[0, 1]`).
    pub ph: f64,
}

/// **Pluggable science seam (invariant #5).** How local soil modulates an organism's fitness given its
/// **per-individual** drought tolerance. Wired into [`selection`](crate::Simulation) from R1.1; Stage-5
/// admits schema-validated LLM-generated impls behind this same trait without touching sim-core.
pub trait EnvironmentModifier {
    /// A strictly-positive multiplicative fitness factor for an organism with the given heritable
    /// `drought_tolerance` (`[0, 1]`) in the given soil. Strictly positive so it can never zero a selection
    /// weight (preserves ADR-005's no-extinction guarantee).
    fn fitness_factor(&self, soil: SoilSample, drought_tolerance: f64) -> f64;
}

/// In-core default modifier: a drought-tolerant individual is favoured on **drier** soil. Linear, bounded,
/// strictly positive.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinearTraitMatchModifier;

impl EnvironmentModifier for LinearTraitMatchModifier {
    fn fitness_factor(&self, soil: SoilSample, drought_tolerance: f64) -> f64 {
        // Want: high drought tolerance ↔ low moisture. Match = 1 - |drought - (1 - moisture)|.
        let target = 1.0 - soil.moisture;
        let match_ = 1.0 - (drought_tolerance - target).abs();
        // Map a [0,1] match to a strictly-positive band [0.5, 1.5].
        0.5 + match_.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soil_is_reproducible_and_seed_sensitive() {
        let a = SoilField::generate(42, 16, 12);
        let b = SoilField::generate(42, 16, 12);
        let c = SoilField::generate(43, 16, 12);
        assert_eq!(a, b, "same seed ⇒ identical soil");
        assert_ne!(a, c, "different seed ⇒ different soil");
    }

    #[test]
    fn channels_in_unit_range() {
        let s = SoilField::generate(7, 24, 24);
        for ch in 0..SOIL_CHANNELS {
            assert!(
                s.channel(ch).iter().all(|&v| (0.0..=1.0).contains(&v)),
                "channel {ch} out of [0,1]"
            );
        }
    }

    #[test]
    fn soil_is_smooth_not_white_noise() {
        // Bilinear interpolation ⇒ adjacent cells are highly correlated (small mean |Δ|).
        let s = SoilField::generate(99, 32, 32);
        let w = s.width as usize;
        let mut total = 0.0f64;
        let mut n = 0u32;
        for y in 0..s.height as usize {
            for x in 1..w {
                total += (s.moisture[y * w + x] - s.moisture[y * w + x - 1]).abs() as f64;
                n += 1;
            }
        }
        let mean_step = total / f64::from(n);
        assert!(
            mean_step < 0.1,
            "soil should be smooth, mean |Δ| = {mean_step}"
        );
    }

    #[test]
    fn resample_clamps_to_field() {
        let s = SoilField::generate(1, 8, 8);
        // Sampling a larger target grid stays in range and never indexes out of bounds.
        for ty in 0..40u32 {
            for tx in 0..40u32 {
                let v = s.sample_to(0, tx, ty, 40, 40);
                assert!((0.0..=1.0).contains(&v));
            }
        }
    }

    #[test]
    fn modifier_favours_drought_tolerant_on_dry_soil() {
        let m = LinearTraitMatchModifier;
        let dry = SoilSample {
            moisture: 0.1,
            nutrients: 0.5,
            ph: 0.5,
        };
        let wet = SoilSample {
            moisture: 0.9,
            nutrients: 0.5,
            ph: 0.5,
        };
        // A drought-tolerant individual scores higher on dry soil than wet, always strictly positive.
        assert!(m.fitness_factor(dry, 0.9) > m.fitness_factor(wet, 0.9));
        assert!(m.fitness_factor(wet, 0.9) > 0.0);
        // ...and a drought-intolerant individual is favoured on wet soil instead.
        assert!(m.fitness_factor(wet, 0.1) > m.fitness_factor(dry, 0.1));
    }

    #[test]
    fn mean_sample_in_range() {
        let s = SoilField::generate(5, 20, 20);
        let m = s.mean_sample();
        assert!((0.0..=1.0).contains(&m.moisture));
        assert!((0.0..=1.0).contains(&m.nutrients));
        assert!((0.0..=1.0).contains(&m.ph));
    }
}
