//! Player-set climate environment (ADR-012, Phase E): latitude / longitude / season / average temperature
//! shape a derived [`ClimateSample`] (insolation, temperature, day length) that a later slice (E3) couples
//! into selection via the [`EnvironmentModifier`](crate::soil::EnvironmentModifier) seam (inv #5).
//!
//! ## Determinism (invariant #3)
//! The climate is a **pure deterministic function of the [`EnvParams`]** — multiply/add/clamp/`match` only,
//! **NO sin/cos/acos** (libm's transcendentals differ across platforms and would break "same seed+build+
//! platform → identical bytes"; `soil.rs` sets the same precedent). It draws **zero** from the run's `SimRng`,
//! so adding it is HASH-NEUTRAL until E3 actually couples it into the selection weight. Any future per-cell
//! spatial variation must come off a DISJOINT `derive_seed` family ([`CLIM_STREAM_BASE`]), never `SimRng`.

/// Disjoint `derive_seed` stream base for future per-cell climate variation (ASCII "CLIM"), kept far from the
/// soil ([`SOIL_STREAM_BASE`](crate::soil::SOIL_STREAM_BASE)) + placement families. Unused while the climate is
/// global (E1); reserved here so the stream registry stays authoritative (DECISIONS.md).
pub const CLIM_STREAM_BASE: u64 = 0x0043_4C49_4D00_0000;

/// The world's climate knobs, set by the player (the main menu, E4) or the CLI. `Default` = the neutral
/// temperate world the pinned determinism config has always used, so default runs stay byte-identical.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvParams {
    /// Latitude in degrees, `[-90, 90]` (drives day length + insolation with `season`).
    pub lat: f64,
    /// Longitude in degrees, `[-180, 180]` (currently presentation/locale only; reserved for local time).
    pub lon: f64,
    /// Average temperature, normalized `[0, 1]` (`0.5` ≈ temperate). The baseline the season/latitude shift.
    pub avg_temp: f64,
    /// Season as a 4-value index: `0` Spring · `1` Summer · `2` Autumn · `3` Winter (fixed declination, no trig).
    pub season: i64,
}

impl Default for EnvParams {
    fn default() -> Self {
        Self {
            lat: 0.0,
            lon: 0.0,
            avg_temp: 0.5,
            season: 0,
        }
    }
}

/// A single climate reading (all `[0, 1]`), handed to the climate [`EnvironmentModifier`] in E3.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimateSample {
    /// Insolation index `[0, 1]` — incident light energy (long days + low latitude ⇒ high).
    pub insolation: f64,
    /// Temperature index `[0, 1]` (`avg_temp` shifted warmer in summer / near the equator).
    pub temperature: f64,
    /// Day-length index `[0, 1]` (`0.5` ≈ a 12 h day; high latitude + summer ⇒ long, + winter ⇒ short).
    pub day_length: f64,
}

/// The world climate derived from [`EnvParams`]. Global (one sample) in E1; a per-cell field can layer on top
/// (off [`CLIM_STREAM_BASE`]) later. Built in [`Simulation::reset_with_env`](crate::Simulation) next to the
/// soil field — zero `SimRng` draws (inv #3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimateField {
    sample: ClimateSample,
}

impl ClimateField {
    /// Derive the world climate from the player's [`EnvParams`] — pure multiply/add/clamp/`match`, no trig.
    #[must_use]
    pub fn from_params(env: &EnvParams) -> Self {
        let lat_norm = (env.lat / 90.0).clamp(-1.0, 1.0); // -1 (S pole) … 0 (equator) … +1 (N pole)
                                                          // Season declination proxy: Summer tilts toward the lit hemisphere, Winter away; equinoxes neutral.
        let season_decl: f64 = match env.season {
            1 => 1.0,  // Summer
            3 => -1.0, // Winter
            _ => 0.0,  // Spring / Autumn (equinox)
        };
        // Day length: equator ≈ 0.5 year-round; |latitude| amplifies the seasonal swing. Linear, no acos.
        let day_length = (0.5 + 0.5 * lat_norm * season_decl * 0.9).clamp(0.0, 1.0);
        // Insolation: longer days + more direct sun near the equator (|lat| penalty) ⇒ more light energy.
        let insolation = (day_length * (1.0 - 0.35 * lat_norm.abs())).clamp(0.0, 1.0);
        // Temperature: the avg-temp baseline, warmer in summer, colder toward the poles.
        let temperature =
            (env.avg_temp + 0.25 * season_decl - 0.4 * lat_norm.abs()).clamp(0.0, 1.0);
        Self {
            sample: ClimateSample {
                insolation,
                temperature,
                day_length,
            },
        }
    }

    /// The world climate sample (global coupling, E3). A per-cell `sample_at(x, y)` can refine this later.
    #[must_use]
    pub fn sample(&self) -> ClimateSample {
        self.sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn climate_is_deterministic_and_in_range() {
        let env = EnvParams {
            lat: 50.1,
            lon: 14.4,
            avg_temp: 0.55,
            season: 1,
        };
        let a = ClimateField::from_params(&env).sample();
        let b = ClimateField::from_params(&env).sample();
        assert_eq!(
            a, b,
            "same params → identical climate (deterministic, no RNG)"
        );
        for v in [a.insolation, a.temperature, a.day_length] {
            assert!((0.0..=1.0).contains(&v), "climate channel {v} out of [0,1]");
        }
    }

    #[test]
    fn season_and_latitude_shift_climate() {
        // High-latitude summer has longer days + is warmer than the same latitude in winter.
        let summer = ClimateField::from_params(&EnvParams {
            lat: 60.0,
            lon: 0.0,
            avg_temp: 0.5,
            season: 1,
        })
        .sample();
        let winter = ClimateField::from_params(&EnvParams {
            lat: 60.0,
            lon: 0.0,
            avg_temp: 0.5,
            season: 3,
        })
        .sample();
        assert!(
            summer.day_length > winter.day_length,
            "summer days are longer"
        );
        assert!(summer.temperature > winter.temperature, "summer is warmer");
        // The equator is ~neutral (0.5 day length) regardless of season.
        let equator = ClimateField::from_params(&EnvParams::default()).sample();
        assert!(
            (equator.day_length - 0.5).abs() < 1e-9,
            "equator day length ≈ 0.5"
        );
    }

    #[test]
    fn default_env_is_neutral() {
        let c = ClimateField::from_params(&EnvParams::default()).sample();
        assert!((c.day_length - 0.5).abs() < 1e-9);
        assert!(
            (c.temperature - 0.5).abs() < 1e-9,
            "default avg_temp 0.5 at the equator ≈ 0.5"
        );
    }
}
