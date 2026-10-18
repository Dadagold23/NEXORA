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

// ── Stepping sessions (variables + step) ──────────────────────────────────────

/// Render a frame's locals as (name, display value) pairs for assertions.
fn locals_of(stop: &nxl_compiler::vm::StepStop) -> Vec<(String, String)> {
    stop.frames
        .last()
        .expect("at least one frame")
        .locals
        .iter()
        .map(|l| (l.name.clone(), l.value.to_string()))
        .collect()
}

#[test]
fn session_starts_at_the_first_statement_with_named_locals() {
    let mut session = debugger::DebugSession::new(SAMPLE, &[]).unwrap();
    let stop = session.start().unwrap();
    // The first stop is before the first statement, in the entry frame.
    assert_eq!(stop.statement, 0);
    assert_eq!(stop.line, 1);
    assert!(!stop.breakpoint_hit);
    assert_eq!(stop.frames.last().unwrap().function, "main");
    // The entry frame names the program's locals (values not yet stored).
    let locals = locals_of(&stop);
    let names: Vec<&str> = locals.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"x"), "{names:?}");
    assert!(names.contains(&"y"), "{names:?}");
    // Nothing has executed yet.
    assert!(stop.output.is_empty(), "{:?}", stop.output);
}

#[test]
fn session_step_walks_statements_in_order_and_completes() {
    let mut session = debugger::DebugSession::new(SAMPLE, &[]).unwrap();
    session.start().unwrap();
    let mut lines = Vec::new();
    let end = loop {
        match session.step() {
            Ok(stop) => lines.push(stop.line),
            Err(end) => break end,
        }
    };
    // Every source line is reached, and a straight-line program never
    // revisits an earlier line.
    assert!(lines.contains(&2), "{lines:?}");
    assert!(lines.contains(&3), "{lines:?}");
    assert!(lines.windows(2).all(|w| w[0] <= w[1]), "{lines:?}");
    match end {
        debugger::SessionEnd::Completed { output, .. } => assert_eq!(output, vec!["3"]),
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn session_locals_hold_values_before_the_print() {
    let mut session = debugger::DebugSession::new(SAMPLE, &[]).unwrap();
    session.start().unwrap();
    // Walk to the pause on the print statement (line 3).
    let stop = loop {
        let stop = session.step().unwrap();
        if stop.line == 3 {
            break stop;
        }
    };
    let locals = locals_of(&stop);
    assert!(
        locals.contains(&("x".to_owned(), "1".to_owned())),
        "{locals:?}"
    );
    assert!(
        locals.contains(&("y".to_owned(), "2".to_owned())),
        "{locals:?}"
    );
    // The pause is *before* the statement runs, so nothing is printed yet.
    assert!(stop.output.is_empty(), "{:?}", stop.output);
    assert_eq!(session.last().unwrap().statement, stop.statement);
}

#[test]
fn session_breakpoint_stops_mid_loop_with_iter_state() {
    let src = "let mut i = 0;\nloop {\n    if i >= 3 { break; }\n    print(i);\n    i = i + 1;\n}\nprint(\"done\");\n";
    let mut session = debugger::DebugSession::new(src, &[4]).unwrap();
    // The first stop is the first breakpoint hit: line 4, i = 0.
    let stop = session.start().unwrap();
    assert_eq!(stop.line, 4);
    assert!(stop.breakpoint_hit);
    let i = locals_of(&stop)
        .into_iter()
        .find(|(name, _)| name == "i")
        .map(|(_, value)| value);
    assert_eq!(i.as_deref(), Some("0"));
    // Keep continuing to the end: hits show i = 0..=2, never decreasing,
    // and the program completes with the loop's full output.
    let mut seen = Vec::new();
    let end = loop {
        match session.continue_run() {
            Ok(stop) => {
                assert_eq!(stop.line, 4, "stopped off the breakpoint line");
                assert!(stop.breakpoint_hit);
                let value = locals_of(&stop)
                    .into_iter()
                    .find(|(name, _)| name == "i")
                    .map(|(_, value)| value.parse::<i64>().expect("i is an integer"));
                seen.push(value.expect("i is in scope"));
            }
            Err(end) => break end,
        }
    };
    assert_eq!(seen.first(), Some(&0), "{seen:?}");
    assert_eq!(seen.last(), Some(&2), "{seen:?}");
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    match end {
        debugger::SessionEnd::Completed { output, .. } => {
            assert_eq!(output, vec!["0", "1", "2", "done"])
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

const WITH_CALL: &str = "fn add(a: Int, b: Int) -> Int {\n    let sum = a + b;\n    return sum;\n}\nlet r = add(1, 2);\nprint(r);\n";

#[test]
fn session_step_over_stays_out_of_callees() {
    let mut session = debugger::DebugSession::new(WITH_CALL, &[]).unwrap();
    session.start().unwrap();
    // Stepping over every statement in main must never enter `add`.
    loop {
        let stop = session.step_over().unwrap();
        let inner = stop.frames.last().unwrap();
        assert_eq!(inner.function, "main", "step over descended: {stop:?}");
        if stop.line == 6 {
            break;
        }
    }
    // By the time line 6's print is about to run, the call has completed.
    let locals = locals_of(session.last().unwrap());
    assert!(
        locals.contains(&("r".to_owned(), "3".to_owned())),
        "{locals:?}"
    );
}

#[test]
fn session_step_into_shows_callee_locals_and_step_out_returns() {
    let mut session = debugger::DebugSession::new(WITH_CALL, &[]).unwrap();
    session.start().unwrap();
    // Step until paused inside `add`.
    let stop = loop {
        let stop = session.step().unwrap();
        if stop.frames.last().unwrap().function == "add" {
            break stop;
        }
    };
    // The first in-callee stop sits on the function's opening span or its
    // first body statement.
    assert!(
        (1..=2).contains(&stop.line),
        "unexpected first line inside add: {stop:?}"
    );
    assert_eq!(stop.frames.len(), 2, "main -> add: {stop:?}");
    let locals = locals_of(&stop);
    assert!(
        locals.contains(&("a".to_owned(), "1".to_owned())),
        "{locals:?}"
    );
    assert!(
        locals.contains(&("b".to_owned(), "2".to_owned())),
        "{locals:?}"
    );
    // Stepping out returns to the caller's frame.
    let out = session.step_out().unwrap();
    assert_eq!(out.frames.last().unwrap().function, "main");
    assert_eq!(out.frames.len(), 1);
}

#[test]
fn session_reports_runtime_failure_and_stays_ended() {
    let mut session = debugger::DebugSession::new("let x = 1;\nthrow \"boom\";\n", &[]).unwrap();
    session.start().unwrap();
    let end = loop {
        match session.step() {
            Ok(_) => continue,
            Err(end) => break end,
        }
    };
    match &end {
        debugger::SessionEnd::Failed(message) => assert!(message.contains("boom"), "{message}"),
        other => panic!("expected failure, got {other:?}"),
    }
    // An ended session replays its outcome instead of restarting.
    assert_eq!(session.step(), Err(end));
}

#[test]
fn compiled_chunks_carry_local_names() {
    let sf = nxl_compiler::source::SourceFile::new("names.nxl", SAMPLE);
    let module = nxl_compiler::compile_to_nir(&sf).unwrap();
    let program = nxl_compiler::vm::compile(&module).unwrap();
    let chunk = program
        .chunks
        .iter()
        .find(|c| c.name == "main")
        .expect("top-level chunk");
    let names: Vec<&str> = chunk
        .local_names
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();
    assert!(names.contains(&"x"), "{:?}", chunk.local_names);
    assert!(names.contains(&"y"), "{:?}", chunk.local_names);
    // The table is slot-sorted so bytecode stays deterministic.
    let slots: Vec<u32> = chunk.local_names.iter().map(|(slot, _)| *slot).collect();
    let mut sorted = slots.clone();
    sorted.sort_unstable();
    assert_eq!(slots, sorted, "{:?}", chunk.local_names);
}

#[test]
fn local_names_survive_bytecode_roundtrip() {
    let sf = nxl_compiler::source::SourceFile::new("rt_names.nxl", SAMPLE);
    let module = nxl_compiler::compile_to_nir(&sf).unwrap();
    let program = nxl_compiler::vm::compile(&module).unwrap();
    let restored = nxl_compiler::vm::CompiledProgram::deserialize(&program.serialize()).unwrap();
    assert_eq!(restored, program, "debug info must survive serialization");
}
