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
//! ## Placement model (PoC: derived spatial layout)
//! The core has **no spatial dynamics yet** — organisms are not positioned in 2D. To give the renderer
//! something to draw, each organism is placed into a grid cell by a **deterministic function of its
//! [`OrgId`](crate::OrgId) only** (via the splitmix in [`derive_seed`](crate::derive_seed)):
//! `x = derive_seed(orgid, 1) % width`, `y = derive_seed(orgid, 2) % height`. This is reproducible and
//! independent of the RNG stream, so a given `(seed, generation, grid)` always yields byte-identical
//! snapshots. Real spatial dynamics are future work; this is a layout for visualization, not biology.
//!
//! ## Binary format ([`GridSnapshot::write_snapshot_bytes`])
//! Little-endian, `std`-only (no `bincode`/`serde`):
//! ```text
//!   bytes 0..4 : ASCII magic "GSS1"
//!   u32 LE     : width
//!   u32 LE     : height
//!   u32 LE     : channel_count (= 3)
//!   u64 LE     : generation
//!   u32 LE     : population
//!   then channel-major, each channel's width*height f32 LE values in row-major order,
//!   channels in order: density, allele_freq, fitness.
//! ```

use std::io;
use std::path::Path;

/// ASCII magic header identifying the snapshot binary format (`"GSS1"` = Gene-Sim Snapshot v1).
pub const SNAPSHOT_MAGIC: [u8; 4] = *b"GSS1";

/// The number of per-cell channels a [`GridSnapshot`] carries: `density`, `allele_freq`, `fitness`.
pub const CHANNEL_COUNT: u32 = 3;

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
}

impl GridSnapshot {
    /// Serialize to the exact little-endian binary format documented on this module (`std`-only).
    ///
    /// Header (`magic`, dims, `channel_count`, `generation`, `population`) followed by the three channels
    /// channel-major — `density`, then `allele_freq`, then `fitness` — each `width * height` `f32` LE in
    /// row-major order. Deterministic for a given snapshot.
    #[must_use]
    pub fn write_snapshot_bytes(&self) -> Vec<u8> {
        let cells = (self.width as usize) * (self.height as usize);
        // 4 (magic) + 4+4+4 (dims+channels) + 8 (gen) + 4 (pop) + 3 channels * cells * 4 bytes.
        let mut buf = Vec::with_capacity(28 + CHANNEL_COUNT as usize * cells * 4);

        buf.extend_from_slice(&SNAPSHOT_MAGIC);
        buf.extend_from_slice(&self.width.to_le_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&CHANNEL_COUNT.to_le_bytes());
        buf.extend_from_slice(&self.generation.to_le_bytes());
        buf.extend_from_slice(&self.population.to_le_bytes());

        for channel in [&self.density, &self.allele_freq, &self.fitness] {
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
        assert_eq!(off, bytes.len(), "trailing bytes");

        GridSnapshot {
            width,
            height,
            generation,
            population,
            density,
            allele_freq,
            fitness,
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
        };
        let back = parse(&snap.write_snapshot_bytes());

        // Header.
        assert_eq!(back.width, 3);
        assert_eq!(back.height, 2);
        assert_eq!(back.generation, 17);
        assert_eq!(back.population, 5);
        // Sample cells across all three channels (exact f32 bit equality).
        assert_eq!(back.density, snap.density);
        assert_eq!(back.allele_freq, snap.allele_freq);
        assert_eq!(back.fitness, snap.fitness);
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
        };
        let bytes = snap.write_snapshot_bytes();
        // 28-byte header + 3 channels * 16 cells * 4 bytes.
        assert_eq!(bytes.len(), 28 + 3 * 16 * 4);
        assert_eq!(&bytes[0..4], b"GSS1");
    }
}
