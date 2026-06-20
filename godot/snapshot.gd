extends RefCounted
## Reads a gene-sim binary snapshot (.bin, format "GSS2") written by the Rust core
## (sim-core `GridSnapshot::write_snapshot_bytes`). Little-endian layout:
##   "GSS2" | u32 width | u32 height | u32 channel_count(=6) | u64 generation | u32 population
##   | f32[w*h] density | f32[w*h] allele_freq | f32[w*h] fitness
##   | f32[w*h] soil_moisture | f32[w*h] soil_nutrients | f32[w*h] soil_ph   (each channel row-major)
## The soil_* channels (roadmap R1.0) are PARSED here for inspection; the data-layer shader still samples
## only density/allele_freq/fitness — a visible soil overlay is later UI work (Godot is built last).
##
## INVARIANT #2 (STOP THE LINE if violated): this ONLY parses snapshot bytes and exposes them for rendering.
## It computes NO biology / genotype→phenotype — all of that lives in the Rust core. The renderer is read-only.
##
## NOTE: this script deliberately has NO `class_name` global. That registry is only populated by an editor
## import pass, so a bare `Snapshot` identifier is unresolved under a fresh `--headless` run (CI / the gate).
## Consumers `preload("res://snapshot.gd")`; the static factory self-references via `SnapshotData` below.
## Both are resolved at parse time and need no `.godot/` cache.

const MAGIC := "GSS2"
const SnapshotData := preload("res://snapshot.gd")

var width: int = 0
var height: int = 0
var channel_count: int = 0
var generation: int = 0
var population: int = 0
var density: PackedFloat32Array
var allele_freq: PackedFloat32Array
var fitness: PackedFloat32Array
var soil_moisture: PackedFloat32Array
var soil_nutrients: PackedFloat32Array
var soil_ph: PackedFloat32Array


static func load_from(path: String) -> SnapshotData:
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		push_error("snapshot: cannot open %s (err %d)" % [path, FileAccess.get_open_error()])
		return null
	f.set_big_endian(false)  # the format is little-endian
	var magic := f.get_buffer(4).get_string_from_ascii()
	if magic != MAGIC:
		push_error("snapshot: bad magic %s (expected %s)" % [magic, MAGIC])
		return null
	var s := SnapshotData.new()
	s.width = f.get_32()
	s.height = f.get_32()
	s.channel_count = f.get_32()
	s.generation = f.get_64()
	s.population = f.get_32()
	var n := s.width * s.height
	s.density = f.get_buffer(n * 4).to_float32_array()
	s.allele_freq = f.get_buffer(n * 4).to_float32_array()
	s.fitness = f.get_buffer(n * 4).to_float32_array()
	s.soil_moisture = f.get_buffer(n * 4).to_float32_array()
	s.soil_nutrients = f.get_buffer(n * 4).to_float32_array()
	s.soil_ph = f.get_buffer(n * 4).to_float32_array()
	return s


## Parse a GSS2 snapshot from an in-memory byte buffer (e.g. `LiveSim.snapshot()` in --live mode) rather
## than a file. Same layout as load_from. Returns null on bad magic / short buffer. Read-only (inv #2).
static func parse_bytes(buf: PackedByteArray) -> SnapshotData:
	if buf.size() < 28 or buf.slice(0, 4).get_string_from_ascii() != MAGIC:
		push_error("snapshot: bad/short byte buffer")
		return null
	var s := SnapshotData.new()
	s.width = buf.decode_u32(4)
	s.height = buf.decode_u32(8)
	s.channel_count = buf.decode_u32(12)
	s.generation = buf.decode_u64(16)
	s.population = buf.decode_u32(24)
	var n := s.width * s.height
	var off := 28
	var read := func(o: int) -> PackedFloat32Array:
		return buf.slice(o, o + n * 4).to_float32_array()
	s.density = read.call(off); off += n * 4
	s.allele_freq = read.call(off); off += n * 4
	s.fitness = read.call(off); off += n * 4
	s.soil_moisture = read.call(off); off += n * 4
	s.soil_nutrients = read.call(off); off += n * 4
	s.soil_ph = read.call(off)
	return s


func cell_count() -> int:
	return width * height


## Build a CPU-side Image whose pixels encode the population channels (R=density, G=allele_freq, B=fitness).
## A 2D shader samples this as the data-layer texture (SPEC §W10). Pure CPU — safe under headless.
func to_data_image() -> Image:
	var img := Image.create(width, height, false, Image.FORMAT_RGBF)
	for y in height:
		for x in width:
			var i := y * width + x
			img.set_pixel(x, y, Color(density[i], allele_freq[i], fitness[i]))
	return img


## Build a CPU-side Image encoding the soil channels (R=moisture, G=nutrients, B=pH) for the data-layer
## shader's soil layers (R1.0 substrate made visible). Pure CPU — safe under headless.
func to_soil_image() -> Image:
	var img := Image.create(width, height, false, Image.FORMAT_RGBF)
	for y in height:
		for x in width:
			var i := y * width + x
			img.set_pixel(x, y, Color(soil_moisture[i], soil_nutrients[i], soil_ph[i]))
	return img
