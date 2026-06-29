//! In-process animated-GIF assembler for the SCENARIO PREVIEW (the ASSEMBLE half of the GIF preview, paired with
//! the off-hash key-event schedule in [`crate::keyframe`] and the renderer `--shot` capture in
//! `tools/make_starter_gif.sh`).
//!
//! ## GPL stays at the process boundary (inv #1)
//! The encoder is the MIT/Apache `gif` crate (`color_quant` NeuQuant default feature) reading PNGs via the
//! MIT/Apache `png` crate — both pure-Rust, LINKED, never a GPL `imagemagick`/`ffmpeg` subprocess. The slice's
//! documented fallback (an external encoder) would have to be a subprocess at the boundary; the pure-Rust path is
//! light enough that we never take it. Pinned (inv #7): `gif = 0.13`, `png = 0.17` (ADR-032).
//!
//! ## What it does
//! [`encode_gif`] reads the captured per-keyframe PNG frames (one per KEY generation, in gen order), optionally
//! NEAREST-NEIGHBOUR downscales them so the longest side is `<= max_dim` (a small, readable thumbnail), quantizes
//! each to a 256-colour palette, and writes a single LOOPING animated GIF. With the default per-frame delay the
//! clip reads as the ~2-4s loop the slice targets ([`MAX_FRAMES`](crate::keyframe::MAX_FRAMES) `* `
//! [`DEFAULT_DELAY_CS`] `≈` 3.6s). [`collect_frames`] gathers `frame_*.png` from the capture dir in name order
//! (the capture writes zero-padded gen-ordered names, so name order == gen order).
//!
//! ## Read-only (inv #2/#3)
//! Pure post-processing of inert captured PNG bytes — no sim, no RNG, never folded into `hash_world`. It cannot
//! move the pinned literal `0x47a0_3c8f_6701_f240`.

use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Default per-frame delay in CENTISECONDS (1/100 s; the GIF delay unit). At [`crate::keyframe::MAX_FRAMES`]
/// (12) frames this is a ~3.6s loop — inside the readable ~2-4s window the slice targets.
pub const DEFAULT_DELAY_CS: u16 = 30;

/// Default cap on the longest side of a preview frame (px). The renderer viewport is larger; downscaling keeps the
/// committed-next-to-the-starter `.gif` a small, gallery-friendly thumbnail. `0` disables downscaling.
pub const DEFAULT_MAX_DIM: u32 = 480;

/// A summary of an assembled GIF — the frame count + the final (post-downscale) logical screen size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GifReport {
    /// Number of frames written (== the number of input PNGs).
    pub frames: usize,
    /// Final logical screen width (px).
    pub width: u16,
    /// Final logical screen height (px).
    pub height: u16,
}

/// One decoded RGBA8 frame (`rgba.len() == width*height*4`).
struct Rgba8 {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Map a `gif`/`png` codec error into an [`io::Error`] so the public surface is a single error type.
fn to_io<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}

/// Collect the captured `frame_*.png` files in `dir`, sorted by FILE NAME. The capture
/// (`tools/make_starter_gif.sh`) writes zero-padded gen-ordered names (`frame_00001.png`, `frame_00030.png`, …),
/// so lexical name order == generation order — the order the frames must appear in the clip.
///
/// # Errors
/// An [`io::Error`] if `dir` cannot be read.
pub fn collect_frames(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut frames: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png"))
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("frame_"))
        })
        .collect();
    frames.sort();
    Ok(frames)
}

/// Decode an 8-bit PNG (any of RGBA / RGB / grayscale / grayscale+alpha) into an RGBA8 buffer. The renderer's
/// `--shot` writes 8-bit PNGs (`Image::save_png`); a 16-bit or paletted source is rejected with a clear error
/// (never a silently-wrong frame).
fn decode_png_rgba(path: &Path) -> io::Result<Rgba8> {
    let decoder = png::Decoder::new(BufReader::new(File::open(path)?));
    let mut reader = decoder.read_info().map_err(to_io)?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(to_io)?;
    let (w, h) = (info.width, info.height);
    if info.bit_depth != png::BitDepth::Eight {
        return Err(to_io(format!(
            "{}: unsupported PNG bit depth {:?} (expected 8-bit from the renderer --shot)",
            path.display(),
            info.bit_depth
        )));
    }
    let src = &buf[..info.buffer_size()];
    let px = (w as usize) * (h as usize);
    let mut rgba = vec![0u8; px * 4];
    match info.color_type {
        png::ColorType::Rgba => rgba.copy_from_slice(&src[..px * 4]),
        png::ColorType::Rgb => {
            for i in 0..px {
                rgba[i * 4] = src[i * 3];
                rgba[i * 4 + 1] = src[i * 3 + 1];
                rgba[i * 4 + 2] = src[i * 3 + 2];
                rgba[i * 4 + 3] = 255;
            }
        }
        png::ColorType::Grayscale => {
            for i in 0..px {
                let g = src[i];
                rgba[i * 4] = g;
                rgba[i * 4 + 1] = g;
                rgba[i * 4 + 2] = g;
                rgba[i * 4 + 3] = 255;
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for i in 0..px {
                let g = src[i * 2];
                rgba[i * 4] = g;
                rgba[i * 4 + 1] = g;
                rgba[i * 4 + 2] = g;
                rgba[i * 4 + 3] = src[i * 2 + 1];
            }
        }
        png::ColorType::Indexed => {
            return Err(to_io(format!(
                "{}: paletted PNG is not supported (re-export as RGBA)",
                path.display()
            )))
        }
    }
    Ok(Rgba8 {
        width: w,
        height: h,
        rgba,
    })
}

/// NEAREST-NEIGHBOUR downscale `frame` so its longest side is `<= max_dim` (a small, readable preview). A no-op
/// when `max_dim == 0` or the frame already fits, or when a degenerate dimension would round to 0. Pure integer
/// arithmetic — deterministic, no float rounding skew.
fn downscale(frame: Rgba8, max_dim: u32) -> Rgba8 {
    let long = frame.width.max(frame.height);
    if max_dim == 0 || long <= max_dim || frame.width == 0 || frame.height == 0 {
        return frame;
    }
    // Integer-scaled target dims (ceil-free: round to nearest by adding half the divisor), each at least 1.
    let nw = ((frame.width * max_dim + long / 2) / long).max(1);
    let nh = ((frame.height * max_dim + long / 2) / long).max(1);
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    for y in 0..nh {
        // Map the destination row to the nearest source row.
        let sy = (y * frame.height / nh).min(frame.height - 1);
        for x in 0..nw {
            let sx = (x * frame.width / nw).min(frame.width - 1);
            let si = ((sy * frame.width + sx) as usize) * 4;
            let di = ((y * nw + x) as usize) * 4;
            out[di..di + 4].copy_from_slice(&frame.rgba[si..si + 4]);
        }
    }
    Rgba8 {
        width: nw,
        height: nh,
        rgba: out,
    }
}

/// Encode the PNG frames at `frame_paths` (already in clip order — see [`collect_frames`]) into a single LOOPING
/// animated GIF at `out`, `delay_cs` centiseconds per frame, each frame downscaled so its longest side is
/// `<= max_dim` (`0` = no downscale). Every frame is NeuQuant-quantized to a 256-colour palette by the `gif`
/// crate. The logical screen size is the FIRST frame's (post-downscale) dimensions; every later frame must share
/// the SAME pre-downscale dimensions (the renderer viewport is constant) — a mismatch is an error, never a
/// silently-misframed clip. Returns the [`GifReport`] (frame count + final screen size).
///
/// # Errors
/// An [`io::Error`] if `frame_paths` is empty, a frame fails to decode, the frames disagree in size, or any file
/// write fails.
pub fn encode_gif(
    frame_paths: &[PathBuf],
    out: &Path,
    delay_cs: u16,
    max_dim: u32,
) -> io::Result<GifReport> {
    if frame_paths.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no PNG frames to assemble into a GIF",
        ));
    }
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // Decode + downscale every frame up front so we can (a) validate uniform source dims and (b) hand the encoder
    // a known logical screen size. Preview clips are a handful of small frames — the memory is trivial.
    let mut decoded: Vec<Rgba8> = Vec::with_capacity(frame_paths.len());
    let mut src_dims: Option<(u32, u32)> = None;
    for path in frame_paths {
        let frame = decode_png_rgba(path)?;
        match src_dims {
            None => src_dims = Some((frame.width, frame.height)),
            Some((w, h)) if (w, h) != (frame.width, frame.height) => {
                return Err(to_io(format!(
                    "{}: frame size {}x{} != first frame {}x{} (the capture viewport must be constant)",
                    path.display(),
                    frame.width,
                    frame.height,
                    w,
                    h
                )));
            }
            Some(_) => {}
        }
        decoded.push(downscale(frame, max_dim));
    }

    // The logical screen is the (post-downscale) first-frame size; downscale is dimension-preserving across frames
    // (same source dims → same target dims), so all frames match it. Clamp into the GIF u16 screen field.
    let (sw, sh) = (decoded[0].width, decoded[0].height);
    let screen_w =
        u16::try_from(sw).map_err(|_| to_io(format!("width {sw} exceeds the GIF 65535 limit")))?;
    let screen_h =
        u16::try_from(sh).map_err(|_| to_io(format!("height {sh} exceeds the GIF 65535 limit")))?;

    let file = File::create(out)?;
    let mut writer = BufWriter::new(file);
    {
        let mut encoder = gif::Encoder::new(&mut writer, screen_w, screen_h, &[]).map_err(to_io)?;
        encoder.set_repeat(gif::Repeat::Infinite).map_err(to_io)?;
        for frame in &mut decoded {
            // NeuQuant quantization (color_quant default feature); speed 10 trades a little quality for a fast,
            // deterministic encode. from_rgba_speed takes &mut (it may dither in place) — we own the buffer.
            let mut gframe = gif::Frame::from_rgba_speed(screen_w, screen_h, &mut frame.rgba, 10);
            gframe.delay = delay_cs;
            encoder.write_frame(&gframe).map_err(to_io)?;
        }
        // `encoder` writes the GIF trailer on drop (end of this scope), before the BufWriter is flushed below.
    }
    use std::io::Write as _;
    writer.flush()?;

    Ok(GifReport {
        frames: decoded.len(),
        width: screen_w,
        height: screen_h,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A RAII temp dir guard (std-only — the harness has no `tempfile` dep). Removes the dir on drop.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(label: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!("gene-sim-gifenc-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("create temp dir");
            TempDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Write a solid-colour `w x h` RGBA8 PNG (a synthetic captured frame) at `path`.
    fn write_solid_png(path: &Path, w: u32, h: u32, rgba: [u8; 4]) {
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        for px in buf.chunks_exact_mut(4) {
            px.copy_from_slice(&rgba);
        }
        let file = File::create(path).expect("create png");
        let mut enc = png::Encoder::new(BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("png header");
        writer.write_image_data(&buf).expect("png data");
    }

    /// Count the frames in a GIF by decoding it with the `gif` crate (the inverse of [`encode_gif`]).
    fn count_gif_frames(path: &Path) -> usize {
        let file = File::open(path).expect("open gif");
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = opts.read_info(BufReader::new(file)).expect("gif read_info");
        let mut n = 0;
        while decoder.read_next_frame().expect("read frame").is_some() {
            n += 1;
        }
        n
    }

    #[test]
    fn collect_frames_sorts_by_name_and_filters() {
        // The capture writes zero-padded names; collect returns them in lexical (== gen) order, ignoring non-frames.
        let tmp = TempDir::new("collect");
        for g in [30u32, 1, 90] {
            write_solid_png(
                &tmp.path().join(format!("frame_{g:05}.png")),
                4,
                4,
                [10, 20, 30, 255],
            );
        }
        // Decoys that must be IGNORED: a non-frame PNG + a non-PNG file.
        write_solid_png(&tmp.path().join("thumb.png"), 4, 4, [0, 0, 0, 255]);
        std::fs::write(tmp.path().join("frame_notes.txt"), b"x").unwrap();

        let frames = collect_frames(tmp.path()).expect("collect");
        let names: Vec<String> = frames
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["frame_00001.png", "frame_00030.png", "frame_00090.png"],
            "frame_*.png only, sorted ascending (name order == gen order)"
        );
    }

    #[test]
    fn encode_writes_a_valid_looping_multiframe_gif() {
        // THE HEADLESS SMOKE (no GPU): synthesize 4 distinct-colour frames, assemble, and assert the output is a
        // valid, non-empty GIF with > 1 frame — exactly the assertion the macOS-safe pipeline smoke makes.
        let tmp = TempDir::new("encode");
        let colours = [
            [200, 40, 40, 255],
            [40, 200, 40, 255],
            [40, 40, 200, 255],
            [200, 200, 40, 255],
        ];
        for (i, c) in colours.iter().enumerate() {
            write_solid_png(&tmp.path().join(format!("frame_{i:05}.png")), 32, 24, *c);
        }
        let frames = collect_frames(tmp.path()).expect("collect");
        assert_eq!(frames.len(), 4);

        let out = tmp.path().join("preview.gif");
        let report = encode_gif(&frames, &out, DEFAULT_DELAY_CS, DEFAULT_MAX_DIM).expect("encode");
        assert_eq!(report.frames, 4, "one GIF frame per input PNG");
        assert_eq!(
            (report.width, report.height),
            (32, 24),
            "small frame is not upscaled"
        );

        // The file exists, is non-empty, and carries the GIF magic header.
        let bytes = std::fs::read(&out).expect("read gif");
        assert!(bytes.len() > 64, "non-empty GIF, got {} bytes", bytes.len());
        assert!(
            bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a"),
            "valid GIF magic header"
        );

        // It round-trips through a GIF decoder with > 1 frame (a genuine animation, not a single still).
        let n = count_gif_frames(&out);
        assert_eq!(n, 4, "the GIF decodes to 4 frames");
        assert!(n > 1, "a preview GIF is a multi-frame loop");
    }

    #[test]
    fn downscale_caps_the_longest_side_and_keeps_aspect() {
        // A large frame is downscaled so its longest side == max_dim; aspect ratio is preserved (within rounding).
        let tmp = TempDir::new("downscale");
        write_solid_png(
            &tmp.path().join("frame_00000.png"),
            1200,
            600,
            [12, 34, 56, 255],
        );
        write_solid_png(
            &tmp.path().join("frame_00001.png"),
            1200,
            600,
            [56, 34, 12, 255],
        );
        let frames = collect_frames(tmp.path()).expect("collect");

        let out = tmp.path().join("big.gif");
        let report = encode_gif(&frames, &out, DEFAULT_DELAY_CS, 480).expect("encode");
        assert_eq!(report.width, 480, "longest side capped at max_dim");
        assert_eq!(
            report.height, 240,
            "aspect ratio preserved (1200x600 → 480x240)"
        );
        assert_eq!(report.frames, 2);
    }

    #[test]
    fn mismatched_frame_sizes_are_rejected() {
        // A varying capture viewport is an ERROR, never a silently-misframed clip.
        let tmp = TempDir::new("mismatch");
        write_solid_png(&tmp.path().join("frame_00000.png"), 32, 24, [1, 2, 3, 255]);
        write_solid_png(&tmp.path().join("frame_00001.png"), 40, 24, [3, 2, 1, 255]);
        let frames = collect_frames(tmp.path()).expect("collect");
        let out = tmp.path().join("bad.gif");
        let err = encode_gif(&frames, &out, DEFAULT_DELAY_CS, DEFAULT_MAX_DIM)
            .expect_err("differing frame sizes must error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn empty_frame_list_is_an_error() {
        let tmp = TempDir::new("empty");
        let out = tmp.path().join("none.gif");
        let err = encode_gif(&[], &out, DEFAULT_DELAY_CS, DEFAULT_MAX_DIM)
            .expect_err("no frames must error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
