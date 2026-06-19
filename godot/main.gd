extends Node2D
## gene-sim thin UI entry point.
##
## INVARIANT #2 (STOP THE LINE if violated): this renderer READS sim snapshots only. It must NEVER compute
## genotype→phenotype or any biology — all of that lives in the Rust core (crates/genome, crates/sim-core).
## GDScript here only loads/plays snapshot data and draws it. Snapshot reading + data layers land in S4.2+.

func _ready() -> void:
	var v := Engine.get_version_info()
	print("gene-sim UI booted — Godot %s (%s)" % [v.string, DisplayServer.get_name()])
	# Headless smoke (S4.1): boot cleanly and exit. With a display, the scene stays up for rendering (S4.3+).
	if DisplayServer.get_name() == "headless":
		print("headless smoke OK")
		get_tree().quit()
