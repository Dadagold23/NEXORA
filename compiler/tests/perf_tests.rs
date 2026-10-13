use nxl_compiler::perf;

#[test]
fn bench_rows_produce_finite_positive_times() {
    let lex = perf::bench_lex();
    let parse = perf::bench_parse();
    let nir = perf::bench_compile_nir();
    let vm = perf::bench_vm_run();
    let db = perf::bench_migrate();
    for row in [&lex, &parse, &nir, &vm, &db] {
        assert!(row.per_iter_us().is_finite(), "{} not finite", row.name);
        assert!(row.per_iter_us() > 0.0, "{} zero/negative", row.name);
    }
}

#[test]
fn bench_report_lists_all_rows() {
    let report = perf::bench_report();
    assert!(report.contains("lexer"), "{report}");
    assert!(report.contains("parser"));
    assert!(report.contains("compile_to_nir"));
    assert!(report.contains("vm_run"));
    assert!(report.contains("migrate_fresh"));
    assert!(report.contains("median"));
}
