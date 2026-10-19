//! The instruction and heap work of the gate's own decisions over a committed corpus.

use dotguard::{attribution, lang, renovate};
use gungraun::prelude::*;
use std::hint::black_box;

fn corpus(name: &str) -> String {
    std::fs::read_to_string(format!("{}/corpus/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[library_benchmark(
    config = bench_limits::library("scan"),
    setup = corpus,
)]
#[bench::documents("scan.md")]
fn scan(text: String) -> usize {
    black_box(lang::scan(black_box(&text), lang::Mode::Content)).len()
}

#[library_benchmark(
    config = bench_limits::library("strip"),
    setup = corpus,
)]
#[bench::messages("commit-message.txt")]
fn strip(text: String) -> usize {
    black_box(attribution::strip(black_box(&text))).1
}

#[library_benchmark(
    config = bench_limits::library("classify"),
    setup = corpus,
)]
#[bench::paths("renovate-paths.txt")]
fn classify(paths: String) -> usize {
    black_box(&paths)
        .lines()
        .filter(|path| renovate::is_manifest(black_box(path)))
        .count()
}

library_benchmark_group!(name = core, benchmarks = [scan, strip, classify]);

main!(library_benchmark_groups = core);
