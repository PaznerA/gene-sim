//! Compact, read-only sim→render snapshot grids (SPEC §5, §W10; slice S4.2).
//!
//! A [`GridSnapshot`] is a **derived** per-cell view of the live simulation produced by
//! [`Simulation::snapshot`](crate::Simulation::snapshot). It exists purely so the (read-only) Godot
//! renderer can sample a per-cell data texture — channels `density` / `allele_freq` / `fitness` (SPEC §W10)
//! — without crossing the core boundary per entity in a hot loop. It is **never** part of the sim loop and
//! **never** touches the [`SimRng`](crate::SimRng) stream, so producing snapshots cannot change the
//! determinism hash (invariant #3). All genotype→phenotype biology stays in the core (invariant #2);
//! GDScript only reads the bytes this module emits.
//!
//! ## Placement model (ADR-011: real spatial biology)
//! Organisms now carry a real per-individual `Position` on the canonical world grid (`WORLD_DIMS`), seeded
//! off-`SimRng` at spawn and inherited + dispersed by selection so lineages cluster into emergent regions.
//! `Simulation::snapshot` RESAMPLES each organism's real world cell onto the render `(width, height)` grid —
//! it no longer derives a cell from `OrgId`. Still pure w.r.t. the run (no `SimRng` draw, no mutation), so a
//! given `(seed, generation, grid)` yields byte-identical snapshots and producing them never changes the
//! determinism hash (inv #3). This is a READ-ONLY projection of real biology (inv #2), not a viz-only layout.
//!
//! ## Binary format ([`GridSnapshot::write_snapshot_bytes`])
//! Little-endian, `std`-only (no `bincode`/`serde`):
//! ```text
//!   bytes 0..4 : ASCII magic "GSS5"
//!   u32 LE     : width
//!   u32 LE     : height
//!   u32 LE     : channel_count (= 13)
//!   u64 LE     : generation
//!   u32 LE     : population
//!   then channel-major, each channel's width*height f32 LE values in row-major order,
//!   channels in order: density, allele_freq, fitness, soil_moisture, soil_nutrients, soil_ph,
//!   light, free_nutrient, detritus, toxin, kin, alarm, dominant_species_id.
//! ```
//! The last channel `dominant_species_id` (GSS5) carries the MOST-POPULOUS [`crate::SpeciesId`] ordinal in
//! each cell as an exact integer encoded in an `f32` (`u16` 0..=65535 round-trips through `f32` losslessly).
//! It is `0.0` for empty cells; the renderer maps the id → a per-species size/colour (the species-map view).
//! Like the chem/pool planes it is a READ-ONLY projection off the LIVING `Species` tags — no `SimRng` draw,
//! no mutation — so producing it never perturbs the determinism hash (inv #2/#3); the pinned literal is
//! byte-identical for a single-species run (whose dominant id is uniformly `0`).

use std::io;
use std::path::Path;

/// ASCII magic header identifying the snapshot binary format (`"GSS5"` = Gene-Sim Snapshot v5; v2 added the
/// 3 soil channels, v3 appended the 3 live-pool channels, v4 (ADR-013 F5) appended the 3 chem channels
/// (toxin/kin/alarm), v5 appends the per-cell `dominant_species_id` plane (the species-map view) — a bumped
/// magic turns a stale 12-channel reader into a loud bad-magic error, not a silent mis-parse at the wrong offset).
pub const SNAPSHOT_MAGIC: [u8; 4] = *b"GSS5";

/// The number of per-cell channels a [`GridSnapshot`] carries: `density`, `allele_freq`, `fitness`,
/// `soil_moisture`, `soil_nutrients`, `soil_ph`, `light`, `free_nutrient`, `detritus`, `toxin`, `kin`, `alarm`,
/// `dominant_species_id`.
pub const CHANNEL_COUNT: u32 = 13;

/// A read-only, derived per-cell grid view of the simulation for the renderer (SPEC §W10).
///
/// Each channel is a `width * height` vector in **row-major** order (`index = y * width + x`) with values
/// in `[0, 1]`:
/// * `density`     — per-cell organism count, normalized by the busiest cell's count (`0` for empty cells).
/// * `allele_freq` — mean per-individual `Genotype` of the cell's organisms (`0` for empty cells).
/// * `fitness`     — mean per-individual `Energy` of the cell's organisms (`0` for empty cells).
///
/// Produced by [`Simulation::snapshot`](crate::Simulation::snapshot); see this module's docs for the
/// deterministic placement model and the binary format.
#[derive(Debug, Clone, PartialEq)]
pub struct GridSnapshot {
    /// Grid width in cells.
    pub width: u32,
    /// Grid height in cells.
    pub height: u32,
    /// Generations advanced so far (the `Tick` counter) at snapshot time.
    pub generation: u64,
    /// Number of living organisms aggregated into the grid.
    pub population: u32,
    /// Per-cell normalized density in `[0, 1]`, row-major (`width * height`).
    pub density: Vec<f32>,
    /// Per-cell mean `Genotype` in `[0, 1]`, row-major (`width * height`); `0` for empty cells.
    pub allele_freq: Vec<f32>,
    /// Per-cell mean `Energy` in `[0, 1]`, row-major (`width * height`); `0` for empty cells.
    pub fitness: Vec<f32>,
    /// Per-cell soil moisture in `[0, 1]`, row-major; resampled from the run's static `SoilField` (R1.0).
    pub soil_moisture: Vec<f32>,
    /// Per-cell soil nutrient level in `[0, 1]`, row-major (resampled from the `SoilField`).
    pub soil_nutrients: Vec<f32>,
    /// Per-cell soil pH (normalized `[0, 1]`), row-major (resampled from the `SoilField`).
    pub soil_ph: Vec<f32>,
    /// Per-cell live `light` joule stock in `[0, 1]`, row-major; resampled from the live `PoolStock` / `POOL_CAP`.
    pub light: Vec<f32>,
    /// Per-cell live `free_nutrient` stock in `[0, 1]`, row-major (resampled from `PoolStock` / `POOL_CAP`).
    pub free_nutrient: Vec<f32>,
    /// Per-cell live `detritus` stock in `[0, 1]`, row-major (resampled from `PoolStock` / `POOL_CAP`).
    pub detritus: Vec<f32>,
    /// Per-cell live `toxin` chem stock in `[0, 1]`, row-major (ADR-013 F5; resampled from `ChemField` / `CHEM_CAP`).
    pub toxin: Vec<f32>,
    /// Per-cell live `kin` marker in `[0, 1]`, row-major (ADR-013 F5; resampled from `ChemField` / `CHEM_CAP`).
    pub kin: Vec<f32>,
    /// Per-cell live `alarm` signal in `[0, 1]`, row-major (ADR-013 F5; resampled from `ChemField` / `CHEM_CAP`).
    pub alarm: Vec<f32>,
    /// Per-cell MOST-POPULOUS [`crate::SpeciesId`] ordinal as an exact `u16`-in-`f32`, row-major (GSS5); `0` for
    /// empty cells. Ties resolve to the LOWEST `SpeciesId` (deterministic). A read-only display projection off
    /// the living `Species` tags (no RNG, no mutation) — the renderer maps it to a per-species size/colour.
    pub dominant_species_id: Vec<f32>,
}

impl GridSnapshot {
    /// Serialize to the exact little-endian binary format documented on this module (`std`-only).
    ///
    /// Header (`magic`, dims, `channel_count`, `generation`, `population`) followed by every channel
    /// channel-major — `density`, `allele_freq`, `fitness`, the soil/pool/chem planes, then
    /// `dominant_species_id` — each `width * height` `f32` LE in row-major order. Deterministic for a snapshot.
    #[must_use]
    pub fn write_snapshot_bytes(&self) -> Vec<u8> {
        let cells = (self.width as usize) * (self.height as usize);
        // 4 (magic) + 4+4+4 (dims+channels) + 8 (gen) + 4 (pop) + CHANNEL_COUNT channels * cells * 4 bytes.
        let mut buf = Vec::with_capacity(28 + CHANNEL_COUNT as usize * cells * 4);

        buf.extend_from_slice(&SNAPSHOT_MAGIC);
        buf.extend_from_slice(&self.width.to_le_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&CHANNEL_COUNT.to_le_bytes());
        buf.extend_from_slice(&self.generation.to_le_bytes());
        buf.extend_from_slice(&self.population.to_le_bytes());

        for channel in [
            &self.density,
            &self.allele_freq,
            &self.fitness,
            &self.soil_moisture,
            &self.soil_nutrients,
            &self.soil_ph,
            &self.light,
            &self.free_nutrient,
            &self.detritus,
            &self.toxin,
            &self.kin,
            &self.alarm,
            &self.dominant_species_id,
        ] {
            for &v in channel {
                buf.extend_from_slice(&v.to_le_bytes());
            }
        }
        buf
    }

    /// Write [`write_snapshot_bytes`](Self::write_snapshot_bytes) to `path` (creates/overwrites the file).
    ///
    /// # Errors
    /// Propagates any [`std::fs::write`] I/O error.
    pub fn write_to(&self, path: impl AsRef<Path>) -> io::Result<()> {
        std::fs::write(path, self.write_snapshot_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal, std-only parser mirroring [`GridSnapshot::write_snapshot_bytes`] for round-trip tests.
    /// (The real reader lives in `godot/` GDScript; this just proves the byte layout.)
    fn parse(bytes: &[u8]) -> GridSnapshot {
        assert_eq!(&bytes[0..4], &SNAPSHOT_MAGIC, "bad magic");
        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let width = u32_at(4);
        let height = u32_at(8);
        let channel_count = u32_at(12);
        assert_eq!(channel_count, CHANNEL_COUNT);
        let generation = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        let population = u32_at(24);

        let cells = (width as usize) * (height as usize);
        let mut off = 28;
        let mut read_channel = || {
            let mut ch = Vec::with_capacity(cells);
            for _ in 0..cells {
                ch.push(f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()));
                off += 4;
            }
            ch
        };
        let density = read_channel();
        let allele_freq = read_channel();
        let fitness = read_channel();
        let soil_moisture = read_channel();
        let soil_nutrients = read_channel();
        let soil_ph = read_channel();
        let light = read_channel();
        let free_nutrient = read_channel();
        let detritus = read_channel();
        let toxin = read_channel();
        let kin = read_channel();
        let alarm = read_channel();
        let dominant_species_id = read_channel();
        assert_eq!(off, bytes.len(), "trailing bytes");

        GridSnapshot {
            width,
            height,
            generation,
            population,
            density,
            allele_freq,
            fitness,
            soil_moisture,
            soil_nutrients,
            soil_ph,
            light,
            free_nutrient,
            detritus,
            toxin,
            kin,
            alarm,
            dominant_species_id,
        }
    }

    #[test]
    fn bytes_round_trip_header_and_cells() {
        let snap = GridSnapshot {
            width: 3,
            height: 2,
            generation: 17,
            population: 5,
            density: vec![0.0, 0.25, 0.5, 0.75, 1.0, 0.125],
            allele_freq: vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6],
            fitness: vec![0.9, 0.8, 0.7, 0.6, 0.5, 0.4],
            soil_moisture: vec![0.11, 0.22, 0.33, 0.44, 0.55, 0.66],
            soil_nutrients: vec![0.6, 0.5, 0.4, 0.3, 0.2, 0.1],
            soil_ph: vec![0.05, 0.15, 0.25, 0.35, 0.45, 0.55],
            light: vec![0.01, 0.12, 0.23, 0.34, 0.45, 0.56],
            free_nutrient: vec![0.99, 0.88, 0.77, 0.66, 0.55, 0.44],
            detritus: vec![0.02, 0.04, 0.08, 0.16, 0.32, 0.64],
            toxin: vec![0.03, 0.06, 0.09, 0.12, 0.15, 0.18],
            kin: vec![0.07, 0.14, 0.21, 0.28, 0.35, 0.42],
            alarm: vec![0.5, 0.4, 0.3, 0.2, 0.1, 0.05],
            dominant_species_id: vec![0.0, 1.0, 2.0, 0.0, 3.0, 1.0],
        };
        let back = parse(&snap.write_snapshot_bytes());

        // Header.
        assert_eq!(back.width, 3);
        assert_eq!(back.height, 2);
        assert_eq!(back.generation, 17);
        assert_eq!(back.population, 5);
        // Sample cells across all nine channels (exact f32 bit equality).
        assert_eq!(back.density, snap.density);
        assert_eq!(back.allele_freq, snap.allele_freq);
        assert_eq!(back.fitness, snap.fitness);
        assert_eq!(back.soil_moisture, snap.soil_moisture);
        assert_eq!(back.soil_nutrients, snap.soil_nutrients);
        assert_eq!(back.soil_ph, snap.soil_ph);
        assert_eq!(back.light, snap.light);
        assert_eq!(back.free_nutrient, snap.free_nutrient);
        assert_eq!(back.detritus, snap.detritus);
        assert_eq!(back.toxin, snap.toxin);
        assert_eq!(back.kin, snap.kin);
        assert_eq!(back.alarm, snap.alarm);
        assert_eq!(back.dominant_species_id, snap.dominant_species_id);
        // Full struct equality.
        assert_eq!(back, snap);
    }

    #[test]
    fn byte_length_matches_layout() {
        let snap = GridSnapshot {
            width: 4,
            height: 4,
            generation: 0,
            population: 0,
            density: vec![0.0; 16],
            allele_freq: vec![0.0; 16],
            fitness: vec![0.0; 16],
            soil_moisture: vec![0.0; 16],
            soil_nutrients: vec![0.0; 16],
            soil_ph: vec![0.0; 16],
            light: vec![0.0; 16],
            free_nutrient: vec![0.0; 16],
            detritus: vec![0.0; 16],
            toxin: vec![0.0; 16],
            kin: vec![0.0; 16],
            alarm: vec![0.0; 16],
            dominant_species_id: vec![0.0; 16],
        };
        let bytes = snap.write_snapshot_bytes();
        // 28-byte header + 13 channels * 16 cells * 4 bytes.
        assert_eq!(bytes.len(), 28 + 13 * 16 * 4);
        assert_eq!(&bytes[0..4], b"GSS5");
    }
}
