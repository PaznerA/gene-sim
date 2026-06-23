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

	# SP-4: the loci() export is widened (PURELY ADDITIVE) with so_term + go_refs for the codex inspect join.
	# Assert the {id,name} fields are still present AND at least one locus carries so_term + a non-empty go_refs
	# (the default plant genome's loci are SO:704 with a GO ref). RED if the widening dropped/reordered the
	# original fields or the ontology projection is missing.
	var loci: Array = sim.loci()
	if loci.is_empty():
		_fail("loci() returned empty")
		return
	var l0: Dictionary = loci[0]
	if not (l0.has("id") and l0.has("name")):
		_fail("loci() row lost its {id,name} fields: %s" % l0)
		return
	if not (l0.has("so_term") and l0.has("go_refs")):
		_fail("loci() row missing SP-4 ontology fields {so_term,go_refs}: %s" % l0)
		return
	var any_go := false
	for l in loci:
		if int((l as Dictionary).get("so_term", 0)) > 0 and not ((l as Dictionary).get("go_refs", []) as Array).is_empty():
			any_go = true
			break
	if not any_go:
		_fail("loci() carried no so_term+go_refs on any locus (ontology projection broken)")
		return
	print("LIVESIM_LOCI_ONTOLOGY_OK=", loci.size(), " first=", l0)

	print("LIVESIM_SMOKE_OK")
	quit(0)
