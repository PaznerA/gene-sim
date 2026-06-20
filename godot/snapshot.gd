extends RefCounted
## Reads a gene-sim binary snapshot (.bin, format "GSS1") written by the Rust core
## (sim-core `GridSnapshot::write_snapshot_bytes`). Little-endian layout:
##   "GSS1" | u32 width | u32 height | u32 channel_count(=3) | u64 generation | u32 population
##   | f32[w*h] density | f32[w*h] allele_freq | f32[w*h] fitness   (each channel row-major)
##
## INVARIANT #2 (STOP THE LINE if violated): this ONLY parses snapshot bytes and exposes them for rendering.
## It computes NO biology / genotype→phenotype — all of that lives in the Rust core. The renderer is read-only.
##
## NOTE: this script deliberately has NO `class_name` global. That registry is only populated by an editor
## import pass, so a bare `Snapshot` identifier is unresolved under a fresh `--headless` run (CI / the gate).
## Consumers `preload("res://snapshot.gd")`; the static factory self-references via `SnapshotData` below.
## Both are resolved at parse time and need no `.godot/` cache.

const MAGIC := "GSS1"
const SnapshotData := preload("res://snapshot.gd")

var width: int = 0
var height: int = 0
var channel_count: int = 0
var generation: int = 0
var population: int = 0
var density: PackedFloat32Array
var allele_freq: PackedFloat32Array
var fitness: PackedFloat32Array


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
	return s


func cell_count() -> int:
	return width * height


## Build a CPU-side Image whose pixels encode the data channels (R=density, G=allele_freq, B=fitness).
## A 2D shader samples this as the data-layer texture (SPEC §W10). Pure CPU — safe under headless.
func to_data_image() -> Image:
	var img := Image.create(width, height, false, Image.FORMAT_RGBF)
	for y in height:
		for x in width:
			var i := y * width + x
			img.set_pixel(x, y, Color(density[i], allele_freq[i], fitness[i]))
	return img
