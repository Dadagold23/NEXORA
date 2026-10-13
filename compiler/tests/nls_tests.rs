use nxl_compiler::nls;

const SAMPLE: &str = r#"fn add(a: Int, b: Int) -> Int {
    return a + b;
}

let total: Int = add(1, 2);

struct User {
    id: Int
}
"#;

#[test]
fn collects_symbols_from_real_ast() {
    let syms = nls::collect_symbols(SAMPLE);
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"add"), "{names:?}");
    assert!(names.contains(&"total"));
    assert!(names.contains(&"User"));
    let user = syms.iter().find(|s| s.name == "User").unwrap();
    assert_eq!(user.kind, "struct");
}

#[test]
fn completions_include_keywords_and_symbols() {
    let items = nls::completions(SAMPLE);
    assert!(items.contains(&"fn".to_owned()));
    assert!(items.contains(&"add".to_owned()));
    assert!(items.contains(&"User".to_owned()));
}

#[test]
fn hover_describes_symbol_types() {
    let text = "let total: Int = 0;\n";
    // 'total' starts at column 5
    let hover = nls::hover(text, 1, 5).unwrap();
    assert!(hover.contains("variable total: Int"), "{hover}");
}

#[test]
fn definition_returns_declaration_position() {
    let (line, _col) = nls::definition(SAMPLE, "add").unwrap();
    assert_eq!(line, 1);
    let (line, _col) = nls::definition(SAMPLE, "User").unwrap();
    assert_eq!(line, 7);
}

#[test]
fn rename_is_whole_word_only() {
    let out = nls::rename("let add = add;\nlet adder = 1;\n", "add", "sum");
    assert!(out.contains("let sum = sum;"), "{out}");
    assert!(out.contains("let adder = 1;"), "{out}");
}

#[test]
fn diagnostics_from_real_compiler() {
    let msgs = nls::diagnostics("let x: Int = \"oops\";\n");
    assert!(!msgs.is_empty(), "expected a type error");
    assert!(nls::diagnostics("let x: Int = 1;\n").is_empty());
}

#[test]
fn lsp_initialize_and_symbol_rpc() {
    let resp = nls::handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
    assert!(resp.contains("completionProvider"), "{resp}");
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{{"textDocument":{{"text":{}}}}}}}"#,
        serde_json::to_string(SAMPLE).unwrap()
    );
    let resp = nls::handle_message(&req);
    assert!(resp.contains("\"name\":\"add\""), "{resp}");
    assert!(resp.contains("\"name\":\"User\""), "{resp}");
}

#[test]
fn unknown_method_returns_method_not_found() {
    let resp = nls::handle_message(r#"{"jsonrpc":"2.0","id":9,"method":"textDocument/notAFeature"}"#);
    assert!(resp.contains("method not found"), "{resp}");
}

#[test]
fn references_skip_strings_and_comments() {
    let src = "let add = 1;\n// add in a comment\nlet s = \"add\";\nlet x = add;\n";
    let refs = nls::references(src, "add");
    assert_eq!(refs, vec![(1, 5), (4, 9)], "{refs:?}");
}

#[test]
fn workspace_indexes_and_finds_references() {
    let mut ws = nls::Workspace::new();
    ws.add_file("a.nxl", "fn helper() {}\nlet x = helper();\n");
    ws.add_file("b.nxl", "fn other() { return helper(); }\n");
    assert_eq!(ws.file_count(), 2);

    let index = ws.symbol_index();
    assert!(index.contains_key("helper"), "{index:?}");
    assert_eq!(index["helper"][0].path, "a.nxl");

    let refs = ws.find_references("helper");
    let paths: Vec<&str> = refs.iter().map(|r| r.path.as_str()).collect();
    assert!(paths.contains(&"a.nxl"), "{paths:?}");
    assert!(paths.contains(&"b.nxl"), "{paths:?}");
    assert_eq!(refs.len(), 3);
}

#[test]
fn workspace_rename_is_cross_file_and_code_only() {
    let mut ws = nls::Workspace::new();
    ws.add_file("a.nxl", "fn helper() {}\nlet msg = \"helper\";\n");
    ws.add_file("b.nxl", "helper();\n");
    let edits = ws.rename("helper", "assist");
    assert_eq!(edits, 2, "only code occurrences should change");
    let (_, a) = ws.files().find(|(p, _)| *p == "a.nxl").unwrap();
    assert!(a.contains("fn assist()"), "{a}");
    assert!(a.contains("\"helper\""), "string literal untouched: {a}");
    let (_, b) = ws.files().find(|(p, _)| *p == "b.nxl").unwrap();
    assert!(b.starts_with("assist();"), "{b}");
}

#[test]
fn code_actions_suggest_rename_for_known_symbols() {
    let mut ws = nls::Workspace::new();
    ws.add_file("a.nxl", "fn known() {}\n");
    assert!(ws.code_actions("known").iter().any(|a| a.contains("Rename")));
    assert!(ws.code_actions("missing").is_empty());
}

#[test]
fn lsp_references_rpc_returns_locations() {
    let text = "let add = 1;\nlet sum = add + add;\n";
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/references","params":{{"textDocument":{{"text":{}}},"position":{{"line":2,"character":11}}}}}}"#,
        serde_json::to_string(text).unwrap()
    );
    let resp = nls::handle_message(&req);
    assert!(resp.contains("\"line\":1"), "{resp}");
    assert!(resp.contains("\"line\":2"), "{resp}");
    assert!(resp.contains("\"path\":\"buffer\""), "{resp}");
}
