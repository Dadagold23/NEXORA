use nxl_compiler::debugger;

const SAMPLE: &str = "let x = 1;\nlet y = 2;\nprint(x + y);\n";

#[test]
fn trace_contains_executed_lines() {
    let dbg = debugger::Debugger::new();
    let report = dbg.run(SAMPLE);
    assert!(report.trace.contains(&1), "{:?}", report.trace);
    assert!(report.trace.contains(&3), "{:?}", report.trace);
    assert_eq!(report.output, vec!["3"]);
}

#[test]
fn breakpoints_track_hits_in_order() {
    let mut dbg = debugger::Debugger::new();
    dbg.add_breakpoint(1);
    dbg.add_breakpoint(3);
    let report = dbg.run(SAMPLE);
    assert!(!report.breakpoint_hits.is_empty());
    assert!(report.breakpoint_hits.contains(&3));
}

#[test]
fn loop_hits_breakpoint_each_iteration() {
    let src = "let mut i = 0;\nloop {\n    if i >= 3 { break; }\n    print(i);\n    i = i + 1;\n}\nprint(\"done\");\n";
    let mut dbg = debugger::Debugger::new();
    dbg.add_breakpoint(4);
    let report = dbg.run(src);
    let hits_at_4 = report.breakpoint_hits.iter().filter(|&&l| l == 4).count();
    assert!(hits_at_4 >= 3, "{hits_at_4}");
    assert!(
        report.output.iter().any(|l| l == "done"),
        "{:?}",
        report.output
    );
}

#[test]
fn compile_error_becomes_report() {
    let dbg = debugger::Debugger::new();
    let report = dbg.run("let x: Int = 1 + ;\n");
    assert!(
        report.return_value.starts_with("error:"),
        "{}",
        report.return_value
    );
}

#[test]
fn line_of_maps_offsets_to_lines() {
    let src = "ab\ncd\nef\n";
    assert_eq!(debugger::line_of(src, 0), 1);
    assert_eq!(debugger::line_of(src, 3), 2);
    assert_eq!(debugger::line_of(src, 6), 3);
}

#[test]
fn format_report_includes_trace_and_hits() {
    let mut dbg = debugger::Debugger::new();
    dbg.add_breakpoint(3);
    let report = dbg.run(SAMPLE);
    let text = debugger::format_report(&report);
    assert!(text.contains("statement trace lines"), "{text}");
    assert!(text.contains("lines executed"), "{text}");
}
