shader_type canvas_item;
// Data-layer overlay shader (S4.4). Samples a per-cell data texture the Rust core produced — R=density,
// G=allele_freq, B=fitness (snapshot.gd::to_data_image, RGBF) — and maps the selected channel through a
// heat colormap. INVARIANT #2: this only VISUALISES values the core already computed; it does no biology.
// The sprite uses NEAREST filtering so each texel renders as one crisp cell block.

uniform int layer = 0;          // 0 = density, 1 = allele_freq, 2 = fitness
uniform float opacity = 0.72;   // overall overlay strength

void fragment() {
	vec3 d = texture(TEXTURE, UV).rgb;
	float v = d.r;
	if (layer == 1) {
		v = d.g;
	} else if (layer == 2) {
		v = d.b;
	}
	v = clamp(v, 0.0, 1.0);

	// transparent -> blue -> cyan -> yellow -> red
	vec3 col;
	if (v < 0.5) {
		col = mix(vec3(0.10, 0.30, 0.90), vec3(0.10, 0.90, 0.70), v / 0.5);
	} else {
		col = mix(vec3(0.95, 0.85, 0.10), vec3(0.95, 0.15, 0.10), (v - 0.5) / 0.5);
	}
	float a = v <= 0.0 ? 0.0 : (0.15 + 0.7 * v) * opacity;
	COLOR = vec4(col, a);
}
