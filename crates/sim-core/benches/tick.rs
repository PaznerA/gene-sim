//! Baseline entity-count × tick-rate bench (SPEC §8 Stage 0 exit gate, §11 perf threshold).
//! Run with `cargo bench -p sim-core`. The recorded baseline is in docs/llm/DECISIONS.md.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use sim_core::{run_headless, SimConfig};

fn bench_tick_loop(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick_loop");
    group.sample_size(20);

    // Fixed generation count; vary entity count → the entity-count × tick-rate baseline.
    let generations = 50;
    for &n in &[1_000u32, 5_000, 10_000] {
        group.throughput(criterion::Throughput::Elements(u64::from(n) * generations));
        group.bench_function(format!("entities_{n}_gens_{generations}"), |b| {
            b.iter(|| {
                run_headless(black_box(&SimConfig {
                    seed: 42,
                    generations,
                    entity_count: n,
                }))
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_tick_loop);
criterion_main!(benches);
