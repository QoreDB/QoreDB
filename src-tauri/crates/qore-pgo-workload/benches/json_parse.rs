// SPDX-License-Identifier: Apache-2.0
//
// JSON parsing benchmark — feeds the §2.2 decision in PERFORMANCE_PLAN.md.
//
// We compare `serde_json::from_slice` (current path: SQLx decodes Postgres
// JSONB through this; MongoDB filter/extjson parsing also goes through it)
// against `simd_json::serde::from_slice` (the serde-compatible API that
// returns the exact same `serde_json::Value` type sqlx expects). Sizes mirror
// realistic production payloads: 10 KB row, 100 KB document, 1 MB blob.
//
// Decision rule: ship simd-json only on the JSONB / Mongo hot paths if and
// only if the **median** wall-clock improvement is ≥ 30 % across the three
// sizes. Below that, the unsafe surface and supply-chain risk of simd-json
// outweigh the gain — we keep serde_json.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use serde_json::{json, Value};

const SIZES: &[(usize, &str)] = &[(10 * 1024, "10kB"), (100 * 1024, "100kB"), (1024 * 1024, "1MB")];

/// Build a JSONB-shaped payload of approximately `target_bytes` bytes.
/// Shape mirrors a typical analytics row: top-level object, nested array
/// of records each carrying scalars + a small nested object + a tag list.
fn build_payload(target_bytes: usize) -> Vec<u8> {
    // Each record is ~120 bytes serialized; size to fit the target.
    let record = json!({
        "id": 4_294_967_296_u64,
        "user": "alice.smith",
        "score": 1234.5678_f64,
        "active": true,
        "meta": {
            "country": "FR",
            "tier": "premium",
            "created_at": "2025-11-21T10:42:13Z"
        },
        "tags": ["alpha", "beta", "gamma", "delta"]
    });
    let record_bytes = serde_json::to_vec(&record).unwrap();
    let approx_record_size = record_bytes.len() + 1; // +1 for ',' separator

    let n_records = target_bytes.saturating_sub(32) / approx_record_size;
    let records: Vec<Value> = (0..n_records).map(|_| record.clone()).collect();
    let payload = json!({
        "version": 3,
        "rows": records,
    });
    serde_json::to_vec(&payload).unwrap()
}

fn bench_serde_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("serde_json::from_slice");
    for (target, label) in SIZES {
        let bytes = build_payload(*target);
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), &bytes, |b, bytes| {
            b.iter(|| {
                let v: Value = serde_json::from_slice(black_box(bytes)).unwrap();
                black_box(v)
            });
        });
    }
    group.finish();
}

fn bench_simd_json_serde(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_json::serde::from_slice");
    for (target, label) in SIZES {
        let bytes = build_payload(*target);
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), &bytes, |b, bytes| {
            b.iter_batched(
                || bytes.clone(),
                |mut buf| {
                    let v: Value = simd_json::serde::from_slice(black_box(&mut buf)).unwrap();
                    black_box(v)
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_simd_json_owned(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_json::to_owned_value");
    for (target, label) in SIZES {
        let bytes = build_payload(*target);
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), &bytes, |b, bytes| {
            b.iter_batched(
                || bytes.clone(),
                |mut buf| {
                    let v = simd_json::to_owned_value(black_box(&mut buf)).unwrap();
                    black_box(v)
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_serde_json,
    bench_simd_json_serde,
    bench_simd_json_owned
);
criterion_main!(benches);
