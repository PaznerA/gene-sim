//! Emit the abstract default species as a `SpeciesSpec` JSON (the `data/species/default.json` starter).
//! Run: `cargo run -q -p genome --example emit_species > data/species/default.json`.
fn main() {
    let g = genome::sample_genome();
    let mut spec = genome::spec::SpeciesSpec::from_genome(&g, "default", "Abstract Default");
    spec.niche.entity_count = 1000;
    spec.niche.description =
        "The abstract fast-sim species — 9 decoupled morphology traits (plant-ish).".to_string();
    println!(
        "{}",
        serde_json::to_string_pretty(&spec).expect("serialize SpeciesSpec")
    );
}
