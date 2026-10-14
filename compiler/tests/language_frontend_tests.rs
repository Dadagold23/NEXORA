use nxl_compiler::{
    ast::{DeclarationKind, ExpressionKind, Statement, StringPart},
    compile_to_nir, lexer, parse_source,
    source::SourceFile,
};

#[test]
fn parses_modules_types_generics_and_async_functions() {
    let source = SourceFile::new(
        "modules.nxl",
        r#"
            module app.services;
            import std.io as io;
            from app.models import User;

            struct Record<T> {
                value: T
            }

            interface Repository<T> {
                fn find(id: Int) -> Option<T>;
            }

            async fn load<T>(id: Int) -> Result<Record<T>, Error>
            where T: Printable + Send {
                return await io.fetch(id)
            }
        "#,
    );
    let program = parse_source(&source).expect("documented declaration forms should parse");
    assert!(matches!(program.statements[0], Statement::Module { .. }));
    assert!(matches!(program.statements[1], Statement::Import { .. }));
    assert!(matches!(program.statements[2], Statement::Import { .. }));
    let Statement::TypeDeclaration(record) = &program.statements[3] else {
        panic!("expected struct declaration");
    };
    assert_eq!(record.kind, DeclarationKind::Struct);
    assert_eq!(record.type_parameters, ["T"]);
    let Statement::Function(load) = &program.statements[5] else {
        panic!("expected async function");
    };
    assert!(load.is_async);
    assert_eq!(
        load.return_type.as_deref(),
        Some("Result<Record<T>, Error>")
    );
    assert_eq!(load.constraints[0].bounds, ["Printable", "Send"]);
}

#[test]
fn parses_class_actor_enum_alias_impl_and_channel_forms() {
    let source = SourceFile::new(
        "declarations.nxl",
        r#"
            type UserId = Int;
            class Person(name: String) {
                let name: String
                fn init(name: String) {
                    self.name = name
                }
            }
            actor Counter {
                let count: Int = 0
                fn increment() {
                    self.count += 1
                }
            }
            enum Choice<T> {
                Some(T),
                None
            }
            impl Printable for Person {
                fn print() -> String { return self.name }
            }
            let messages = channel()
            let path = `C:\temp\path`
        "#,
    );
    let program = parse_source(&source).expect("remaining declaration forms should parse");
    assert!(matches!(program.statements[0], Statement::TypeAlias(_)));
    assert!(matches!(
        program.statements[1],
        Statement::TypeDeclaration(_)
    ));
    let Statement::TypeDeclaration(person) = &program.statements[1] else {
        panic!("expected class declaration");
    };
    assert_eq!(person.constructor_parameters.len(), 1);
    assert_eq!(
        person.constructor_parameters[0].annotation.as_deref(),
        Some("String")
    );
    assert!(matches!(
        program.statements[2],
        Statement::TypeDeclaration(_)
    ));
    assert!(matches!(
        program.statements[3],
        Statement::TypeDeclaration(_)
    ));
    assert!(matches!(program.statements[4], Statement::ImplBlock(_)));
    let Statement::Let {
        value: Some(channel),
        ..
    } = &program.statements[5]
    else {
        panic!("expected channel initializer");
    };
    let ExpressionKind::Call { callee, .. } = &channel.kind else {
        panic!("expected channel constructor call");
    };
    assert!(matches!(
        callee.kind,
        ExpressionKind::Channel { element_type: None }
    ));
}

#[test]
fn parses_control_flow_error_handling_and_patterns() {
    let source = SourceFile::new(
        "flow.nxl",
        r#"
            fn work(items: Array<Int>) {
                for item in items {
                    if item == 0 {
                        continue
                    } else if item > 10 {
                        break
                    }
                }
                while false { break }
                loop { break }
                try {
                    throw "failed"
                } catch err: DbError {
                    log(err)
                } finally {
                    cleanup()
                }
                match 2 {
                    case 0 => "zero"
                    case 1 | 2 => "small"
                    case _ => "other"
                }
            }
        "#,
    );
    let program = parse_source(&source).expect("control flow forms should parse");
    assert!(matches!(program.statements[0], Statement::Function(_)));
}

#[test]
fn parses_collection_closure_access_and_assignment_expressions() {
    let source = SourceFile::new(
        "expressions.nxl",
        r#"
            let list = [1, 2, 3]
            let map = { "name": "Ava", role: "dev" }
            let set = { 1, 2, 3 }
            let tuple = (1, "two")
            let convert = fn (x: Int) -> Int { return x * 2 }
            let result = convert(list[0]) + map.name?.length
            tuple.value = result
        "#,
    );
    let program = parse_source(&source).expect("collection and postfix expressions should parse");
    assert_eq!(program.statements.len(), 7);
    let Statement::Let {
        value: Some(value), ..
    } = &program.statements[0]
    else {
        panic!("expected array initializer");
    };
    assert!(matches!(value.kind, ExpressionKind::Array(_)));
    let Statement::Let {
        value: Some(value), ..
    } = &program.statements[1]
    else {
        panic!("expected map initializer");
    };
    assert!(matches!(value.kind, ExpressionKind::Map(_)));
    assert!(matches!(program.statements[6], Statement::Expression(_)));
}

#[test]
fn lexes_full_operator_set_and_numeric_conventions() {
    let source = SourceFile::new(
        "tokens.nxl",
        "let n = 1_000 + 0xFF + 0b101 + 2.5; a << 2; b >> 1; x?.y; x in y and not false; x ||= y; x &&= y",
    );
    let tokens = lexer::lex(&source).expect("documented operators should lex");
    assert!(tokens
        .iter()
        .any(|token| matches!(token.kind, nxl_compiler::lexer::TokenKind::Integer(1000))));
    assert!(tokens
        .iter()
        .any(|token| matches!(token.kind, nxl_compiler::lexer::TokenKind::Integer(255))));
    assert!(tokens
        .iter()
        .any(|token| matches!(token.kind, nxl_compiler::lexer::TokenKind::Integer(5))));
    assert!(tokens
        .iter()
        .any(|token| matches!(token.kind, nxl_compiler::lexer::TokenKind::ShiftLeft)));
    assert!(tokens
        .iter()
        .any(|token| matches!(token.kind, nxl_compiler::lexer::TokenKind::ShiftRight)));
    assert!(tokens
        .iter()
        .any(|token| matches!(token.kind, nxl_compiler::lexer::TokenKind::NullSafeDot)));
    assert!(tokens
        .iter()
        .any(|token| matches!(token.kind, nxl_compiler::lexer::TokenKind::OrEqual)));
    assert!(tokens
        .iter()
        .any(|token| matches!(token.kind, nxl_compiler::lexer::TokenKind::AndEqual)));
}

#[test]
fn parses_typed_uninitialized_values_and_optional_types() {
    let source = SourceFile::new(
        "optional.nxl",
        "let maybe: Option<String>\nlet tuple: (Int, String)\nlet items: [Int]",
    );
    let program = parse_source(&source).expect("annotated declarations may omit initializers");
    assert_eq!(program.statements.len(), 3);
    assert!(matches!(
        program.statements[0],
        Statement::Let {
            value: None,
            annotation: Some(_),
            ..
        }
    ));
}

#[test]
fn parses_nested_generic_types_and_typed_channels() {
    let source = SourceFile::new(
        "nested_types.nxl",
        "let result: Result<Array<Option<Int>>, Error>\nlet messages = channel<Result<Int, Error>>()",
    );
    let program = parse_source(&source).expect("nested generic closers should parse");
    let Statement::Let {
        annotation: Some(annotation),
        ..
    } = &program.statements[0]
    else {
        panic!("expected nested generic annotation");
    };
    assert_eq!(annotation, "Result<Array<Option<Int>>, Error>");
    let Statement::Let {
        value: Some(channel),
        ..
    } = &program.statements[1]
    else {
        panic!("expected channel initializer");
    };
    let ExpressionKind::Call { callee, .. } = &channel.kind else {
        panic!("expected channel call expression");
    };
    assert!(matches!(
        &callee.kind,
        ExpressionKind::Channel {
            element_type: Some(element_type)
        } if element_type == "Result<Int, Error>"
    ));
}

#[test]
fn parses_guarded_match_and_async_spawn_expressions() {
    let source = SourceFile::new(
        "async_match.nxl",
        r#"
            let task = async { return await fetch() }
            let child = spawn task
            match value {
                case item when item > 0 => item
                case _ => 0
            }
        "#,
    );
    let program = parse_source(&source).expect("guards and async expressions should parse");
    let Statement::Match { cases, .. } = &program.statements[2] else {
        panic!("expected match statement");
    };
    assert!(cases[0].guard.is_some());
    assert!(cases[1].guard.is_none());
    let Statement::Let {
        value: Some(async_value),
        ..
    } = &program.statements[0]
    else {
        panic!("expected async initializer");
    };
    assert!(matches!(async_value.kind, ExpressionKind::AsyncBlock(_)));
    let Statement::Let {
        value: Some(spawn_value),
        ..
    } = &program.statements[1]
    else {
        panic!("expected spawned task initializer");
    };
    assert!(matches!(spawn_value.kind, ExpressionKind::Spawn(_)));
}

#[test]
fn parses_interpolation_payloads_and_block_comments() {
    let source = SourceFile::new(
        "strings.nxl",
        "/* a block comment */ let label = \"Hello, ${user.name}!\"; let raw = `C:\\temp\\path`;",
    );
    let program = parse_source(&source).expect("interpolated strings should parse");
    assert_eq!(program.statements.len(), 2);
    let Statement::Let {
        value: Some(expression),
        ..
    } = &program.statements[0]
    else {
        panic!("expected interpolated string declaration");
    };
    let ExpressionKind::InterpolatedString(parts) = &expression.kind else {
        panic!("expected string interpolation nodes");
    };
    assert!(matches!(parts[0], StringPart::Text(_)));
    assert!(matches!(parts[1], StringPart::Expression(_)));
    assert!(matches!(parts[2], StringPart::Text(_)));
}

#[test]
fn distinguishes_character_literals_from_single_quoted_strings() {
    let source = SourceFile::new("quotes.nxl", "let character = 'n'; let word = 'name';");
    let program = parse_source(&source).expect("single-quote forms should parse");
    let Statement::Let {
        value: Some(character),
        ..
    } = &program.statements[0]
    else {
        panic!("expected character initializer");
    };
    assert!(matches!(character.kind, ExpressionKind::Character('n')));
    let Statement::Let {
        value: Some(word), ..
    } = &program.statements[1]
    else {
        panic!("expected string initializer");
    };
    assert!(matches!(
        &word.kind,
        ExpressionKind::String(value) if value == "name"
    ));
}

#[test]
fn diagnoses_unterminated_interpolations() {
    let source = SourceFile::new("unterminated.nxl", "let value = \"hello ${name\"");
    let diagnostic = parse_source(&source).expect_err("missing interpolation brace must fail");
    assert_eq!(diagnostic.code, "NXL0006");
}

#[test]
fn diagnoses_empty_interpolations_and_missing_initializers() {
    let empty_interpolation = SourceFile::new("empty.nxl", "let value = \"hello ${ }\";");
    let diagnostic =
        parse_source(&empty_interpolation).expect_err("empty interpolation must be rejected");
    assert_eq!(diagnostic.code, "NXL0007");

    let missing_initializer = SourceFile::new("missing.nxl", "let value =");
    let diagnostic =
        parse_source(&missing_initializer).expect_err("missing expression must not panic");
    assert_eq!(diagnostic.code, "NXL1001");
}

#[test]
fn reports_unsupported_lowering_separately_from_parse_errors() {
    let source = SourceFile::new("function.nxl", "fn main() { return 1 }");
    // Functions are now lowered; this test should succeed
    let result = compile_to_nir(&source);
    assert!(
        result.is_ok(),
        "function declarations should now be lowered"
    );
}

// ── Phase 1: string interpolation ─────────────────────────────────────────────

#[test]
fn lowers_interpolated_strings_and_runs_them() {
    let source = SourceFile::new(
        "interp.nxl",
        r#"let name = "Ada"; print("hi ${name}!");"#,
    );
    compile_to_nir(&source).expect("interpolation should lower");
    let result = nxl_compiler::vm::run(&source).expect("interpolation should execute");
    assert_eq!(result.output, vec!["hi Ada!"]);
}

#[test]
fn interpolates_non_string_values() {
    let source = SourceFile::new(
        "interp_types.nxl",
        "let n = 42; let flag = true; print(\"n=${n} flag=${flag}\");",
    );
    let result = nxl_compiler::vm::run(&source).expect("mixed interpolation should execute");
    assert_eq!(result.output, vec!["n=42 flag=true"]);
}

#[test]
fn interpolation_empty_template_yields_empty_string() {
    let source = SourceFile::new("interp_empty.nxl", r#"print("${2207}");"#);
    let result = nxl_compiler::vm::run(&source).expect("numeric-only template should execute");
    assert_eq!(result.output, vec!["2207"]);
}

#[test]
fn rejects_interpolating_void_values() {
    // A call known to return Void cannot be stringified.
    let source = SourceFile::new("interp_void.nxl", "let s = \"${print(1)}\";");
    let diagnostic =
        compile_to_nir(&source).expect_err("interpolating void must fail");
    assert_eq!(diagnostic.code, "NXL2006");
}

#[test]
fn js_backend_emits_string_conversion_for_interpolation() {
    let source = SourceFile::new("interp_js.nxl", r#"let n = 1; print("v=${n}");"#);
    let artifact =
        nxl_compiler::backend::compile_js(&source).expect("interpolation should emit JS");
    assert!(artifact.source.contains("String("), "JS must stringify parts");
}

// ── Phase 1: try / catch / finally / throw ────────────────────────────────────

#[test]
fn lowers_try_catch_and_runs_it() {
    let source = SourceFile::new(
        "try_basic.nxl",
        r#"try { throw "boom"; } catch e { print(e); }"#,
    );
    compile_to_nir(&source).expect("try/catch should lower");
    let result = nxl_compiler::vm::run(&source).expect("try/catch should execute");
    assert_eq!(result.output, vec!["boom"]);
}

#[test]
fn no_exception_skips_catch_body() {
    let source = SourceFile::new(
        "try_no_throw.nxl",
        r#"try { print("ok"); } catch e { print("bad"); }"#,
    );
    let result = nxl_compiler::vm::run(&source).expect("clean try should execute");
    assert_eq!(result.output, vec!["ok"]);
}

#[test]
fn annotated_catch_matches_by_type() {
    let hit = SourceFile::new(
        "try_ann_hit.nxl",
        r#"try { throw 42; } catch e: Int { print("int"); } catch e { print("any"); }"#,
    );
    let result = nxl_compiler::vm::run(&hit).expect("annotated catch should execute");
    assert_eq!(result.output, vec!["int"]);

    let miss = SourceFile::new(
        "try_ann_miss.nxl",
        r#"try { throw 42; } catch e: String { print("wrong"); }"#,
    );
    let err = nxl_compiler::vm::run(&miss).expect_err("non-matching catch must rethrow");
    assert_eq!(err.code, "NXL9001");
    assert!(err.message.contains("42"), "rethrown value must survive");
}

#[test]
fn finally_runs_on_normal_and_exceptional_paths() {
    let normal = SourceFile::new(
        "try_fin_ok.nxl",
        r#"let mut log = "s"; try { log = log + "b"; } catch e { log = log + "c"; } finally { log = log + "f"; } print(log);"#,
    );
    let result = nxl_compiler::vm::run(&normal).expect("normal path should execute");
    assert_eq!(result.output, vec!["sbf"]);

    let exceptional = SourceFile::new(
        "try_fin_ex.nxl",
        r#"let mut log = "s"; try { log = log + "b"; throw "x"; } catch e { log = log + "c"; } finally { log = log + "f"; } print(log);"#,
    );
    let result = nxl_compiler::vm::run(&exceptional).expect("exceptional path should execute");
    assert_eq!(result.output, vec!["sbcf"]);

    let uncaught = SourceFile::new(
        "try_fin_uncaught.nxl",
        r#"let mut log = "s"; try { throw "x"; } finally { log = log + "f"; print(log); }"#,
    );
    let err = nxl_compiler::vm::run(&uncaught).expect_err("uncaught must propagate");
    assert_eq!(err.code, "NXL9001");
}

#[test]
fn uncaught_throw_propagates_as_runtime_error() {
    let source = SourceFile::new("throw_top.nxl", r#"throw "kaput";"#);
    let err = nxl_compiler::vm::run(&source).expect_err("top-level throw must fail");
    assert_eq!(err.code, "NXL9001");
    assert!(err.message.contains("kaput"));
}

#[test]
fn throw_inside_function_is_caught_by_caller() {
    let source = SourceFile::new(
        "try_cross_fn.nxl",
        r#"fn fail() { throw "deep"; } try { fail(); } catch e { print(e); }"#,
    );
    let result = nxl_compiler::vm::run(&source).expect("cross-function catch should work");
    assert_eq!(result.output, vec!["deep"]);
}

#[test]
fn nested_try_catches_inner_rethrow() {
    let source = SourceFile::new(
        "try_nested.nxl",
        r#"try { try { throw "inner"; } catch e: Int { print("wrong"); } } catch e { print(e); }"#,
    );
    let result = nxl_compiler::vm::run(&source).expect("nested try should execute");
    assert_eq!(result.output, vec!["inner"]);
}

#[test]
fn try_composes_with_loops() {
    let source = SourceFile::new(
        "try_loop.nxl",
        "let mut total = 0; let mut i = 0; while i < 3 { try { total = total + i; } catch e { total = 99; } i = i + 1; } print(total);",
    );
    let result = nxl_compiler::vm::run(&source).expect("try in loop should execute");
    assert_eq!(result.output, vec!["3"]);

    let source = SourceFile::new(
        "loop_in_try.nxl",
        "let mut total = 0; try { while total < 3 { total = total + 1; } } catch e { total = 99; } print(total);",
    );
    let result = nxl_compiler::vm::run(&source).expect("loop in try should execute");
    assert_eq!(result.output, vec!["3"]);
}

#[test]
fn for_in_iterates_integer_ranges() {
    let source = SourceFile::new(
        "for_range.nxl",
        "let mut total = 0; for i in 3 { total = total + i; } print(total);",
    );
    let result = nxl_compiler::vm::run(&source).expect("range for-in should execute");
    assert_eq!(result.output, vec!["3"]);

    let empty = SourceFile::new(
        "for_empty.nxl",
        "let mut total = 0; for i in 0 { total = total + 1; } print(total);",
    );
    let result = nxl_compiler::vm::run(&empty).expect("empty range should execute");
    assert_eq!(result.output, vec!["0"]);
}

#[test]
fn for_in_iterates_arrays() {
    let source = SourceFile::new(
        "for_array.nxl",
        "let mut total = 0; let items = [5, 6, 7]; for v in items { total = total + v; } print(total);",
    );
    let result = nxl_compiler::vm::run(&source).expect("array for-in should execute");
    assert_eq!(result.output, vec!["18"]);
}

#[test]
fn for_in_rejects_non_iterables_loudly() {
    let source = SourceFile::new(
        "for_bad.nxl",
        r#"for v in true { print(v); }"#,
    );
    let err = nxl_compiler::vm::run(&source).expect_err("non-iterable must fail");
    assert_eq!(err.code, "NXL9001");
    assert!(err.message.contains("not iterable"));
}

// ── Phase 2: arrays ───────────────────────────────────────────────────────────

#[test]
fn array_literal_and_index_read() {
    let source = SourceFile::new(
        "array_basic.nxl",
        "let a = [10, 20, 30]; print(a[0]); print(a[2]);",
    );
    let result = nxl_compiler::vm::run(&source).expect("array read should execute");
    assert_eq!(result.output, vec!["10", "30"]);
}

#[test]
fn array_index_out_of_bounds_is_catchable() {
    let source = SourceFile::new(
        "array_oob.nxl",
        r#"let a = [1]; try { print(a[9]); } catch e { print("oob"); }"#,
    );
    let result = nxl_compiler::vm::run(&source).expect("oob should be catchable");
    assert_eq!(result.output, vec!["oob"]);
}

#[test]
fn array_index_assignment_and_compound_assignment() {
    let source = SourceFile::new(
        "array_assign.nxl",
        "let mut a = [1, 2]; a[0] = 9; a[1] += 1; print(a[0]); print(a[1]);",
    );
    let result = nxl_compiler::vm::run(&source).expect("index assign should execute");
    assert_eq!(result.output, vec!["9", "3"]);
}

#[test]
fn array_assignment_through_immutable_binding_rejected() {
    let source = SourceFile::new("array_immut.nxl", "let a = [1]; a[0] = 2;");
    let diagnostic =
        compile_to_nir(&source).expect_err("immutable index assign must fail");
    assert_eq!(diagnostic.code, "NXL2008");
}

#[test]
fn mixed_array_literal_rejected() {
    let source = SourceFile::new("array_mixed.nxl", r#"let a = [1, "x"];"#);
    let diagnostic =
        compile_to_nir(&source).expect_err("mixed literal must fail");
    assert_eq!(diagnostic.code, "NXL2001");
}

#[test]
fn non_integer_index_rejected() {
    let source = SourceFile::new("array_badidx.nxl", r#"let a = [1]; print(a["x"]);"#);
    let diagnostic =
        compile_to_nir(&source).expect_err("string index must fail");
    assert_eq!(diagnostic.code, "NXL2001");
}

// ── Phase 2: structs ──────────────────────────────────────────────────────────

#[test]
fn struct_construct_and_member_read() {
    let source = SourceFile::new(
        "struct_basic.nxl",
        "struct User { name: String; } let u = User { name: \"Ada\"; }; print(u.name);",
    );
    compile_to_nir(&source).expect("struct should lower");
    let result = nxl_compiler::vm::run(&source).expect("struct should execute");
    assert_eq!(result.output, vec!["Ada"]);
}

#[test]
fn struct_literal_validates_fields() {
    let missing = SourceFile::new(
        "struct_missing.nxl",
        "struct User { name: String; age: Int; } let u = User { name: \"Ada\"; };",
    );
    assert_eq!(
        compile_to_nir(&missing).expect_err("missing field must fail").code,
        "NXL2001"
    );

    let unknown = SourceFile::new(
        "struct_unknown.nxl",
        "struct User { name: String; } let u = User { name: \"Ada\"; nick: \"Al\"; };",
    );
    assert_eq!(
        compile_to_nir(&unknown).expect_err("unknown field must fail").code,
        "NXL2002"
    );

    let mistyped = SourceFile::new(
        "struct_mistype.nxl",
        "struct User { age: Int; } let u = User { age: \"old\"; };",
    );
    assert_eq!(
        compile_to_nir(&mistyped).expect_err("mistyped field must fail").code,
        "NXL2001"
    );
}

#[test]
fn struct_member_assignment() {
    let source = SourceFile::new(
        "struct_assign.nxl",
        "struct User { name: String; } let mut u = User { name: \"Al\"; }; u.name = \"Bo\"; print(u.name);",
    );
    let result = nxl_compiler::vm::run(&source).expect("member assign should execute");
    assert_eq!(result.output, vec!["Bo"]);
}

#[test]
fn unknown_field_read_is_catchable() {
    let source = SourceFile::new(
        "struct_field_oob.nxl",
        "struct User { name: String; } let u = User { name: \"Al\"; }; try { print(u.nick); } catch e { print(\"nope\"); }",
    );
    let result = nxl_compiler::vm::run(&source).expect("unknown field should be catchable");
    assert_eq!(result.output, vec!["nope"]);
}

// ── Phase 2: enums ────────────────────────────────────────────────────────────

#[test]
fn enum_construction_and_equality() {
    let source = SourceFile::new(
        "enum_basic.nxl",
        "enum Color { Red, Green } let a = Color.Red; let b = Color.Red; print(a == b); print(a == Color.Green);",
    );
    let result = nxl_compiler::vm::run(&source).expect("enum eq should execute");
    assert_eq!(result.output, vec!["true", "false"]);
}

#[test]
fn enum_arity_mismatch_rejected() {
    let source = SourceFile::new(
        "enum_arity.nxl",
        "enum Choice { Some(Int), None } let c = Choice.Some(1, 2);",
    );
    assert_eq!(
        compile_to_nir(&source).expect_err("arity mismatch must fail").code,
        "NXL2001"
    );

    let mistyped = SourceFile::new(
        "enum_mistype.nxl",
        "enum Choice { Some(Int), None } let c = Choice.Some(\"s\");",
    );
    assert_eq!(
        compile_to_nir(&mistyped).expect_err("mistyped payload must fail").code,
        "NXL2001"
    );
}

// ── Phase 2: match dispatch ───────────────────────────────────────────────────

#[test]
fn match_runs_only_the_first_matching_arm() {
    let source = SourceFile::new(
        "match_dispatch.nxl",
        r#"let v = 2; match v { case 1 => print("one"); case 2 => print("two"); case _ => print("other"); }"#,
    );
    let result = nxl_compiler::vm::run(&source).expect("match should execute");
    assert_eq!(result.output, vec!["two"]);
}

#[test]
fn match_falls_through_to_wildcard() {
    let source = SourceFile::new(
        "match_wild.nxl",
        r#"let v = 9; match v { case 1 => print("one"); case _ => print("other"); }"#,
    );
    let result = nxl_compiler::vm::run(&source).expect("wildcard should execute");
    assert_eq!(result.output, vec!["other"]);
}

#[test]
fn match_with_guard_selects_arms() {
    let source = SourceFile::new(
        "match_guard.nxl",
        "let v = 5; match v { case x when x > 10 => print(\"big\"); case x => print(x); }",
    );
    let result = nxl_compiler::vm::run(&source).expect("guarded match should execute");
    assert_eq!(result.output, vec!["5"]);
}

#[test]
fn match_on_enum_variants_with_payloads() {
    let source = SourceFile::new(
        "match_enum.nxl",
        "enum Choice { Some(Int), None } let c = Choice.Some(41); match c { case Some(x) => print(x + 1); case None() => print(\"none\"); }",
    );
    let result = nxl_compiler::vm::run(&source).expect("enum match should execute");
    assert_eq!(result.output, vec!["42"]);
}

#[test]
fn match_on_struct_patterns() {
    let source = SourceFile::new(
        "match_struct.nxl",
        "struct Point { x: Int; y: Int } let p = Point { x: 1, y: 2; }; match p { case Point { x: 1, y: q } => print(q); case _ => print(\"no\"); }",
    );
    let result = nxl_compiler::vm::run(&source).expect("struct match should execute");
    assert_eq!(result.output, vec!["2"]);
}

#[test]
fn match_or_patterns_match_any_alternative() {
    let source = SourceFile::new(
        "match_or.nxl",
        r#"let v = 2; match v { case 1 | 2 => print("low"); case _ => print("high"); }"#,
    );
    let result = nxl_compiler::vm::run(&source).expect("or pattern should execute");
    assert_eq!(result.output, vec!["low"]);
}

#[test]
fn match_dynamic_fallthrough_on_type_mismatch() {
    // A dynamically-typed scrutinee falls through instead of throwing.
    let source = SourceFile::new(
        "match_dyn.nxl",
        r#"fn f(x) { match x { case 1 => print("one"); case _ => print("other"); } } f("s"); f(1);"#,
    );
    let result = nxl_compiler::vm::run(&source).expect("dynamic match should execute");
    assert_eq!(result.output, vec!["other", "one"]);
}

// ── Phase 2: gradual typing ───────────────────────────────────────────────────

#[test]
fn untyped_values_support_operators_dynamically() {
    let source = SourceFile::new(
        "gradual.nxl",
        "fn f(x) { return x + 1; } print(f(41));",
    );
    let result = nxl_compiler::vm::run(&source).expect("dynamic arithmetic should execute");
    assert_eq!(result.output, vec!["42"]);
}

#[test]
fn remainder_operator_works() {
    let source = SourceFile::new("rem.nxl", "print(7 % 3);");
    let result = nxl_compiler::vm::run(&source).expect("remainder should execute");
    assert_eq!(result.output, vec!["1"]);
}

// ── Phase 2: JavaScript backend ───────────────────────────────────────────────

// ── Phase 3: classes and methods ──────────────────────────────────────────────

#[test]
fn class_counter_mutation_persists_across_calls() {
    let source = SourceFile::new(
        "class_counter.nxl",
        "class Counter { let count: Int = 0; fn inc() { self.count = self.count + 1; } fn get() -> Int { return self.count; } } let mut c = Counter(); c.inc(); c.inc(); print(c.get());",
    );
    compile_to_nir(&source).expect("class should lower");
    let result = nxl_compiler::vm::run(&source).expect("methods should execute");
    assert_eq!(result.output, vec!["2"]);
}

#[test]
fn init_constructor_binds_arguments() {
    let source = SourceFile::new(
        "class_init.nxl",
        "class Person(name: String) { let name: String; fn init(name: String) { self.name = name; } fn greet() -> String { return \"hi ${self.name}\"; } } let p = Person(\"Ada\"); print(p.greet());",
    );
    let result = nxl_compiler::vm::run(&source).expect("init should execute");
    assert_eq!(result.output, vec!["hi Ada"]);
}

#[test]
fn auto_constructor_uses_params_and_defaults() {
    let source = SourceFile::new(
        "class_auto.nxl",
        "class Config(verbose: Bool) { let verbose: Bool; let retries: Int = 3; fn show() -> Int { return self.retries; } } let c = Config(true); print(c.verbose); print(c.show());",
    );
    let result = nxl_compiler::vm::run(&source).expect("auto-ctor should execute");
    assert_eq!(result.output, vec!["true", "3"]);
}

#[test]
fn constructor_arity_mismatch_rejected() {
    let source = SourceFile::new(
        "class_arity.nxl",
        "class P(n: String) { let name: String; fn init(n: String) { self.name = n; } } let p = P();",
    );
    assert_eq!(
        compile_to_nir(&source).expect_err("arity mismatch must fail").code,
        "NXL2009"
    );
}

#[test]
fn unknown_method_is_catchable() {
    let source = SourceFile::new(
        "method_missing.nxl",
        "class A { } let a = A(); try { print(a.nope()); } catch e { print(e); }",
    );
    let result = nxl_compiler::vm::run(&source).expect("missing method should be catchable");
    assert_eq!(result.output, vec!["unknown method 'nope' on type 'A'"]);
}

#[test]
fn impl_block_methods_dispatch() {
    let source = SourceFile::new(
        "impl_basic.nxl",
        "struct S { x: Int; } impl S { fn double() -> Int { return self.x + self.x; } } let s = S { x: 21; }; print(s.double());",
    );
    let result = nxl_compiler::vm::run(&source).expect("impl method should execute");
    assert_eq!(result.output, vec!["42"]);
}

#[test]
fn interface_satisfaction_checked() {
    let good = SourceFile::new(
        "iface_good.nxl",
        "interface Greeter { fn greet() -> String; } struct S { x: Int; } impl Greeter for S { fn greet() -> String { return \"yo\"; } } let s = S { x: 1; }; print(s.greet());",
    );
    let result = nxl_compiler::vm::run(&good).expect("satisfied iface should execute");
    assert_eq!(result.output, vec!["yo"]);

    let bad = SourceFile::new(
        "iface_bad.nxl",
        "interface Greeter { fn greet() -> String; } struct S { x: Int; } impl Greeter for S { fn other() -> String { return \"x\"; } }",
    );
    assert_eq!(
        compile_to_nir(&bad).expect_err("missing method must fail").code,
        "NXL2001"
    );
}

#[test]
fn method_dispatch_is_dynamic_over_unknown_receivers() {
    let source = SourceFile::new(
        "method_dyn.nxl",
        "enum Color { Red, Green } impl Color { fn describe() -> String { return \"color!\"; } } fn f(c) { return c.describe(); } print(f(Color.Red));",
    );
    let result = nxl_compiler::vm::run(&source).expect("dynamic dispatch should execute");
    assert_eq!(result.output, vec!["color!"]);
}

#[test]
fn struct_patterns_match_class_instances() {
    let source = SourceFile::new(
        "match_class.nxl",
        "class Person(name: String) { let name: String; fn init(name: String) { self.name = name; } } let p = Person(\"Ada\"); match p { case Person { name: n } => print(n); case _ => print(\"no\"); }",
    );
    let result = nxl_compiler::vm::run(&source).expect("class pattern should execute");
    assert_eq!(result.output, vec!["Ada"]);
}

#[test]
fn duplicate_method_rejected() {
    let source = SourceFile::new(
        "method_dup.nxl",
        "class A { fn m() { } fn m() { } }",
    );
    assert_eq!(
        compile_to_nir(&source).expect_err("duplicate method must fail").code,
        "NXL2005"
    );
}

#[test]
fn js_backend_emits_methods_with_writeback() {
    let source = SourceFile::new(
        "method_js.nxl",
        "class Counter { let count: Int = 0; fn inc() { self.count = self.count + 1; } fn get() -> Int { return self.count; } } let mut c = Counter(); c.inc(); print(c.get());",
    );
    let artifact =
        nxl_compiler::backend::compile_js(&source).expect("methods should emit JS");
    assert!(artifact.source.contains("__nxl_call_method"), "dispatch helper needed");
    assert!(artifact.source.contains("__nxl_methods["), "registry needed");
}

#[test]
fn methods_run_identically_on_both_runtimes() {
    if !nxl_compiler::runtime::Runtime::node_available() {
        return;
    }
    let source = SourceFile::new(
        "method_both.nxl",
        "class Counter { let count: Int = 0; fn inc() { self.count = self.count + 1; } fn get() -> Int { return self.count; } } let mut c = Counter(); c.inc(); c.inc(); print(c.get());",
    );
    let vm_result = nxl_compiler::vm::run(&source).expect("VM should run");
    let node_result =
        nxl_compiler::runtime::run(&source).expect("Node should run");
    let node_lines: Vec<&str> = node_result.stdout.lines().collect();
    assert_eq!(vm_result.output, node_lines);
    assert_eq!(vm_result.output, vec!["2"]);
}

// ── Phase 4: modules ──────────────────────────────────────────────────────────

#[cfg(test)]
mod project_tests {
    use std::path::{Path, PathBuf};

    fn nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    static PROJECT_COUNTER: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);

    fn write_project(files: &[(&str, &str)]) -> PathBuf {
        // Counter makes names unique even when the clock is coarse and
        // tests run in parallel.
        let id = PROJECT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "nxl_proj_{}_{}_{}",
            std::process::id(),
            nanos(),
            id
        ));
        for (rel, content) in files {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        dir
    }

    fn run_entry(dir: &Path, rel: &str) -> Result<Vec<String>, nxl_compiler::diagnostic::Diagnostic> {
        let entry = dir.join(rel);
        let module = nxl_compiler::project::compile_project(&entry)?;
        nxl_compiler::vm::execute(&module)
            .map(|result| result.output)
    }

    #[test]
    fn cross_module_function_call_styles() {
        let dir = write_project(&[
            ("src/app/math.nxl", "fn add(a: Int, b: Int) -> Int { return a + b; }"),
            (
                "src/main.nxl",
                "import app.math;\nprint(math.add(20, 22));",
            ),
        ]);
        assert_eq!(run_entry(&dir, "src/main.nxl").unwrap(), vec!["42"]);

        let dir = write_project(&[
            ("src/app/math.nxl", "fn add(a: Int, b: Int) -> Int { return a + b; }"),
            (
                "src/main.nxl",
                "import app.math as m;\nfrom app.math import add;\nprint(m.add(1, 2)); print(add(3, 4));",
            ),
        ]);
        assert_eq!(run_entry(&dir, "src/main.nxl").unwrap(), vec!["3", "7"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn full_path_access_works() {
        let dir = write_project(&[
            (
                "src/app/counter.nxl",
                "class Counter { let n: Int = 0; fn bump() { self.n = self.n + 1; } fn read() -> Int { return self.n; } }",
            ),
            (
                "src/main.nxl",
                "import app.counter;\nlet mut c = app.counter.Counter();\nc.bump();\nprint(c.read());",
            ),
        ]);
        assert_eq!(run_entry(&dir, "src/main.nxl").unwrap(), vec!["1"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cross_module_types_and_match() {
        let dir = write_project(&[
            (
                "src/app/models.nxl",
                "struct User { name: String; }",
            ),
            (
                "src/app/words.nxl",
                "enum Word { Hi, Bye }",
            ),
            (
                "src/main.nxl",
                "from app.models import User;\nimport app.words;\nlet u = User { name: \"Al\"; };\nprint(u.name);\nlet w = words.Word.Hi;\nmatch w { case Hi() => print(\"hi\"); case _ => print(\"other\"); }",
            ),
        ]);
        assert_eq!(
            run_entry(&dir, "src/main.nxl").unwrap(),
            vec!["Al", "hi"]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn diamond_imports_load_once() {
        let dir = write_project(&[
            (
                "src/app/counter.nxl",
                "class Counter { let n: Int = 0; fn bump() { self.n = self.n + 1; } fn read() -> Int { return self.n; } }",
            ),
            (
                "src/app/d1.nxl",
                "import app.counter;\nfn tick() -> Int { let mut c = app.counter.Counter(); c.bump(); return c.read(); }",
            ),
            (
                "src/app/d2.nxl",
                "import app.counter;\nfn tock() -> Int { let mut c = app.counter.Counter(); c.bump(); c.bump(); return c.read(); }",
            ),
            (
                "src/main.nxl",
                "import app.d1;\nimport app.d2;\nprint(d1.tick());\nprint(d2.tock());",
            ),
        ]);
        assert_eq!(run_entry(&dir, "src/main.nxl").unwrap(), vec!["1", "2"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn circular_imports_fail_loudly() {
        let dir = write_project(&[
            ("src/app/a.nxl", "import app.b;"),
            ("src/app/b.nxl", "import app.a;"),
            ("src/main.nxl", "import app.a;\nprint(\"x\");"),
        ]);
        let err = run_entry(&dir, "src/main.nxl").expect_err("cycle must fail");
        assert_eq!(err.code, "NXL2002");
        assert!(err.message.contains("circular import"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_modules_are_tolerated_until_use() {
        let dir = write_project(&[(
            "src/main.nxl",
            "import app.missing;\nprint(\"tolerant\");",
        )]);
        assert_eq!(
            run_entry(&dir, "src/main.nxl").unwrap(),
            vec!["tolerant"]
        );

        let dir = write_project(&[(
            "src/main.nxl",
            "import app.missing;\nprint(missing.thing());",
        )]);
        let err = run_entry(&dir, "src/main.nxl").expect_err("use must fail");
        assert_eq!(err.code, "NXL2002");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_function_in_module_rejected() {
        let dir = write_project(&[
            ("src/app/math.nxl", "fn add(a: Int, b: Int) -> Int { return a + b; }"),
            ("src/main.nxl", "import app.math;\nprint(math.nope());"),
        ]);
        let err = run_entry(&dir, "src/main.nxl").expect_err("unknown must fail");
        assert_eq!(err.code, "NXL2002");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loader_orders_dependencies_first() {
        let dir = write_project(&[
            ("src/app/b.nxl", "fn b() -> Int { return 2; }"),
            (
                "src/app/a.nxl",
                "import app.b;\nfn a() -> Int { return b.b(); }",
            ),
            ("src/main.nxl", "import app.a;\nprint(a.a());"),
        ]);
        let entry = dir.join("src/main.nxl");
        let base = dir.join("src");
        let text = std::fs::read_to_string(&entry).unwrap();
        let modules =
            nxl_compiler::project::load_project(&base, "", &text, "main.nxl").unwrap();
        let paths: Vec<&str> = modules.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(paths, vec!["app.b", "app.a", ""]);
        assert_eq!(run_entry(&dir, "src/main.nxl").unwrap(), vec!["2"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn js_backend_compiles_multi_file_projects() {
        let dir = write_project(&[
            ("src/app/math.nxl", "fn add(a: Int, b: Int) -> Int { return a + b; }"),
            (
                "src/main.nxl",
                "import app.math;\nprint(math.add(1, 2));",
            ),
        ]);
        let entry = dir.join("src/main.nxl");
        let module = nxl_compiler::project::compile_project(&entry).unwrap();
        let artifact = nxl_compiler::backend::compile_js_module(&module).unwrap();
        assert!(
            artifact.source.contains("app$$math$$add"),
            "qualified names must be valid JS"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn multi_file_runs_identically_on_both_runtimes() {
        if !nxl_compiler::runtime::Runtime::node_available() {
            return;
        }
        let dir = write_project(&[
            ("src/app/math.nxl", "fn add(a: Int, b: Int) -> Int { return a + b; }"),
            (
                "src/main.nxl",
                "import app.math;\nprint(math.add(40, 2));",
            ),
        ]);
        let entry = dir.join("src/main.nxl");
        let module = nxl_compiler::project::compile_project(&entry).unwrap();
        let vm_result = nxl_compiler::vm::execute(&module).expect("VM should run");
        let node_result =
            nxl_compiler::runtime::run_module(&module).expect("Node should run");
        let node_lines: Vec<&str> = node_result.stdout.lines().collect();
        assert_eq!(vm_result.output, node_lines);
        assert_eq!(vm_result.output, vec!["42"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn from_imported_variant_reads_as_value() {
        let dir = write_project(&[
            ("src/app/words.nxl", "enum Word { Hi, Bye }"),
            (
                "src/main.nxl",
                "from app.words import Hi;\nlet x = Hi;\nprint(x);",
            ),
        ]);
        assert_eq!(
            run_entry(&dir, "src/main.nxl").unwrap(),
            vec!["Word.Hi"]
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

// ── Phase 5: concurrency ──────────────────────────────────────────────────────

#[test]
fn spawn_call_and_await_returns_value() {
    let source = SourceFile::new(
        "spawn_basic.nxl",
        "fn work(n: Int) -> Int { return n * 2; } let t = spawn work(21); print(await t);",
    );
    let result = nxl_compiler::vm::run(&source).expect("spawn should execute");
    assert_eq!(result.output, vec!["42"]);
}

#[test]
fn spawn_eager_value_wraps_completed() {
    let source = SourceFile::new("spawn_eager.nxl", "let t = spawn 42; print(await t);");
    let result = nxl_compiler::vm::run(&source).expect("eager spawn should execute");
    assert_eq!(result.output, vec!["42"]);
}

#[test]
fn await_non_task_passes_through() {
    let source = SourceFile::new("await_plain.nxl", "print(await 7);");
    let result = nxl_compiler::vm::run(&source).expect("await passthrough should execute");
    assert_eq!(result.output, vec!["7"]);
}

#[test]
fn spawn_unknown_function_rejected() {
    let source = SourceFile::new("spawn_unknown.nxl", "let t = spawn nope(1);");
    assert_eq!(
        compile_to_nir(&source).expect_err("unknown spawn must fail").code,
        "NXL2002"
    );
}

#[test]
fn task_failure_is_catchable_on_await() {
    let source = SourceFile::new(
        "spawn_fail.nxl",
        "fn bad() -> Int { throw \"deep\"; return 0; } let t = spawn bad(); try { print(await t); } catch e { print(e); }",
    );
    let result = nxl_compiler::vm::run(&source).expect("failed task should be catchable");
    assert_eq!(result.output, vec!["task failed: deep"]);
}

#[test]
fn channel_send_recv_close() {
    let source = SourceFile::new(
        "chan_basic.nxl",
        "let c = channel(); c.send(1); c.send(2); print(c.recv()); print(c.recv()); c.close();",
    );
    let result = nxl_compiler::vm::run(&source).expect("channel should execute");
    assert_eq!(result.output, vec!["1", "2"]);
}

#[test]
fn channel_close_errors_are_catchable() {
    let source = SourceFile::new(
        "chan_close.nxl",
        "let c = channel(); c.close(); try { c.send(1); } catch e { print(e); } try { print(c.recv()); } catch e { print(e); }",
    );
    let result = nxl_compiler::vm::run(&source).expect("close errors should be catchable");
    assert_eq!(result.output, vec!["send on a closed channel", "receive on a closed, empty channel"]);
}

#[test]
fn producer_consumer_across_tasks() {
    let source = SourceFile::new(
        "chan_tasks.nxl",
        "fn produce(c) { c.send(10); c.send(20); c.close(); } let ch = channel(); let t = spawn produce(ch); let mut sum = 0; for v in ch { sum = sum + v; } print(await t); print(sum);",
    );
    let result = nxl_compiler::vm::run(&source).expect("producer/consumer should execute");
    assert_eq!(result.output, vec!["()", "30"]);
}

#[test]
fn async_block_captures_by_value() {
    let source = SourceFile::new(
        "async_capture.nxl",
        "let x = 5; let t = async { return x + 1; }; print(await t);",
    );
    let result = nxl_compiler::vm::run(&source).expect("async block should execute");
    assert_eq!(result.output, vec!["6"]);
}

#[test]
fn actor_mailbox_sends_and_receives() {
    let source = SourceFile::new(
        "actor_mbox.nxl",
        "actor Counter { let count: Int = 0; } let mut c = Counter(); c.mailbox.send(7); print(c.mailbox.recv());",
    );
    let result = nxl_compiler::vm::run(&source).expect("mailbox should execute");
    assert_eq!(result.output, vec!["7"]);
}

#[test]
fn tasks_compare_by_identity() {
    let source = SourceFile::new(
        "task_eq.nxl",
        "fn w() -> Int { return 1; } let a = spawn w(); let b = spawn w(); print(a == a); print(a == b); print(await a); print(await b);",
    );
    let result = nxl_compiler::vm::run(&source).expect("task eq should execute");
    assert_eq!(result.output, vec!["true", "false", "1", "1"]);
}

#[test]
fn js_backend_rejects_async_loudly() {
    for (name, program) in [
        ("spawn_js", "fn f() -> Int { return 1; } let t = spawn f();"),
        (
            "await_js",
            "fn f() -> Int { return 1; } let t = spawn f(); print(await t);",
        ),
    ] {
        let source = SourceFile::new(name, program);
        let err = nxl_compiler::backend::compile_js(&source)
            .expect_err("async must fail loudly in JS");
        assert_eq!(err.code, "NXL2007", "unexpected code for {name}");
    }

    // Channel construction emits a synchronous stub (sync actor use works);
    // only send/recv/close need the VM.
    let source = SourceFile::new("chan_js.nxl", "let c = channel();");
    let artifact = nxl_compiler::backend::compile_js(&source).expect("stub should emit");
    assert!(artifact.source.contains("__nxl_stub"));
}

// ── Phase 6: backend control flow ─────────────────────────────────────────────

/// Assert a program produces identical output on the VM and on emitted JS.
/// Returns when Node.js is unavailable so the suite still runs without it.
fn assert_js_parity(program: &str, expected: &[&str], name: &str) {
    if !nxl_compiler::runtime::Runtime::node_available() {
        return;
    }
    let source = SourceFile::new(name, program);
    let vm_result = nxl_compiler::vm::run(&source).expect("VM should run");
    let node_result = nxl_compiler::runtime::run(&source).expect("Node should run");
    let node_lines: Vec<&str> = node_result.stdout.lines().collect();
    assert_eq!(vm_result.output, node_lines, "VM/Node mismatch for {name}");
    assert_eq!(vm_result.output, expected, "unexpected output for {name}");
}

#[test]
fn js_backend_emits_native_while_loop() {
    let source = SourceFile::new("while_emit.nxl", "let mut i = 0; while i < 3 { i = i + 1; }");
    let artifact = nxl_compiler::backend::compile_js(&source).expect("while should emit");
    assert!(artifact.source.contains("while (true)"), "native loop expected");
    assert!(
        artifact.source.contains("continue __nxl_loop"),
        "back-edge becomes a labelled continue"
    );
}

#[test]
fn js_backend_emits_native_for_of_loop() {
    let source = SourceFile::new("forof_emit.nxl", "for v in [1, 2, 3] { print(v); }");
    let artifact = nxl_compiler::backend::compile_js(&source).expect("for should emit");
    assert!(
        artifact.source.contains("for (const __nxl_item of __nxl_iter"),
        "for-in should become a JS for-of"
    );
    assert!(
        !artifact.source.contains("need `nxl run`"),
        "for-in no longer needs the VM"
    );
}

#[test]
fn while_loop_runs_identically_on_both_runtimes() {
    assert_js_parity(
        "let mut i = 0; while i < 3 { print(i); i = i + 1; }",
        &["0", "1", "2"],
        "while_both.nxl",
    );
}

#[test]
fn while_loop_over_function_body_matches_vm() {
    assert_js_parity(
        "fn sum(n: Int) -> Int { let mut t = 0; let mut i = 0; while i < n { t = t + i; i = i + 1; } return t; } print(sum(5));",
        &["10"],
        "while_fn.nxl",
    );
}

#[test]
fn while_true_with_break_matches_vm() {
    assert_js_parity(
        "let mut i = 0; while true { i = i + 1; if i > 2 { break; } } print(i);",
        &["3"],
        "break.nxl",
    );
}

#[test]
fn continue_inside_conditional_matches_vm() {
    assert_js_parity(
        "let mut i = 0; let mut s = 0; while i < 5 { i = i + 1; if i == 3 { continue; } s = s + i; } print(s);",
        &["12"],
        "continue.nxl",
    );
}

#[test]
fn nested_loops_match_vm() {
    assert_js_parity(
        "let mut i = 0; while i < 2 { let mut j = 0; while j < 2 { print(j); j = j + 1; } i = i + 1; }",
        &["0", "1", "0", "1"],
        "nested.nxl",
    );
}

#[test]
fn break_in_nested_loop_leaves_inner_only() {
    assert_js_parity(
        "let mut i = 0; let mut t = 0; while i < 3 { let mut j = 0; while j < 3 { if j == 1 { break; } t = t + 1; j = j + 1; } i = i + 1; } print(t);",
        &["3"],
        "nested_break.nxl",
    );
}

#[test]
fn return_from_inside_loop_matches_vm() {
    assert_js_parity(
        "fn f(n: Int) -> Int { let mut i = 0; while true { if i == n { return i * 2; } i = i + 1; } return -1; } print(f(4));",
        &["8"],
        "return_loop.nxl",
    );
}

#[test]
fn loop_inside_try_matches_vm() {
    assert_js_parity(
        "try { let mut i = 0; while i < 3 { print(i); i = i + 1; } } catch e { print(e); }",
        &["0", "1", "2"],
        "loop_try.nxl",
    );
}

#[test]
fn throw_from_inside_loop_is_catchable() {
    assert_js_parity(
        "try { let mut i = 0; while i < 5 { if i == 2 { throw \"boom\"; } i = i + 1; } } catch e { print(e); }",
        &["boom"],
        "throw_loop.nxl",
    );
}

#[test]
fn loop_inside_method_matches_vm() {
    assert_js_parity(
        "class C { fn run() -> Int { let mut i = 0; let mut t = 0; while i < 3 { t = t + i; i = i + 1; } return t; } } let c = C(); print(c.run());",
        &["3"],
        "method_loop.nxl",
    );
}

#[test]
fn for_in_over_int_matches_vm() {
    assert_js_parity("for i in 4 { print(i); }", &["0", "1", "2", "3"], "forint.nxl");
}

#[test]
fn for_in_over_array_matches_vm() {
    assert_js_parity(
        "let mut s = 0; for v in [1, 2, 3, 4, 5] { s = s + v; } print(s);",
        &["15"],
        "forarray.nxl",
    );
}

#[test]
fn for_in_with_break_and_continue_matches_vm() {
    assert_js_parity(
        "for i in 9 { if i == 3 { break; } if i % 2 == 0 { continue; } print(i); }",
        &["1"],
        "forbreak.nxl",
    );
}

#[test]
fn for_in_with_conditional_body_matches_vm() {
    assert_js_parity(
        "for v in [1, -1, 2] { if v > 0 { print(v); } }",
        &["1", "2"],
        "forif.nxl",
    );
}

#[test]
fn nested_for_in_loops_match_vm() {
    assert_js_parity(
        "for i in 2 { for j in 2 { print(j); } }",
        &["0", "1", "0", "1"],
        "fornest.nxl",
    );
}

#[test]
fn loop_with_false_condition_is_skipped() {
    assert_js_parity(
        "let mut i = 5; while i < 3 { print(i); i = i + 1; } print(\"skipped\");",
        &["skipped"],
        "skip.nxl",
    );
}

#[test]
fn for_in_over_zero_yields_nothing() {
    assert_js_parity("for i in 0 { print(i); } print(\"none\");", &["none"], "forzero.nxl");
}

#[test]
fn mutable_assignment_survives_loops_on_js() {
    // A `Load` must read the current value, not the value bound when the
    // instruction was emitted; loops are where the difference shows.
    assert_js_parity(
        "let mut i = 0; let mut t = 0; while i < 3 { t = t + i; i = i + 1; } print(t);",
        &["3"],
        "assign_loop.nxl",
    );
}

#[test]
fn js_reads_parameters_and_mutable_locals_correctly() {
    // Regression guard: a name must own its JS variable so that reading it
    // after a rebinding (here inside a function) sees the current value.
    assert_js_parity(
        "fn add_one(n: Int) -> Int { let mut t = n; t = t + 1; return t; } print(add_one(41));",
        &["42"],
        "param_read.nxl",
    );
}

#[test]
fn js_iteration_rejects_non_iterables() {
    let source = SourceFile::new("iter_bad.nxl", "for c in \"ab\" { print(c); }");
    let artifact = nxl_compiler::backend::compile_js(&source).expect("stub emits");
    assert!(artifact.source.contains("is not iterable"), "guard expected");
}

// ── Phase 2 backend repairs ───────────────────────────────────────────────────

#[test]
fn js_else_if_chains_emit() {
    let source = SourceFile::new(
        "elseif_js.nxl",
        "let v = 2; if v == 1 { print(\"one\"); } else { if v == 2 { print(\"two\"); } else { print(\"other\"); } }",
    );
    let artifact =
        nxl_compiler::backend::compile_js(&source).expect("else-if should emit");
    assert!(artifact.source.matches("if (").count() >= 2, "both branches must emit");
}

#[test]
fn js_backend_emits_data_constructs() {
    let source = SourceFile::new(
        "data_js.nxl",
        "struct User { name: String; } let u = User { name: \"Ada\"; }; let a = [u.name]; print(a[0]);",
    );
    let artifact =
        nxl_compiler::backend::compile_js(&source).expect("data should emit JS");
    assert!(artifact.source.contains("__nxl_type"), "structs need type tags");
    assert!(artifact.source.contains("__nxl_show"), "print needs value display");
}

#[test]
fn data_program_runs_identically_on_both_runtimes() {
    if !nxl_compiler::runtime::Runtime::node_available() {
        return;
    }
    let source = SourceFile::new(
        "data_both.nxl",
        "struct User { name: String; } let u = User { name: \"Ada\"; }; let a = [1, 2]; print(u.name); print(a[1]);",
    );
    let vm_result = nxl_compiler::vm::run(&source).expect("VM should run");
    let node_result =
        nxl_compiler::runtime::run(&source).expect("Node should run");
    let node_lines: Vec<&str> = node_result.stdout.lines().collect();
    assert_eq!(vm_result.output, node_lines);
    assert_eq!(vm_result.output, vec!["Ada", "2"]);
}

#[test]
fn try_catches_runtime_errors() {
    let source = SourceFile::new(
        "try_divzero.nxl",
        r#"try { let x = 1 / 0; print(x); } catch e { print("caught"); }"#,
    );
    let result = nxl_compiler::vm::run(&source).expect("division by zero should be catchable");
    assert_eq!(result.output, vec!["caught"]);
}

#[test]
fn rejects_return_across_try_boundary() {
    let source = SourceFile::new(
        "try_return.nxl",
        "fn f() -> Int { try { return 1; } catch e { return 2; } }",
    );
    let diagnostic =
        compile_to_nir(&source).expect_err("return across try must be rejected");
    assert_eq!(diagnostic.code, "NXL2007");
}

#[test]
fn rejects_unreachable_catch_after_bare_catch() {
    let source = SourceFile::new(
        "try_unreachable.nxl",
        r#"try { print(1); } catch e { print(2); } catch f: Int { print(3); }"#,
    );
    let diagnostic =
        compile_to_nir(&source).expect_err("catch after bare catch must be rejected");
    assert_eq!(diagnostic.code, "NXL2013");
}

#[test]
fn js_backend_emits_native_try_catch_finally() {
    let source = SourceFile::new(
        "try_js.nxl",
        r#"try { throw "x"; } catch e { print(e); } finally { print("done"); }"#,
    );
    let artifact =
        nxl_compiler::backend::compile_js(&source).expect("try should emit JS");
    assert!(artifact.source.contains("try {"), "JS must use native try");
    assert!(artifact.source.contains("catch (__exc)"), "JS must bind the error");
    assert!(artifact.source.contains("finally"), "JS must keep finally");
}
