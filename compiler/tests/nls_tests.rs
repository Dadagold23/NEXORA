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
fn lsp_initialize_advertises_what_it_answers() {
    let mut server = nls::Server::new();
    let out = server.handle(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
    assert_eq!(out.len(), 1, "{out:?}");
    let response: serde_json::Value = serde_json::from_str(&out[0]).unwrap();
    let capabilities = &response["result"]["capabilities"];
    // Every capability advertised here is one the server actually answers.
    assert_eq!(capabilities["hoverProvider"], true, "{response}");
    assert_eq!(capabilities["definitionProvider"], true, "{response}");
    assert_eq!(capabilities["documentSymbolProvider"], true, "{response}");
    assert!(capabilities["completionProvider"].is_object(), "{response}");
    assert_eq!(capabilities["textDocumentSync"], 1, "{response}");
}

#[test]
fn lsp_answers_requests_about_an_open_document() {
    // The protocol is stateful: the client opens a document and then asks
    // questions about it by URI. An earlier version read the source inline in
    // every request, which no real client does.
    let mut server = nls::Server::new();
    let uri = "file:///project/src/main.nxl";
    let open = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{uri}","languageId":"nxl","version":1,"text":{}}}}}}}"#,
        serde_json::to_string(SAMPLE).unwrap()
    );
    let published = server.handle(&open);
    assert_eq!(published.len(), 1, "{published:?}");
    let notification: serde_json::Value = serde_json::from_str(&published[0]).unwrap();
    assert_eq!(
        notification["method"], "textDocument/publishDiagnostics",
        "{notification}"
    );
    assert!(
        notification["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a valid document must not be reported as broken: {notification}"
    );
    assert_eq!(server.document(uri).map(|d| d.version), Some(1));

    let symbols = server.handle(&format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{{"textDocument":{{"uri":"{uri}"}}}}}}"#
    ));
    let response: serde_json::Value = serde_json::from_str(&symbols[0]).unwrap();
    let names: Vec<&str> = response["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|symbol| symbol["name"].as_str().unwrap_or_default())
        .collect();
    assert!(names.contains(&"add"), "{response}");
    assert!(names.contains(&"User"), "{response}");
    // An outline view needs ranges, not just names.
    assert!(
        response["result"][0]["range"]["start"]["line"].is_number(),
        "{response}"
    );
}

#[test]
fn lsp_hover_and_definition_use_the_open_document() {
    let mut server = nls::Server::new();
    let uri = "file:///project/src/main.nxl";
    server.handle(&format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{uri}","text":{}}}}}}}"#,
        serde_json::to_string(SAMPLE).unwrap()
    ));

    // `add` is declared on source line 1 and used on source line 5.
    let hover = server.handle(&format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{{"textDocument":{{"uri":"{uri}"}},"position":{{"line":0,"character":4}}}}}}"#
    ));
    let response: serde_json::Value = serde_json::from_str(&hover[0]).unwrap();
    assert!(
        response["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_default()
            .contains("add"),
        "{response}"
    );

    // The use on source line 5 (`let total: Int = add(1, 2);`, where `add`
    // starts at character 17) resolves back to the declaration.
    let definition = server.handle(&format!(
        r#"{{"jsonrpc":"2.0","id":4,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{uri}"}},"position":{{"line":4,"character":17}}}}}}"#
    ));
    let response: serde_json::Value = serde_json::from_str(&definition[0]).unwrap();
    let location = &response["result"][0];
    assert_eq!(location["uri"], uri, "{response}");
    assert_eq!(location["range"]["start"]["line"], 0, "{response}");
}

#[test]
fn lsp_publishes_diagnostics_after_a_change_and_clears_them_on_close() {
    let mut server = nls::Server::new();
    let uri = "file:///project/src/main.nxl";
    server.handle(&format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{uri}","version":1,"text":"fn main() {{ print(1); }}"}}}}}}"#
    ));

    // Break it.
    let changed = server.handle(&format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{uri}","version":2}},"contentChanges":[{{"text":"fn main() {{ let x = ; }}"}}]}}}}"#
    ));
    let notification: serde_json::Value = serde_json::from_str(&changed[0]).unwrap();
    let items = notification["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{notification}");
    assert_eq!(items[0]["severity"], 1, "{notification}");
    assert!(items[0]["message"].is_string(), "{notification}");
    assert!(
        items[0]["range"]["start"]["line"].is_number(),
        "a diagnostic without a position is not shown on the right line: {notification}"
    );
    assert_eq!(server.document(uri).map(|d| d.version), Some(2));

    // Fix it: the error must disappear rather than stick.
    let fixed = server.handle(&format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{uri}","version":3}},"contentChanges":[{{"text":"fn main() {{ print(1); }}"}}]}}}}"#
    ));
    let notification: serde_json::Value = serde_json::from_str(&fixed[0]).unwrap();
    assert!(
        notification["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{notification}"
    );

    // Closing clears what the editor is still showing.
    let closed = server.handle(&format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didClose","params":{{"textDocument":{{"uri":"{uri}"}}}}}}"#
    ));
    let notification: serde_json::Value = serde_json::from_str(&closed[0]).unwrap();
    assert!(
        notification["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{notification}"
    );
    assert_eq!(server.open_document_count(), 0);
}

#[test]
fn lsp_shutdown_and_exit_are_honoured() {
    let mut server = nls::Server::new();
    let out = server.handle(r#"{"jsonrpc":"2.0","id":9,"method":"shutdown"}"#);
    let response: serde_json::Value = serde_json::from_str(&out[0]).unwrap();
    assert!(response["result"].is_null(), "{response}");
    assert!(server.is_shutdown());

    assert!(server
        .handle(r#"{"jsonrpc":"2.0","method":"exit"}"#)
        .is_empty());
    assert!(server.should_exit(), "the stdio loop must stop on exit");
}

#[test]
fn unknown_method_returns_method_not_found() {
    let mut server = nls::Server::new();
    let out = server.handle(r#"{"jsonrpc":"2.0","id":9,"method":"textDocument/notAFeature"}"#);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].contains("method not found"), "{}", out[0]);

    // A notification with no id must not be answered at all: answering one
    // makes a client report a protocol error for an unknown notification.
    assert!(server
        .handle(r#"{"jsonrpc":"2.0","method":"some/notification","params":{}}"#)
        .is_empty());
}

#[test]
fn malformed_input_is_a_parse_error_not_a_crash() {
    let mut server = nls::Server::new();
    let out = server.handle("{not json");
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].contains("-32700"), "{}", out[0]);
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
    assert!(ws
        .code_actions("known")
        .iter()
        .any(|a| a.contains("Rename")));
    assert!(ws.code_actions("missing").is_empty());
}

#[test]
fn lsp_references_rpc_returns_locations() {
    let mut server = nls::Server::new();
    let uri = "file:///project/src/basic.nxl";
    let text = "let add = 1;\nlet sum = add + add;\n";
    server.handle(&format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{uri}","text":{}}}}}}}"#,
        serde_json::to_string(text).unwrap()
    ));
    let out = server.handle(&format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/references","params":{{"textDocument":{{"uri":"{uri}"}},"position":{{"line":1,"character":11}}}}}}"#
    ));
    let response: serde_json::Value = serde_json::from_str(&out[0]).unwrap();
    let locations = response["result"].as_array().unwrap();
    let lines: Vec<u64> = locations
        .iter()
        .map(|location| location["range"]["start"]["line"].as_u64().unwrap())
        .collect();
    // References include the declaration, then the two uses. LSP lines are
    // 0-based, so source lines 1, 2 and 2 come back as 0, 1 and 1.
    assert_eq!(lines, vec![0, 1, 1], "{response}");
    assert_eq!(locations[0]["uri"], uri, "{response}");
}

// ── the server over real stdio ────────────────────────────────────────────
//
// These drive the shipped binary the way an editor does. The in-process tests
// above check the protocol; these catch what only a process boundary shows:
// framing, startup, and the `nxlc lsp` entry point actually being wired up.

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler/ has a parent")
        .to_path_buf()
}

fn node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn run_node_script(script: &str, args: &[&str]) -> Option<std::process::Output> {
    let path = repo_root().join(script);
    if !path.exists() {
        return None; // not running from a checkout
    }
    Some(
        std::process::Command::new("node")
            .arg(&path)
            .args(args)
            .output()
            .expect("node runs"),
    )
}

#[test]
fn the_language_server_answers_over_stdio() {
    if !node_available() {
        return;
    }
    let Some(output) = run_node_script("scripts/lsp-smoke.js", &[env!("CARGO_BIN_EXE_nxlc")])
    else {
        return;
    };
    assert!(
        output.status.success(),
        "nxlc lsp did not behave like a language server:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_extensions_language_client_talks_to_the_server() {
    // The editor's features are only real if the shipped client can hold a
    // conversation with the shipped server.
    if !node_available() {
        return;
    }
    let Some(output) =
        run_node_script("scripts/lsp-client-check.js", &[env!("CARGO_BIN_EXE_nxlc")])
    else {
        return;
    };
    assert!(
        output.status.success(),
        "the extension's language client could not use nxlc lsp:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
