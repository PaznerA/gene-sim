shader_type canvas_item;
// Data-layer overlay shader (S4.4, palette polished). Samples a per-cell data texture the Rust core produced
// — R=density, G=allele_freq, B=fitness (snapshot.gd::to_data_image, RGBF) — and maps the selected channel
// through an "inferno"-style heat ramp (indigo→purple→red→orange→pale-yellow) that contrasts with the green
// field. INVARIANT #2: this only VISUALISES values the core already computed; it does no biology.
// The sprite uses NEAREST filtering so each texel renders as one crisp cell block.

uniform int layer = 0;          // 0 = density, 1 = allele_freq, 2 = fitness
uniform float opacity = 0.78;   // overall overlay strength
uniform float alpha_gamma = 1.6;  // >1 fades low values further toward transparent (smoother heat read)

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
	vec3 d = texture(TEXTURE, UV).rgb;
	float v = d.r;
	if (layer == 1) {
		v = d.g;
	} else if (layer == 2) {
		v = d.b;
	}
	v = clamp(v, 0.0, 1.0);
	// Colour uses the raw value (inferno mapping is byte-identical); only ALPHA is shaped by the gamma curve.
	float shaped = pow(v, alpha_gamma);
	float a = v <= 0.0 ? 0.0 : (0.16 + 0.64 * shaped) * opacity;
	COLOR = vec4(inferno(v), a);
}
