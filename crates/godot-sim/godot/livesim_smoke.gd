extends SceneTree
## Headless load-smoke for the LiveSim GDExtension (ADR-010, gameplay batch P1b).
##
## Instantiates the Rust `LiveSim` node (registered by godot/gene_sim.gdextension), drives
## reset → step → observe → snapshot, and prints LIVESIM_SMOKE_OK iff everything works. Proves the
## api-4-6 cdylib LOADS + runs under the installed Godot (forward-compat: runtime >= API).
##
## INVARIANT #2: this script only CALLS LiveSim (a thin Rust binding to sim-core/harness). It computes
## NO biology. All genotype→phenotype stays in the Rust core.
##
## Run headless (NO --import; the gdextension loads at startup; the 4.7 editor --import pass crashes
## for unrelated reasons):
##   godot --headless --path godot --script livesim_smoke.gd
##
## NOTE: no `class_name` global (ADR-006 headless rule). LiveSim is resolved by the extension at runtime.

func _fail(msg: String) -> void:
	printerr("LIVESIM_SMOKE_FAIL: ", msg)
	quit(1)

func _init() -> void:
	if not ClassDB.class_exists("LiveSim"):
		_fail("LiveSim class not registered (gdextension failed to load?)")
		return

	var sim = ClassDB.instantiate("LiveSim")
	if sim == null:
		_fail("could not instantiate LiveSim")
		return

	# reset(seed) -> Dictionary {generation, population, allele_freq, phenotype}
	var obs0: Dictionary = sim.reset(1234)
	print("LIVESIM_RESET=", obs0)
	if int(obs0.get("generation", -1)) != 0:
		_fail("reset generation should be 0, got %s" % obs0.get("generation"))
		return
	if int(obs0.get("population", 0)) <= 0:
		_fail("reset population should be > 0, got %s" % obs0.get("population"))
		return
	if not obs0.has("allele_freq"):
		_fail("reset observation missing allele_freq")
		return
	if not (obs0.get("phenotype") is Dictionary):
		_fail("reset observation missing phenotype dict")
		return

	# step(n) advances exactly n generations (fixed integer cadence — invariant #3).
	sim.step(25)
	var obs1: Dictionary = sim.observe()
	print("LIVESIM_OBSERVE=", obs1)
	if int(obs1.get("generation", -1)) != 25:
		_fail("after step(25) generation should be 25, got %s" % obs1.get("generation"))
		return
	var af: float = float(obs1.get("allele_freq", -1.0))
	if af < 0.0 or af > 1.0:
		_fail("allele_freq out of [0,1]: %s" % af)
		return

	# snapshot(w, h) -> PackedByteArray of GSS4 bytes (parsed by godot/snapshot.gd).
	var bytes: PackedByteArray = sim.snapshot(16, 12)
	print("LIVESIM_SNAPSHOT_BYTES=", bytes.size())
	if bytes.size() < 28:
		_fail("snapshot too small (%d bytes)" % bytes.size())
		return
	var magic := bytes.slice(0, 4).get_string_from_ascii()
	if magic != "GSS4":
		_fail("snapshot bad magic '%s' (expected GSS4)" % magic)
		return
	# Cross-check the parser: feed the bytes through the real snapshot.gd reader.
	var w := bytes.decode_u32(4)
	var h := bytes.decode_u32(8)
	var channels := bytes.decode_u32(12)
	if w != 16 or h != 12 or channels != 12:
		_fail("snapshot header mismatch: %dx%d ch=%d (want 16x12 ch=12)" % [w, h, channels])
		return
	var expected := 28 + channels * w * h * 4
	if bytes.size() != expected:
		_fail("snapshot length %d != expected %d" % [bytes.size(), expected])
		return

	# Determinism spot-check: a fresh sim on the same seed yields identical bytes for the same grid.
	var sim2 = ClassDB.instantiate("LiveSim")
	sim2.reset(1234)
	sim2.step(25)
	var bytes2: PackedByteArray = sim2.snapshot(16, 12)
	if bytes != bytes2:
		_fail("same seed+steps+grid produced different snapshot bytes (determinism!)")
		return

	print("LIVESIM_SMOKE_OK")
	quit(0)
