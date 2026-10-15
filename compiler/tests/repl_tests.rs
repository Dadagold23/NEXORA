//! Interactive REPL behaviour.

use nxl_compiler::repl::{run, ReplConfig};
use std::io::Cursor;

/// Feed a session to the REPL and return what it printed.
fn session(input: &str) -> String {
    let mut output = Vec::new();
    run(
        &mut Cursor::new(input.as_bytes()),
        &mut output,
        &ReplConfig::default(),
    )
    .expect("session runs");
    String::from_utf8(output).expect("utf-8 output")
}

#[test]
fn evaluates_an_expression_and_shows_its_value() {
    assert_eq!(session("1 + 2\n"), "3\n");
}

#[test]
fn definitions_stay_available_to_later_inputs() {
    let out = session("let x = 10;\nx * 2\n");
    assert!(out.contains('2'), "expected 20 in {out:?}");
    assert!(out.contains("0"), "expected the literal 20 in {out:?}");
}

#[test]
fn functions_stay_available_to_later_inputs() {
    let out = session("fn twice(n: Int) -> Int { return n * 2; }\ntwice(21)\n");
    assert!(out.contains("42"), "{out:?}");
}

#[test]
fn statements_run_for_their_effect() {
    let out = session("print(\"hello\");\n");
    assert_eq!(out, "hello\n");
}

#[test]
fn side_effects_from_earlier_statements_are_visible() {
    let out = session("let mut total = 0;\nfor i in 4 { total = total + i; }\ntotal\n");
    assert!(out.contains('6'), "{out:?}");
}

#[test]
fn multi_line_blocks_are_collected_until_complete() {
    let out = session("fn main() {\n  print(\"multi\");\n}\n");
    assert!(out.contains("multi"), "{out:?}");
}

#[test]
fn errors_are_reported_without_ending_the_session() {
    let out = session("not valid code\n1 + 1\n");
    assert!(out.starts_with("error:"), "{out:?}");
    assert!(out.contains('2'), "the session must continue: {out:?}");
}

#[test]
fn errors_point_at_the_users_own_line() {
    let out = session("undefined_name\n");
    assert!(out.contains("line 1"), "{out:?}");
}

#[test]
fn clear_forgets_the_session() {
    let out = session("let x = 1;\n.clear\nx\n");
    assert!(out.contains("session state cleared"), "{out:?}");
    assert!(
        out.contains("use of undeclared name 'x'"),
        "x must be gone: {out:?}"
    );
}

#[test]
fn help_lists_the_session_commands() {
    let out = session(".help\n");
    for command in [".exit", ".clear", ".help"] {
        assert!(
            out.contains(command),
            "help should mention {command}: {out:?}"
        );
    }
}

#[test]
fn exit_ends_the_session() {
    let mut output = Vec::new();
    let outcome = run(
        &mut Cursor::new(b"1 + 1\n.exit\n2 + 2\n".as_slice()),
        &mut output,
        &ReplConfig::default(),
    )
    .expect("session runs");
    assert!(outcome.finished, ".exit must end the session");
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains('2'), "{text:?}");
    assert!(!text.contains('4'), "nothing after .exit may run: {text:?}");
}

#[test]
fn end_of_input_flushes_the_final_expression() {
    let out = session("1 + 41");
    assert!(out.contains("42"), "{out:?}");
}

#[test]
fn prompts_are_written_when_enabled() {
    let mut output = Vec::new();
    run(
        &mut Cursor::new(b"1 + 1\n".as_slice()),
        &mut output,
        &ReplConfig { echo_prompts: true },
    )
    .expect("session runs");
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains(">>> "), "{text:?}");
}

#[test]
fn braces_inside_strings_do_not_confuse_completion() {
    let out = session("print(\"}\");\n1 + 1\n");
    assert!(out.contains("}"), "{out:?}");
    assert!(out.contains('2'), "the session must continue: {out:?}");
}
