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
