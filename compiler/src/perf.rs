//! P24 — Performance Engineering
//!
//! Baseline measurements without external crates: each benchmark times
//! representative workloads over several repetitions and reports the median
//! nanoseconds-per-iteration. These are smoke-level baselines to detect
//! order-of-magnitude regressions, not statistical CI benchmarks.

use std::time::Instant;

pub struct BenchRow {
    pub name: &'static str,
    pub iterations: u64,
    pub total_ns: u128,
}

impl BenchRow {
    pub fn per_iter_us(&self) -> f64 {
        (self.total_ns as f64 / self.iterations as f64) / 1_000.0
    }
}

fn bench_fn<F: FnMut()>(mut f: F, samples: usize) -> u128 {
    let mut totals = Vec::new();
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..50 {
            f();
        }
        totals.push(start.elapsed().as_nanos() / 50);
    }
    totals.sort_unstable();
    totals[totals.len() / 2]
}

const LEX_SAMPLE: &str = r#"fn main() { let x = 1 + 2 * 3; print(x); }"#;
const RUN_SAMPLE: &str =
    "fn main() { let mut i = 0; loop { if i >= 100 { break; } i = i + 1; } print(i); }";
const DB_MIGRATION_SQL: &str =
    "-- up\nCREATE TABLE bench (id INTEGER PRIMARY KEY, v TEXT);\n-- down\nDROP TABLE bench;\n";

/// Lex median ns/iter.
pub fn bench_lex() -> BenchRow {
    let ns = bench_fn(
        || {
            let sf = crate::source::SourceFile::new("bench.nxl", LEX_SAMPLE);
            let _ = crate::lexer::lex(&sf);
        },
        7,
    );
    BenchRow {
        name: "lexer",
        iterations: 50,
        total_ns: ns * 50,
    }
}

/// Parse median ns/iter.
pub fn bench_parse() -> BenchRow {
    let ns = bench_fn(
        || {
            let sf = crate::source::SourceFile::new("bench.nxl", LEX_SAMPLE);
            let _ = crate::parse_source(&sf);
        },
        7,
    );
    BenchRow {
        name: "parser",
        iterations: 50,
        total_ns: ns * 50,
    }
}

/// Full frontend to NIR median ns/iter.
pub fn bench_compile_nir() -> BenchRow {
    let ns = bench_fn(
        || {
            let sf = crate::source::SourceFile::new("bench.nxl", LEX_SAMPLE);
            let _ = crate::compile_to_nir(&sf);
        },
        7,
    );
    BenchRow {
        name: "compile_to_nir",
        iterations: 50,
        total_ns: ns * 50,
    }
}

/// VM execution of a 100-iteration loop median ns/iter.
pub fn bench_vm_run() -> BenchRow {
    let ns = bench_fn(
        || {
            let sf = crate::source::SourceFile::new("bench_run.nxl", RUN_SAMPLE);
            let _ = crate::vm::run(&sf);
        },
        7,
    );
    BenchRow {
        name: "vm_run",
        iterations: 50,
        total_ns: ns * 50,
    }
}

/// NDB: fresh-project migrate + rollback timing.
pub fn bench_migrate() -> BenchRow {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("nxl_p24_{}_{}", std::process::id(), unique));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("database").join("migrations")).unwrap();
    std::fs::write(
        dir.join("nxl.toml"),
        "[package]\nname = \"b\"\n\n[database]\ndriver = \"sqlite\"\npath = \"database/database.sqlite\"\n",
    )
    .unwrap();
    let ns = bench_fn(
        || {
            std::fs::write(
                dir.join("database").join("migrations").join("0001_t.sql"),
                DB_MIGRATION_SQL,
            )
            .unwrap();
            let _ = crate::ndb::fresh(&dir, false);
        },
        5,
    );
    let _ = std::fs::remove_dir_all(&dir);
    BenchRow {
        name: "migrate_fresh",
        iterations: 50,
        total_ns: ns * 50,
    }
}

/// Run all benchmark rows and return the report table.
pub fn bench_report() -> String {
    let rows = vec![
        bench_lex(),
        bench_parse(),
        bench_compile_nir(),
        bench_vm_run(),
        bench_migrate(),
    ];
    let mut out = String::from("nxl bench (median of samples, per-iter mean shown):\n");
    for r in &rows {
        out.push_str(&format!(
            "  {:<16} {:>10.1} µs/iter\n",
            r.name,
            r.per_iter_us()
        ));
    }
    out.push_str("\nNote: indicative smoke baselines, not CI-stable percentiles.\n");
    out
}
