extends Node2D
## gene-sim thin UI entry point.
##
## INVARIANT #2 (STOP THE LINE if violated): this renderer READS sim snapshots only. It must NEVER compute
## genotype→phenotype or any biology — all of that lives in the Rust core (crates/genome, crates/sim-core).
## GDScript here only loads/plays snapshot data and draws it. Snapshot reading + data layers land in S4.2+.

## Load the snapshot reader by path rather than via its `class_name` global: the global script-class registry
## is only populated by an editor import pass, so a fresh `--headless` run (CI / the gate) wouldn't resolve a
## bare `Snapshot` identifier. `preload` is resolved at parse time and needs no `.godot/` cache.
const SnapshotReader := preload("res://snapshot.gd")

func _ready() -> void:
	var v := Engine.get_version_info()
	print("gene-sim UI booted — Godot %s (%s)" % [v.string, DisplayServer.get_name()])

	# Headless snapshot-reader check (S4.2): `godot --headless --path godot -- --snap <file.bin>`.
	var snap_path := _arg_value("--snap")
	if snap_path != "":
		var snap := SnapshotReader.load_from(snap_path)
		if snap == null:
			printerr("snapshot load FAILED: %s" % snap_path)
			get_tree().quit(1)
			return
		print("snapshot OK — %dx%d, gen=%d, population=%d, cells=%d, channels=%d" % [
			snap.width, snap.height, snap.generation, snap.population, snap.cell_count(), snap.channel_count])
		get_tree().quit()
		return

	# Headless smoke (S4.1): boot cleanly and exit. With a display, the scene stays up for rendering (S4.3+).
	if DisplayServer.get_name() == "headless":
		print("headless smoke OK")
		get_tree().quit()


## Read a `--flag value` pair from the user command line (args after `--`). Returns "" if absent.
func _arg_value(flag: String) -> String:
	var args := OS.get_cmdline_user_args()
	var idx := args.find(flag)
	if idx != -1 and idx + 1 < args.size():
		return args[idx + 1]
	return ""
