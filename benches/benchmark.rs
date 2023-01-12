use criterion::{criterion_group, criterion_main, Criterion};
#[cfg(feature = "benchmark_flamegraph")]
use pprof::criterion::PProfProfiler;

mod benchmark_encrypt;

fn run_benchmarks(c: &mut Criterion) {
    benchmark_encrypt::encrypt_message_with_bytes(c, 16).unwrap();
    benchmark_encrypt::encrypt_message_with_bytes(c, 32).unwrap();
    benchmark_encrypt::encrypt_message_with_bytes(c, 64).unwrap();
}

#[cfg(feature = "benchmark_flamegraph")]
criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, pprof::criterion::Output::Flamegraph(None)));
    targets = run_benchmarks
}

#[cfg(all(not(feature = "benchmark_flamegraph")))]
criterion_group!(benches, run_benchmarks);

criterion_main!(benches);
