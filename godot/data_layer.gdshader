shader_type canvas_item;
// Data-layer overlay shader (S4.4, palette polished). Samples a per-cell data texture the Rust core produced
// — R=density, G=allele_freq, B=fitness (snapshot.gd::to_data_image, RGBF) — and maps the selected channel
// through an "inferno"-style heat ramp (indigo→purple→red→orange→pale-yellow) that contrasts with the green
// field. INVARIANT #2: this only VISUALISES values the core already computed; it does no biology.
// The sprite uses NEAREST filtering so each texel renders as one crisp cell block.

// layer: 0 density, 1 allele_freq, 2 fitness (sampled from TEXTURE);
//        3 soil_moisture, 4 soil_nutrients, 5 soil_ph (sampled from soil_tex);
//        6 light, 7 free_nutrient, 8 detritus (sampled from pool_tex — the GSS3 joule-economy field);
//        9 toxin, 10 kin, 11 alarm (sampled from chem_tex — the GSS4 chem field, ADR-013 F5).
// Four banks of 3 channels each: bank 0 population (TEXTURE), 1 soil (soil_tex), 2 pool (pool_tex), 3 chem (chem_tex).
uniform int layer = 0;
uniform sampler2D soil_tex : filter_nearest;  // R=moisture G=nutrients B=pH (the R1.0 soil field)
uniform sampler2D pool_tex : filter_nearest;  // R=light G=free_nutrient B=detritus (the GSS3 pool field)
uniform sampler2D chem_tex : filter_nearest;  // R=toxin G=kin B=alarm (the GSS4 chem field, ADR-013 F5)
uniform float opacity = 0.78;   // overall overlay strength
uniform float alpha_gamma = 1.6;  // >1 fades low population values further toward transparent

vec3 inferno(float t) {
	vec3 c0 = vec3(0.05, 0.03, 0.15);  // deep indigo
	vec3 c1 = vec3(0.35, 0.07, 0.43);  // purple
	vec3 c2 = vec3(0.75, 0.18, 0.33);  // red-magenta
	vec3 c3 = vec3(0.96, 0.49, 0.14);  // orange
	vec3 c4 = vec3(0.99, 0.92, 0.55);  // pale yellow
	if (t < 0.25) {
		return mix(c0, c1, t / 0.25);
	} else if (t < 0.5) {
		return mix(c1, c2, (t - 0.25) / 0.25);
	} else if (t < 0.75) {
		return mix(c2, c3, (t - 0.5) / 0.25);
	}
	return mix(c3, c4, (t - 0.75) / 0.25);
}

void fragment() {
	int bank = layer / 3;  // 0 pop (density/allele/fitness), 1 soil, 2 pool, 3 chem
	int ch = layer - bank * 3;
	vec3 d = bank == 3 ? texture(chem_tex, UV).rgb
		: (bank == 2 ? texture(pool_tex, UV).rgb
		: (bank == 1 ? texture(soil_tex, UV).rgb : texture(TEXTURE, UV).rgb));
	float v = clamp(ch == 1 ? d.g : (ch == 2 ? d.b : d.r), 0.0, 1.0);

	// Soil + pool are FULL fields (every cell has a value) → uniform alpha, value shown via colour.
	// Population is SPARSE → fade low/empty cells toward transparent (gamma-shaped).
	bool full_field = bank >= 1;
	float a = full_field
		? 0.7 * opacity
		: (v <= 0.0 ? 0.0 : (0.16 + 0.64 * pow(v, alpha_gamma)) * opacity);
	COLOR = vec4(inferno(v), a);
}
